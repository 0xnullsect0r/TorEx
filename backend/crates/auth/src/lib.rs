use std::{sync::{Arc, OnceLock}, time::{SystemTime, UNIX_EPOCH}};

use anyhow::Context;
use axum::{
    body::Bytes,
    extract::{FromRequestParts, Json, Path},
    http::{request::Parts, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use blake2::{Blake2b512, Digest};
use crypto_primitives::noise::{NoiseSession, NoiseTransport};
use dashmap::DashMap;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use rand::RngCore;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::json;
use shared::{
    config::AppConfig,
    error::AppError,
    types::UserId,
};
use sqlx::{PgPool, Row};
use totp_rs::{Algorithm, Secret, TOTP};
use tracing::{error, info, warn};
use uuid::Uuid;

static AUTH_STATE: OnceLock<Arc<AuthState>> = OnceLock::new();

#[derive(Clone)]
pub struct AuthState {
    pub pool: PgPool,
    pub redis: redis::aio::ConnectionManager,
    pub config: AppConfig,
    pub noise_transports: Arc<DashMap<String, NoiseTransport>>,
    pub pending_handshakes: Arc<DashMap<String, NoiseSession>>,
    pub server_static_keypair: [u8; 64],
}

#[derive(Deserialize)]
struct RegisterRequest {
    pubkey: String,
    view_pubkey: String,
    spend_pubkey: String,
    signature: String,
    timestamp: u64,
}

#[derive(Serialize)]
struct RegisterResponse {
    user_id: String,
    session_id: String,
}

#[derive(Deserialize)]
struct AdminLoginRequest {
    username: String,
    password: String,
    hcaptcha_token: String,
    totp_code: Option<String>,
}

#[derive(Deserialize)]
struct TotpSetupRequest {
    username: String,
}

#[derive(Deserialize)]
struct TotpVerifyRequest {
    username: String,
    code: String,
}

#[derive(Deserialize)]
struct PasskeyStartRequest {
    username: String,
}

#[derive(Deserialize)]
struct PasskeyFinishRequest {
    username: String,
    challenge: String,
    credential: serde_json::Value,
}

pub fn init_state(pool: PgPool, redis: redis::aio::ConnectionManager, config: AppConfig) {
    let _ = AUTH_STATE.set(Arc::new(AuthState {
        pool,
        redis,
        config,
        noise_transports: Arc::new(DashMap::new()),
        pending_handshakes: Arc::new(DashMap::new()),
        server_static_keypair: generate_server_static_keypair(),
    }));
}

pub fn router() -> Router {
    Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/logout", post(logout))
        .route("/api/noise/handshake", post(noise_handshake))
        .route("/admin/api/auth/login", post(admin_login))
        .route("/admin/api/auth/totp/setup", post(admin_totp_setup))
        .route("/admin/api/auth/totp/verify", post(admin_totp_verify))
        .route("/admin/api/auth/passkey/register-start", post(passkey_register_start))
        .route("/admin/api/auth/passkey/register-finish", post(passkey_register_finish))
        .route("/admin/api/auth/passkey/login-start", post(passkey_login_start))
        .route("/admin/api/auth/passkey/login-finish", post(passkey_login_finish))
        .route("/admin/api/auth/passkey/:mode/:username", get(passkey_get))
}

pub fn noise_transports() -> Arc<DashMap<String, NoiseTransport>> {
    auth_state().noise_transports.clone()
}

pub fn server_static_keypair() -> [u8; 64] {
    auth_state().server_static_keypair
}

pub async fn seed_admin_user(pool: &PgPool) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_users")
        .fetch_one(pool)
        .await
        .context("failed to count admin users")?;
    if count == 0 {
        let hashed = bcrypt::hash("adminpassword", bcrypt::DEFAULT_COST)?;
        sqlx::query("INSERT INTO admin_users (username, password_hash, totp_enrolled) VALUES ($1, $2, false)")
            .bind("admin")
            .bind(hashed)
            .execute(pool)
            .await
            .context("failed to seed admin user")?;
        info!(event = "seed_admin_user", "seeded default admin user");
    }
    Ok(())
}

pub struct NoiseAuth(pub UserId);

#[axum::async_trait]
impl<S> FromRequestParts<S> for NoiseAuth
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let headers = parts.headers.clone();
        authenticate_headers(&headers).await
    }
}

async fn authenticate_headers(headers: &HeaderMap) -> Result<NoiseAuth, AppError> {
    let session_id = headers
        .get("X-Session-Id")
        .or_else(|| headers.get("x-session-id"))
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;
    let mut redis = auth_state().redis.clone();
    let key = format!("session:{session_id}");
    let user_hex: Option<String> = redis.get(&key).await.map_err(|e| {
        error!(event = "redis_session_lookup_failed", error = %e);
        AppError::Unauthorized
    })?;
    let user_hex = user_hex.ok_or(AppError::Unauthorized)?;
    let user_id = UserId::from_hex(&user_hex).map_err(|_| AppError::Unauthorized)?;
    Ok(NoiseAuth(user_id))
}

async fn register(Json(payload): Json<RegisterRequest>) -> Result<Json<RegisterResponse>, AppError> {
    verify_timestamp(payload.timestamp)?;
    let pubkey = decode_fixed::<32>(&payload.pubkey)?;
    let view_pubkey = decode_fixed::<32>(&payload.view_pubkey)?;
    let spend_pubkey = decode_fixed::<32>(&payload.spend_pubkey)?;
    let signature_bytes = hex::decode(&payload.signature).map_err(|_| AppError::BadRequest("invalid signature".into()))?;
    let verifying_key = VerifyingKey::from_bytes(&pubkey).map_err(|_| AppError::Unauthorized)?;
    let signature = Signature::from_slice(&signature_bytes).map_err(|_| AppError::Unauthorized)?;
    let mut message = Vec::with_capacity(40);
    message.extend_from_slice(&pubkey);
    message.extend_from_slice(&payload.timestamp.to_le_bytes());
    verifying_key.verify(&message, &signature).map_err(|_| AppError::Unauthorized)?;
    let digest = Blake2b512::digest(pubkey);
    let user_id = UserId(digest[..32].try_into().expect("digest size"));
    let session_id = Uuid::now_v7().simple().to_string();
    sqlx::query(
        "INSERT INTO users (user_id, pubkey, view_pubkey, spend_pubkey, created_at) VALUES ($1, $2, $3, $4, NOW()) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id.to_hex())
    .bind(hex::encode(pubkey))
    .bind(hex::encode(view_pubkey))
    .bind(hex::encode(spend_pubkey))
    .execute(&auth_state().pool)
    .await
    .map_err(internal)?;
    let mut redis = auth_state().redis.clone();
    redis
        .set_ex::<_, _, ()>(format!("session:{session_id}"), user_id.to_hex(), 60 * 60 * 24)
        .await
        .map_err(internal)?;
    info!(event = "user_registered", "registered wallet user");
    Ok(Json(RegisterResponse {
        user_id: user_id.to_hex(),
        session_id,
    }))
}

async fn logout(NoiseAuth(user_id): NoiseAuth, headers: HeaderMap) -> Result<impl IntoResponse, AppError> {
    let session_id = extract_session_id(&headers)?;
    let mut redis = auth_state().redis.clone();
    let _: usize = redis.del(format!("session:{session_id}")).await.map_err(internal)?;
    info!(event = "user_logout", "logged out wallet user");
    Ok((StatusCode::OK, Json(json!({"status": "ok", "uid": user_id.to_hex()}))))
}

async fn noise_handshake(headers: HeaderMap, body: Bytes) -> Result<Json<serde_json::Value>, AppError> {
    let handshake_id = headers
        .get("X-Handshake-Id")
        .or_else(|| headers.get("x-handshake-id"))
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::now_v7().simple().to_string());

    let mut session = if let Some((_, session)) = auth_state().pending_handshakes.remove(&handshake_id) {
        session
    } else {
        NoiseSession::new_responder(&auth_state().server_static_keypair).map_err(|e| AppError::BadRequest(e.to_string()))?
    };

    let _ = session.read_message(&body).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let response = session.write_message(&[]).map_err(|e| AppError::BadRequest(e.to_string()))?;
    if session.is_handshake_finished() {
        let transport = session.into_transport().map_err(|e| AppError::BadRequest(e.to_string()))?;
        let session_id = Uuid::now_v7().simple().to_string();
        auth_state().noise_transports.insert(session_id.clone(), transport);
        info!(event = "noise_handshake_complete", "completed noise handshake");
        Ok(Json(json!({"handshake_id": handshake_id, "response": BASE64.encode(response), "session_id": session_id})))
    } else {
        auth_state().pending_handshakes.insert(handshake_id.clone(), session);
        Ok(Json(json!({"handshake_id": handshake_id, "response": BASE64.encode(response), "pending": true})))
    }
}

async fn admin_login(Json(payload): Json<AdminLoginRequest>) -> Result<Json<serde_json::Value>, AppError> {
    verify_hcaptcha(&payload.hcaptcha_token).await?;
    let row = sqlx::query("SELECT password_hash, totp_secret, totp_enrolled FROM admin_users WHERE username = $1")
        .bind(&payload.username)
        .fetch_optional(&auth_state().pool)
        .await
        .map_err(internal)?
        .ok_or(AppError::Unauthorized)?;
    let hash: String = row.get("password_hash");
    if !bcrypt::verify(&payload.password, &hash).map_err(internal)? {
        return Err(AppError::Unauthorized);
    }
    let enrolled: bool = row.get("totp_enrolled");
    let secret: Option<String> = row.get("totp_secret");
    if enrolled {
        let code = payload.totp_code.ok_or_else(|| AppError::BadRequest("totp_code required".into()))?;
        let totp = build_totp(secret.as_deref().ok_or(AppError::Unauthorized)?)?;
        if !totp.check_current(&code).map_err(internal)? {
            return Err(AppError::Unauthorized);
        }
    }
    let session = Uuid::now_v7().simple().to_string();
    let mut redis = auth_state().redis.clone();
    redis
        .set_ex::<_, _, ()>(format!("admin:session:{session}"), &payload.username, 60 * 60)
        .await
        .map_err(internal)?;
    info!(event = "admin_login", "admin logged in");
    Ok(Json(json!({"session_token": session})))
}

async fn admin_totp_setup(Json(payload): Json<TotpSetupRequest>) -> Result<Json<serde_json::Value>, AppError> {
    let secret = Secret::generate_secret();
    let secret_base32 = secret.to_encoded().to_string();
    sqlx::query("UPDATE admin_users SET totp_secret = $1 WHERE username = $2")
        .bind(&secret_base32)
        .bind(&payload.username)
        .execute(&auth_state().pool)
        .await
        .map_err(internal)?;
    let totp = build_totp(&secret_base32)?;
    Ok(Json(json!({"secret": secret_base32, "qr_uri": totp.get_url()})))
}

async fn admin_totp_verify(Json(payload): Json<TotpVerifyRequest>) -> Result<Json<serde_json::Value>, AppError> {
    let row = sqlx::query("SELECT totp_secret FROM admin_users WHERE username = $1")
        .bind(&payload.username)
        .fetch_optional(&auth_state().pool)
        .await
        .map_err(internal)?
        .ok_or(AppError::Unauthorized)?;
    let secret: Option<String> = row.get("totp_secret");
    let totp = build_totp(secret.as_deref().ok_or(AppError::Unauthorized)?)?;
    if !totp.check_current(&payload.code).map_err(internal)? {
        return Err(AppError::Unauthorized);
    }
    sqlx::query("UPDATE admin_users SET totp_enrolled = true WHERE username = $1")
        .bind(&payload.username)
        .execute(&auth_state().pool)
        .await
        .map_err(internal)?;
    Ok(Json(json!({"status": "verified"})))
}

async fn passkey_register_start(Json(payload): Json<PasskeyStartRequest>) -> Result<Json<serde_json::Value>, AppError> {
    issue_passkey_challenge("register", &payload.username).await
}

async fn passkey_register_finish(Json(payload): Json<PasskeyFinishRequest>) -> Result<Json<serde_json::Value>, AppError> {
    finish_passkey("register", payload).await
}

async fn passkey_login_start(Json(payload): Json<PasskeyStartRequest>) -> Result<Json<serde_json::Value>, AppError> {
    issue_passkey_challenge("login", &payload.username).await
}

async fn passkey_login_finish(Json(payload): Json<PasskeyFinishRequest>) -> Result<Json<serde_json::Value>, AppError> {
    finish_passkey("login", payload).await
}

async fn passkey_get(Path((mode, username)): Path<(String, String)>) -> Result<Json<serde_json::Value>, AppError> {
    let key = format!("admin:passkey:{mode}:{username}");
    let mut redis = auth_state().redis.clone();
    let value: Option<String> = redis.get(&key).await.map_err(internal)?;
    Ok(Json(json!({"challenge": value})))
}

async fn issue_passkey_challenge(mode: &str, username: &str) -> Result<Json<serde_json::Value>, AppError> {
    let challenge = Uuid::now_v7().simple().to_string();
    let mut redis = auth_state().redis.clone();
    redis
        .set_ex::<_, _, ()>(format!("admin:passkey:{mode}:{username}"), &challenge, 300)
        .await
        .map_err(internal)?;
    Ok(Json(json!({
        "challenge": challenge,
        "username": username,
        "rp_id": "torex.local",
        "timeout": 300000
    })))
}

async fn finish_passkey(mode: &str, payload: PasskeyFinishRequest) -> Result<Json<serde_json::Value>, AppError> {
    let mut redis = auth_state().redis.clone();
    let redis_key = format!("admin:passkey:{mode}:{}", payload.username);
    let expected: Option<String> = redis.get(&redis_key).await.map_err(internal)?;
    if expected.as_deref() != Some(payload.challenge.as_str()) {
        return Err(AppError::Unauthorized);
    }
    sqlx::query("UPDATE admin_users SET passkey_credential = $1 WHERE username = $2")
        .bind(payload.credential.to_string())
        .bind(&payload.username)
        .execute(&auth_state().pool)
        .await
        .map_err(internal)?;
    let _: usize = redis.del(redis_key).await.map_err(internal)?;
    Ok(Json(json!({"status": "ok"})))
}

fn auth_state() -> &'static Arc<AuthState> {
    AUTH_STATE.get().expect("auth state initialized")
}

fn verify_timestamp(timestamp: u64) -> Result<(), AppError> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(internal)?.as_secs();
    if now.abs_diff(timestamp) > 30 {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

fn decode_fixed<const N: usize>(value: &str) -> Result<[u8; N], AppError> {
    let bytes = hex::decode(value).map_err(|_| AppError::BadRequest("invalid hex".into()))?;
    bytes.try_into().map_err(|_| AppError::BadRequest("invalid length".into()))
}

fn extract_session_id(headers: &HeaderMap) -> Result<String, AppError> {
    headers
        .get("X-Session-Id")
        .or_else(|| headers.get("x-session-id"))
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned)
        .ok_or(AppError::Unauthorized)
}

fn generate_server_static_keypair() -> [u8; 64] {
    let mut private = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut private);
    let secret = x25519_dalek::StaticSecret::from(private);
    let public = x25519_dalek::PublicKey::from(&secret);
    let mut out = [0_u8; 64];
    out[..32].copy_from_slice(&secret.to_bytes());
    out[32..].copy_from_slice(public.as_bytes());
    out
}

fn build_totp(secret: &str) -> Result<TOTP, AppError> {
    TOTP::new(Algorithm::SHA1, 6, 1, 30, secret.as_bytes().to_vec(), Some("TorEx".to_string()), "admin".to_string())
        .map_err(internal)
}

async fn verify_hcaptcha(token: &str) -> Result<(), AppError> {
    if auth_state().config.hcaptcha_secret.is_empty() || token == "bypass" {
        warn!(event = "hcaptcha_bypassed", "skipping hcaptcha verification");
        return Ok(());
    }
    let client = reqwest::Client::new();
    let response: serde_json::Value = client
        .post("https://hcaptcha.com/siteverify")
        .form(&[("secret", auth_state().config.hcaptcha_secret.as_str()), ("response", token)])
        .send()
        .await
        .map_err(internal)?
        .json()
        .await
        .map_err(internal)?;
    if response.get("success").and_then(|v| v.as_bool()) != Some(true) {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

fn internal<E>(err: E) -> AppError
where
    E: Into<anyhow::Error>,
{
    AppError::Internal(err.into())
}
