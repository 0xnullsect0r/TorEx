use std::sync::{Arc, OnceLock};

use anyhow::Context;
use auth::NoiseAuth;
use axum::{extract::Json, routing::{get, post}, Router};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use bigdecimal::BigDecimal;
use crypto_primitives::{
    balance_enc::decrypt_balance,
    stealth::generate_stealth_address,
    zk::ZkProver,
};
use serde::{Deserialize, Serialize};
use shared::{config::AppConfig, error::AppError, kafka::KafkaProducer};
use sqlx::{PgPool, Row};
use tracing::info;

static WALLET_STATE: OnceLock<Arc<WalletState>> = OnceLock::new();

#[derive(Clone)]
pub struct WalletState {
    pool: PgPool,
    kafka: KafkaProducer,
    zk: Arc<ZkProver>,
    config: AppConfig,
}

#[derive(Serialize)]
struct DepositAddressResponse {
    stealth_address: String,
    ephemeral_pubkey: String,
    chain: String,
}

#[derive(Deserialize)]
struct WithdrawRequest {
    dest_address: String,
    amount: String,
    chain: String,
    zk_proof: String,
}

#[derive(Serialize)]
struct BalanceResponse {
    enc_balance: String,
    tier_proofs: Vec<TierProof>,
}

#[derive(Serialize)]
struct TierProof {
    tier: u64,
    proof: String,
}

pub fn init_state(pool: PgPool, kafka: KafkaProducer, config: AppConfig) -> anyhow::Result<()> {
    let zk = Arc::new(ZkProver::setup()?);
    let _ = WALLET_STATE.set(Arc::new(WalletState { pool, kafka, zk, config }));
    Ok(())
}

pub fn router() -> Router {
    Router::new()
        .route("/api/wallet/deposit-address", get(deposit_address))
        .route("/api/wallet/withdraw", post(withdraw))
        .route("/api/wallet/balance", get(balance))
}

async fn deposit_address(NoiseAuth(user_id): NoiseAuth) -> Result<Json<DepositAddressResponse>, AppError> {
    let row = sqlx::query("SELECT spend_pubkey, view_pubkey FROM users WHERE user_id = $1")
        .bind(user_id.to_hex())
        .fetch_optional(&wallet_state().pool)
        .await
        .map_err(internal)?
        .ok_or(AppError::NotFound)?;
    let spend = hex::decode(row.get::<String, _>("spend_pubkey")).map_err(internal)?;
    let view = hex::decode(row.get::<String, _>("view_pubkey")).map_err(internal)?;
    let spend_bytes: [u8; 32] = spend[..32].try_into().map_err(|_| AppError::BadRequest("invalid spend key".into()))?;
    let view_bytes: [u8; 32] = view[..32].try_into().map_err(|_| AppError::BadRequest("invalid view key".into()))?;
    let spend_pubkey = x25519_dalek::PublicKey::from(spend_bytes);
    let view_pubkey = x25519_dalek::PublicKey::from(view_bytes);
    let (address, _ephemeral_secret) = generate_stealth_address(&spend_pubkey, &view_pubkey);
    let chain = if chrono::Utc::now().timestamp() % 2 == 0 { "ERC20" } else { "TRC20" }.to_string();
    sqlx::query(
        "INSERT INTO deposits (user_id, stealth_addr, ephemeral_pubkey, chain, created_at) VALUES ($1, $2, $3, $4, NOW())",
    )
    .bind(user_id.to_hex())
    .bind(hex::encode(address.address_pubkey.as_bytes()))
    .bind(hex::encode(address.ephemeral_pubkey.as_bytes()))
    .bind(&chain)
    .execute(&wallet_state().pool)
    .await
    .map_err(internal)?;
    Ok(Json(DepositAddressResponse {
        stealth_address: hex::encode(address.address_pubkey.as_bytes()),
        ephemeral_pubkey: hex::encode(address.ephemeral_pubkey.as_bytes()),
        chain,
    }))
}

async fn withdraw(NoiseAuth(user_id): NoiseAuth, Json(payload): Json<WithdrawRequest>) -> Result<Json<serde_json::Value>, AppError> {
    let amount = payload.amount.parse::<u64>().map_err(|_| AppError::BadRequest("invalid amount".into()))?;
    let proof = BASE64.decode(payload.zk_proof).map_err(|_| AppError::BadRequest("invalid zk_proof".into()))?;
    if !wallet_state().zk.verify(&proof, amount).map_err(|e| AppError::BadRequest(e.to_string()))? {
        return Err(AppError::Unauthorized);
    }
    let mut tx = wallet_state().pool.begin().await.map_err(internal)?;
    let row = sqlx::query("SELECT enc_balance FROM balances WHERE user_id = $1 FOR UPDATE")
        .bind(user_id.to_hex())
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?
        .ok_or(AppError::NotFound)?;
    let enc_balance: Vec<u8> = row.get("enc_balance");
    let key = fetch_balance_key().await.unwrap_or([7_u8; 32]);
    let current = decrypt_balance(&key, &enc_balance).map_err(|_| AppError::Unauthorized)?;
    if current < amount {
        return Err(AppError::Conflict);
    }
    let remaining = current - amount;
    let new_enc = crypto_primitives::balance_enc::encrypt_balance(&key, remaining);
    sqlx::query("UPDATE balances SET enc_balance = $1, updated_at = NOW() WHERE user_id = $2")
        .bind(new_enc)
        .bind(user_id.to_hex())
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
    let withdrawal_id = uuid::Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO withdrawals (withdrawal_id, user_id, dest_address, amount, chain, status, created_at) VALUES ($1, $2, $3, $4, $5, 'queued', NOW())",
    )
    .bind(&withdrawal_id)
    .bind(user_id.to_hex())
    .bind(&payload.dest_address)
    .bind(BigDecimal::from(amount))
    .bind(&payload.chain)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let event = serde_json::json!({
        "withdrawal_id": withdrawal_id,
        "dest_address": payload.dest_address,
        "amount": amount,
        "chain": payload.chain,
    });
    wallet_state().kafka.send("withdrawals", &user_id.to_hex(), &event.to_string()).await.map_err(internal)?;
    info!(event = "withdrawal_requested", "queued withdrawal");
    Ok(Json(serde_json::json!({"status": "queued"})))
}

async fn balance(NoiseAuth(user_id): NoiseAuth) -> Result<Json<BalanceResponse>, AppError> {
    let row = sqlx::query("SELECT enc_balance FROM balances WHERE user_id = $1")
        .bind(user_id.to_hex())
        .fetch_optional(&wallet_state().pool)
        .await
        .map_err(internal)?
        .ok_or(AppError::NotFound)?;
    let enc_balance: Vec<u8> = row.get("enc_balance");
    let key = fetch_balance_key().await.unwrap_or([7_u8; 32]);
    let plain = decrypt_balance(&key, &enc_balance).unwrap_or_default();
    let mut proofs = Vec::new();
    for tier in [100_u64, 1_000, 10_000, 100_000] {
        let proof = wallet_state()
            .zk
            .prove(plain, tier, [tier as u8; 32])
            .unwrap_or_default();
        proofs.push(TierProof {
            tier,
            proof: BASE64.encode(proof),
        });
    }
    Ok(Json(BalanceResponse {
        enc_balance: BASE64.encode(enc_balance),
        tier_proofs: proofs,
    }))
}

async fn fetch_balance_key() -> anyhow::Result<[u8; 32]> {
    if wallet_state().config.vault_addr.is_empty() {
        return Ok([7_u8; 32]);
    }
    let client = reqwest::Client::new();
    let response: serde_json::Value = client
        .get(format!("{}/v1/secret/data/torex", wallet_state().config.vault_addr.trim_end_matches('/')))
        .header("X-Vault-Token", &wallet_state().config.vault_token)
        .send()
        .await
        .context("failed to read vault secret")?
        .json()
        .await
        .context("failed to parse vault secret")?;
    let key_hex = response
        .pointer("/data/data/balance_key")
        .and_then(|v| v.as_str())
        .unwrap_or("0707070707070707070707070707070707070707070707070707070707070707");
    let bytes = hex::decode(key_hex)?;
    Ok(bytes.try_into().map_err(|_| anyhow::anyhow!("invalid vault key length"))?)
}

fn wallet_state() -> &'static Arc<WalletState> {
    WALLET_STATE.get().expect("wallet state initialized")
}

fn internal<E>(err: E) -> AppError
where
    E: Into<anyhow::Error>,
{
    AppError::Internal(err.into())
}
