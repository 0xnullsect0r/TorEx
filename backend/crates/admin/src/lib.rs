use std::collections::HashMap;

use axum::{
    extract::{Extension, Path, Query},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tracing::{error, info};

async fn check_admin_auth(headers: &HeaderMap, redis: &mut ConnectionManager) -> Result<(), Response> {
    use redis::AsyncCommands;

    let token = headers
        .get("X-Admin-Session")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "missing session").into_response())?;

    let key = format!("admin:session:{token}");
    let exists: bool = redis.exists(&key).await.unwrap_or(false);
    if !exists {
        return Err((StatusCode::UNAUTHORIZED, "invalid session").into_response());
    }

    Ok(())
}

#[derive(Serialize)]
struct StatsResponse {
    total_users: i64,
    active_orders: i64,
    pending_withdrawals: i64,
    volume_24h: f64,
    trades_24h: i64,
    deposits_24h: i64,
    ws_connections: i64,
    healthy: bool,
}

async fn handle_stats(
    headers: HeaderMap,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
) -> Response {
    let mut redis = redis;
    if let Err(response) = check_admin_auth(&headers, &mut redis).await {
        return response;
    }

    let total_users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let active_orders = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM orders WHERE status IN ('open', 'partially_filled')",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0);
    let pending_withdrawals = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM withdrawals WHERE status = 'pending'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0);
    let volume_24h = sqlx::query_scalar::<_, f64>(
        "SELECT COALESCE(SUM(price * quantity), 0) FROM trades WHERE executed_at >= NOW() - INTERVAL '24 hours'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0.0);
    let trades_24h = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trades WHERE executed_at >= NOW() - INTERVAL '24 hours'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0);
    let deposits_24h = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM deposits WHERE confirmed_at >= NOW() - INTERVAL '24 hours'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0);

    Json(StatsResponse {
        total_users,
        active_orders,
        pending_withdrawals,
        volume_24h,
        trades_24h,
        deposits_24h,
        ws_connections: 0,
        healthy: true,
    })
    .into_response()
}

#[derive(Deserialize)]
struct UpdateFeeTierBody {
    min_volume: f64,
    max_volume: Option<f64>,
    maker_bps: i32,
    taker_bps: i32,
}

async fn handle_list_fee_tiers(
    headers: HeaderMap,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
) -> Response {
    let mut redis = redis;
    if let Err(response) = check_admin_auth(&headers, &mut redis).await {
        return response;
    }

    match sqlx::query(
        "SELECT id, min_volume, max_volume, maker_bps, taker_bps, enabled FROM fee_tiers ORDER BY min_volume",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => {
            let tiers: Vec<_> = rows
                .into_iter()
                .map(|row| {
                    let maker_bps = row.get::<i32, _>("maker_bps");
                    let taker_bps = row.get::<i32, _>("taker_bps");
                    serde_json::json!({
                        "id": row.get::<i32, _>("id"),
                        "min_volume": row.get::<f64, _>("min_volume"),
                        "max_volume": row.get::<Option<f64>, _>("max_volume"),
                        "maker_bps": maker_bps,
                        "taker_bps": taker_bps,
                        "maker_pct": format!("{:.2}%", maker_bps as f64 / 100.0),
                        "taker_pct": format!("{:.2}%", taker_bps as f64 / 100.0),
                        "enabled": row.get::<bool, _>("enabled"),
                    })
                })
                .collect();
            Json(serde_json::json!({ "tiers": tiers })).into_response()
        }
        Err(error) => {
            error!(event = "list_fee_tiers_error", error = %error);
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
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
    if let Err(response) = check_admin_auth(&headers, &mut redis).await {
        return response;
    }

    if body.maker_bps < 0 || body.taker_bps < 0 || body.maker_bps > 1000 || body.taker_bps > 1000 {
        return (StatusCode::BAD_REQUEST, "bps out of range").into_response();
    }

    match sqlx::query(
        "UPDATE fee_tiers SET min_volume = $1, max_volume = $2, maker_bps = $3, taker_bps = $4 WHERE id = $5",
    )
    .bind(body.min_volume)
    .bind(body.max_volume)
    .bind(body.maker_bps)
    .bind(body.taker_bps)
    .bind(id)
    .execute(&pool)
    .await
    {
        Ok(result) if result.rows_affected() > 0 => {
            info!(event = "fee_tier_updated", tier_id = id);
            Json(serde_json::json!({ "updated": true })).into_response()
        }
        Ok(_) => (StatusCode::NOT_FOUND, "tier not found").into_response(),
        Err(error) => {
            error!(event = "fee_tier_update_error", error = %error);
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

async fn handle_list_users(
    headers: HeaderMap,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let mut redis = redis;
    if let Err(response) = check_admin_auth(&headers, &mut redis).await {
        return response;
    }

    let limit = params
        .get("limit")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(50)
        .min(200);
    let offset = params
        .get("offset")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);

    match sqlx::query(
        r#"
        SELECT u.user_id,
               u.created_at,
               COALESCE(rv.volume_30d, 0) AS volume_30d,
               COALESCE(o_count.order_count, 0) AS order_count
        FROM users u
        LEFT JOIN rolling_volume rv ON rv.user_id = u.user_id
        LEFT JOIN (
            SELECT user_id, COUNT(*) AS order_count
            FROM orders
            GROUP BY user_id
        ) o_count ON o_count.user_id = u.user_id
        ORDER BY u.created_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => {
            let users: Vec<_> = rows
                .into_iter()
                .map(|row| {
                    serde_json::json!({
                        "user_id": row.get::<String, _>("user_id"),
                        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                        "volume_30d": row.get::<f64, _>("volume_30d"),
                        "order_count": row.get::<i64, _>("order_count"),
                    })
                })
                .collect();
            Json(serde_json::json!({ "users": users, "limit": limit, "offset": offset })).into_response()
        }
        Err(error) => {
            error!(event = "list_users_error", error = %error);
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

async fn handle_list_withdrawals(
    headers: HeaderMap,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let mut redis = redis;
    if let Err(response) = check_admin_auth(&headers, &mut redis).await {
        return response;
    }

    let status = params
        .get("status")
        .cloned()
        .unwrap_or_else(|| "pending".to_string());

    match sqlx::query(
        "SELECT withdrawal_id, dest_address, CAST(amount AS DOUBLE PRECISION) AS amount, chain, status, created_at, tx_hash FROM withdrawals WHERE status = $1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(status)
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => {
            let withdrawals: Vec<_> = rows
                .into_iter()
                .map(|row| {
                    serde_json::json!({
                        "withdrawal_id": row.get::<String, _>("withdrawal_id"),
                        "dest_address": row.get::<String, _>("dest_address"),
                        "amount": row.get::<f64, _>("amount"),
                        "chain": row.get::<String, _>("chain"),
                        "status": row.get::<String, _>("status"),
                        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                        "tx_hash": row.get::<Option<String>, _>("tx_hash"),
                    })
                })
                .collect();
            Json(serde_json::json!({ "withdrawals": withdrawals })).into_response()
        }
        Err(error) => {
            error!(event = "list_withdrawals_error", error = %error);
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

async fn handle_approve_withdrawal(
    headers: HeaderMap,
    Path(id): Path<String>,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
) -> Response {
    let mut redis = redis;
    if let Err(response) = check_admin_auth(&headers, &mut redis).await {
        return response;
    }

    match sqlx::query(
        "UPDATE withdrawals SET status = 'approved' WHERE withdrawal_id = $1 AND status = 'pending_cold_wallet'",
    )
    .bind(id)
    .execute(&pool)
    .await
    {
        Ok(result) if result.rows_affected() > 0 => Json(serde_json::json!({ "approved": true })).into_response(),
        Ok(_) => (StatusCode::NOT_FOUND, "withdrawal not found or not pending cold wallet approval").into_response(),
        Err(error) => {
            error!(event = "approve_withdrawal_error", error = %error);
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

async fn handle_volume_by_pair(
    headers: HeaderMap,
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<ConnectionManager>,
) -> Response {
    let mut redis = redis;
    if let Err(response) = check_admin_auth(&headers, &mut redis).await {
        return response;
    }

    match sqlx::query(
        r#"
        SELECT pair,
               COALESCE(SUM(CASE WHEN executed_at >= NOW() - INTERVAL '24 hours' THEN price * quantity ELSE 0 END), 0) AS volume_24h,
               COALESCE(SUM(CASE WHEN executed_at >= NOW() - INTERVAL '7 days' THEN price * quantity ELSE 0 END), 0) AS volume_7d,
               COUNT(*) AS trade_count
        FROM trades
        WHERE executed_at >= NOW() - INTERVAL '30 days'
        GROUP BY pair
        ORDER BY volume_24h DESC
        "#,
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => {
            let volume: Vec<_> = rows
                .into_iter()
                .map(|row| {
                    serde_json::json!({
                        "pair": row.get::<String, _>("pair"),
                        "volume_24h": row.get::<f64, _>("volume_24h"),
                        "volume_7d": row.get::<f64, _>("volume_7d"),
                        "trade_count": row.get::<i64, _>("trade_count"),
                    })
                })
                .collect();
            Json(serde_json::json!({ "volume": volume })).into_response()
        }
        Err(error) => {
            error!(event = "volume_metrics_error", error = %error);
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

pub fn router(pool: PgPool, redis: ConnectionManager) -> Router {
    Router::new()
        .route("/admin/api/stats", get(handle_stats))
        .route("/admin/api/volume", get(handle_volume_by_pair))
        .route("/admin/api/users", get(handle_list_users))
        .route("/admin/api/withdrawals", get(handle_list_withdrawals))
        .route("/admin/api/withdrawals/:id/approve", post(handle_approve_withdrawal))
        .route("/admin/api/fee-tiers", get(handle_list_fee_tiers))
        .route("/admin/api/fee-tiers/:id", put(handle_update_fee_tier))
        .layer(Extension(pool))
        .layer(Extension(redis))
}
