use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrderBookLevel {
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub pair: String,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub trade_id: String,
    pub pair: String,
    pub price: f64,
    pub quantity: f64,
    pub executed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id: String,
    pub pair: String,
    pub side: String,
    pub order_type: String,
    pub price: f64,
    pub quantity: f64,
    pub filled: f64,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Balance {
    pub usdt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub time: u64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub user_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositAddressResponse {
    pub address: String,
    pub chain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeTier {
    pub tier_id: i64,
    pub min_volume: f64,
    pub max_volume: Option<f64>,
    pub maker_fee: f64,
    pub taker_fee: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminStats {
    pub total_users: i64,
    pub active_users_24h: i64,
    pub volume_24h: f64,
    pub total_volume: f64,
    pub fee_revenue_24h: f64,
    pub total_fee_revenue: f64,
    pub open_orders: i64,
    pub pending_withdrawals: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Withdrawal {
    pub withdrawal_id: String,
    pub dest_address: String,
    pub amount: f64,
    pub chain: String,
    pub status: String,
    pub created_at: String,
}

/// Side-channel-safe Signal message (wraps snow transport payload)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalMessage {
    pub ratchet_key: [u8; 32],
    pub counter: u32,
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
}
