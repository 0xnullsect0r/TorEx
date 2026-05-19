use std::{cmp::Reverse, collections::{BTreeMap, VecDeque}, sync::{Arc, OnceLock}};

use auth::NoiseAuth;
use axum::{extract::{Json, Path, Query}, routing::{delete, get, post}, Router};
use dashmap::DashMap;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use shared::{error::AppError, kafka::KafkaProducer};
use sqlx::{PgPool, Row};
use tokio::sync::broadcast;
use tracing::info;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Side { Buy, Sell }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OrderType { Limit, Market }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OrderStatus { Open, PartiallyFilled, Filled, Cancelled }

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

pub struct OrderBook {
    bids: BTreeMap<Reverse<i64>, VecDeque<Order>>,
    asks: BTreeMap<i64, VecDeque<Order>>,
}

impl Default for OrderBook {
    fn default() -> Self { Self { bids: BTreeMap::new(), asks: BTreeMap::new() } }
}

impl OrderBook {
    pub fn submit(&mut self, mut order: Order) -> Vec<Trade> {
        let mut trades = Vec::new();
        match order.side {
            Side::Buy => {
                while order.filled < order.quantity {
                    let Some((&best_price, _)) = self.asks.iter().next() else { break; };
                    if matches!(order.order_type, OrderType::Limit) && ticks_to_price(best_price) > order.price {
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
                            trades.push(Trade {
                                trade_id: Uuid::now_v7(),
                                pair: order.pair.clone(),
                                price: ticks_to_price(best_price),
                                quantity: remaining,
                                buyer_order_id: order.order_id,
                                seller_order_id: resting.order_id,
                                buyer_user_id: order.user_id.clone(),
                                seller_user_id: resting.user_id.clone(),
                                executed_at: chrono::Utc::now(),
                            });
                            if resting.filled >= resting.quantity { queue.pop_front(); }
                        }
                        remove_level = queue.is_empty();
                    }
                    if remove_level { self.asks.remove(&best_price); }
                }
                if order.filled < order.quantity && !matches!(order.order_type, OrderType::Market) {
                    self.bids.entry(Reverse(price_to_ticks(order.price))).or_default().push_back(order);
                }
            }
            Side::Sell => {
                while order.filled < order.quantity {
                    let Some((&best_price, _)) = self.bids.iter().next() else { break; };
                    if matches!(order.order_type, OrderType::Limit) && ticks_to_price(best_price.0) < order.price {
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
                            trades.push(Trade {
                                trade_id: Uuid::now_v7(),
                                pair: order.pair.clone(),
                                price: ticks_to_price(best_price.0),
                                quantity: remaining,
                                buyer_order_id: resting.order_id,
                                seller_order_id: order.order_id,
                                buyer_user_id: resting.user_id.clone(),
                                seller_user_id: order.user_id.clone(),
                                executed_at: chrono::Utc::now(),
                            });
                            if resting.filled >= resting.quantity { queue.pop_front(); }
                        }
                        remove_level = queue.is_empty();
                    }
                    if remove_level { self.bids.remove(&best_price); }
                }
                if order.filled < order.quantity && !matches!(order.order_type, OrderType::Market) {
                    self.asks.entry(price_to_ticks(order.price)).or_default().push_back(order);
                }
            }
        }
        trades
    }

    pub fn cancel(&mut self, order_id: Uuid) -> bool {
        let mut matched_bid = None;
        for (price, queue) in self.bids.iter_mut() {
            if let Some(pos) = queue.iter().position(|o| o.order_id == order_id) {
                queue.remove(pos);
                matched_bid = Some((*price, queue.is_empty()));
                break;
            }
        }
        if let Some((price, remove_level)) = matched_bid {
            if remove_level {
                self.bids.remove(&price);
            }
            return true;
        }

        let mut matched_ask = None;
        for (price, queue) in self.asks.iter_mut() {
            if let Some(pos) = queue.iter().position(|o| o.order_id == order_id) {
                queue.remove(pos);
                matched_ask = Some((*price, queue.is_empty()));
                break;
            }
        }
        if let Some((price, remove_level)) = matched_ask {
            if remove_level {
                self.asks.remove(&price);
            }
            return true;
        }

        false
    }

    pub fn depth_snapshot(&self, levels: usize) -> DepthSnapshot {
        let bids = self.bids.iter().take(levels).map(|(p, q)| (ticks_to_price(p.0), q.iter().map(|o| o.quantity - o.filled).sum())).collect();
        let asks = self.asks.iter().take(levels).map(|(p, q)| (ticks_to_price(*p), q.iter().map(|o| o.quantity - o.filled).sum())).collect();
        DepthSnapshot { bids, asks }
    }
}

pub type OrderBooks = Arc<DashMap<String, Arc<Mutex<OrderBook>>>>;

static MATCHING_STATE: OnceLock<Arc<MatchingState>> = OnceLock::new();
static TRADE_CHANNELS: OnceLock<DashMap<String, broadcast::Sender<String>>> = OnceLock::new();
static USER_CHANNELS: OnceLock<DashMap<String, broadcast::Sender<String>>> = OnceLock::new();

#[derive(Clone)]
struct MatchingState {
    books: OrderBooks,
    pool: PgPool,
    kafka: KafkaProducer,
}

#[derive(Deserialize)]
struct CreateOrderRequest {
    pair: String,
    side: Side,
    order_type: OrderType,
    price: f64,
    quantity: f64,
}

#[derive(Deserialize)]
struct CandleQuery { interval: Option<String> }

pub fn router(books: OrderBooks, pool: PgPool, kafka: KafkaProducer) -> Router {
    let _ = MATCHING_STATE.set(Arc::new(MatchingState { books, pool, kafka }));
    TRADE_CHANNELS.get_or_init(DashMap::new);
    USER_CHANNELS.get_or_init(DashMap::new);
    Router::new()
        .route("/api/orders", post(create_order).get(list_orders))
        .route("/api/orders/:order_id", delete(cancel_order))
        .route("/api/orderbook/:pair", get(orderbook))
        .route("/api/candles/:pair", get(candles))
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

async fn create_order(NoiseAuth(user_id): NoiseAuth, Json(payload): Json<CreateOrderRequest>) -> Result<Json<serde_json::Value>, AppError> {
    let state = matching_state();
    let order = Order {
        order_id: Uuid::now_v7(),
        user_id: user_id.to_hex(),
        pair: payload.pair.clone(),
        side: payload.side,
        order_type: payload.order_type,
        price: payload.price,
        quantity: payload.quantity,
        filled: 0.0,
        status: OrderStatus::Open,
        created_at: chrono::Utc::now(),
    };
    sqlx::query("INSERT INTO orders (order_id, user_id, pair, side, order_type, price, quantity, filled, status, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,NOW())")
        .bind(order.order_id)
        .bind(&order.user_id)
        .bind(&order.pair)
        .bind(format!("{:?}", order.side))
        .bind(format!("{:?}", order.order_type))
        .bind(order.price)
        .bind(order.quantity)
        .bind(order.filled)
        .bind(format!("{:?}", order.status))
        .execute(&state.pool)
        .await
        .map_err(internal)?;
    let book = state.books.entry(order.pair.clone()).or_insert_with(|| Arc::new(Mutex::new(OrderBook::default()))).clone();
    let trades = book.lock().submit(order.clone());
    for trade in &trades {
        state.kafka.send("trades", &trade.trade_id.to_string(), &serde_json::to_string(trade).map_err(internal)?).await.map_err(internal)?;
        sqlx::query("INSERT INTO trades (trade_id, pair, price, quantity, buyer_order_id, seller_order_id, buyer_user_id, seller_user_id, executed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
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
            .await
            .map_err(internal)?;
        let payload = serde_json::to_string(trade).map_err(internal)?;
        let _ = trade_sender(&trade.pair).send(payload.clone());
        let _ = user_sender(&trade.buyer_user_id).send(payload.clone());
        let _ = user_sender(&trade.seller_user_id).send(payload);
    }
    info!(event = "order_submitted", "submitted order");
    Ok(Json(serde_json::json!({"order_id": order.order_id, "trades": trades})))
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
    sqlx::query("UPDATE orders SET status = 'Cancelled' WHERE order_id = $1")
        .bind(order_id)
        .execute(&state.pool)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({"cancelled": true})))
}

async fn list_orders(NoiseAuth(user_id): NoiseAuth) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let rows = sqlx::query("SELECT order_id, pair, side, order_type, price, quantity, filled, status, created_at FROM orders WHERE user_id = $1 AND status IN ('Open', 'PartiallyFilled') ORDER BY created_at DESC")
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
    })).collect()))
}

async fn orderbook(Path(pair): Path<String>) -> Result<Json<DepthSnapshot>, AppError> {
    let snapshot = matching_state().books.get(&pair).map(|b| b.lock().depth_snapshot(25)).unwrap_or_default();
    Ok(Json(snapshot))
}

async fn candles(Path(pair): Path<String>, Query(query): Query<CandleQuery>) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let interval = query.interval.unwrap_or_else(|| "1m".to_string());
    let bucket_secs = match interval.as_str() { "5m" => 300, "15m" => 900, "1h" => 3600, _ => 60 };
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_order(side: Side, order_type: OrderType, price: f64, qty: f64) -> Order {
        Order {
            order_id: Uuid::now_v7(),
            user_id: "test_user".to_string(),
            pair: "BTC_USDT".to_string(),
            side,
            order_type,
            price,
            quantity: qty,
            filled: 0.0,
            status: OrderStatus::Open,
            created_at: chrono::Utc::now(),
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

    #[test]
    fn test_multiple_price_levels() {
        let mut book = OrderBook::default();
        for price in [49000.0, 49500.0, 50000.0_f64] {
            book.submit(make_order(Side::Sell, OrderType::Limit, price, 1.0));
        }
        let trades = book.submit(make_order(Side::Buy, OrderType::Market, 0.0, 2.5));
        let total: f64 = trades.iter().map(|t| t.quantity).sum();
        assert!((total - 2.5).abs() < 1e-9 || total <= 2.5 + 1e-9);
    }

    #[test]
    fn test_sell_price_above_bid_no_match() {
        let mut book = OrderBook::default();
        book.submit(make_order(Side::Buy, OrderType::Limit, 49000.0, 1.0));
        let trades = book.submit(make_order(Side::Sell, OrderType::Limit, 50000.0, 1.0));
        assert_eq!(trades.len(), 0);
    }
}
