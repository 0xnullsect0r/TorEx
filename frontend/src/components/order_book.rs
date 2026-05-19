use leptos::prelude::*;

use crate::core::types::{OrderBookLevel, OrderBookSnapshot, OrderSide, Trade};

#[component]
pub fn OrderBook(
    #[prop(into)] snapshot: Signal<OrderBookSnapshot>,
    #[prop(into)] trades: Signal<Vec<Trade>>,
) -> impl IntoView {
    view! {
        <div class="orderbook-grid">
            <div class="panel-header">
                <div>
                    <div class="panel-title">"Order Book"</div>
                    <div class="text-muted">"Top 20 levels with cumulative depth"</div>
                </div>
            </div>

            <div style="padding: 0 18px;">
                <table class="orderbook-table mono">
                    <thead><tr><th style="text-align:left;">"Price"</th><th>"Size"</th><th>"Total"</th></tr></thead>
                    <tbody>
                        {move || {
                            let book = snapshot.get();
                            if book.asks.is_empty() && book.bids.is_empty() {
                                return view! { <tr><td colspan="3" style="text-align:center;padding:28px 12px;" class="text-muted">"Waiting for order book…"</td></tr> }.into_any();
                            }
                            let asks = depth_rows(book.asks.into_iter().take(20).collect::<Vec<_>>(), OrderSide::Sell).into_iter().rev().collect::<Vec<_>>();
                            view! { <>{asks.into_iter().map(render_depth_row).collect::<Vec<_>>()}</> }.into_any()
                        }}
                    </tbody>
                </table>
            </div>

            {move || {
                let book = snapshot.get();
                let spread = match (book.asks.first(), book.bids.first()) { (Some(ask), Some(bid)) => ask.price - bid.price, _ => 0.0 };
                let mid = match (book.asks.first(), book.bids.first()) { (Some(ask), Some(bid)) => (ask.price + bid.price) / 2.0, _ => 0.0 };
                view! { <div class="spread-strip mono"><span class="text-muted">"Spread"</span><strong>{format!("{spread:.4}")}</strong><span class="text-muted">{format!("Mid {mid:.4}")}</span></div> }
            }}

            <div>
                <div style="padding: 0 18px 8px;">
                    <table class="orderbook-table mono">
                        <tbody>
                            {move || {
                                let book = snapshot.get();
                                let bids = depth_rows(book.bids.into_iter().take(20).collect::<Vec<_>>(), OrderSide::Buy);
                                if bids.is_empty() {
                                    return view! { <tr><td colspan="3" style="text-align:center;padding:16px 12px;" class="text-muted">"No bid liquidity"</td></tr> }.into_any();
                                }
                                view! { <>{bids.into_iter().map(render_depth_row).collect::<Vec<_>>()}</> }.into_any()
                            }}
                        </tbody>
                    </table>
                </div>
                <div class="recent-trades">
                    <div class="section-header" style="padding: 0 0 12px; border-bottom: none;"><div class="section-title">"Recent Trades"</div><div class="text-muted">"Last 20"</div></div>
                    <div class="recent-trades-list mono">
                        {move || {
                            let rows = trades.get().into_iter().take(20).collect::<Vec<_>>();
                            if rows.is_empty() {
                                return view! { <div class="empty-state" style="min-height: 140px;">"No recent trades yet."</div> }.into_any();
                            }
                            view! {
                                <>{rows.into_iter().map(|trade| {
                                    let class_name = match trade.side.unwrap_or(OrderSide::Buy) { OrderSide::Buy => "text-buy", OrderSide::Sell => "text-sell" };
                                    view! { <div class="trade-row"><span class=class_name>{format!("{:.4}", trade.price)}</span><span>{format!("{:.6}", trade.quantity)}</span><span class="text-muted">{trade.executed_at}</span></div> }
                                }).collect::<Vec<_>>()}</>
                            }.into_any()
                        }}
                    </div>
                </div>
            </div>
        </div>
    }
}

#[derive(Clone)]
struct DepthRow { side: OrderSide, price: f64, size: f64, total: f64, width_percent: f64 }

fn depth_rows(levels: Vec<OrderBookLevel>, side: OrderSide) -> Vec<DepthRow> {
    let mut cumulative = 0.0;
    let mut totals = Vec::with_capacity(levels.len());
    for level in &levels { cumulative += level.quantity; totals.push(cumulative); }
    let max_total = totals.iter().copied().fold(0.0_f64, f64::max).max(1.0);
    levels.into_iter().zip(totals).map(|(level, total)| DepthRow { side, price: level.price, size: level.quantity, total, width_percent: (total / max_total) * 100.0 }).collect()
}

fn render_depth_row(row: DepthRow) -> impl IntoView {
    let background = match row.side { OrderSide::Buy => "var(--depth-bid)", OrderSide::Sell => "var(--depth-ask)" };
    let price_class = match row.side { OrderSide::Buy => "text-buy", OrderSide::Sell => "text-sell" };
    view! {
        <tr>
            <td colspan="3" style="padding:0;border-bottom:none;">
                <div style="position:relative; display:grid; grid-template-columns: 1fr 1fr 1fr; align-items:center; padding:8px 12px;">
                    <div class="orderbook-row-bg" style=format!("width: {:.2}%; background: {};", row.width_percent, background)></div>
                    <span class=price_class style="position:relative; z-index:1; text-align:left;">{format!("{:.4}", row.price)}</span>
                    <span style="position:relative; z-index:1; text-align:right;">{format!("{:.6}", row.size)}</span>
                    <span style="position:relative; z-index:1; text-align:right; color: var(--text-muted);">{format!("{:.6}", row.total)}</span>
                </div>
            </td>
        </tr>
    }
}
