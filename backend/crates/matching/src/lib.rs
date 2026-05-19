use std::{cmp::Reverse, collections::{BTreeMap, VecDeque}, sync::{Arc, OnceLock}};

use auth::NoiseAuth;
use axum::{extract::{Json, Path, Query}, routing::{delete, get, post}, Router};
use dashmap::DashMap;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use shared::{error::AppError, kafka::KafkaProducer};
use sqlx::{PgPool, Row};
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

// ── Order types ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Side { Buy, Sell }
impl Side {
    pub fn to_db_str(&self) -> &'static str { match self { Side::Buy => "buy", Side::Sell => "sell" } }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum OrderType {
    Limit, Market,
    StopLimit, StopMarket, TrailingStop,
    OCO, Iceberg, TWAP,
    FOK, IOC, PostOnly,
}
impl OrderType {
    pub fn to_db_str(&self) -> &'static str {
        match self {
            OrderType::Limit     => "limit",
            OrderType::Market    => "market",
            OrderType::StopLimit => "stop_limit",
            OrderType::StopMarket => "stop_market",
            OrderType::TrailingStop => "trailing_stop",
            OrderType::OCO       => "oco",
            OrderType::Iceberg   => "iceberg",
            OrderType::TWAP      => "twap",
            OrderType::FOK       => "fok",
            OrderType::IOC       => "ioc",
            OrderType::PostOnly  => "post_only",
        }
    }
    pub fn is_conditional(&self) -> bool {
        matches!(self, OrderType::StopLimit | OrderType::StopMarket | OrderType::TrailingStop)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OrderStatus { Open, PendingTrigger, PartiallyFilled, Filled, Cancelled, Rejected }
impl OrderStatus {
    pub fn to_db_str(&self) -> &'static str {
        match self {
            OrderStatus::Open            => "open",
            OrderStatus::PendingTrigger  => "pending_trigger",
            OrderStatus::PartiallyFilled => "partially_filled",
            OrderStatus::Filled          => "filled",
            OrderStatus::Cancelled       => "cancelled",
            OrderStatus::Rejected        => "rejected",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id: Uuid,
    pub user_id: String,
    pub pair: String,
    pub side: Side,
    pub order_type: OrderType,
    pub price: f64,
    pub quantity: f64,
    pub filled: f64,
    pub status: OrderStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    // Extended fields
    pub stop_price: Option<f64>,
    pub trailing_pct: Option<f64>,
    pub oco_link_id: Option<Uuid>,
    pub iceberg_peak_qty: Option<f64>,
    pub time_in_force: String, // "GTC", "IOC", "FOK", "GTX"
}

impl Default for Order {
    fn default() -> Self {
        Self {
            order_id: Uuid::now_v7(),
            user_id: String::new(),
            pair: String::new(),
            side: Side::Buy,
            order_type: OrderType::Limit,
            price: 0.0,
            quantity: 0.0,
            filled: 0.0,
            status: OrderStatus::Open,
            created_at: chrono::Utc::now(),
            stop_price: None,
            trailing_pct: None,
            oco_link_id: None,
            iceberg_peak_qty: None,
            time_in_force: "GTC".to_string(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Trade {
    pub trade_id: Uuid,
    pub pair: String,
    pub price: f64,
    pub quantity: f64,
    pub buyer_order_id: Uuid,
    pub seller_order_id: Uuid,
    pub buyer_user_id: String,
    pub seller_user_id: String,
    pub executed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct DepthSnapshot { pub bids: Vec<(f64, f64)>, pub asks: Vec<(f64, f64)> }

// ── Conditional (stop) order tracking ─────────────────────────────────────────

#[derive(Clone)]
struct ConditionalOrder {
    order: Order,
    /// For TrailingStop: track best price seen since order placement
    best_price: f64,
}

type ConditionalOrders = Arc<DashMap<Uuid, ConditionalOrder>>;

static CONDITIONAL_ORDERS: OnceLock<ConditionalOrders> = OnceLock::new();

fn conditional_orders() -> &'static ConditionalOrders {
    CONDITIONAL_ORDERS.get_or_init(|| Arc::new(DashMap::new()))
}

// ── Order book ────────────────────────────────────────────────────────────────

pub struct OrderBook {
    bids: BTreeMap<Reverse<i64>, VecDeque<Order>>,
    asks: BTreeMap<i64, VecDeque<Order>>,
    /// Track last traded price for stop/trailing order triggers
    pub last_price: Option<f64>,
}

impl Default for OrderBook {
    fn default() -> Self { Self { bids: BTreeMap::new(), asks: BTreeMap::new(), last_price: None } }
}

impl OrderBook {
    pub fn submit(&mut self, mut order: Order) -> Vec<Trade> {
        let mut trades = Vec::new();

        // PostOnly: reject if it would immediately match
        if matches!(order.order_type, OrderType::PostOnly) {
            let would_match = match order.side {
                Side::Buy => self.asks.iter().next().map(|(&p, _)| ticks_to_price(p) <= order.price).unwrap_or(false),
                Side::Sell => self.bids.iter().next().map(|(&p, _)| ticks_to_price(p.0) >= order.price).unwrap_or(false),
            };
            if would_match {
                order.status = OrderStatus::Rejected;
                return trades; // Return empty, caller handles rejection
            }
            // Falls through as a limit order (GTX)
            let mut limit_order = order;
            limit_order.order_type = OrderType::Limit;
            self.bids.entry(Reverse(price_to_ticks(limit_order.price))).or_default();
            self.add_resting(limit_order);
            return trades;
        }

        // Determine if this is a FOK/IOC order
        let is_fok = matches!(order.order_type, OrderType::FOK)
            || order.time_in_force == "FOK";
        let is_ioc = matches!(order.order_type, OrderType::IOC)
            || order.time_in_force == "IOC";

        // For FOK: pre-check if we can fill the entire quantity before executing
        if is_fok {
            let available = self.available_qty_at_price(&order.side, order.price);
            if available < order.quantity - 1e-9 {
                order.status = OrderStatus::Cancelled;
                return trades; // Can't fill, cancel immediately
            }
        }

        // Iceberg: only put iceberg_peak_qty visible in book at a time
        let iceberg_hidden = if matches!(order.order_type, OrderType::Iceberg) {
            let peak = order.iceberg_peak_qty.unwrap_or(order.quantity);
            let hidden = order.quantity - peak.min(order.quantity);
            order.quantity = peak.min(order.quantity);
            hidden
        } else {
            0.0
        };

        match order.side {
            Side::Buy => {
                while order.filled < order.quantity {
                    let Some((&best_price, _)) = self.asks.iter().next() else { break; };
                    if matches!(order.order_type, OrderType::Limit | OrderType::FOK | OrderType::IOC | OrderType::Iceberg)
                        && ticks_to_price(best_price) > order.price
                    {
                        break;
                    }
                    let mut remove_level = false;
                    if let Some(queue) = self.asks.get_mut(&best_price) {
                        if let Some(resting) = queue.front_mut() {
                            let remaining = (order.quantity - order.filled).min(resting.quantity - resting.filled);
                            order.filled += remaining;
                            resting.filled += remaining;
                            resting.status = if resting.filled >= resting.quantity { OrderStatus::Filled } else { OrderStatus::PartiallyFilled };
                            order.status = if order.filled >= order.quantity { OrderStatus::Filled } else { OrderStatus::PartiallyFilled };
                            let trade_price = ticks_to_price(best_price);
                            trades.push(Trade {
                                trade_id: Uuid::now_v7(),
                                pair: order.pair.clone(),
                                price: trade_price,
                                quantity: remaining,
                                buyer_order_id: order.order_id,
                                seller_order_id: resting.order_id,
                                buyer_user_id: order.user_id.clone(),
                                seller_user_id: resting.user_id.clone(),
                                executed_at: chrono::Utc::now(),
                            });
                            self.last_price = Some(trade_price);
                            if resting.filled >= resting.quantity { queue.pop_front(); }
                        }
                        remove_level = queue.is_empty();
                    }
                    if remove_level { self.asks.remove(&best_price); }
                }
                // IOC: cancel unfilled portion
                if is_ioc && order.filled < order.quantity {
                    order.status = OrderStatus::Cancelled;
                } else if order.filled < order.quantity && !matches!(order.order_type, OrderType::Market) && !is_fok {
                    // Add unfilled limit portion to book (possibly with iceberg hidden qty)
                    if iceberg_hidden > 0.0 {
                        order.quantity += iceberg_hidden;
                    }
                    self.add_resting(order);
                }
            }
            Side::Sell => {
                while order.filled < order.quantity {
                    let Some((&best_price, _)) = self.bids.iter().next() else { break; };
                    if matches!(order.order_type, OrderType::Limit | OrderType::FOK | OrderType::IOC | OrderType::Iceberg)
                        && ticks_to_price(best_price.0) < order.price
                    {
                        break;
                    }
                    let mut remove_level = false;
                    if let Some(queue) = self.bids.get_mut(&best_price) {
                        if let Some(resting) = queue.front_mut() {
                            let remaining = (order.quantity - order.filled).min(resting.quantity - resting.filled);
                            order.filled += remaining;
                            resting.filled += remaining;
                            resting.status = if resting.filled >= resting.quantity { OrderStatus::Filled } else { OrderStatus::PartiallyFilled };
                            order.status = if order.filled >= order.quantity { OrderStatus::Filled } else { OrderStatus::PartiallyFilled };
                            let trade_price = ticks_to_price(best_price.0);
                            trades.push(Trade {
                                trade_id: Uuid::now_v7(),
                                pair: order.pair.clone(),
                                price: trade_price,
                                quantity: remaining,
                                buyer_order_id: resting.order_id,
                                seller_order_id: order.order_id,
                                buyer_user_id: resting.user_id.clone(),
                                seller_user_id: order.user_id.clone(),
                                executed_at: chrono::Utc::now(),
                            });
                            self.last_price = Some(trade_price);
                            if resting.filled >= resting.quantity { queue.pop_front(); }
                        }
                        remove_level = queue.is_empty();
                    }
                    if remove_level { self.bids.remove(&best_price); }
                }
                if is_ioc && order.filled < order.quantity {
                    order.status = OrderStatus::Cancelled;
                } else if order.filled < order.quantity && !matches!(order.order_type, OrderType::Market) && !is_fok {
                    if iceberg_hidden > 0.0 {
                        order.quantity += iceberg_hidden;
                    }
                    self.add_resting(order);
                }
            }
        }
        trades
    }

    fn add_resting(&mut self, order: Order) {
        match order.side {
            Side::Buy => self.bids.entry(Reverse(price_to_ticks(order.price))).or_default().push_back(order),
            Side::Sell => self.asks.entry(price_to_ticks(order.price)).or_default().push_back(order),
        }
    }

    fn available_qty_at_price(&self, side: &Side, price: f64) -> f64 {
        match side {
            Side::Buy => self.asks.iter()
                .take_while(|(&p, _)| ticks_to_price(p) <= price)
                .flat_map(|(_, q)| q.iter().map(|o| o.quantity - o.filled))
                .sum(),
            Side::Sell => self.bids.iter()
                .take_while(|(&p, _)| ticks_to_price(p.0) >= price)
                .flat_map(|(_, q)| q.iter().map(|o| o.quantity - o.filled))
                .sum(),
        }
    }

    pub fn cancel(&mut self, order_id: Uuid) -> bool {
        let mut remove_bid_key: Option<Reverse<i64>> = None;
        for (price, queue) in self.bids.iter_mut() {
            if let Some(pos) = queue.iter().position(|o| o.order_id == order_id) {
                queue.remove(pos);
                if queue.is_empty() { remove_bid_key = Some(*price); }
                break;
            }
        }
        if let Some(key) = remove_bid_key { self.bids.remove(&key); return true; }

        let mut remove_ask_key: Option<i64> = None;
        for (price, queue) in self.asks.iter_mut() {
            if let Some(pos) = queue.iter().position(|o| o.order_id == order_id) {
                queue.remove(pos);
                if queue.is_empty() { remove_ask_key = Some(*price); }
                break;
            }
        }
        if let Some(key) = remove_ask_key { self.asks.remove(&key); return true; }

        false
    }

    pub fn depth_snapshot(&self, levels: usize) -> DepthSnapshot {
        let bids = self.bids.iter().take(levels).map(|(p, q)| (ticks_to_price(p.0), q.iter().map(|o| o.quantity - o.filled).sum())).collect();
        let asks = self.asks.iter().take(levels).map(|(p, q)| (ticks_to_price(*p), q.iter().map(|o| o.quantity - o.filled).sum())).collect();
        DepthSnapshot { bids, asks }
    }
}

pub type OrderBooks = Arc<DashMap<String, Arc<Mutex<OrderBook>>>>;

// ── Global state ──────────────────────────────────────────────────────────────

static MATCHING_STATE: OnceLock<Arc<MatchingState>> = OnceLock::new();
static TRADE_CHANNELS: OnceLock<DashMap<String, broadcast::Sender<String>>> = OnceLock::new();
static USER_CHANNELS: OnceLock<DashMap<String, broadcast::Sender<String>>> = OnceLock::new();

#[derive(Clone)]
struct MatchingState {
    books: OrderBooks,
    pool: PgPool,
    kafka: KafkaProducer,
}

// ── API types ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateOrderRequest {
    pair: String,
    side: Side,
    order_type: OrderType,
    price: Option<f64>,
    quantity: f64,
    stop_price: Option<f64>,
    trailing_pct: Option<f64>,
    oco_link_id: Option<Uuid>,
    iceberg_peak_qty: Option<f64>,
    time_in_force: Option<String>,
}

#[derive(Deserialize)]
struct CandleQuery { interval: Option<String> }

#[derive(Deserialize)]
struct OrderHistoryQuery { pair: Option<String>, limit: Option<i64>, offset: Option<i64> }

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router(books: OrderBooks, pool: PgPool, kafka: KafkaProducer) -> Router {
    let _ = MATCHING_STATE.set(Arc::new(MatchingState { books, pool, kafka }));
    TRADE_CHANNELS.get_or_init(DashMap::new);
    USER_CHANNELS.get_or_init(DashMap::new);
    Router::new()
        .route("/api/orders", post(create_order).get(list_open_orders))
        .route("/api/orders/:order_id", delete(cancel_order))
        .route("/api/order-history", get(list_order_history))
        .route("/api/orderbook/:pair", get(orderbook))
        .route("/api/candles/:pair", get(candles))
        .route("/api/ticker/:pair", get(ticker))
        .route("/api/recent-trades/:pair", get(recent_trades))
}

pub fn subscribe_trades(pair: &str) -> broadcast::Receiver<String> {
    trade_sender(pair).subscribe()
}

pub fn subscribe_user(user_id: &str) -> broadcast::Receiver<String> {
    user_sender(user_id).subscribe()
}

fn trade_sender(pair: &str) -> broadcast::Sender<String> {
    TRADE_CHANNELS.get().expect("trade channels").entry(pair.to_string()).or_insert_with(|| broadcast::channel(256).0).clone()
}

fn user_sender(user_id: &str) -> broadcast::Sender<String> {
    USER_CHANNELS.get().expect("user channels").entry(user_id.to_string()).or_insert_with(|| broadcast::channel(256).0).clone()
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn create_order(NoiseAuth(user_id): NoiseAuth, Json(payload): Json<CreateOrderRequest>) -> Result<Json<serde_json::Value>, AppError> {
    let state = matching_state();
    let price = payload.price.unwrap_or(0.0);
    let tif = payload.time_in_force.clone().unwrap_or_else(|| "GTC".to_string());

    let order = Order {
        order_id: Uuid::now_v7(),
        user_id: user_id.to_hex(),
        pair: payload.pair.clone(),
        side: payload.side,
        order_type: payload.order_type.clone(),
        price,
        quantity: payload.quantity,
        filled: 0.0,
        status: if payload.order_type.is_conditional() { OrderStatus::PendingTrigger } else { OrderStatus::Open },
        created_at: chrono::Utc::now(),
        stop_price: payload.stop_price,
        trailing_pct: payload.trailing_pct,
        oco_link_id: payload.oco_link_id,
        iceberg_peak_qty: payload.iceberg_peak_qty,
        time_in_force: tif,
    };

    // Persist to DB
    sqlx::query(
        "INSERT INTO orders (order_id, user_id, pair, side, order_type, price, quantity, filled, status, created_at, stop_price, trailing_pct, oco_link_id, iceberg_peak_qty, time_in_force) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,NOW(),$10,$11,$12,$13,$14)"
    )
    .bind(order.order_id)
    .bind(&order.user_id)
    .bind(&order.pair)
    .bind(order.side.to_db_str())
    .bind(order.order_type.to_db_str())
    .bind(order.price)
    .bind(order.quantity)
    .bind(order.filled)
    .bind(order.status.to_db_str())
    .bind(order.stop_price)
    .bind(order.trailing_pct)
    .bind(order.oco_link_id)
    .bind(order.iceberg_peak_qty)
    .bind(&order.time_in_force)
    .execute(&state.pool)
    .await
    .map_err(internal)?;

    // Conditional orders: store for price-trigger monitoring
    if order.order_type.is_conditional() {
        let best_price = {
            let book = state.books.entry(order.pair.clone()).or_default().clone();
            let book = book.lock();
            match order.side {
                Side::Buy => book.asks.iter().next().map(|(p, _)| ticks_to_price(*p)).unwrap_or(price),
                Side::Sell => book.bids.iter().next().map(|(p, _)| ticks_to_price(p.0)).unwrap_or(price),
            }
        };
        conditional_orders().insert(order.order_id, ConditionalOrder { order: order.clone(), best_price });
        return Ok(Json(serde_json::json!({"order_id": order.order_id, "status": "pending_trigger", "trades": []})));
    }

    // Submit to book
    let book = state.books.entry(order.pair.clone()).or_insert_with(|| Arc::new(Mutex::new(OrderBook::default()))).clone();
    let trades = book.lock().submit(order.clone());

    // Check if PostOnly was rejected
    if matches!(order.order_type, OrderType::PostOnly) && trades.is_empty() {
        // Could be resting or rejected — update DB based on current book state
    }

    // Persist trades and send notifications
    persist_trades(&state, &trades).await.map_err(internal)?;

    // Update order status in DB
    let new_status = if order.filled >= order.quantity { "filled" }
        else if order.filled > 0.0 { "partially_filled" }
        else { "open" };
    sqlx::query("UPDATE orders SET filled = $1, status = $2 WHERE order_id = $3")
        .bind(order.filled)
        .bind(new_status)
        .bind(order.order_id)
        .execute(&state.pool)
        .await
        .map_err(internal)?;

    // Handle OCO: cancel linked order if this one triggered trades
    if !trades.is_empty() {
        if let Some(link_id) = order.oco_link_id {
            cancel_oco_linked(&state, link_id).await;
        }
    }

    // Check conditional orders triggered by new trade prices
    if let Some(last_trade) = trades.last() {
        check_conditional_triggers(&state, &order.pair, last_trade.price).await;
    }

    info!(event = "order_submitted", order_type = order.order_type.to_db_str());
    Ok(Json(serde_json::json!({"order_id": order.order_id, "trades": trades})))
}

async fn cancel_oco_linked(state: &MatchingState, link_id: Uuid) {
    // Cancel the linked OCO order in the DB and book
    if let Ok(row) = sqlx::query("SELECT pair FROM orders WHERE order_id = $1")
        .bind(link_id)
        .fetch_optional(&state.pool)
        .await
    {
        if let Some(row) = row {
            let pair: String = row.get("pair");
            if let Some(book) = state.books.get(&pair) {
                book.lock().cancel(link_id);
            }
            let _ = sqlx::query("UPDATE orders SET status = 'cancelled' WHERE order_id = $1 AND status IN ('open','pending_trigger')")
                .bind(link_id)
                .execute(&state.pool)
                .await;
        }
    }
    // Also remove from conditional orders if it's there
    conditional_orders().remove(&link_id);
}

async fn check_conditional_triggers(state: &MatchingState, pair: &str, last_price: f64) {
    let mut to_trigger = Vec::new();

    for entry in conditional_orders().iter() {
        let cond = entry.value();
        if cond.order.pair != pair { continue; }

        let stop_price = match cond.order.stop_price {
            Some(p) => p,
            None => continue,
        };

        let triggered = match cond.order.order_type {
            OrderType::StopLimit | OrderType::StopMarket => match cond.order.side {
                Side::Buy  => last_price >= stop_price,  // Stop buy triggers when price rises to stop
                Side::Sell => last_price <= stop_price,  // Stop sell triggers when price falls to stop
            },
            OrderType::TrailingStop => {
                let pct = cond.order.trailing_pct.unwrap_or(1.0) / 100.0;
                match cond.order.side {
                    Side::Buy  => last_price >= cond.best_price * (1.0 + pct),
                    Side::Sell => last_price <= cond.best_price * (1.0 - pct),
                }
            }
            _ => false,
        };

        if triggered {
            to_trigger.push(*entry.key());
        } else if matches!(cond.order.order_type, OrderType::TrailingStop) {
            // Update best_price for trailing stops
            drop(entry);
            if let Some(mut cond_entry) = conditional_orders().get_mut(&to_trigger.last().copied().unwrap_or(Uuid::nil())) {
                match cond_entry.order.side {
                    Side::Buy  => { if last_price < cond_entry.best_price { cond_entry.best_price = last_price; } }
                    Side::Sell => { if last_price > cond_entry.best_price { cond_entry.best_price = last_price; } }
                }
            }
        }
    }

    for order_id in to_trigger {
        if let Some((_, cond)) = conditional_orders().remove(&order_id) {
            let mut triggered_order = cond.order;
            // Convert to the underlying order type
            let new_type = match triggered_order.order_type {
                OrderType::StopLimit     => OrderType::Limit,
                OrderType::StopMarket    => OrderType::Market,
                OrderType::TrailingStop  => OrderType::Market,
                _                        => OrderType::Market,
            };
            triggered_order.order_type = new_type;
            triggered_order.status = OrderStatus::Open;

            let book = state.books.entry(triggered_order.pair.clone())
                .or_insert_with(|| Arc::new(Mutex::new(OrderBook::default()))).clone();
            let trades = book.lock().submit(triggered_order.clone());

            if let Err(e) = persist_trades(state, &trades).await {
                warn!(event = "conditional_trigger_persist_failed", error = %e);
            }

            let _ = sqlx::query("UPDATE orders SET status = 'open', order_type = $1 WHERE order_id = $2")
                .bind(triggered_order.order_type.to_db_str())
                .bind(triggered_order.order_id)
                .execute(&state.pool)
                .await;

            if !trades.is_empty() {
                if let Some(link_id) = triggered_order.oco_link_id {
                    cancel_oco_linked(state, link_id).await;
                }
            }
        }
    }
}

async fn persist_trades(state: &MatchingState, trades: &[Trade]) -> anyhow::Result<()> {
    for trade in trades {
        state.kafka.send("trades", &trade.trade_id.to_string(), &serde_json::to_string(trade)?).await?;
        sqlx::query(
            "INSERT INTO trades (trade_id, pair, price, quantity, buyer_order_id, seller_order_id, buyer_user_id, seller_user_id, executed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"
        )
        .bind(trade.trade_id)
        .bind(&trade.pair)
        .bind(trade.price)
        .bind(trade.quantity)
        .bind(trade.buyer_order_id)
        .bind(trade.seller_order_id)
        .bind(&trade.buyer_user_id)
        .bind(&trade.seller_user_id)
        .bind(trade.executed_at)
        .execute(&state.pool)
        .await?;
        let payload = serde_json::to_string(trade)?;
        let _ = trade_sender(&trade.pair).send(payload.clone());
        let _ = user_sender(&trade.buyer_user_id).send(payload.clone());
        let _ = user_sender(&trade.seller_user_id).send(payload);
    }
    Ok(())
}

async fn cancel_order(NoiseAuth(user_id): NoiseAuth, Path(order_id): Path<Uuid>) -> Result<Json<serde_json::Value>, AppError> {
    let state = matching_state();
    let row = sqlx::query("SELECT pair, user_id FROM orders WHERE order_id = $1")
        .bind(order_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal)?
        .ok_or(AppError::NotFound)?;
    let owner: String = row.get("user_id");
    if owner != user_id.to_hex() { return Err(AppError::Unauthorized); }
    let pair: String = row.get("pair");
    if let Some(book) = state.books.get(&pair) { book.lock().cancel(order_id); }
    conditional_orders().remove(&order_id);
    sqlx::query("UPDATE orders SET status = 'cancelled' WHERE order_id = $1")
        .bind(order_id)
        .execute(&state.pool)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({"cancelled": true})))
}

async fn list_open_orders(NoiseAuth(user_id): NoiseAuth) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let rows = sqlx::query(
        "SELECT order_id, pair, side, order_type, price, quantity, filled, status, created_at, stop_price, time_in_force FROM orders WHERE user_id = $1 AND status IN ('open', 'partially_filled', 'pending_trigger') ORDER BY created_at DESC"
    )
    .bind(user_id.to_hex())
    .fetch_all(&matching_state().pool)
    .await
    .map_err(internal)?;
    Ok(Json(rows.into_iter().map(|row| serde_json::json!({
        "order_id": row.get::<Uuid,_>("order_id"),
        "pair": row.get::<String,_>("pair"),
        "side": row.get::<String,_>("side"),
        "order_type": row.get::<String,_>("order_type"),
        "price": row.get::<f64,_>("price"),
        "quantity": row.get::<f64,_>("quantity"),
        "filled": row.get::<f64,_>("filled"),
        "status": row.get::<String,_>("status"),
        "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
        "stop_price": row.get::<Option<f64>,_>("stop_price"),
        "time_in_force": row.get::<String,_>("time_in_force"),
    })).collect()))
}

async fn list_order_history(NoiseAuth(user_id): NoiseAuth, Query(q): Query<OrderHistoryQuery>) -> Result<Json<serde_json::Value>, AppError> {
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let rows = if let Some(pair) = q.pair {
        sqlx::query(
            "SELECT order_id, pair, side, order_type, price, quantity, filled, status, created_at FROM orders WHERE user_id = $1 AND pair = $2 ORDER BY created_at DESC LIMIT $3 OFFSET $4"
        )
        .bind(user_id.to_hex()).bind(pair).bind(limit).bind(offset)
        .fetch_all(&matching_state().pool).await.map_err(internal)?
    } else {
        sqlx::query(
            "SELECT order_id, pair, side, order_type, price, quantity, filled, status, created_at FROM orders WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(user_id.to_hex()).bind(limit).bind(offset)
        .fetch_all(&matching_state().pool).await.map_err(internal)?
    };
    let orders: Vec<_> = rows.into_iter().map(|row| serde_json::json!({
        "order_id": row.get::<Uuid,_>("order_id"),
        "pair": row.get::<String,_>("pair"),
        "side": row.get::<String,_>("side"),
        "order_type": row.get::<String,_>("order_type"),
        "price": row.get::<f64,_>("price"),
        "quantity": row.get::<f64,_>("quantity"),
        "filled": row.get::<f64,_>("filled"),
        "status": row.get::<String,_>("status"),
        "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
    })).collect();
    Ok(Json(serde_json::json!({"orders": orders, "limit": limit, "offset": offset})))
}

async fn orderbook(Path(pair): Path<String>) -> Result<Json<DepthSnapshot>, AppError> {
    let snapshot = matching_state().books.get(&pair).map(|b| b.lock().depth_snapshot(25)).unwrap_or_default();
    Ok(Json(snapshot))
}

async fn ticker(Path(pair): Path<String>) -> Result<Json<serde_json::Value>, AppError> {
    let pool = &matching_state().pool;
    let row = sqlx::query(
        r#"
        SELECT
            COALESCE((SELECT price FROM trades WHERE pair = $1 ORDER BY executed_at DESC LIMIT 1), 0) AS last_price,
            COALESCE(MAX(price), 0) AS high_24h,
            COALESCE(MIN(price), 0) AS low_24h,
            COALESCE(SUM(quantity), 0) AS volume_24h,
            COALESCE(SUM(price * quantity), 0) AS quote_volume_24h,
            COUNT(*) AS trade_count_24h
        FROM trades
        WHERE pair = $1 AND executed_at >= NOW() - INTERVAL '24 hours'
        "#
    )
    .bind(&pair)
    .fetch_one(pool)
    .await
    .map_err(internal)?;

    let last_price: f64 = row.get("last_price");
    let open_price: f64 = sqlx::query_scalar(
        "SELECT COALESCE(price, $2) FROM trades WHERE pair = $1 AND executed_at >= NOW() - INTERVAL '24 hours' ORDER BY executed_at ASC LIMIT 1"
    )
    .bind(&pair)
    .bind(last_price)
    .fetch_optional(pool)
    .await
    .map_err(internal)?
    .unwrap_or(last_price);

    let change_pct = if open_price > 0.0 { (last_price - open_price) / open_price * 100.0 } else { 0.0 };

    Ok(Json(serde_json::json!({
        "pair": pair,
        "last_price": last_price,
        "open_price": open_price,
        "high_24h": row.get::<f64,_>("high_24h"),
        "low_24h": row.get::<f64,_>("low_24h"),
        "volume_24h": row.get::<f64,_>("volume_24h"),
        "quote_volume_24h": row.get::<f64,_>("quote_volume_24h"),
        "change_pct": change_pct,
        "trade_count_24h": row.get::<i64,_>("trade_count_24h"),
    })))
}

async fn recent_trades(Path(pair): Path<String>) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let rows = sqlx::query(
        "SELECT trade_id, price, quantity, buyer_user_id, seller_user_id, executed_at FROM trades WHERE pair = $1 ORDER BY executed_at DESC LIMIT 50"
    )
    .bind(&pair)
    .fetch_all(&matching_state().pool)
    .await
    .map_err(internal)?;
    Ok(Json(rows.into_iter().map(|row| serde_json::json!({
        "trade_id": row.get::<Uuid,_>("trade_id"),
        "price": row.get::<f64,_>("price"),
        "quantity": row.get::<f64,_>("quantity"),
        "side": "buy", // simplified
        "executed_at": row.get::<chrono::DateTime<chrono::Utc>,_>("executed_at"),
    })).collect()))
}

async fn candles(Path(pair): Path<String>, Query(query): Query<CandleQuery>) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let interval = query.interval.unwrap_or_else(|| "1m".to_string());
    let bucket_secs: i64 = match interval.as_str() {
        "1m" => 60, "5m" => 300, "15m" => 900, "30m" => 1800,
        "1h" => 3600, "4h" => 14400, "1d" => 86400, "1w" => 604800,
        _ => 60,
    };
    let rows = sqlx::query("SELECT price, quantity, executed_at FROM trades WHERE pair = $1 ORDER BY executed_at ASC")
        .bind(&pair)
        .fetch_all(&matching_state().pool)
        .await
        .map_err(internal)?;
    let mut map: BTreeMap<i64, Vec<(f64, f64)>> = BTreeMap::new();
    for row in rows {
        let ts = row.get::<chrono::DateTime<chrono::Utc>,_>("executed_at").timestamp();
        let bucket = ts - (ts % bucket_secs);
        map.entry(bucket).or_default().push((row.get("price"), row.get("quantity")));
    }
    let candles = map.into_iter().map(|(bucket, items)| {
        let open = items.first().map(|i| i.0).unwrap_or_default();
        let close = items.last().map(|i| i.0).unwrap_or_default();
        let high = items.iter().map(|i| i.0).fold(f64::MIN, f64::max);
        let low = items.iter().map(|i| i.0).fold(f64::MAX, f64::min);
        let volume: f64 = items.iter().map(|i| i.1).sum();
        serde_json::json!({"time": bucket, "open": open, "high": high, "low": low, "close": close, "volume": volume})
    }).collect();
    Ok(Json(candles))
}

fn price_to_ticks(price: f64) -> i64 { (price * 1_000_000.0).round() as i64 }
fn ticks_to_price(ticks: i64) -> f64 { ticks as f64 / 1_000_000.0 }
fn matching_state() -> &'static Arc<MatchingState> { MATCHING_STATE.get().expect("matching state initialized") }
fn internal<E>(err: E) -> AppError where E: Into<anyhow::Error> { AppError::Internal(err.into()) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_order(side: Side, order_type: OrderType, price: f64, qty: f64) -> Order {
        Order {
            order_id: Uuid::now_v7(),
            user_id: "test_user".to_string(),
            pair: "BTC_USDT".to_string(),
            side,
            order_type,
            price,
            quantity: qty,
            ..Order::default()
        }
    }

    #[test]
    fn test_limit_buy_matches_limit_sell() {
        let mut book = OrderBook::default();
        book.submit(make_order(Side::Sell, OrderType::Limit, 50000.0, 1.0));
        let trades = book.submit(make_order(Side::Buy, OrderType::Limit, 50000.0, 1.0));
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].price, 50000.0);
        assert_eq!(trades[0].quantity, 1.0);
    }

    #[test]
    fn test_buy_price_below_ask_no_match() {
        let mut book = OrderBook::default();
        book.submit(make_order(Side::Sell, OrderType::Limit, 50000.0, 1.0));
        let trades = book.submit(make_order(Side::Buy, OrderType::Limit, 49000.0, 1.0));
        assert_eq!(trades.len(), 0);
    }

    #[test]
    fn test_partial_fill() {
        let mut book = OrderBook::default();
        book.submit(make_order(Side::Sell, OrderType::Limit, 50000.0, 2.0));
        let trades = book.submit(make_order(Side::Buy, OrderType::Limit, 50000.0, 1.0));
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].quantity, 1.0);
    }

    #[test]
    fn test_market_buy_drains_asks() {
        let mut book = OrderBook::default();
        book.submit(make_order(Side::Sell, OrderType::Limit, 50000.0, 0.5));
        book.submit(make_order(Side::Sell, OrderType::Limit, 50100.0, 0.5));
        let trades = book.submit(make_order(Side::Buy, OrderType::Market, 0.0, 1.0));
        assert_eq!(trades.len(), 2);
        let total: f64 = trades.iter().map(|t| t.quantity).sum();
        assert!((total - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_price_time_priority_fifo() {
        let mut book = OrderBook::default();
        let first_sell = make_order(Side::Sell, OrderType::Limit, 50000.0, 1.0);
        let first_id = first_sell.order_id;
        book.submit(first_sell);
        book.submit(make_order(Side::Sell, OrderType::Limit, 50000.0, 1.0));
        let trades = book.submit(make_order(Side::Buy, OrderType::Limit, 50000.0, 1.0));
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].seller_order_id, first_id);
    }

    #[test]
    fn test_market_sell_drains_bids() {
        let mut book = OrderBook::default();
        book.submit(make_order(Side::Buy, OrderType::Limit, 49000.0, 0.5));
        book.submit(make_order(Side::Buy, OrderType::Limit, 48000.0, 0.5));
        let trades = book.submit(make_order(Side::Sell, OrderType::Market, 0.0, 1.0));
        assert_eq!(trades.len(), 2);
    }

    #[test]
    fn test_fok_cancels_if_not_fully_fillable() {
        let mut book = OrderBook::default();
        book.submit(make_order(Side::Sell, OrderType::Limit, 50000.0, 0.5));
        let trades = book.submit(make_order(Side::Buy, OrderType::FOK, 50000.0, 1.0));
        assert_eq!(trades.len(), 0); // FOK cancelled because can't fill 1.0 (only 0.5 available)
    }

    #[test]
    fn test_ioc_fills_partial_cancels_rest() {
        let mut book = OrderBook::default();
        book.submit(make_order(Side::Sell, OrderType::Limit, 50000.0, 0.5));
        let trades = book.submit(make_order(Side::Buy, OrderType::IOC, 50000.0, 1.0));
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].quantity, 0.5);
    }

    #[test]
    fn test_post_only_rejected_if_would_match() {
        let mut book = OrderBook::default();
        book.submit(make_order(Side::Sell, OrderType::Limit, 50000.0, 1.0));
        let trades = book.submit(make_order(Side::Buy, OrderType::PostOnly, 50000.0, 1.0));
        // PostOnly should be rejected (no trades, order rejected)
        assert_eq!(trades.len(), 0);
    }

    #[test]
    fn test_depth_snapshot_structure() {
        let mut book = OrderBook::default();
        book.submit(make_order(Side::Buy, OrderType::Limit, 49000.0, 1.0));
        book.submit(make_order(Side::Buy, OrderType::Limit, 48500.0, 2.0));
        book.submit(make_order(Side::Sell, OrderType::Limit, 50000.0, 1.0));
        let snap = book.depth_snapshot(10);
        assert!(!snap.bids.is_empty());
        assert!(!snap.asks.is_empty());
        if snap.bids.len() > 1 {
            assert!(snap.bids[0].0 >= snap.bids[1].0);
        }
    }

    #[test]
    fn test_cancel_removes_from_book() {
        let mut book = OrderBook::default();
        let order = make_order(Side::Buy, OrderType::Limit, 49000.0, 1.0);
        let oid = order.order_id;
        book.submit(order);
        let snap_before = book.depth_snapshot(10);
        assert!(!snap_before.bids.is_empty());
        book.cancel(oid);
        let snap_after = book.depth_snapshot(10);
        assert!(snap_after.bids.is_empty());
    }
}
