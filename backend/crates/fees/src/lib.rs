use anyhow::Context;
use axum::{extract::Extension, response::IntoResponse, routing::get, Json, Router};
use serde::Serialize;
use sqlx::{PgPool, Row};
use tracing::error;

/// Fee tier loaded from DB
#[derive(Debug, Clone)]
pub struct FeeTier {
    pub maker_bps: i32,
    pub taker_bps: i32,
}

/// Fetch the applicable fee tier for a user's 30-day rolling volume.
/// Queries the fee_tiers table; falls back to 16/26 bps if table doesn't exist yet.
pub async fn get_user_fee_tier(pool: &PgPool, user_id: &str) -> FeeTier {
    let result = sqlx::query(
        r#"
        SELECT ft.maker_bps, ft.taker_bps
        FROM fee_tiers ft
        WHERE ft.enabled = TRUE
          AND COALESCE(
            (SELECT volume_30d FROM rolling_volume WHERE user_id = $1),
            0
          ) >= ft.min_volume
          AND (
            ft.max_volume IS NULL
            OR COALESCE(
              (SELECT volume_30d FROM rolling_volume WHERE user_id = $1),
              0
            ) <= ft.max_volume
          )
        ORDER BY ft.min_volume DESC
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await;

    match result {
        Ok(Some(row)) => FeeTier {
            maker_bps: row.get("maker_bps"),
            taker_bps: row.get("taker_bps"),
        },
        _ => FeeTier {
            maker_bps: 16,
            taker_bps: 26,
        },
    }
}

/// Calculate fee in USDT given a trade value and basis points.
/// Returns fee as f64 USDT.
pub fn calculate_fee(trade_value_usdt: f64, bps: i32) -> f64 {
    trade_value_usdt * (bps as f64) / 10_000.0
}

/// Record fees for a settled trade and update rolling volumes.
/// Returns (maker_fee_usdt, taker_fee_usdt).
pub async fn record_trade_fees(
    pool: &PgPool,
    pair: &str,
    maker_user_id: &str,
    taker_user_id: &str,
    trade_value: f64,
    maker_tier: &FeeTier,
    taker_tier: &FeeTier,
) -> anyhow::Result<(f64, f64)> {
    let maker_fee = calculate_fee(trade_value, maker_tier.maker_bps);
    let taker_fee = calculate_fee(trade_value, taker_tier.taker_bps);

    let mut tx = pool.begin().await.context("begin fee tx")?;

    sqlx::query("INSERT INTO fee_revenue (pair, fee_type, amount) VALUES ($1, 'maker', $2)")
        .bind(pair)
        .bind(maker_fee)
        .execute(&mut *tx)
        .await?;

    sqlx::query("INSERT INTO fee_revenue (pair, fee_type, amount) VALUES ($1, 'taker', $2)")
        .bind(pair)
        .bind(taker_fee)
        .execute(&mut *tx)
        .await?;

    for uid in [maker_user_id, taker_user_id] {
        sqlx::query(
            r#"
            INSERT INTO rolling_volume (user_id, volume_30d, last_updated)
            VALUES ($1, $2, NOW())
            ON CONFLICT (user_id) DO UPDATE
            SET volume_30d = rolling_volume.volume_30d + $2,
                last_updated = NOW()
            "#,
        )
        .bind(uid)
        .bind(trade_value)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await.context("commit fee tx")?;
    Ok((maker_fee, taker_fee))
}

#[derive(Serialize)]
struct FeeScheduleRow {
    min_volume: f64,
    max_volume: Option<f64>,
    maker_pct: String,
    taker_pct: String,
}

async fn handle_fee_schedule(Extension(pool): Extension<PgPool>) -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT min_volume, max_volume, maker_bps, taker_bps FROM fee_tiers WHERE enabled ORDER BY min_volume",
    )
    .fetch_all(&pool)
    .await;

    match rows {
        Ok(rows) => {
            let schedule: Vec<FeeScheduleRow> = rows
                .into_iter()
                .map(|r| FeeScheduleRow {
                    min_volume: r.get("min_volume"),
                    max_volume: r.get("max_volume"),
                    maker_pct: format!("{:.2}%", r.get::<i32, _>("maker_bps") as f64 / 100.0),
                    taker_pct: format!("{:.2}%", r.get::<i32, _>("taker_bps") as f64 / 100.0),
                })
                .collect();
            Json(serde_json::json!({ "tiers": schedule })).into_response()
        }
        Err(e) => {
            error!(event = "fee_schedule_error", error = %e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

async fn handle_admin_fee_revenue(Extension(pool): Extension<PgPool>) -> impl IntoResponse {
    let rows = sqlx::query(
        r#"
        SELECT pair, fee_type,
               CAST(SUM(amount) AS DOUBLE PRECISION) as total,
               COUNT(*) as count
        FROM fee_revenue
        WHERE collected_at >= NOW() - INTERVAL '30 days'
        GROUP BY pair, fee_type
        ORDER BY total DESC
        "#,
    )
    .fetch_all(&pool)
    .await;

    match rows {
        Ok(rows) => {
            let data: Vec<_> = rows
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                        "pair": r.get::<String, _>("pair"),
                        "fee_type": r.get::<String, _>("fee_type"),
                        "total_30d_usdt": r.get::<Option<f64>, _>("total").unwrap_or(0.0),
                        "trade_count": r.get::<Option<i64>, _>("count").unwrap_or(0),
                    })
                })
                .collect();
            Json(serde_json::json!({ "fee_revenue": data })).into_response()
        }
        Err(e) => {
            error!(event = "fee_revenue_error", error = %e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

/// HTTP router — merged into the main app
pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/api/fees", get(handle_fee_schedule))
        .route("/admin/api/fees/revenue", get(handle_admin_fee_revenue))
        .layer(Extension(pool))
}

/// Create fee-related tables if they don't exist (called from app migrations)
pub async fn ensure_fee_tables(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS fee_tiers (
            id         SERIAL PRIMARY KEY,
            min_volume DOUBLE PRECISION NOT NULL,
            max_volume DOUBLE PRECISION,
            maker_bps  INT NOT NULL,
            taker_bps  INT NOT NULL,
            enabled    BOOLEAN NOT NULL DEFAULT TRUE
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS rolling_volume (
            user_id      TEXT PRIMARY KEY,
            volume_30d   DOUBLE PRECISION NOT NULL DEFAULT 0,
            last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS fee_revenue (
            id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            pair         TEXT NOT NULL,
            fee_type     TEXT NOT NULL,
            amount       DOUBLE PRECISION NOT NULL,
            collected_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM fee_tiers")
        .fetch_one(pool)
        .await?;
    if count.0 == 0 {
        sqlx::query(
            r#"INSERT INTO fee_tiers (min_volume, max_volume, maker_bps, taker_bps) VALUES
               (0,          9999.999999,     16, 26),
               (10000,      49999.999999,    14, 24),
               (50000,      99999.999999,    12, 22),
               (100000,     249999.999999,   10, 20),
               (250000,     999999.999999,    8, 18),
               (1000000,    9999999.999999,   6, 16),
               (10000000,   NULL,             0, 10)"#,
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_zero_bps() {
        let fee = calculate_fee(10000.0, 0);
        assert_eq!(fee, 0.0);
    }

    #[test]
    fn test_taker_fee_default_tier() {
        let fee = calculate_fee(1000.0, 26);
        assert!((fee - 2.60).abs() < 0.001);
    }

    #[test]
    fn test_maker_fee_default_tier() {
        let fee = calculate_fee(1000.0, 16);
        assert!((fee - 1.60).abs() < 0.001);
    }

    #[test]
    fn test_high_volume_maker_zero_fee() {
        let fee = calculate_fee(50000.0, 0);
        assert_eq!(fee, 0.0);
    }

    #[test]
    fn test_high_volume_taker_fee() {
        let fee = calculate_fee(100_000.0, 10);
        assert!((fee - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_fee_rounding_small_trade() {
        let fee = calculate_fee(1.0, 26);
        assert!(fee >= 0.0);
        assert!(fee < 0.01);
    }
}
