pub mod error {
    #[derive(thiserror::Error, Debug)]
    pub enum CryptoError {
        #[error("noise error: {0}")]
        Noise(String),
        #[error("signature verification failed")]
        SignatureInvalid,
        #[error("zk proof invalid")]
        ZkInvalid,
        #[error("stealth scan error")]
        StealthError,
        #[error("encryption error")]
        EncryptionError,
        #[error(transparent)]
        Other(#[from] anyhow::Error),
    }
}

pub mod noise {
    use snow::{params::NoiseParams, Builder, HandshakeState, TransportState};

    use crate::error::CryptoError;

    pub struct NoiseSession {
        state: HandshakeState,
    }

    impl NoiseSession {
        pub fn new_initiator(_server_static_pubkey: &[u8]) -> Result<Self, CryptoError> {
            let params: NoiseParams = "Noise_XX_25519_ChaChaPoly_BLAKE2s"
                .parse::<NoiseParams>()
                .map_err(|e: snow::Error| CryptoError::Noise(e.to_string()))?;
            let builder = Builder::new(params);
            let state = builder
                .build_initiator()
                .map_err(|e| CryptoError::Noise(e.to_string()))?;
            Ok(Self { state })
        }

        pub fn new_responder(static_keypair: &[u8; 64]) -> Result<Self, CryptoError> {
            let params: NoiseParams = "Noise_XX_25519_ChaChaPoly_BLAKE2s"
                .parse::<NoiseParams>()
                .map_err(|e: snow::Error| CryptoError::Noise(e.to_string()))?;
            let state = Builder::new(params)
                .local_private_key(&static_keypair[..32])
                .build_responder()
                .map_err(|e| CryptoError::Noise(e.to_string()))?;
            Ok(Self { state })
        }

        pub fn read_message(&mut self, input: &[u8]) -> Result<Vec<u8>, CryptoError> {
            let mut buf = vec![0_u8; input.len() + 1024];
            let len = self
                .state
                .read_message(input, &mut buf)
                .map_err(|e| CryptoError::Noise(e.to_string()))?;
            buf.truncate(len);
            Ok(buf)
        }

        pub fn write_message(&mut self, payload: &[u8]) -> Result<Vec<u8>, CryptoError> {
            let mut buf = vec![0_u8; payload.len() + 1024];
            let len = self
                .state
                .write_message(payload, &mut buf)
                .map_err(|e| CryptoError::Noise(e.to_string()))?;
            buf.truncate(len);
            Ok(buf)
        }

        pub fn into_transport(self) -> Result<NoiseTransport, CryptoError> {
            let state = self
                .state
                .into_transport_mode()
                .map_err(|e| CryptoError::Noise(e.to_string()))?;
            Ok(NoiseTransport { state })
        }

        pub fn is_handshake_finished(&self) -> bool {
            self.state.is_handshake_finished()
        }
    }

    pub struct NoiseTransport {
        state: TransportState,
    }

    impl NoiseTransport {
        pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
            let mut out = vec![0_u8; plaintext.len() + 16];
            let len = self
                .state
                .write_message(plaintext, &mut out)
                .map_err(|e| CryptoError::Noise(e.to_string()))?;
            out.truncate(len);
            Ok(out)
        }

        pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
            let mut out = vec![0_u8; ciphertext.len()];
            let len = self
                .state
                .read_message(ciphertext, &mut out)
                .map_err(|e| CryptoError::Noise(e.to_string()))?;
            out.truncate(len);
            Ok(out)
        }

        pub fn remote_static_pubkey(&self) -> Option<Vec<u8>> {
            self.state.get_remote_static().map(|bytes| bytes.to_vec())
        }
    }
}

pub mod signal_ratchet {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    use hmac::{Hmac, Mac};
    use rand::RngCore;
    use sha2::{Digest, Sha256};
    use x25519_dalek::{PublicKey, StaticSecret};

    use crate::error::CryptoError;

    type HmacSha256 = Hmac<Sha256>;

    #[derive(Clone)]
    pub struct RatchetSession {
        root_key: [u8; 32],
        sending_chain_key: [u8; 32],
        receiving_chain_key: [u8; 32],
        sending_ratchet_key: [u8; 32],
        receiving_ratchet_key: [u8; 32],
        sending_counter: u32,
        receiving_counter: u32,
    }

    #[derive(serde::Serialize, serde::Deserialize, Clone)]
    pub struct SignalMessage {
        pub ratchet_key: [u8; 32],
        pub counter: u32,
        pub ciphertext: Vec<u8>,
        pub nonce: [u8; 12],
    }

    impl RatchetSession {
        pub fn init_sender(identity_key: &StaticSecret, recipient_pubkey: &PublicKey) -> Self {
            let root = root_from_shared(identity_key.diffie_hellman(recipient_pubkey).as_bytes());
            Self {
                root_key: root,
                sending_chain_key: derive_label(&root, b"initiator-send"),
                receiving_chain_key: derive_label(&root, b"initiator-recv"),
                sending_ratchet_key: identity_key.to_bytes(),
                receiving_ratchet_key: recipient_pubkey.to_bytes(),
                sending_counter: 0,
                receiving_counter: 0,
            }
        }

        pub fn init_receiver(identity_key: &StaticSecret, sender_pubkey: &PublicKey) -> Self {
            let root = root_from_shared(identity_key.diffie_hellman(sender_pubkey).as_bytes());
            Self {
                root_key: root,
                sending_chain_key: derive_label(&root, b"initiator-recv"),
                receiving_chain_key: derive_label(&root, b"initiator-send"),
                sending_ratchet_key: identity_key.to_bytes(),
                receiving_ratchet_key: sender_pubkey.to_bytes(),
                sending_counter: 0,
                receiving_counter: 0,
            }
        }

        pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<SignalMessage, CryptoError> {
            let (next_chain, msg_key) = kdf_chain(self.sending_chain_key);
            self.sending_chain_key = next_chain;
            let mut nonce = [0_u8; 12];
            rand::rngs::OsRng.fill_bytes(&mut nonce);
            let cipher = Aes256Gcm::new_from_slice(&msg_key).map_err(|_| CryptoError::EncryptionError)?;
            let ciphertext = cipher
                .encrypt(Nonce::from_slice(&nonce), plaintext)
                .map_err(|_| CryptoError::EncryptionError)?;
            let ratchet_secret = StaticSecret::from(self.sending_ratchet_key);
            let ratchet_pub = PublicKey::from(&ratchet_secret).to_bytes();
            let message = SignalMessage {
                ratchet_key: ratchet_pub,
                counter: self.sending_counter,
                ciphertext,
                nonce,
            };
            self.sending_counter = self.sending_counter.saturating_add(1);
            Ok(message)
        }

        pub fn decrypt(&mut self, msg: &SignalMessage) -> Result<Vec<u8>, CryptoError> {
            if self.receiving_ratchet_key != msg.ratchet_key {
                self.receiving_ratchet_key = msg.ratchet_key;
                let secret = StaticSecret::from(self.sending_ratchet_key);
                let public = PublicKey::from(msg.ratchet_key);
                let dh = secret.diffie_hellman(&public);
                self.root_key = root_from_shared(dh.as_bytes());
                self.receiving_chain_key = derive_label(&self.root_key, b"initiator-send");
                self.receiving_counter = 0;
            }
            let mut chain = self.receiving_chain_key;
            let mut msg_key = None;
            for counter in self.receiving_counter..=msg.counter {
                let (next_chain, candidate_key) = kdf_chain(chain);
                chain = next_chain;
                if counter == msg.counter {
                    msg_key = Some(candidate_key);
                }
            }
            self.receiving_chain_key = chain;
            self.receiving_counter = msg.counter.saturating_add(1);
            let key = msg_key.ok_or(CryptoError::EncryptionError)?;
            let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| CryptoError::EncryptionError)?;
            cipher
                .decrypt(Nonce::from_slice(&msg.nonce), msg.ciphertext.as_ref())
                .map_err(|_| CryptoError::EncryptionError)
        }
    }

    fn root_from_shared(shared: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(shared);
        hasher.finalize().into()
    }

    fn derive_label(root: &[u8; 32], label: &[u8]) -> [u8; 32] {
        let mut mac = <HmacSha256 as Mac>::new_from_slice(root).expect("hmac key");
        mac.update(label);
        mac.finalize().into_bytes().into()
    }

    fn kdf_chain(chain_key: [u8; 32]) -> ([u8; 32], [u8; 32]) {
        let mut mac_chain = <HmacSha256 as Mac>::new_from_slice(&chain_key).expect("hmac key");
        mac_chain.update(b"chain");
        let next_chain: [u8; 32] = mac_chain.finalize().into_bytes().into();

        let mut mac_msg = <HmacSha256 as Mac>::new_from_slice(&chain_key).expect("hmac key");
        mac_msg.update(b"message");
        let msg_key: [u8; 32] = mac_msg.finalize().into_bytes().into();
        (next_chain, msg_key)
    }
}

pub mod zk {
    use anyhow::Context;
    use ark_bn254::{Bn254, Fr};
    use ark_ff::PrimeField;
    use ark_groth16::{prepare_verifying_key, Groth16, Proof, ProvingKey, VerifyingKey};
    use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
    use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
    use ark_snark::SNARK;
    use ark_std::rand::{rngs::StdRng, SeedableRng};

    use crate::error::CryptoError;

    #[derive(Clone)]
    pub struct BalanceCircuit {
        pub actual_balance: Option<u64>,
        pub min_required: u64,
        pub randomness: Option<[u8; 32]>,
    }

    impl ConstraintSynthesizer<Fr> for BalanceCircuit {
        fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
            let actual = FpVar::<Fr>::new_witness(cs.clone(), || {
                Ok(Fr::from(self.actual_balance.unwrap_or_default()))
            })?;
            let min = FpVar::<Fr>::new_input(cs.clone(), || Ok(Fr::from(self.min_required)))?;
            let diff = FpVar::<Fr>::new_witness(cs.clone(), || {
                Ok(Fr::from(
                    self.actual_balance
                        .unwrap_or_default()
                        .saturating_sub(self.min_required),
                ))
            })?;
            let randomness_bytes = self.randomness.unwrap_or([0_u8; 32]);
            let randomness = FpVar::<Fr>::new_witness(cs, || {
                Ok(Fr::from_le_bytes_mod_order(&randomness_bytes))
            })?;
            let lhs = &actual + &randomness;
            let rhs = &diff + &min + &randomness;
            lhs.enforce_equal(&rhs)
        }
    }

    pub struct ZkProver {
        pub proving_key: ProvingKey<Bn254>,
        pub verifying_key: VerifyingKey<Bn254>,
    }

    impl ZkProver {
        pub fn setup() -> Result<Self, CryptoError> {
            let mut rng = StdRng::from_seed([7_u8; 32]);
            let circuit = BalanceCircuit {
                actual_balance: None,
                min_required: 0,
                randomness: None,
            };
            let (proving_key, verifying_key) = Groth16::<Bn254>::circuit_specific_setup(circuit, &mut rng)
                .map_err(|e| CryptoError::Other(anyhow::anyhow!(e)))?;
            Ok(Self {
                proving_key,
                verifying_key,
            })
        }

        pub fn prove(
            &self,
            balance: u64,
            min_required: u64,
            randomness: [u8; 32],
        ) -> Result<Vec<u8>, CryptoError> {
            if balance < min_required {
                return Err(CryptoError::ZkInvalid);
            }
            let circuit = BalanceCircuit {
                actual_balance: Some(balance),
                min_required,
                randomness: Some(randomness),
            };
            let mut rng = StdRng::from_seed([9_u8; 32]);
            let proof = Groth16::<Bn254>::prove(&self.proving_key, circuit, &mut rng)
                .map_err(|e| CryptoError::Other(anyhow::anyhow!(e)))?;
            let mut bytes = Vec::new();
            proof
                .serialize_compressed(&mut bytes)
                .context("failed to serialize proof")?;
            Ok(bytes)
        }

        pub fn verify(&self, proof_bytes: &[u8], min_required: u64) -> Result<bool, CryptoError> {
            let proof = Proof::<Bn254>::deserialize_compressed(proof_bytes)
                .context("failed to deserialize proof")?;
            let prepared = prepare_verifying_key(&self.verifying_key);
            Groth16::<Bn254>::verify_with_processed_vk(&prepared, &[Fr::from(min_required)], &proof)
                .map_err(|e| CryptoError::Other(anyhow::anyhow!(e)))
        }
    }
}

pub mod stealth {
    use blake2::{Blake2s256, Digest};
    use rand::RngCore;
    use x25519_dalek::{PublicKey, StaticSecret};

    #[derive(Clone)]
    pub struct StealthKeyPair {
        pub spend_secret: StaticSecret,
        pub view_secret: StaticSecret,
        pub spend_pubkey: PublicKey,
        pub view_pubkey: PublicKey,
    }

    pub struct StealthAddress {
        pub address_pubkey: PublicKey,
        pub ephemeral_pubkey: PublicKey,
    }

    impl StealthKeyPair {
        pub fn from_seed(seed: &[u8; 64]) -> Self {
            let spend_bytes: [u8; 32] = seed[..32].try_into().expect("spend bytes");
            let view_bytes: [u8; 32] = seed[32..].try_into().expect("view bytes");
            let spend_secret = StaticSecret::from(spend_bytes);
            let view_secret = StaticSecret::from(view_bytes);
            let spend_pubkey = PublicKey::from(&spend_secret);
            let view_pubkey = PublicKey::from(&view_secret);
            Self {
                spend_secret,
                view_secret,
                spend_pubkey,
                view_pubkey,
            }
        }
    }

    pub fn generate_stealth_address(
        spend_pubkey: &PublicKey,
        view_pubkey: &PublicKey,
    ) -> (StealthAddress, StaticSecret) {
        let mut secret_bytes = [0_u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret_bytes);
        let ephemeral_secret = StaticSecret::from(secret_bytes);
        let ephemeral_pubkey = PublicKey::from(&ephemeral_secret);
        let shared = ephemeral_secret.diffie_hellman(view_pubkey);
        let tweak = derive_tweak(shared.as_bytes(), spend_pubkey.as_bytes());
        let tweak_secret = StaticSecret::from(tweak);
        let address_pubkey = PublicKey::from(&tweak_secret);
        (
            StealthAddress {
                address_pubkey,
                ephemeral_pubkey,
            },
            ephemeral_secret,
        )
    }

    pub fn scan_stealth_address(
        keys: &StealthKeyPair,
        ephemeral_pubkey: &PublicKey,
        candidate_pubkey: &PublicKey,
    ) -> bool {
        let shared = keys.view_secret.diffie_hellman(ephemeral_pubkey);
        let tweak = derive_tweak(shared.as_bytes(), keys.spend_pubkey.as_bytes());
        let tweak_secret = StaticSecret::from(tweak);
        let expected = PublicKey::from(&tweak_secret);
        expected.to_bytes() == candidate_pubkey.to_bytes()
    }

    fn derive_tweak(shared: &[u8], spend_pubkey: &[u8; 32]) -> [u8; 32] {
        let mut hasher = Blake2s256::new();
        hasher.update(shared);
        hasher.update(spend_pubkey);
        hasher.finalize().into()
    }
}

pub mod balance_enc {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    use rand::RngCore;

    use crate::error::CryptoError;

    pub fn encrypt_balance(key: &[u8; 32], balance: u64) -> Vec<u8> {
        let cipher = Aes256Gcm::new_from_slice(key).expect("valid key length");
        let mut nonce = [0_u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let mut out = nonce.to_vec();
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), balance.to_le_bytes().as_ref())
            .expect("balance encryption");
        out.extend(ciphertext);
        out
    }

    pub fn decrypt_balance(key: &[u8; 32], ciphertext: &[u8]) -> Result<u64, CryptoError> {
        if ciphertext.len() < 12 {
            return Err(CryptoError::EncryptionError);
        }
        let (nonce, body) = ciphertext.split_at(12);
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::EncryptionError)?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce), body)
            .map_err(|_| CryptoError::EncryptionError)?;
        let bytes: [u8; 8] = plaintext
            .try_into()
            .map_err(|_| CryptoError::EncryptionError)?;
        Ok(u64::from_le_bytes(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use x25519_dalek::{PublicKey, StaticSecret};

    #[test]
    fn test_balance_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 32];
        let original = 123_456_789u64;
        let encrypted = balance_enc::encrypt_balance(&key, original);
        let decrypted = balance_enc::decrypt_balance(&key, &encrypted).expect("decrypt failed");
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_balance_encrypt_zero() {
        let key = [0x01u8; 32];
        let encrypted = balance_enc::encrypt_balance(&key, 0);
        let decrypted = balance_enc::decrypt_balance(&key, &encrypted).unwrap();
        assert_eq!(decrypted, 0);
    }

    #[test]
    fn test_balance_wrong_key_fails() {
        let key1 = [0x42u8; 32];
        let key2 = [0x43u8; 32];
        let encrypted = balance_enc::encrypt_balance(&key1, 42);
        let result = balance_enc::decrypt_balance(&key2, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_stealth_address_scan_owns() {
        let seed = [0xAAu8; 64];
        let keys = stealth::StealthKeyPair::from_seed(&seed);
        let (addr, _ephemeral_secret) = stealth::generate_stealth_address(&keys.spend_pubkey, &keys.view_pubkey);
        let owns = stealth::scan_stealth_address(&keys, &addr.ephemeral_pubkey, &addr.address_pubkey);
        assert!(owns, "owner should be able to scan their own stealth address");
    }

    #[test]
    fn test_stealth_address_different_keys_dont_match() {
        let seed1 = [0xAAu8; 64];
        let seed2 = [0xBBu8; 64];
        let keys1 = stealth::StealthKeyPair::from_seed(&seed1);
        let keys2 = stealth::StealthKeyPair::from_seed(&seed2);
        let (addr, _) = stealth::generate_stealth_address(&keys1.spend_pubkey, &keys1.view_pubkey);
        let owns = stealth::scan_stealth_address(&keys2, &addr.ephemeral_pubkey, &addr.address_pubkey);
        assert!(!owns, "wrong key should not scan stealth address");
    }

    #[test]
    fn test_noise_responder_init() {
        let static_keypair = [0u8; 64];
        let result = noise::NoiseSession::new_responder(&static_keypair);
        let _ = result;
    }

    #[test]
    fn test_signal_ratchet_roundtrip() {
        let alice_secret = StaticSecret::from([1u8; 32]);
        let bob_secret = StaticSecret::from([2u8; 32]);
        let alice_pub = PublicKey::from(&alice_secret);
        let bob_pub = PublicKey::from(&bob_secret);
        let mut sender = signal_ratchet::RatchetSession::init_sender(&alice_secret, &bob_pub);
        let mut receiver = signal_ratchet::RatchetSession::init_receiver(&bob_secret, &alice_pub);

        let msg = sender.encrypt(b"hello").expect("encrypt failed");
        let plaintext = receiver.decrypt(&msg).expect("decrypt failed");
        assert_eq!(plaintext, b"hello");
    }

    #[test]
    fn test_zk_prover_roundtrip() {
        let prover = zk::ZkProver::setup().expect("setup failed");
        let proof = prover.prove(100, 50, [3u8; 32]).expect("prove failed");
        let verified = prover.verify(&proof, 50).expect("verify failed");
        assert!(verified);
    }
}
