use std::sync::Arc;

use anyhow::Context;
use axum::{routing::get, Router};
use dashmap::DashMap;
use matching::OrderBooks;
use shared::{cache::create_redis, config::AppConfig, db::create_pool, kafka::{KafkaConsumer, KafkaProducer}};
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let config = AppConfig::from_env().context("failed to load configuration")?;
    let pool = create_pool(&config.postgres_url).await;
    let redis = create_redis(&config.redis_url).await;
    let kafka_producer = KafkaProducer::new(&config.kafka_broker)?;

    run_migrations(&pool).await?;
    fees::ensure_fee_tables(&pool).await?;
    auth::init_state(pool.clone(), redis.clone(), config.clone());
    auth::seed_admin_user(&pool).await?;
    wallet::init_state(pool.clone(), kafka_producer.clone(), config.clone())?;

    let books: OrderBooks = Arc::new(DashMap::new());
    let app = Router::new()
        .merge(auth::router())
        .merge(wallet::router())
        .merge(matching::router(books.clone(), pool.clone(), kafka_producer.clone()))
        .merge(fees::router(pool.clone()))
        .merge(admin::router(pool.clone(), redis.clone()))
        .merge(ws_gateway::router(books.clone(), redis.clone()))
        .route("/health", get(|| async { "OK" }))
        .route("/metrics", get(|| async { "# TYPE torex_up gauge\ntorex_up 1\n" }))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let settlement_consumer = KafkaConsumer::new(&config.kafka_broker, "torex-settlement", &["trades"])?;
    let withdrawal_consumer = KafkaConsumer::new(&config.kafka_broker, "torex-withdrawals", &["withdrawals"])?;
    tokio::spawn(settlement::run_settlement_consumer(pool.clone(), settlement_consumer, kafka_producer.clone()));
    tokio::spawn(blockchain::run_eth_monitor(pool.clone(), config.clone()));
    tokio::spawn(blockchain::run_tron_monitor(pool.clone(), config.clone()));
    tokio::spawn(blockchain::run_withdrawal_executor(pool.clone(), withdrawal_consumer, config.clone()));

    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    info!(event = "server_started", "TorEx backend listening on 0.0.0.0:8080");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn run_migrations(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    let statements = [
        "CREATE TABLE IF NOT EXISTS users (user_id TEXT PRIMARY KEY, pubkey TEXT NOT NULL, view_pubkey TEXT NOT NULL, spend_pubkey TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL)",
        "CREATE TABLE IF NOT EXISTS admin_users (username TEXT PRIMARY KEY, password_hash TEXT NOT NULL, totp_secret TEXT NULL, totp_enrolled BOOLEAN NOT NULL DEFAULT false, passkey_credential TEXT NULL)",
        "CREATE TABLE IF NOT EXISTS balances (user_id TEXT PRIMARY KEY, enc_balance BYTEA NOT NULL, updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW())",
        "CREATE TABLE IF NOT EXISTS deposits (id UUID PRIMARY KEY DEFAULT gen_random_uuid(), user_id TEXT NOT NULL, stealth_addr TEXT NOT NULL UNIQUE, ephemeral_pubkey TEXT NOT NULL, chain TEXT NOT NULL, amount NUMERIC NULL, tx_hash TEXT NULL, created_at TIMESTAMPTZ NOT NULL, confirmed_at TIMESTAMPTZ NULL)",
        "CREATE TABLE IF NOT EXISTS withdrawals (withdrawal_id TEXT PRIMARY KEY, user_id TEXT NOT NULL, dest_address TEXT NOT NULL, amount NUMERIC NOT NULL, chain TEXT NOT NULL, status TEXT NOT NULL, tx_hash TEXT NULL, created_at TIMESTAMPTZ NOT NULL, broadcast_at TIMESTAMPTZ NULL)",
        "CREATE TABLE IF NOT EXISTS orders (order_id UUID PRIMARY KEY, user_id TEXT NOT NULL, pair TEXT NOT NULL, side TEXT NOT NULL, order_type TEXT NOT NULL, price DOUBLE PRECISION NOT NULL, quantity DOUBLE PRECISION NOT NULL, filled DOUBLE PRECISION NOT NULL, status TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL)",
        "CREATE TABLE IF NOT EXISTS trades (trade_id UUID PRIMARY KEY, pair TEXT NOT NULL, price DOUBLE PRECISION NOT NULL, quantity DOUBLE PRECISION NOT NULL, buyer_order_id UUID NOT NULL, seller_order_id UUID NOT NULL, buyer_user_id TEXT NOT NULL, seller_user_id TEXT NOT NULL, executed_at TIMESTAMPTZ NOT NULL)",
        // New columns for extended order types (idempotent ALTER TABLE)
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS fee_free BOOLEAN NOT NULL DEFAULT false",
        "ALTER TABLE orders ADD COLUMN IF NOT EXISTS stop_price DOUBLE PRECISION NULL",
        "ALTER TABLE orders ADD COLUMN IF NOT EXISTS trailing_pct DOUBLE PRECISION NULL",
        "ALTER TABLE orders ADD COLUMN IF NOT EXISTS oco_link_id UUID NULL",
        "ALTER TABLE orders ADD COLUMN IF NOT EXISTS iceberg_peak_qty DOUBLE PRECISION NULL",
        "ALTER TABLE orders ADD COLUMN IF NOT EXISTS time_in_force TEXT NOT NULL DEFAULT 'GTC'",
        "ALTER TABLE orders ADD COLUMN IF NOT EXISTS expire_at TIMESTAMPTZ NULL",
        "ALTER TABLE orders ADD COLUMN IF NOT EXISTS client_order_id TEXT NULL",
        "ALTER TABLE orders ADD COLUMN IF NOT EXISTS reduce_only BOOLEAN NOT NULL DEFAULT false",
        // Login tracking (ip_hash only, no raw IPs stored)
        "CREATE TABLE IF NOT EXISTS user_logins (id UUID PRIMARY KEY DEFAULT gen_random_uuid(), user_id TEXT NOT NULL REFERENCES users(user_id), ip_hash TEXT NOT NULL DEFAULT 'unknown', logged_in_at TIMESTAMPTZ NOT NULL DEFAULT NOW())",
        "CREATE INDEX IF NOT EXISTS idx_user_logins_user_id ON user_logins(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_orders_user_pair ON orders(user_id, pair)",
        "CREATE INDEX IF NOT EXISTS idx_trades_buyer ON trades(buyer_user_id)",
        "CREATE INDEX IF NOT EXISTS idx_trades_seller ON trades(seller_user_id)",
        "CREATE INDEX IF NOT EXISTS idx_trades_pair_executed ON trades(pair, executed_at DESC)"
    ];
    for statement in statements {
        sqlx::query(statement).execute(pool).await?;
    }
    Ok(())
}
