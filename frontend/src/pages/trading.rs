use leptos::*;

use crate::components::{chart::Chart, nav::Nav, order_book::OrderBook, order_entry::OrderEntry};
use crate::core::{api, types::Order, ws};

// Top-200 USDT pairs (first 20 shown; user can scroll)
const PAIRS: &[&str] = &[
    "BTC/USDT", "ETH/USDT", "BNB/USDT", "SOL/USDT", "XRP/USDT",
    "DOGE/USDT", "ADA/USDT", "AVAX/USDT", "DOT/USDT", "MATIC/USDT",
    "LTC/USDT", "SHIB/USDT", "TRX/USDT", "LINK/USDT", "UNI/USDT",
    "ATOM/USDT", "XLM/USDT", "BCH/USDT", "FIL/USDT", "APT/USDT",
];

#[component]
pub fn Trading() -> impl IntoView {
    let (pair, set_pair) = create_signal(PAIRS[0].to_string());
    let (open_orders, set_open_orders) = create_signal(Vec::<Order>::new());

    // WebSocket handle — reconnects when pair changes
    let initial_handle = ws::connect(pair.get());
    let (ws_ob, set_ws_ob) = create_signal(initial_handle.orderbook);
    let (ws_trades, set_ws_trades) = create_signal(initial_handle.trades);
    create_effect(move |prev_pair: Option<String>| {
        let p = pair.get();
        if prev_pair.as_deref() == Some(&p) { return p; }
        let h = ws::connect(p.clone());
        set_ws_ob.set(h.orderbook);
        set_ws_trades.set(h.trades);
        p
    });

    // Load open orders
    let fetch_orders = create_action(move |_: &()| async move {
        match api::get("/api/orders").await {
            Ok(v) => {
                if let Ok(orders) =
                    serde_json::from_value::<Vec<Order>>(v["orders"].clone())
                {
                    set_open_orders.set(orders);
                }
            }
            Err(_) => {}
        }
    });
    fetch_orders.dispatch(());

    // Cancel order
    let cancel = create_action(move |id: &String| {
        let id = id.clone();
        let fetch = fetch_orders.clone();
        async move {
            let _ = api::delete(&format!("/api/orders/{id}")).await;
            fetch.dispatch(());
        }
    });

    view! {
        <Nav pair=pair set_pair=set_pair pairs=PAIRS/>
        <div class="trading-grid" style="padding:8px;">
            // Order book column
            <div class="orderbook-col card">
                <div class="card-title">"Order Book"</div>
                <OrderBook snapshot=Signal::derive(move || ws_ob.get().get())/>
            </div>

            // Chart
            <div class="chart-area card">
                <Chart pair=pair/>
            </div>

            // Order entry
            <div class="entry-panel card">
                <OrderEntry pair=pair on_placed=Callback::new(move |_| fetch_orders.dispatch(()))/>
            </div>

            // Open orders
            <div class="orders-area card" style="overflow:auto;">
                <div class="card-title">"Open Orders"</div>
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>"Pair"</th><th>"Side"</th><th>"Type"</th>
                            <th>"Price"</th><th>"Qty"</th><th>"Filled"</th><th></th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || open_orders.get()
                            key=|o| o.order_id.clone()
                            children=move |o| {
                                let oid = o.order_id.clone();
                                view! {
                                    <tr>
                                        <td>{o.pair}</td>
                                        <td class=if o.side=="buy" {"text-buy"} else {"text-sell"}>
                                            {o.side.to_uppercase()}
                                        </td>
                                        <td>{o.order_type}</td>
                                        <td>{format!("{:.4}", o.price)}</td>
                                        <td>{format!("{:.6}", o.quantity)}</td>
                                        <td>{format!("{:.6}", o.filled)}</td>
                                        <td>
                                            <button
                                                style="background:none;border:none;color:var(--sell);cursor:pointer;"
                                                on:click=move |_| cancel.dispatch(oid.clone())
                                            >"×"</button>
                                        </td>
                                    </tr>
                                }
                            }
                        />
                    </tbody>
                </table>

                // Recent trades feed
                <div class="card-title mt-12">"Recent Trades"</div>
                <table class="data-table">
                    <thead>
                        <tr><th>"Price"</th><th>"Qty"</th><th>"Time"</th></tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || ws_trades.get().get()
                            key=|t| t.trade_id.clone()
                            children=move |t| view! {
                                <tr>
                                    <td>{format!("{:.4}", t.price)}</td>
                                    <td>{format!("{:.6}", t.quantity)}</td>
                                    <td style="color:var(--muted);font-size:11px;">
                                        {t.executed_at.chars().take(19).collect::<String>()}
                                    </td>
                                </tr>
                            }
                        />
                    </tbody>
                </table>
            </div>
        </div>
    }
}
