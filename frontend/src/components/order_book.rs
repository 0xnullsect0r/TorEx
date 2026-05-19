use leptos::*;
use crate::core::types::OrderBookSnapshot;

#[component]
pub fn OrderBook(snapshot: Signal<OrderBookSnapshot>) -> impl IntoView {
    view! {
        <div class="orderbook">
            {move || {
                let book = snapshot.get();
                if book.bids.is_empty() && book.asks.is_empty() {
                    return view! {
                        <div class="text-muted" style="text-align:center;padding:16px;">"Connecting…"</div>
                    }.into_view();
                }

                let max_qty: f64 = book.asks.iter().chain(book.bids.iter())
                    .map(|l| l.quantity)
                    .fold(0.0_f64, f64::max)
                    .max(1.0);

                let asks: Vec<_> = book.asks.iter().rev().take(15).cloned().collect();
                let bids: Vec<_> = book.bids.iter().take(15).cloned().collect();

                let spread = if let (Some(a), Some(b)) = (book.asks.first(), book.bids.first()) {
                    format!("{:.4}", a.price - b.price)
                } else {
                    "—".into()
                };

                view! {
                    <div>
                        // Asks (sell orders) — red
                        <div class="ob-header">
                            <span>"Price"</span><span>"Qty"</span><span>"Total"</span>
                        </div>
                        {asks.into_iter().map(|lvl| {
                            let pct = lvl.quantity / max_qty * 100.0;
                            let total = lvl.price * lvl.quantity;
                            view! {
                                <div class="ob-row ob-ask"
                                    style=format!("background: linear-gradient(to left, rgba(255,23,68,.18) {pct:.1}%, transparent 0)")>
                                    <span class="text-sell">{format!("{:.4}", lvl.price)}</span>
                                    <span>{format!("{:.6}", lvl.quantity)}</span>
                                    <span class="text-muted">{format!("{:.2}", total)}</span>
                                </div>
                            }
                        }).collect::<Vec<_>>()}

                        // Spread row
                        <div class="ob-spread">
                            <span class="text-muted" style="font-size:11px;">"Spread: "</span>
                            <span>{spread}</span>
                        </div>

                        // Bids (buy orders) — green
                        {bids.into_iter().map(|lvl| {
                            let pct = lvl.quantity / max_qty * 100.0;
                            let total = lvl.price * lvl.quantity;
                            view! {
                                <div class="ob-row ob-bid"
                                    style=format!("background: linear-gradient(to left, rgba(0,200,83,.18) {pct:.1}%, transparent 0)")>
                                    <span class="text-buy">{format!("{:.4}", lvl.price)}</span>
                                    <span>{format!("{:.6}", lvl.quantity)}</span>
                                    <span class="text-muted">{format!("{:.2}", total)}</span>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_view()
            }}
        </div>
    }
}
