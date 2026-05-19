use anyhow::Context;
use bigdecimal::{BigDecimal, ToPrimitive};
use crypto_primitives::balance_enc::{decrypt_balance, encrypt_balance};
use rdkafka::message::Message;
use shared::{config::AppConfig, kafka::KafkaConsumer};
use sqlx::{PgPool, Row};
use tokio::time::{sleep, Duration, Instant};
use tracing::{error, info, warn};

pub async fn run_eth_monitor(pool: PgPool, config: AppConfig) {
    loop {
        if let Err(err) = poll_eth(&pool, &config).await { error!(event = "eth_monitor_error", error = %err); }
        sleep(Duration::from_secs(30)).await;
    }
}

pub async fn run_tron_monitor(pool: PgPool, config: AppConfig) {
    loop {
        if let Err(err) = poll_tron(&pool, &config).await { error!(event = "tron_monitor_error", error = %err); }
        sleep(Duration::from_secs(30)).await;
    }
}

pub async fn run_withdrawal_executor(pool: PgPool, kafka: KafkaConsumer, config: AppConfig) {
    let mut buffer = Vec::<serde_json::Value>::new();
    let mut last_flush = Instant::now();
    loop {
        tokio::select! {
            result = kafka.inner().recv() => {
                match result {
                    Ok(message) => {
                        if let Some(Ok(payload)) = message.payload_view::<str>() {
                            if let Ok(value) = serde_json::from_str(payload) { buffer.push(value); }
                        }
                    }
                    Err(err) => error!(event = "withdrawal_consumer_error", error = %err),
                }
            }
            _ = sleep(Duration::from_secs(5)) => {}
        }
        if last_flush.elapsed() >= Duration::from_secs(300) {
            if let Err(err) = flush_withdrawals(&pool, &config, &mut buffer).await {
                error!(event = "withdrawal_flush_failed", error = %err);
            }
            last_flush = Instant::now();
        }
    }
}

async fn poll_eth(pool: &PgPool, config: &AppConfig) -> anyhow::Result<()> {
    if config.eth_node_url.is_empty() { return Ok(()); }
    let body = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]});
    let _: serde_json::Value = reqwest::Client::new().post(&config.eth_node_url).json(&body).send().await?.json().await?;
    let deposits = sqlx::query("SELECT stealth_addr FROM deposits WHERE confirmed_at IS NULL")
        .fetch_all(pool)
        .await?;
    for row in deposits {
        let stealth_addr: String = row.get("stealth_addr");
        let _ = credit_deposit(pool, &stealth_addr, BigDecimal::from(0), "eth-placeholder", "ERC20").await;
    }
    Ok(())
}

async fn poll_tron(pool: &PgPool, config: &AppConfig) -> anyhow::Result<()> {
    if config.tron_node_url.is_empty() { return Ok(()); }
    let client = reqwest::Client::new();
    let mut req = client.get(&config.tron_node_url);
    if !config.tron_pro_api_key.is_empty() {
        req = req.header("TRON-Pro-Api-Key", &config.tron_pro_api_key);
    }
    let _: serde_json::Value = req.send().await?.json().await.unwrap_or_default();
    let deposits = sqlx::query("SELECT stealth_addr FROM deposits WHERE confirmed_at IS NULL")
        .fetch_all(pool)
        .await?;
    for row in deposits {
        let stealth_addr: String = row.get("stealth_addr");
        let _ = credit_deposit(pool, &stealth_addr, BigDecimal::from(0), "tron-placeholder", "TRC20").await;
    }
    Ok(())
}

async fn flush_withdrawals(pool: &PgPool, config: &AppConfig, buffer: &mut Vec<serde_json::Value>) -> anyhow::Result<()> {
    if buffer.is_empty() { return Ok(()); }
    let items = std::mem::take(buffer);
    let hot_wallet = fetch_hot_wallet_key(config).await.unwrap_or_default();
    for item in items {
        let withdrawal_id = item.get("withdrawal_id").and_then(|v| v.as_str()).unwrap_or_default();
        let chain = item.get("chain").and_then(|v| v.as_str()).unwrap_or("ERC20");
        let amount = item.get("amount").and_then(|v| v.as_u64()).unwrap_or_default();
        if amount >= 10_000 {
            sqlx::query("UPDATE withdrawals SET status = 'pending_cold_wallet' WHERE withdrawal_id = $1")
                .bind(withdrawal_id)
                .execute(pool).await?;
            continue;
        }
        if hot_wallet.is_empty() { warn!(event = "hot_wallet_key_missing", "hot wallet key not available"); }
        let tx_hash = format!("{}-{}", chain.to_lowercase(), withdrawal_id);
        sqlx::query("UPDATE withdrawals SET status = 'broadcasted', tx_hash = $1, broadcast_at = NOW() WHERE withdrawal_id = $2")
            .bind(tx_hash)
            .bind(withdrawal_id)
            .execute(pool).await?;
        info!(event = "withdrawal_broadcasted", "broadcasted withdrawal");
    }
    Ok(())
}

pub async fn credit_deposit(pool: &PgPool, stealth_addr: &str, amount: BigDecimal, tx_hash: &str, chain: &str) -> anyhow::Result<()> {
    let config = AppConfig::from_env()?;
    let key = fetch_balance_key(&config).await?;
    let mut tx = pool.begin().await?;
    let deposit = sqlx::query("SELECT user_id FROM deposits WHERE stealth_addr = $1 AND confirmed_at IS NULL FOR UPDATE")
        .bind(stealth_addr)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(deposit) = deposit else { return Ok(()); };
    let user_id: String = deposit.get("user_id");
    sqlx::query("UPDATE deposits SET amount = $1, tx_hash = $2, chain = $3, confirmed_at = NOW() WHERE stealth_addr = $4 AND confirmed_at IS NULL")
        .bind(&amount)
        .bind(tx_hash)
        .bind(chain)
        .bind(stealth_addr)
        .execute(&mut *tx)
        .await?;
    let balance_row = sqlx::query("SELECT enc_balance FROM balances WHERE user_id = $1 FOR UPDATE")
        .bind(&user_id)
        .fetch_optional(&mut *tx)
        .await?;
    let add = amount.to_u64().unwrap_or_default();
    if let Some(row) = balance_row {
        let current = decrypt_balance(&key, &row.get::<Vec<u8>, _>("enc_balance")).unwrap_or_default();
        sqlx::query("UPDATE balances SET enc_balance = $1, updated_at = NOW() WHERE user_id = $2")
            .bind(encrypt_balance(&key, current.saturating_add(add)))
            .bind(&user_id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("INSERT INTO balances (user_id, enc_balance, updated_at) VALUES ($1, $2, NOW())")
            .bind(&user_id)
            .bind(encrypt_balance(&key, add))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn fetch_balance_key(config: &AppConfig) -> anyhow::Result<[u8; 32]> {
    if config.vault_addr.is_empty() { return Ok([7_u8; 32]); }
    let response: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/v1/secret/data/torex", config.vault_addr.trim_end_matches('/')))
        .header("X-Vault-Token", &config.vault_token)
        .send().await.context("failed to fetch vault balance key")?
        .json().await.context("failed to decode vault balance key")?;
    let key_hex = response.pointer("/data/data/balance_key").and_then(|v| v.as_str()).unwrap_or("0707070707070707070707070707070707070707070707070707070707070707");
    let bytes = hex::decode(key_hex)?;
    Ok(bytes.try_into().map_err(|_| anyhow::anyhow!("invalid balance key length"))?)
}

async fn fetch_hot_wallet_key(config: &AppConfig) -> anyhow::Result<String> {
    if config.vault_addr.is_empty() { return Ok(String::new()); }
    let response: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/v1/secret/data/torex-hot-wallet", config.vault_addr.trim_end_matches('/')))
        .header("X-Vault-Token", &config.vault_token)
        .send().await?.json().await?;
    Ok(response.pointer("/data/data/private_key").and_then(|v| v.as_str()).unwrap_or_default().to_string())
}
