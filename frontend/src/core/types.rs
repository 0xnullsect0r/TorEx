use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    #[default]
    Buy,
    Sell,
}

impl OrderSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Buy => "Buy",
            Self::Sell => "Sell",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    #[default]
    Limit,
    Market,
    StopLimit,
    StopMarket,
    TrailingStop,
    Oco,
    Iceberg,
    Twap,
    Fok,
    Ioc,
    PostOnly,
}

impl OrderType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Limit => "Limit",
            Self::Market => "Market",
            Self::StopLimit => "Stop-Limit",
            Self::StopMarket => "Stop-Market",
            Self::TrailingStop => "Trailing Stop",
            Self::Oco => "OCO",
            Self::Iceberg => "Iceberg",
            Self::Twap => "TWAP",
            Self::Fok => "FOK",
            Self::Ioc => "IOC",
            Self::PostOnly => "Post Only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum TimeInForce {
    #[default]
    Gtc,
    Ioc,
    Fok,
    Gtx,
}

impl TimeInForce {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Gtc => "GTC",
            Self::Ioc => "IOC",
            Self::Fok => "FOK",
            Self::Gtx => "GTX",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Midnight,
    Cyberpunk,
}

impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Midnight => "midnight",
            Self::Cyberpunk => "cyberpunk",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Midnight => "Midnight",
            Self::Cyberpunk => "Cyberpunk",
        }
    }

    pub fn all() -> [Theme; 4] {
        [Theme::Dark, Theme::Light, Theme::Midnight, Theme::Cyberpunk]
    }
}

impl From<String> for Theme {
    fn from(value: String) -> Self {
        match value.as_str() {
            "light" => Self::Light,
            "midnight" => Self::Midnight,
            "cyberpunk" => Self::Cyberpunk,
            _ => Self::Dark,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrderBookLevel {
    pub price: f64,
    #[serde(default, alias = "size", alias = "qty")]
    pub quantity: f64,
}

impl OrderBookLevel {
    pub fn total(&self) -> f64 { self.price * self.quantity }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    #[serde(default)] pub pair: String,
    #[serde(default)] pub bids: Vec<OrderBookLevel>,
    #[serde(default)] pub asks: Vec<OrderBookLevel>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChartCandle {
    pub time: u64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PairStats {
    pub last_price: f64,
    pub change_percent: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub volume_24h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    #[serde(default, alias = "id")] pub trade_id: String,
    #[serde(default)] pub pair: String,
    pub price: f64,
    #[serde(default, alias = "size", alias = "qty")] pub quantity: f64,
    #[serde(default, alias = "time", alias = "timestamp")] pub executed_at: String,
    #[serde(default)] pub side: Option<OrderSide>,
}

impl Default for Trade {
    fn default() -> Self {
        Self { trade_id: String::new(), pair: String::new(), price: 0.0, quantity: 0.0, executed_at: String::new(), side: None }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Order {
    #[serde(default, alias = "id")] pub order_id: String,
    #[serde(default)] pub pair: String,
    #[serde(default)] pub side: OrderSide,
    #[serde(default)] pub order_type: OrderType,
    #[serde(default)] pub time_in_force: Option<TimeInForce>,
    #[serde(default)] pub price: f64,
    #[serde(default, alias = "quantity", alias = "qty")] pub amount: f64,
    #[serde(default)] pub filled: f64,
    #[serde(default)] pub status: String,
    #[serde(default)] pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Balance {
    #[serde(default, alias = "available", alias = "balance")] pub usdt: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegisterResponse {
    #[serde(default)] pub user_id: String,
    #[serde(default)] pub session_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DepositAddressResponse {
    #[serde(default)] pub address: String,
    #[serde(default)] pub chain: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WalletTransaction {
    #[serde(default, alias = "id")] pub tx_id: String,
    #[serde(default, alias = "transaction_type", alias = "kind")] pub tx_type: String,
    #[serde(default)] pub chain: String,
    #[serde(default)] pub amount: f64,
    #[serde(default)] pub fee: f64,
    #[serde(default)] pub status: String,
    #[serde(default)] pub address: String,
    #[serde(default)] pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WalletHistoryResponse {
    #[serde(default)] pub deposits: Vec<WalletTransaction>,
    #[serde(default)] pub withdrawals: Vec<WalletTransaction>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeeTier {
    #[serde(default, alias = "tier_id")] pub id: i64,
    #[serde(default)] pub min_volume: f64,
    #[serde(default)] pub max_volume: Option<f64>,
    #[serde(default)] pub maker_fee: f64,
    #[serde(default)] pub taker_fee: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminStats {
    #[serde(default)] pub total_users: i64,
    #[serde(default, alias = "open_orders")] pub active_orders: i64,
    #[serde(default)] pub pending_withdrawals: i64,
    #[serde(default, alias = "volume_24h")] pub volume_24h: f64,
    #[serde(default, alias = "active_trades_24h", alias = "trade_count_24h")] pub trades_24h: i64,
    #[serde(default, alias = "deposits_24h")] pub deposits_24h: f64,
    #[serde(default = "default_true")] pub system_healthy: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct User {
    #[serde(default, alias = "id")] pub user_id: String,
    #[serde(default)] pub created_at: String,
    #[serde(default, alias = "rolling_volume_30d")] pub volume_30d: f64,
    #[serde(default)] pub order_count: i64,
    #[serde(default)] pub fee_free: bool,
}

pub type AdminUser = User;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserLoginEvent {
    #[serde(default, alias = "time", alias = "timestamp")] pub occurred_at: String,
    #[serde(default, alias = "channel")] pub source: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminUserActivity {
    #[serde(default)] pub last_logins: Vec<UserLoginEvent>,
    #[serde(default)] pub last_orders: Vec<Order>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Withdrawal {
    #[serde(default, alias = "id")] pub withdrawal_id: String,
    #[serde(default)] pub user_id: String,
    #[serde(default, alias = "dest_address")] pub address: String,
    #[serde(default)] pub amount: f64,
    #[serde(default)] pub fee: f64,
    #[serde(default)] pub chain: String,
    #[serde(default)] pub status: String,
    #[serde(default)] pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VolumeByPair {
    #[serde(default)] pub pair: String,
    #[serde(default, alias = "volume")] pub volume_24h: f64,
    #[serde(default)] pub trades_24h: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SignalMessage {
    pub ratchet_key: [u8; 32],
    pub counter: u32,
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CreateOrderRequest {
    pub pair: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    #[serde(skip_serializing_if = "Option::is_none")] pub time_in_force: Option<TimeInForce>,
    #[serde(skip_serializing_if = "Option::is_none")] pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub stop_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub limit_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub trailing_offset_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub visible_amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub duration_minutes: Option<i64>,
    pub amount: f64,
    #[serde(skip_serializing_if = "Option::is_none")] pub total: Option<f64>,
}

pub fn short_id(value: &str) -> String {
    if value.len() <= 12 { value.to_string() } else { format!("{}…{}", &value[..6], &value[value.len() - 4..]) }
}

pub fn format_compact(value: f64) -> String {
    if value >= 1_000_000_000.0 { format!("{:.2}B", value / 1_000_000_000.0) }
    else if value >= 1_000_000.0 { format!("{:.2}M", value / 1_000_000.0) }
    else if value >= 1_000.0 { format!("{:.2}K", value / 1_000.0) }
    else { format!("{value:.2}") }
}

pub fn compute_pair_stats(candles: &[ChartCandle]) -> PairStats {
    let mut stats = PairStats::default();
    let Some(first) = candles.first() else { return stats; };
    let Some(last) = candles.last() else { return stats; };
    let mut high = f64::MIN;
    let mut low = f64::MAX;
    let mut volume = 0.0;
    for candle in candles { high = high.max(candle.high); low = low.min(candle.low); volume += candle.volume; }
    stats.last_price = last.close;
    stats.change_percent = if first.open.abs() > f64::EPSILON { ((last.close - first.open) / first.open) * 100.0 } else { 0.0 };
    stats.high_24h = if high.is_finite() { high } else { 0.0 };
    stats.low_24h = if low.is_finite() { low } else { 0.0 };
    stats.volume_24h = volume;
    stats
}

const fn default_true() -> bool { true }
