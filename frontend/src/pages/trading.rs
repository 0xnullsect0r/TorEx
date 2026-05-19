use leptos::prelude::*;

use crate::components::{chart::Chart, nav::Nav, order_book::OrderBook, order_entry::OrderEntry};
use crate::core::{api, storage, ws};
use crate::core::types::{ChartCandle, Order, OrderBookSnapshot, OrderSide, PairStats, Trade, compute_pair_stats, short_id};

const PAIRS: [&str; 20] = [
    "BTC/USDT", "ETH/USDT", "BNB/USDT", "SOL/USDT", "XRP/USDT",
    "DOGE/USDT", "ADA/USDT", "AVAX/USDT", "DOT/USDT", "MATIC/USDT",
    "LTC/USDT", "SHIB/USDT", "TRX/USDT", "LINK/USDT", "UNI/USDT",
    "ATOM/USDT", "XLM/USDT", "BCH/USDT", "FIL/USDT", "APT/USDT",
];

#[component]
pub fn TradingPage() -> impl IntoView {
    let (pair, set_pair) = signal(PAIRS[0].to_string());
    let (timeframe, set_timeframe) = signal("1h".to_string());
    let (candles, set_candles) = signal(Vec::<ChartCandle>::new());
    let (pair_stats, set_pair_stats) = signal(PairStats::default());
    let (balance, set_balance) = signal(0.0_f64);
    let (open_orders, set_open_orders) = signal(Vec::<Order>::new());
    let (order_history, set_order_history) = signal(Vec::<Order>::new());
    let (book_fallback, set_book_fallback) = signal(OrderBookSnapshot::default());
    let (active_tab, set_active_tab) = signal("open_orders".to_string());
    let (history_page, set_history_page) = signal(0_usize);
    let (error, set_error) = signal(None::<String>);

    let ws_handle = Memo::new(move |_| ws::connect(pair.get()));
    let connected = Signal::derive(move || ws_handle.get().connected.get());
    let live_book = Signal::derive(move || {
        let snapshot = ws_handle.get().orderbook.get();
        if snapshot.bids.is_empty() && snapshot.asks.is_empty() {
            book_fallback.get()
        } else {
            snapshot
        }
    });
    let live_trades = Signal::derive(move || ws_handle.get().trades.get());
    let user_id = Signal::derive(storage::get_user_id);

    let refresh_market = Action::new_local(move |input: &(String, String)| {
        let (pair, timeframe) = input.clone();
        async move {
            let book_path = format!("/api/orderbook/{pair}");
            let candle_path = format!("/api/candles/{pair}?interval={timeframe}");

            if let Ok(value) = api::get_value(&book_path).await {
                let snapshot = serde_json::from_value::<OrderBookSnapshot>(value.clone())
                    .or_else(|_| serde_json::from_value(value["orderbook"].clone()))
                    .unwrap_or_default();
                set_book_fallback.set(snapshot);
            }

            match api::get_value(&candle_path).await {
                Ok(value) => {
                    let parsed = serde_json::from_value::<Vec<ChartCandle>>(value.clone())
                        .or_else(|_| serde_json::from_value(value["candles"].clone()))
                        .unwrap_or_default();
                    set_pair_stats.set(compute_pair_stats(&parsed));
                    set_candles.set(parsed);
                    set_error.set(None);
                }
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    let refresh_orders = Action::new_local(move |_: &()| async move {
        if let Ok(value) = api::get_value("/api/orders").await {
            let orders = serde_json::from_value::<Vec<Order>>(value.clone())
                .or_else(|_| serde_json::from_value(value["orders"].clone()))
                .unwrap_or_default();
            set_open_orders.set(orders);
        }

        let history_result = match api::get_value("/api/order-history").await {
            Ok(value) => Ok(value),
            Err(_) => api::get_value("/api/orders").await,
        };

        if let Ok(value) = history_result {
            let orders = serde_json::from_value::<Vec<Order>>(value.clone())
                .or_else(|_| serde_json::from_value(value["orders"].clone()))
                .unwrap_or_default();
            set_order_history.set(orders);
        }
    });

    let refresh_balance = Action::new_local(move |_: &()| async move {
        match api::get_value("/api/wallet/balance").await {
            Ok(value) => {
                let parsed = value["usdt"].as_f64().unwrap_or_else(|| {
                    value["usdt"].as_str().and_then(|text| text.parse().ok()).unwrap_or(0.0)
                });
                set_balance.set(parsed);
            }
            Err(message) => set_error.set(Some(message)),
        }
    });

    let cancel_order_action = Action::new_local(move |order_id: &String| {
        let order_id = order_id.clone();
        async move {
            match api::delete_value(&format!("/api/orders/{order_id}")).await {
                Ok(_) => {
                    refresh_orders.dispatch_local(());
                    set_error.set(None);
                }
                Err(message) => set_error.set(Some(message)),
            }
        }
    });
    let cancel_order = UnsyncCallback::new(move |order_id: String| {
        cancel_order_action.dispatch_local(order_id);
    });

    Effect::new(move |_| {
        refresh_market.dispatch_local((pair.get(), timeframe.get()));
        refresh_orders.dispatch_local(());
        refresh_balance.dispatch_local(());
    });

    let on_placed = UnsyncCallback::new(move |_: ()| {
        refresh_orders.dispatch_local(());
        refresh_balance.dispatch_local(());
    });

    view! {
        <div class="shell">
            <Nav connected=connected show_admin=Signal::derive(|| storage::get_admin_session().is_some()) user_id=user_id />
            <div class="page-wide trading-page">
                <div class="market-toolbar">
                    <section class="panel">
                        <div class="panel-body form-grid">
                            <div>
                                <label class="label">"Trading Pair"</label>
                                <select class="pair-select" prop:value=move || pair.get() on:change=move |event| set_pair.set(event_target_value(&event))>
                                    {PAIRS.into_iter().map(|market| view! { <option value=market>{market}</option> }).collect::<Vec<_>>()}
                                </select>
                            </div>
                            <div class="inline-stat mono">
                                <span class="text-muted">"Current timeframe"</span>
                                <span>{move || timeframe.get()}</span>
                            </div>
                        </div>
                    </section>

                    <div class="stats-bar">
                        <div class="stat-chip"><div class="stat-label">"Last Price"</div><div class="stat-value mono">{move || format!("{:.4}", pair_stats.get().last_price)}</div></div>
                        <div class="stat-chip"><div class="stat-label">"24h Change"</div><div class=move || { if pair_stats.get().change_percent >= 0.0 { "stat-value mono text-buy" } else { "stat-value mono text-sell" } }>{move || format!("{:+.2}%", pair_stats.get().change_percent)}</div></div>
                        <div class="stat-chip"><div class="stat-label">"24h High"</div><div class="stat-value mono">{move || format!("{:.4}", pair_stats.get().high_24h)}</div></div>
                        <div class="stat-chip"><div class="stat-label">"24h Low"</div><div class="stat-value mono">{move || format!("{:.4}", pair_stats.get().low_24h)}</div></div>
                        <div class="stat-chip"><div class="stat-label">"24h Volume"</div><div class="stat-value mono">{move || format!("{:.2}", pair_stats.get().volume_24h)}</div><div class="stat-subvalue">"Base asset volume"</div></div>
                    </div>
                </div>

                <div class="trading-grid">
                    <section class="panel left-stack"><OrderBook snapshot=live_book trades=live_trades /></section>
                    <section class="panel chart-panel"><Chart candles=Signal::derive(move || candles.get()) timeframe=timeframe set_timeframe=set_timeframe orderbook=live_book /></section>
                    <section class="panel right-stack"><OrderEntry pair=pair available_balance=Signal::derive(move || balance.get()) last_price=Signal::derive(move || pair_stats.get().last_price) on_placed=on_placed /></section>
                </div>

                <section class="panel bottom-panel">
                    <div class="panel-header">
                        <div class="bottom-tabs">
                            <button class=move || if active_tab.get() == "open_orders" { "tab-button active" } else { "tab-button" } on:click=move |_| set_active_tab.set("open_orders".to_string())>"Open Orders"</button>
                            <button class=move || if active_tab.get() == "trade_history" { "tab-button active" } else { "tab-button" } on:click=move |_| set_active_tab.set("trade_history".to_string())>"Trade History"</button>
                            <button class=move || if active_tab.get() == "order_history" { "tab-button active" } else { "tab-button" } on:click=move |_| set_active_tab.set("order_history".to_string())>"Order History"</button>
                            <button class=move || if active_tab.get() == "positions" { "tab-button active" } else { "tab-button" } on:click=move |_| set_active_tab.set("positions".to_string())>"Positions"</button>
                        </div>
                        <button class="btn-ghost btn-small" on:click=move |_| { refresh_orders.dispatch_local(()); }>"Refresh"</button>
                    </div>
                    <div class="panel-body">
                        {move || match active_tab.get().as_str() {
                            "open_orders" => render_open_orders(open_orders.get(), cancel_order),
                            "trade_history" => render_trade_history(live_trades.get()),
                            "order_history" => render_order_history(order_history.get(), history_page, set_history_page),
                            _ => view! { <div class="empty-state"><div><div class="panel-title">"Spot Market"</div><div class="text-muted">"No leveraged positions. Exposure is tracked through balances and order history."</div></div></div> }.into_any(),
                        }}
                    </div>
                </section>

                {move || error.get().map(|message| view! { <div class="form-error">{message}</div> })}
            </div>
        </div>
    }
}

fn render_open_orders(orders: Vec<Order>, cancel_order: UnsyncCallback<String>) -> AnyView {
    if orders.is_empty() {
        return view! { <div class="empty-state">"No open orders."</div> }.into_any();
    }

    view! {
        <table class="data-table mono">
            <thead><tr><th>"ID"</th><th>"Pair"</th><th>"Side"</th><th>"Type"</th><th>"Price"</th><th>"Amount"</th><th>"Filled"</th><th>"Status"</th><th></th></tr></thead>
            <tbody>
                {orders.into_iter().map(|order| {
                    let cancel_id = order.order_id.clone();
                    view! {
                        <tr>
                            <td>{short_id(&order.order_id)}</td>
                            <td>{order.pair}</td>
                            <td class=if order.side == OrderSide::Buy { "text-buy" } else { "text-sell" }>{order.side.label()}</td>
                            <td>{order.order_type.label()}</td>
                            <td>{format!("{:.4}", order.price)}</td>
                            <td>{format!("{:.6}", order.amount)}</td>
                            <td>{format!("{:.6}", order.filled)}</td>
                            <td>{order.status}</td>
                            <td><button class="btn-danger btn-small" on:click=move |_| cancel_order.run(cancel_id.clone())>"Cancel"</button></td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }.into_any()
}

fn render_trade_history(trades: Vec<Trade>) -> AnyView {
    if trades.is_empty() {
        return view! { <div class="empty-state">"No trade history yet."</div> }.into_any();
    }

    view! {
        <table class="data-table mono">
            <thead><tr><th>"Time"</th><th>"Price"</th><th>"Size"</th><th>"Side"</th></tr></thead>
            <tbody>
                {trades.into_iter().take(50).map(|trade| {
                    let side = trade.side.unwrap_or(OrderSide::Buy);
                    view! {
                        <tr>
                            <td>{trade.executed_at}</td>
                            <td>{format!("{:.4}", trade.price)}</td>
                            <td>{format!("{:.6}", trade.quantity)}</td>
                            <td class=if side == OrderSide::Buy { "text-buy" } else { "text-sell" }>{side.label()}</td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }.into_any()
}

fn render_order_history(orders: Vec<Order>, history_page: ReadSignal<usize>, set_history_page: WriteSignal<usize>) -> AnyView {
    if orders.is_empty() {
        return view! { <div class="empty-state">"No historical orders available."</div> }.into_any();
    }

    let page_size = 10;
    let current_page = history_page.get();
    let total_pages = (orders.len() + page_size - 1) / page_size;
    let start = current_page * page_size;
    let end = (start + page_size).min(orders.len());
    let page_rows = orders[start..end].to_vec();

    view! {
        <>
            <table class="data-table mono">
                <thead><tr><th>"ID"</th><th>"Pair"</th><th>"Side"</th><th>"Type"</th><th>"Price"</th><th>"Amount"</th><th>"Status"</th><th>"Created"</th></tr></thead>
                <tbody>
                    {page_rows.into_iter().map(|order| view! {
                        <tr>
                            <td>{short_id(&order.order_id)}</td>
                            <td>{order.pair}</td>
                            <td class=if order.side == OrderSide::Buy { "text-buy" } else { "text-sell" }>{order.side.label()}</td>
                            <td>{order.order_type.label()}</td>
                            <td>{format!("{:.4}", order.price)}</td>
                            <td>{format!("{:.6}", order.amount)}</td>
                            <td>{order.status}</td>
                            <td>{order.created_at}</td>
                        </tr>
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
            <div class="pagination">
                <button class="btn-ghost btn-small" disabled=current_page == 0 on:click=move |_| set_history_page.update(|page| *page = page.saturating_sub(1))>"Previous"</button>
                <span class="text-muted">{format!("Page {} of {}", current_page + 1, total_pages.max(1))}</span>
                <button class="btn-ghost btn-small" disabled=current_page + 1 >= total_pages on:click=move |_| set_history_page.update(|page| *page = (*page + 1).min(total_pages.saturating_sub(1)))>"Next"</button>
            </div>
        </>
    }.into_any()
}
