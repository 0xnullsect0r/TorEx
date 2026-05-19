/// WebSocket client with Noise XX + Signal ratchet encryption.
///
/// Key exchange (fixed DH):
///   1. Client generates random x25519 static key → provides to snow AND keeps for Signal
///   2. Noise XX handshake (3 binary frames)
///   3. After handshake: server_noise_static_pub = transport.get_remote_static()
///   4. Signal ratchet: init_sender(client_noise_static_priv, server_noise_static_pub)
///      Server must mirror: init_receiver(server_noise_static_priv, client_noise_static_pub)
///      ECDH property ensures both sides derive the same root key.
use std::cell::RefCell;
use std::rc::Rc;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use futures_util::{SinkExt, StreamExt};
use gloo_net::websocket::{futures::WebSocket, Message};
use hmac::{Hmac, Mac};
use leptos::*;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519Pub, StaticSecret};

use crate::core::storage;
use crate::core::types::{OrderBookSnapshot, SignalMessage, Trade};

type HmacSha256 = Hmac<Sha256>;

// ──────────────────────────────────────────────────────────────────────────
// Signal ratchet (mirrors backend crypto_primitives::signal_ratchet)
// ──────────────────────────────────────────────────────────────────────────

fn kdf_chain(chain_key: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut mac = HmacSha256::new_from_slice(chain_key).expect("hmac");
    mac.update(&[1u8]);
    let next: [u8; 32] = mac.finalize().into_bytes().into();
    let mut mac2 = HmacSha256::new_from_slice(chain_key).expect("hmac");
    mac2.update(&[2u8]);
    let msg: [u8; 32] = mac2.finalize().into_bytes().into();
    (next, msg)
}

fn derive_label(root: &[u8; 32], label: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(root).expect("hmac");
    mac.update(label);
    mac.finalize().into_bytes().into()
}

fn root_from_shared(shared: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(b"torex-root-v1").expect("hmac");
    mac.update(shared);
    mac.finalize().into_bytes().into()
}

struct Ratchet {
    root_key: [u8; 32],
    sending_chain_key: [u8; 32],
    receiving_chain_key: [u8; 32],
    sending_ratchet_key: [u8; 32],
    receiving_ratchet_key: [u8; 32],
    sending_counter: u32,
    receiving_counter: u32,
}

impl Ratchet {
    /// Client (initiator) side: init_sender(client_priv, server_pub)
    fn init_sender(local: &StaticSecret, remote_pub: &X25519Pub) -> Self {
        let shared = local.diffie_hellman(remote_pub);
        let root = root_from_shared(shared.as_bytes());
        let local_pub = X25519Pub::from(local).to_bytes();
        Self {
            root_key: root,
            sending_chain_key: derive_label(&root, b"initiator-send"),
            receiving_chain_key: derive_label(&root, b"initiator-recv"),
            sending_ratchet_key: local_pub,
            receiving_ratchet_key: remote_pub.to_bytes(),
            sending_counter: 0,
            receiving_counter: 0,
        }
    }

    fn encrypt(&mut self, plaintext: &[u8]) -> Result<SignalMessage, String> {
        let (next_chain, msg_key) = kdf_chain(&self.sending_chain_key);
        self.sending_chain_key = next_chain;
        let mut nonce_bytes = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
        let cipher = Aes256Gcm::new_from_slice(&msg_key).map_err(|e| e.to_string())?;
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
            .map_err(|_| "encrypt failed")?;
        let msg = SignalMessage {
            ratchet_key: self.sending_ratchet_key,
            counter: self.sending_counter,
            ciphertext: ct,
            nonce: nonce_bytes,
        };
        self.sending_counter = self.sending_counter.saturating_add(1);
        Ok(msg)
    }

    fn decrypt(&mut self, msg: &SignalMessage) -> Result<Vec<u8>, String> {
        if self.receiving_ratchet_key != msg.ratchet_key {
            self.receiving_ratchet_key = msg.ratchet_key;
            let secret = StaticSecret::from(self.sending_ratchet_key);
            let public = X25519Pub::from(msg.ratchet_key);
            let dh = secret.diffie_hellman(&public);
            self.root_key = root_from_shared(dh.as_bytes());
            self.receiving_chain_key = derive_label(&self.root_key, b"initiator-send");
            self.receiving_counter = 0;
        }
        let mut chain = self.receiving_chain_key;
        let mut msg_key_opt = None;
        for counter in self.receiving_counter..=msg.counter {
            let (next_chain, candidate) = kdf_chain(&chain);
            chain = next_chain;
            if counter == msg.counter {
                msg_key_opt = Some(candidate);
            }
        }
        self.receiving_chain_key = chain;
        self.receiving_counter = msg.counter + 1;
        let msg_key = msg_key_opt.ok_or("counter mismatch")?;
        let cipher = Aes256Gcm::new_from_slice(&msg_key).map_err(|e| e.to_string())?;
        cipher
            .decrypt(Nonce::from_slice(&msg.nonce), msg.ciphertext.as_slice())
            .map_err(|_| "decrypt failed".into())
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Public WS handle
// ──────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct WsHandle {
    pub orderbook: ReadSignal<OrderBookSnapshot>,
    pub trades: ReadSignal<Vec<Trade>>,
    pub connected: ReadSignal<bool>,
}

pub fn connect(pair: String) -> WsHandle {
    let (ob_r, ob_w) = create_signal(OrderBookSnapshot::default());
    let (trades_r, trades_w) = create_signal(Vec::<Trade>::new());
    let (connected_r, connected_w) = create_signal(false);

    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = run_ws(pair, ob_w, trades_w, connected_w).await {
            leptos::logging::warn!("WS error: {e}");
        }
    });

    WsHandle { orderbook: ob_r, trades: trades_r, connected: connected_r }
}

async fn run_ws(
    pair: String,
    ob_w: WriteSignal<OrderBookSnapshot>,
    trades_w: WriteSignal<Vec<Trade>>,
    connected_w: WriteSignal<bool>,
) -> Result<(), String> {
    let session_id = storage::get_session_id().unwrap_or_default();
    let url = build_ws_url(&session_id);

    let ws = WebSocket::open(&url).map_err(|e| format!("WS open: {e:?}"))?;
    let (mut sink, mut stream) = ws.split();

    // ── Generate client's Noise static key (kept for Signal ratchet) ───
    let mut client_static_bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut client_static_bytes);
    let client_static_secret = StaticSecret::from(client_static_bytes);

    // ── Noise XX handshake (initiator) ──────────────────────────────────
    let params: snow::params::NoiseParams =
        "Noise_XX_25519_ChaChaPoly_BLAKE2s".parse().map_err(|e: snow::Error| e.to_string())?;
    let mut hs = snow::Builder::new(params)
        .local_private_key(&client_static_bytes)
        .build_initiator()
        .map_err(|e| e.to_string())?;

    // msg1: initiator → responder
    let mut buf = vec![0u8; 1024];
    let len = hs.write_message(&[], &mut buf).map_err(|e| e.to_string())?;
    sink.send(Message::Bytes(buf[..len].to_vec()))
        .await
        .map_err(|e| format!("send msg1: {e:?}"))?;

    // msg2: responder → initiator
    let msg2 = match stream.next().await {
        Some(Ok(Message::Bytes(b))) => b,
        other => return Err(format!("Expected binary msg2, got {other:?}")),
    };
    let mut rx_buf = vec![0u8; msg2.len() + 1024];
    let _ = hs.read_message(&msg2, &mut rx_buf).map_err(|e| e.to_string())?;

    // msg3: initiator → responder
    let mut buf3 = vec![0u8; 1024];
    let len3 = hs.write_message(&[], &mut buf3).map_err(|e| e.to_string())?;
    sink.send(Message::Bytes(buf3[..len3].to_vec()))
        .await
        .map_err(|e| format!("send msg3: {e:?}"))?;

    // Enter transport mode → extract server's Noise static pub for Signal
    let transport = hs.into_transport_mode().map_err(|e| e.to_string())?;
    let server_noise_pub_bytes: [u8; 32] = transport
        .get_remote_static()
        .and_then(|b| b.try_into().ok())
        .unwrap_or([9u8; 32]);
    let server_noise_pub = X25519Pub::from(server_noise_pub_bytes);

    // ── Signal ratchet: DH(client_static, server_noise_pub) ────────────
    // Server must mirror with: init_receiver(server_noise_static_priv, client_noise_static_pub)
    let ratchet = Rc::new(RefCell::new(Ratchet::init_sender(
        &client_static_secret,
        &server_noise_pub,
    )));

    // ── Subscribe ───────────────────────────────────────────────────────
    let subscribe_msg = serde_json::json!({
        "type": "subscribe",
        "channels": [
            format!("orderbook:{pair}"),
            format!("trades:{pair}"),
            "user:orders",
        ]
    });
    let encrypted = ratchet
        .borrow_mut()
        .encrypt(subscribe_msg.to_string().as_bytes())?;
    sink.send(Message::Text(
        serde_json::to_string(&encrypted).map_err(|e| e.to_string())?,
    ))
    .await
    .map_err(|e| format!("subscribe send: {e:?}"))?;

    connected_w.set(true);

    // ── Receive loop ────────────────────────────────────────────────────
    while let Some(msg) = stream.next().await {
        let text = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Bytes(b)) => String::from_utf8_lossy(&b).into_owned(),
            Err(_) => break,
        };
        let signal: SignalMessage = match serde_json::from_str(&text) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let plaintext = match ratchet.borrow_mut().decrypt(&signal) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let value: serde_json::Value = match serde_json::from_slice(&plaintext) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value.get("bids").is_some() || value.get("asks").is_some() {
            if let Ok(snapshot) = serde_json::from_value::<OrderBookSnapshot>(value) {
                ob_w.set(snapshot);
            }
        } else if let Some(arr) = value.as_array() {
            if let Ok(trades) = serde_json::from_value::<Vec<Trade>>(
                serde_json::Value::Array(arr.clone()),
            ) {
                trades_w.update(|v| {
                    for t in trades {
                        v.insert(0, t);
                    }
                    v.truncate(50);
                });
            }
        }
    }

    connected_w.set(false);
    Ok(())
}

fn build_ws_url(session_id: &str) -> String {
    let loc = web_sys::window().unwrap().location();
    let host = loc.host().unwrap_or_default();
    let scheme = if loc.protocol().unwrap_or_default() == "https:" {
        "wss"
    } else {
        "ws"
    };
    format!("{scheme}://{host}/ws?session_id={session_id}")
}

