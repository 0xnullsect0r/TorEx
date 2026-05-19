use anyhow::Context;
use crypto_primitives::balance_enc::{decrypt_balance, encrypt_balance};
use fees::{calculate_fee, get_user_fee_tier, record_trade_fees};
use matching::Trade;
use rdkafka::message::Message;
use shared::{config::AppConfig, kafka::{KafkaConsumer, KafkaProducer}};
use sqlx::{PgPool, Row};
use tracing::{error, info};

pub async fn run_settlement_consumer(pool: PgPool, kafka_consumer: KafkaConsumer, kafka_producer: KafkaProducer) {
    let config = AppConfig::from_env().unwrap_or_else(|_| AppConfig {
        postgres_url: String::new(), redis_url: String::new(), kafka_broker: String::new(), vault_addr: String::new(), vault_token: String::new(), eth_node_url: String::new(), tron_node_url: String::new(), tron_pro_api_key: String::new(), hcaptcha_secret: String::new()
    });
    loop {
        match kafka_consumer.inner().recv().await {
            Ok(message) => {
                let Some(Ok(payload)) = message.payload_view::<str>() else { continue; };
                if let Err(err) = process_trade(&pool, &kafka_producer, &config, payload).await {
                    error!(event = "settlement_failed", error = %err);
                }
            }
            Err(err) => error!(event = "settlement_consumer_error", error = %err),
        }
    }
}

async fn process_trade(pool: &PgPool, producer: &KafkaProducer, config: &AppConfig, payload: &str) -> anyhow::Result<()> {
    let trade: Trade = serde_json::from_str(payload).context("failed to decode trade payload")?;
    let key = fetch_balance_key(config).await?;
    let mut tx = pool.begin().await.context("failed to open settlement transaction")?;
    let seller_row = sqlx::query("SELECT enc_balance FROM balances WHERE user_id = $1 FOR UPDATE")
        .bind(&trade.seller_user_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to lock seller balance")?;
    let buyer_row = sqlx::query("SELECT enc_balance FROM balances WHERE user_id = $1 FOR UPDATE")
        .bind(&trade.buyer_user_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to lock buyer balance")?;
    let seller_balance = decrypt_balance(&key, &seller_row.get::<Vec<u8>, _>("enc_balance")).context("failed to decrypt seller balance")?;
    let buyer_balance = decrypt_balance(&key, &buyer_row.get::<Vec<u8>, _>("enc_balance")).context("failed to decrypt buyer balance")?;
    let quote_delta = (trade.price * trade.quantity).round() as u64;
    let base_delta = trade.quantity.round() as u64;
    let maker_tier = get_user_fee_tier(pool, &trade.seller_user_id).await;
    let taker_tier = get_user_fee_tier(pool, &trade.buyer_user_id).await;
    let trade_value = trade.price * trade.quantity;
    let maker_fee = calculate_fee(trade_value, maker_tier.maker_bps).round().max(0.0) as u64;
    let taker_fee = calculate_fee(trade_value, taker_tier.taker_bps).round().max(0.0) as u64;
    sqlx::query("UPDATE balances SET enc_balance = $1, updated_at = NOW() WHERE user_id = $2")
        .bind(encrypt_balance(
            &key,
            seller_balance
                .saturating_sub(quote_delta)
                .saturating_sub(maker_fee),
        ))
        .bind(&trade.seller_user_id)
        .execute(&mut *tx)
        .await
        .context("failed to update seller balance")?;
    sqlx::query("UPDATE balances SET enc_balance = $1, updated_at = NOW() WHERE user_id = $2")
        .bind(encrypt_balance(
            &key,
            buyer_balance
                .saturating_add(base_delta)
                .saturating_sub(taker_fee),
        ))
        .bind(&trade.buyer_user_id)
        .execute(&mut *tx)
        .await
        .context("failed to update buyer balance")?;
    for (order_id, filled_status) in [(trade.buyer_order_id, "filled"), (trade.seller_order_id, "filled")] {
        sqlx::query("UPDATE orders SET filled = quantity, status = $1 WHERE order_id = $2")
            .bind(filled_status)
            .bind(order_id)
            .execute(&mut *tx)
            .await
            .context("failed to update order state")?;
    }
    tx.commit().await.context("failed to commit settlement")?;
    record_trade_fees(
        pool,
        &trade.pair,
        &trade.seller_user_id,
        &trade.buyer_user_id,
        trade_value,
        &maker_tier,
        &taker_tier,
    )
    .await
    .context("failed to record trade fees")?;
    producer.send("settlement_complete", &trade.trade_id.to_string(), &serde_json::json!({"trade_id": trade.trade_id}).to_string()).await?;
    info!(event = "settlement_complete", maker_fee, taker_fee, "settled trade");
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
