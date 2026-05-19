use std::{sync::Arc, time::Duration};

use auth::server_static_keypair;
use axum::{
    extract::{Query, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use crypto_primitives::{
    noise::NoiseSession,
    signal_ratchet::{RatchetSession, SignalMessage},
};
use futures_util::{SinkExt, StreamExt};
use matching::{OrderBooks, subscribe_trades, subscribe_user};
use rand::RngCore;
use redis::aio::ConnectionManager;
use serde::Deserialize;
use tokio::{sync::Mutex, time::interval};
use tracing::{info, warn};

#[derive(Clone)]
struct WsState {
    books: OrderBooks,
    #[allow(dead_code)]
    redis: ConnectionManager,
}

#[derive(Deserialize)]
struct WsParams { session_id: Option<String> }

pub fn router(books: OrderBooks, redis: ConnectionManager) -> Router {
    let state = Arc::new(WsState { books, redis });
    Router::new().route("/ws", get(move |ws: WebSocketUpgrade, query: Query<WsParams>| ws_handler(ws, query, state.clone())))
}

async fn ws_handler(ws: WebSocketUpgrade, Query(params): Query<WsParams>, state: Arc<WsState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, params.session_id, state))
}

async fn handle_socket(stream: axum::extract::ws::WebSocket, session_id: Option<String>, state: Arc<WsState>) {
    let (sender, mut receiver) = stream.split();
    let sender = Arc::new(Mutex::new(sender));
    let mut handshake = match NoiseSession::new_responder(&server_static_keypair()) { Ok(v) => v, Err(_) => return };
    let Some(Ok(axum::extract::ws::Message::Binary(first))) = receiver.next().await else { return; };
    let _ = handshake.read_message(&first);
    let response = match handshake.write_message(&[]) { Ok(v) => v, Err(_) => return };
    if sender.lock().await.send(axum::extract::ws::Message::Binary(response)).await.is_err() { return; }
    let transport = if handshake.is_handshake_finished() {
        handshake.into_transport().ok()
    } else {
        let Some(Ok(axum::extract::ws::Message::Binary(third))) = receiver.next().await else { return; };
        let _ = handshake.read_message(&third);
        handshake.into_transport().ok()
    };
    let Some(transport) = transport else { return; };
    let remote = transport.remote_static_pubkey().unwrap_or([9_u8; 32].to_vec());
    let mut local_bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut local_bytes);
    let local_secret = x25519_dalek::StaticSecret::from(local_bytes);
    let remote_bytes: [u8; 32] = remote[..32].try_into().unwrap_or([9_u8; 32]);
    let remote_pub = x25519_dalek::PublicKey::from(remote_bytes);
    let ratchet = Arc::new(Mutex::new(RatchetSession::init_receiver(&local_secret, &remote_pub)));
    while let Some(Ok(message)) = receiver.next().await {
        let axum::extract::ws::Message::Text(text) = message else { continue; };
        let Ok(signal) = serde_json::from_str::<SignalMessage>(&text) else { continue; };
        let Ok(plaintext) = ratchet.lock().await.decrypt(&signal) else { continue; };
        let Ok(command) = serde_json::from_slice::<serde_json::Value>(&plaintext) else { continue; };
        if command.get("type").and_then(|v| v.as_str()) != Some("subscribe") { continue; }
        let channels = command.get("channels").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        for channel in channels.into_iter().filter_map(|v| v.as_str().map(|s| s.to_string())) {
            if let Some(pair) = channel.strip_prefix("orderbook:") {
                let books = state.books.clone();
                let tx = sender.clone();
                let ratchet = ratchet.clone();
                let pair = pair.to_string();
                tokio::spawn(async move {
                    let mut ticker = interval(Duration::from_millis(100));
                    loop {
                        ticker.tick().await;
                        let snapshot = books.get(&pair).map(|b| b.lock().depth_snapshot(25)).unwrap_or_default();
                        let plaintext = serde_json::to_vec(&snapshot).unwrap_or_default();
                        let Ok(signal) = ratchet.lock().await.encrypt(&plaintext) else { continue; };
                        if tx.lock().await.send(axum::extract::ws::Message::Text(serde_json::to_string(&signal).unwrap_or_default())).await.is_err() { break; }
                    }
                });
            } else if let Some(pair) = channel.strip_prefix("trades:") {
                let mut rx = subscribe_trades(pair);
                let tx = sender.clone();
                let ratchet = ratchet.clone();
                tokio::spawn(async move {
                    while let Ok(msg) = rx.recv().await {
                        let Ok(signal) = ratchet.lock().await.encrypt(msg.as_bytes()) else { continue; };
                        if tx.lock().await.send(axum::extract::ws::Message::Text(serde_json::to_string(&signal).unwrap_or_default())).await.is_err() { break; }
                    }
                });
            } else if channel == "user:orders" || channel == "user:balance_tier" {
                if let Some(session_id) = &session_id {
                    let mut rx = subscribe_user(session_id);
                    let tx = sender.clone();
                    let ratchet = ratchet.clone();
                    tokio::spawn(async move {
                        while let Ok(msg) = rx.recv().await {
                            let Ok(signal) = ratchet.lock().await.encrypt(msg.as_bytes()) else { continue; };
                            if tx.lock().await.send(axum::extract::ws::Message::Text(serde_json::to_string(&signal).unwrap_or_default())).await.is_err() { break; }
                        }
                    });
                } else {
                    warn!(event = "ws_missing_session_id", "user-scoped websocket channel requested without session id");
                }
            }
        }
        info!(event = "ws_subscribed", "websocket subscriptions started");
    }
}
