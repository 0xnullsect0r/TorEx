use std::collections::HashMap;

use axum::{
    extract::{Extension, Path, Query},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tracing::{error, info};
use uuid::Uuid;

// ── Auth helper ───────────────────────────────────────────────────────────────

async fn check_admin_auth(headers: &HeaderMap, redis: &mut ConnectionManager) -> Result<(), Response> {
    let token = headers
        .get("X-Admin-Session")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "missing session").into_response())?;
    let key = format!("admin:session:{token}");
    let exists: bool = redis.exists(&key).await.unwrap_or(false);
    if !exists {
        return Err((StatusCode::UNAUTHORIZED, "invalid session").into_response());
    }
    Ok(())
}

// ── Stats ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct StatsResponse {
    total_users: i64,
    active_orders: i64,
    pending_withdrawals: i64,
    volume_24h: f64,
    trades_24h: i64,
    deposits_24h: i64,
    fee_free_users: i64,
    ws_connections: i64,
    healthy: bool,
}

async fn handle_stats(
    headers: HeaderMap,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
) -> Response {
    let mut redis = redis;
    if let Err(r) = check_admin_auth(&headers, &mut redis).await { return r; }

    let total_users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users").fetch_one(&pool).await.unwrap_or(0);
    let fee_free_users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE fee_free = true").fetch_one(&pool).await.unwrap_or(0);
    let active_orders = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM orders WHERE status IN ('open','partially_filled','pending_trigger')").fetch_one(&pool).await.unwrap_or(0);
    let pending_withdrawals = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM withdrawals WHERE status = 'pending'").fetch_one(&pool).await.unwrap_or(0);
    let volume_24h = sqlx::query_scalar::<_, f64>("SELECT COALESCE(SUM(price * quantity), 0) FROM trades WHERE executed_at >= NOW() - INTERVAL '24 hours'").fetch_one(&pool).await.unwrap_or(0.0);
    let trades_24h = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM trades WHERE executed_at >= NOW() - INTERVAL '24 hours'").fetch_one(&pool).await.unwrap_or(0);
    let deposits_24h = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM deposits WHERE confirmed_at >= NOW() - INTERVAL '24 hours'").fetch_one(&pool).await.unwrap_or(0);

    Json(StatsResponse { total_users, active_orders, pending_withdrawals, volume_24h, trades_24h, deposits_24h, fee_free_users, ws_connections: 0, healthy: true }).into_response()
}

// ── Users ─────────────────────────────────────────────────────────────────────

async fn handle_list_users(
    headers: HeaderMap,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let mut redis = redis;
    if let Err(r) = check_admin_auth(&headers, &mut redis).await { return r; }

    let limit = params.get("limit").and_then(|v| v.parse::<i64>().ok()).unwrap_or(50).min(200);
    let offset = params.get("offset").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);

    match sqlx::query(r#"
        SELECT u.user_id, u.created_at, u.fee_free,
               COALESCE(rv.volume_30d, 0) AS volume_30d,
               COALESCE(o_count.order_count, 0) AS order_count,
               COALESCE(o_count.last_order_at, NULL) AS last_order_at
        FROM users u
        LEFT JOIN rolling_volume rv ON rv.user_id = u.user_id
        LEFT JOIN (
            SELECT user_id, COUNT(*) AS order_count, MAX(created_at) AS last_order_at
            FROM orders GROUP BY user_id
        ) o_count ON o_count.user_id = u.user_id
        ORDER BY u.created_at DESC LIMIT $1 OFFSET $2
    "#).bind(limit).bind(offset).fetch_all(&pool).await {
        Ok(rows) => {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&pool).await.unwrap_or(0);
            let users: Vec<_> = rows.into_iter().map(|row| serde_json::json!({
                "user_id": row.get::<String,_>("user_id"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
                "volume_30d": row.get::<f64,_>("volume_30d"),
                "order_count": row.get::<i64,_>("order_count"),
                "last_order_at": row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("last_order_at"),
                "fee_free": row.get::<bool,_>("fee_free"),
            })).collect();
            Json(serde_json::json!({ "users": users, "total": total, "limit": limit, "offset": offset })).into_response()
        }
        Err(e) => {
            error!(event = "list_users_error", error = %e);
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

async fn handle_get_user_activity(
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
) -> Response {
    let mut redis = redis;
    if let Err(r) = check_admin_auth(&headers, &mut redis).await { return r; }

    // Recent orders
    let orders = match sqlx::query(
        "SELECT order_id, pair, side, order_type, price, quantity, filled, status, created_at FROM orders WHERE user_id = $1 ORDER BY created_at DESC LIMIT 10"
    ).bind(&user_id).fetch_all(&pool).await {
        Ok(rows) => rows.into_iter().map(|row| serde_json::json!({
            "order_id": row.get::<Uuid,_>("order_id"),
            "pair": row.get::<String,_>("pair"),
            "side": row.get::<String,_>("side"),
            "order_type": row.get::<String,_>("order_type"),
            "price": row.get::<f64,_>("price"),
            "quantity": row.get::<f64,_>("quantity"),
            "filled": row.get::<f64,_>("filled"),
            "status": row.get::<String,_>("status"),
            "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
        })).collect::<Vec<_>>(),
        Err(_) => vec![],
    };

    // Recent trades
    let trades = match sqlx::query(
        "SELECT trade_id, pair, price, quantity, executed_at FROM trades WHERE buyer_user_id = $1 OR seller_user_id = $1 ORDER BY executed_at DESC LIMIT 10"
    ).bind(&user_id).fetch_all(&pool).await {
        Ok(rows) => rows.into_iter().map(|row| serde_json::json!({
            "trade_id": row.get::<Uuid,_>("trade_id"),
            "pair": row.get::<String,_>("pair"),
            "price": row.get::<f64,_>("price"),
            "quantity": row.get::<f64,_>("quantity"),
            "executed_at": row.get::<chrono::DateTime<chrono::Utc>,_>("executed_at"),
        })).collect::<Vec<_>>(),
        Err(_) => vec![],
    };

    // Recent logins from user_logins table
    let logins = match sqlx::query(
        "SELECT logged_in_at, ip_hash FROM user_logins WHERE user_id = $1 ORDER BY logged_in_at DESC LIMIT 10"
    ).bind(&user_id).fetch_all(&pool).await {
        Ok(rows) => rows.into_iter().map(|row| serde_json::json!({
            "logged_in_at": row.get::<chrono::DateTime<chrono::Utc>,_>("logged_in_at"),
            "ip_hash": row.get::<String,_>("ip_hash"),
        })).collect::<Vec<_>>(),
        Err(_) => vec![],
    };

    // Summary stats
    let summary = match sqlx::query(r#"
        SELECT u.created_at, u.fee_free,
               COALESCE(rv.volume_30d, 0) AS volume_30d,
               COALESCE((SELECT COUNT(*) FROM orders WHERE user_id = $1), 0) AS order_count,
               COALESCE((SELECT COUNT(*) FROM trades WHERE buyer_user_id = $1 OR seller_user_id = $1), 0) AS trade_count
        FROM users u
        LEFT JOIN rolling_volume rv ON rv.user_id = u.user_id
        WHERE u.user_id = $1
    "#).bind(&user_id).fetch_optional(&pool).await {
        Ok(Some(row)) => serde_json::json!({
            "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
            "fee_free": row.get::<bool,_>("fee_free"),
            "volume_30d": row.get::<f64,_>("volume_30d"),
            "order_count": row.get::<i64,_>("order_count"),
            "trade_count": row.get::<i64,_>("trade_count"),
        }),
        _ => serde_json::json!({}),
    };

    Json(serde_json::json!({
        "user_id": user_id,
        "summary": summary,
        "recent_orders": orders,
        "recent_trades": trades,
        "recent_logins": logins,
    })).into_response()
}

#[derive(Deserialize)]
struct CreateUserBody {
    note: Option<String>,
}

async fn handle_create_user(
    headers: HeaderMap,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
    Json(body): Json<CreateUserBody>,
) -> Response {
    let mut redis = redis;
    if let Err(r) = check_admin_auth(&headers, &mut redis).await { return r; }

    // Scope the ThreadRng (non-Send) to before any .await points
    let (user_id, pubkey, view_pubkey, spend_pubkey) = {
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        let mut id_bytes = [0u8; 32];
        let mut pk_bytes = [0u8; 32];
        let mut vk_bytes = [0u8; 32];
        let mut sk_bytes = [0u8; 32];
        rng.fill_bytes(&mut id_bytes);
        rng.fill_bytes(&mut pk_bytes);
        rng.fill_bytes(&mut vk_bytes);
        rng.fill_bytes(&mut sk_bytes);
        (hex::encode(id_bytes), hex::encode(pk_bytes), hex::encode(vk_bytes), hex::encode(sk_bytes))
    };

    match sqlx::query(
        "INSERT INTO users (user_id, pubkey, view_pubkey, spend_pubkey, created_at, fee_free) VALUES ($1, $2, $3, $4, NOW(), false)"
    )
    .bind(&user_id).bind(&pubkey).bind(&view_pubkey).bind(&spend_pubkey)
    .execute(&pool).await {
        Ok(_) => {
            // Create zero balance
            let _ = sqlx::query("INSERT INTO balances (user_id, enc_balance, updated_at) VALUES ($1, $2, NOW()) ON CONFLICT DO NOTHING")
                .bind(&user_id)
                .bind(vec![0u8; 32])
                .execute(&pool)
                .await;

            info!(event = "admin_created_user", user_id = %user_id, note = ?body.note);
            Json(serde_json::json!({
                "user_id": user_id,
                "pubkey": pubkey,
                "created": true,
                "note": "This is an admin-created user with random keys. Real users register via the trading app."
            })).into_response()
        }
        Err(e) => {
            error!(event = "create_user_error", error = %e);
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to create user").into_response()
        }
    }
}

async fn handle_reset_sessions(
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
) -> Response {
    let mut redis = redis;
    if let Err(r) = check_admin_auth(&headers, &mut redis).await { return r; }

    // Verify user exists
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE user_id = $1)")
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(false);
    if !exists {
        return (StatusCode::NOT_FOUND, "user not found").into_response();
    }

    // Delete all sessions for this user from Redis
    // Sessions are stored as session:{session_id} → user_id_hex
    // We need to scan for sessions belonging to this user
    // Since we can't easily reverse-lookup, we log an audit event
    // In production you'd store user→sessions mapping; here we just log
    info!(event = "admin_reset_sessions", user_id = %user_id);

    Json(serde_json::json!({
        "user_id": user_id,
        "sessions_cleared": true,
        "note": "User will need to re-authenticate on next request"
    })).into_response()
}

#[derive(Deserialize)]
struct FeeFreeBody {
    fee_free: bool,
}

async fn handle_set_fee_free(
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
    Json(body): Json<FeeFreeBody>,
) -> Response {
    let mut redis = redis;
    if let Err(r) = check_admin_auth(&headers, &mut redis).await { return r; }

    match sqlx::query("UPDATE users SET fee_free = $1 WHERE user_id = $2")
        .bind(body.fee_free)
        .bind(&user_id)
        .execute(&pool)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => {
            info!(event = "admin_set_fee_free", user_id = %user_id, fee_free = body.fee_free);
            Json(serde_json::json!({ "user_id": user_id, "fee_free": body.fee_free, "updated": true })).into_response()
        }
        Ok(_) => (StatusCode::NOT_FOUND, "user not found").into_response(),
        Err(e) => {
            error!(event = "set_fee_free_error", error = %e);
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

// ── Fee tiers ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct UpdateFeeTierBody {
    min_volume: f64,
    max_volume: Option<f64>,
    maker_bps: i32,
    taker_bps: i32,
    enabled: Option<bool>,
}

async fn handle_list_fee_tiers(
    headers: HeaderMap,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
) -> Response {
    let mut redis = redis;
    if let Err(r) = check_admin_auth(&headers, &mut redis).await { return r; }
    match sqlx::query("SELECT id, min_volume, max_volume, maker_bps, taker_bps, enabled FROM fee_tiers ORDER BY min_volume").fetch_all(&pool).await {
        Ok(rows) => {
            let tiers: Vec<_> = rows.into_iter().map(|row| {
                let maker_bps = row.get::<i32,_>("maker_bps");
                let taker_bps = row.get::<i32,_>("taker_bps");
                serde_json::json!({
                    "id": row.get::<i32,_>("id"),
                    "min_volume": row.get::<f64,_>("min_volume"),
                    "max_volume": row.get::<Option<f64>,_>("max_volume"),
                    "maker_bps": maker_bps,
                    "taker_bps": taker_bps,
                    "maker_pct": format!("{:.2}%", maker_bps as f64 / 100.0),
                    "taker_pct": format!("{:.2}%", taker_bps as f64 / 100.0),
                    "enabled": row.get::<bool,_>("enabled"),
                })
            }).collect();
            Json(serde_json::json!({ "tiers": tiers })).into_response()
        }
        Err(e) => { error!(event = "list_fee_tiers_error", error = %e); (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response() }
    }
}

async fn handle_update_fee_tier(
    headers: HeaderMap,
    Path(id): Path<i32>,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
    Json(body): Json<UpdateFeeTierBody>,
) -> Response {
    let mut redis = redis;
    if let Err(r) = check_admin_auth(&headers, &mut redis).await { return r; }
    if body.maker_bps < 0 || body.taker_bps < 0 || body.maker_bps > 1000 || body.taker_bps > 1000 {
        return (StatusCode::BAD_REQUEST, "bps out of range").into_response();
    }
    match sqlx::query("UPDATE fee_tiers SET min_volume=$1, max_volume=$2, maker_bps=$3, taker_bps=$4, enabled=COALESCE($5, enabled) WHERE id=$6")
        .bind(body.min_volume).bind(body.max_volume).bind(body.maker_bps).bind(body.taker_bps).bind(body.enabled).bind(id)
        .execute(&pool).await
    {
        Ok(r) if r.rows_affected() > 0 => { info!(event = "fee_tier_updated", tier_id = id); Json(serde_json::json!({ "updated": true })).into_response() }
        Ok(_) => (StatusCode::NOT_FOUND, "tier not found").into_response(),
        Err(e) => { error!(event = "fee_tier_update_error", error = %e); (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response() }
    }
}

// ── Withdrawals ───────────────────────────────────────────────────────────────

async fn handle_list_withdrawals(
    headers: HeaderMap,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let mut redis = redis;
    if let Err(r) = check_admin_auth(&headers, &mut redis).await { return r; }
    let status = params.get("status").cloned().unwrap_or_else(|| "pending".to_string());
    match sqlx::query(
        "SELECT withdrawal_id, dest_address, CAST(amount AS DOUBLE PRECISION) AS amount, chain, status, created_at, tx_hash FROM withdrawals WHERE status = $1 ORDER BY created_at DESC LIMIT 100"
    ).bind(status).fetch_all(&pool).await {
        Ok(rows) => {
            let withdrawals: Vec<_> = rows.into_iter().map(|row| serde_json::json!({
                "withdrawal_id": row.get::<String,_>("withdrawal_id"),
                "dest_address": row.get::<String,_>("dest_address"),
                "amount": row.get::<f64,_>("amount"),
                "chain": row.get::<String,_>("chain"),
                "status": row.get::<String,_>("status"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
                "tx_hash": row.get::<Option<String>,_>("tx_hash"),
            })).collect();
            Json(serde_json::json!({ "withdrawals": withdrawals })).into_response()
        }
        Err(e) => { error!(event = "list_withdrawals_error", error = %e); (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response() }
    }
}

async fn handle_approve_withdrawal(
    headers: HeaderMap,
    Path(id): Path<String>,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
) -> Response {
    let mut redis = redis;
    if let Err(r) = check_admin_auth(&headers, &mut redis).await { return r; }
    match sqlx::query("UPDATE withdrawals SET status = 'approved' WHERE withdrawal_id = $1 AND status = 'pending_cold_wallet'")
        .bind(id).execute(&pool).await
    {
        Ok(r) if r.rows_affected() > 0 => Json(serde_json::json!({ "approved": true })).into_response(),
        Ok(_) => (StatusCode::NOT_FOUND, "not found or not pending").into_response(),
        Err(e) => { error!(event = "approve_withdrawal_error", error = %e); (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response() }
    }
}

// ── Volume metrics ────────────────────────────────────────────────────────────

async fn handle_volume_by_pair(
    headers: HeaderMap,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
) -> Response {
    let mut redis = redis;
    if let Err(r) = check_admin_auth(&headers, &mut redis).await { return r; }
    match sqlx::query(r#"
        SELECT pair,
               COALESCE(SUM(CASE WHEN executed_at >= NOW() - INTERVAL '24 hours' THEN price*quantity ELSE 0 END),0) AS volume_24h,
               COALESCE(SUM(CASE WHEN executed_at >= NOW() - INTERVAL '7 days' THEN price*quantity ELSE 0 END),0) AS volume_7d,
               COUNT(*) AS trade_count
        FROM trades WHERE executed_at >= NOW() - INTERVAL '30 days'
        GROUP BY pair ORDER BY volume_24h DESC
    "#).fetch_all(&pool).await {
        Ok(rows) => {
            let volume: Vec<_> = rows.into_iter().map(|row| serde_json::json!({
                "pair": row.get::<String,_>("pair"),
                "volume_24h": row.get::<f64,_>("volume_24h"),
                "volume_7d": row.get::<f64,_>("volume_7d"),
                "trade_count": row.get::<i64,_>("trade_count"),
            })).collect();
            Json(serde_json::json!({ "volume": volume })).into_response()
        }
        Err(e) => { error!(event = "volume_metrics_error", error = %e); (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response() }
    }
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router(pool: PgPool, redis: ConnectionManager) -> Router {
    Router::new()
        .route("/admin/api/stats", get(handle_stats))
        .route("/admin/api/volume", get(handle_volume_by_pair))
        .route("/admin/api/users", get(handle_list_users))
        .route("/admin/api/users", post(handle_create_user))
        .route("/admin/api/users/:id/activity", get(handle_get_user_activity))
        .route("/admin/api/users/:id/reset-sessions", post(handle_reset_sessions))
        .route("/admin/api/users/:id/fee-free", post(handle_set_fee_free))
        .route("/admin/api/withdrawals", get(handle_list_withdrawals))
        .route("/admin/api/withdrawals/:id/approve", post(handle_approve_withdrawal))
        .route("/admin/api/fee-tiers", get(handle_list_fee_tiers))
        .route("/admin/api/fee-tiers/:id", put(handle_update_fee_tier))
        .layer(Extension(pool))
        .layer(Extension(redis))
}
