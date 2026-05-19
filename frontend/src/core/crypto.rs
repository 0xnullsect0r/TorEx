use bip39::{Language, Mnemonic};
use ed25519_dalek::{SigningKey, Signer};
use hmac::{Hmac, Mac};
use sha2::Sha512;

type HmacSha512 = Hmac<Sha512>;

/// Derive 64-byte HMAC-SHA512 with a label over a seed.
fn hmac512(seed: &[u8], label: &[u8]) -> [u8; 64] {
    let mut mac = HmacSha512::new_from_slice(label).expect("hmac init");
    mac.update(seed);
    mac.finalize().into_bytes().into()
}

pub struct DerivedKeys {
    pub signing_key: SigningKey,
    /// Compressed ed25519 verifying key (32 bytes hex)
    pub pubkey_hex: String,
    /// x25519 public key for stealth view addresses (32 bytes hex)
    pub view_pubkey_hex: String,
    /// x25519 public key for stealth spend addresses (32 bytes hex)
    pub spend_pubkey_hex: String,
    /// Raw identity bytes (first 32 bytes) — kept for Noise static key
    pub identity_bytes: [u8; 32],
}

impl DerivedKeys {
    pub fn from_seed(seed: &[u8]) -> Self {
        // Derive sub-keys via HMAC-SHA512 (same labels as backend KeyService)
        let identity = hmac512(seed, b"torex-identity-v1");
        let view_raw  = hmac512(seed, b"torex-view-v1");
        let spend_raw = hmac512(seed, b"torex-spend-v1");

        // Ed25519 signing key from first 32 bytes of identity material
        let identity_bytes: [u8; 32] = identity[..32].try_into().expect("32 bytes");
        let signing_key = SigningKey::from_bytes(&identity_bytes);
        let pubkey_hex = hex::encode(signing_key.verifying_key().to_bytes());

        // X25519 view / spend keys
        let view_secret = x25519_dalek::StaticSecret::from(
            <[u8; 32]>::try_from(&view_raw[..32]).expect("32 bytes"),
        );
        let spend_secret = x25519_dalek::StaticSecret::from(
            <[u8; 32]>::try_from(&spend_raw[..32]).expect("32 bytes"),
        );
        let view_pubkey_hex  = hex::encode(x25519_dalek::PublicKey::from(&view_secret).to_bytes());
        let spend_pubkey_hex = hex::encode(x25519_dalek::PublicKey::from(&spend_secret).to_bytes());

        Self { signing_key, pubkey_hex, view_pubkey_hex, spend_pubkey_hex, identity_bytes }
    }
}

/// Generate a new 24-word BIP39 mnemonic.
pub fn generate_mnemonic() -> String {
    let mnemonic = Mnemonic::generate_in(Language::English, 24)
        .expect("entropy available");
    mnemonic.to_string()
}

/// Validate a mnemonic phrase.
pub fn validate_mnemonic(phrase: &str) -> bool {
    Mnemonic::parse_in(Language::English, phrase).is_ok()
}

/// Derive keys from a mnemonic phrase.
/// Returns None if the phrase is invalid.
pub fn keys_from_mnemonic(phrase: &str) -> Option<DerivedKeys> {
    let mnemonic = Mnemonic::parse_in(Language::English, phrase).ok()?;
    let seed = mnemonic.to_seed("");
    Some(DerivedKeys::from_seed(&seed))
}

/// Sign the registration message: pubkey_bytes || timestamp_le_bytes.
/// Returns (signature_hex, timestamp_secs).
pub fn sign_registration(signing_key: &SigningKey, pubkey_bytes: &[u8; 32]) -> (String, u64) {
    let timestamp = (js_sys::Date::now() / 1000.0) as u64;
    let mut message = Vec::with_capacity(40);
    message.extend_from_slice(pubkey_bytes);
    message.extend_from_slice(&timestamp.to_le_bytes());
    let sig = signing_key.sign(&message);
    (hex::encode(sig.to_bytes()), timestamp)
}
