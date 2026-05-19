use leptos::prelude::*;

use crate::core::types::{ChartCandle, OrderBookSnapshot};

const TIMEFRAMES: [&str; 7] = ["1m", "5m", "15m", "1h", "4h", "1d", "1w"];

#[component]
pub fn Chart(
    #[prop(into)] candles: Signal<Vec<ChartCandle>>,
    timeframe: ReadSignal<String>,
    set_timeframe: WriteSignal<String>,
    #[prop(optional, into)] orderbook: Option<Signal<OrderBookSnapshot>>,
) -> impl IntoView {
    let (active_tab, set_active_tab) = signal("candles".to_string());

    view! {
        <div class="chart-shell">
            <div class="chart-meta">
                <div>
                    <div class="panel-title">"Market Chart"</div>
                    <div class="text-muted">"Candlesticks, MA20 overlay, volume, and depth."</div>
                </div>
                <div class="chart-toolbar">
                    <button class=move || if active_tab.get() == "candles" { "tab-button active" } else { "tab-button" } on:click=move |_| set_active_tab.set("candles".to_string())>"Candles"</button>
                    <button class=move || if active_tab.get() == "depth" { "tab-button active" } else { "tab-button" } on:click=move |_| set_active_tab.set("depth".to_string())>"Depth"</button>
                </div>
            </div>

            <div class="tab-row" style="margin-bottom: 14px;">
                {TIMEFRAMES.into_iter().map(|candidate| {
                    let active_value = candidate.to_string();
                    let click_value = active_value.clone();
                    view! {
                        <button class=move || if timeframe.get() == active_value { "timeframe-button active" } else { "timeframe-button" } on:click=move |_| set_timeframe.set(click_value.clone())>{candidate}</button>
                    }
                }).collect::<Vec<_>>()}
            </div>

            {move || {
                if active_tab.get() == "depth" {
                    let snapshot = orderbook.map(|signal| signal.get()).unwrap_or_default();
                    if snapshot.bids.is_empty() && snapshot.asks.is_empty() {
                        return view! { <div class="chart-placeholder"><div><div class="panel-title">"No depth data"</div><div class="text-muted">"Depth data will appear once the market stream is active."</div></div></div> }.into_any();
                    }

                    let width = 900.0;
                    let height = 520.0;
                    let left = 32.0;
                    let right = 76.0;
                    let top = 24.0;
                    let bottom = 40.0;
                    let plot_width = width - left - right;
                    let plot_height = height - top - bottom;
                    let asks = snapshot.asks.into_iter().take(20).collect::<Vec<_>>();
                    let bids = snapshot.bids.into_iter().take(20).collect::<Vec<_>>();
                    let mut bid_points = Vec::new();
                    let mut ask_points = Vec::new();
                    let mut bid_total = 0.0;
                    let mut ask_total = 0.0;
                    let max_total = bids.iter().map(|level| level.quantity).sum::<f64>().max(asks.iter().map(|level| level.quantity).sum::<f64>()).max(1.0);
                    for (index, level) in bids.iter().enumerate() {
                        bid_total += level.quantity;
                        let x = left + plot_width * (index as f64 / (bids.len().saturating_sub(1).max(1) as f64));
                        let y = top + plot_height - ((bid_total / max_total) * plot_height);
                        bid_points.push(format!("{x:.2},{y:.2}"));
                    }
                    for (index, level) in asks.iter().enumerate() {
                        ask_total += level.quantity;
                        let x = left + plot_width * (index as f64 / (asks.len().saturating_sub(1).max(1) as f64));
                        let y = top + plot_height - ((ask_total / max_total) * plot_height);
                        ask_points.push(format!("{x:.2},{y:.2}"));
                    }
                    let bid_path = if bid_points.is_empty() { String::new() } else { format!("M {left:.2},{end:.2} L {} L {plot_end:.2},{end:.2} Z", bid_points.join(" L "), plot_end = left + plot_width, end = top + plot_height) };
                    let ask_path = if ask_points.is_empty() { String::new() } else { format!("M {left:.2},{end:.2} L {} L {plot_end:.2},{end:.2} Z", ask_points.join(" L "), plot_end = left + plot_width, end = top + plot_height) };
                    return view! {
                        <svg class="chart-svg" viewBox="0 0 900 520" preserveAspectRatio="none">
                            <line x1=left y1=top x2=left y2={top + plot_height} stroke="var(--chart-grid)" stroke-width="1"></line>
                            <line x1=left y1={top + plot_height} x2={left + plot_width} y2={top + plot_height} stroke="var(--chart-grid)" stroke-width="1"></line>
                            <path d=bid_path fill="var(--depth-bid)" stroke="var(--buy)" stroke-width="2"></path>
                            <path d=ask_path fill="var(--depth-ask)" stroke="var(--sell)" stroke-width="2"></path>
                            <text x={left + 4.0} y={top + 18.0} fill="var(--buy)" font-size="12">"Bid depth"</text>
                            <text x={left + 92.0} y={top + 18.0} fill="var(--sell)" font-size="12">"Ask depth"</text>
                            <text x={left + plot_width + 8.0} y={top + 14.0} fill="var(--text-muted)" font-size="11">"Depth"</text>
                        </svg>
                    }.into_any();
                }

                let data = candles.get();
                if data.is_empty() {
                    return view! { <div class="chart-placeholder"><div><div class="panel-title">"No data"</div><div class="text-muted">"No candle data is available for this market and timeframe."</div></div></div> }.into_any();
                }

                let width = 900.0;
                let height = 520.0;
                let left = 32.0;
                let right = 78.0;
                let top = 18.0;
                let bottom = 42.0;
                let volume_height = 92.0;
                let price_height = height - top - bottom - volume_height - 18.0;
                let plot_width = width - left - right;
                let volume_top = top + price_height + 18.0;
                let min_price = data.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
                let max_price = data.iter().map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
                let max_volume = data.iter().map(|c| c.volume).fold(0.0_f64, f64::max).max(1.0);
                let price_range = (max_price - min_price).max(1.0);
                let step_x = plot_width / data.len().max(1) as f64;
                let candle_width = (step_x * 0.62).max(4.0);
                let to_y = |price: f64| top + ((max_price - price) / price_range) * price_height;
                let to_x = |index: usize| left + step_x * (index as f64) + (step_x / 2.0);
                let ma20 = data.iter().enumerate().map(|(index, _)| {
                    let start = index.saturating_sub(19);
                    let slice = &data[start..=index];
                    let average = slice.iter().map(|candle| candle.close).sum::<f64>() / slice.len() as f64;
                    (index, average)
                }).collect::<Vec<_>>();
                let ma_path = ma20.iter().enumerate().map(|(index, (candle_index, average))| {
                    let command = if index == 0 { "M" } else { "L" };
                    format!("{command} {:.2} {:.2}", to_x(*candle_index), to_y(*average))
                }).collect::<Vec<_>>().join(" ");
                let y_ticks = (0..=5).map(|index| {
                    let ratio = index as f64 / 5.0;
                    let price = max_price - price_range * ratio;
                    let y = top + price_height * ratio;
                    (price, y)
                }).collect::<Vec<_>>();
                let x_ticks = data.iter().enumerate().filter(|(index, _)| index % ((data.len() / 5).max(1)) == 0 || *index == data.len() - 1).map(|(index, candle)| (index, format_time(candle.time, timeframe.get().as_str()))).collect::<Vec<_>>();

                view! {
                    <svg class="chart-svg" viewBox="0 0 900 520" preserveAspectRatio="none">
                        {y_ticks.iter().map(|(_, y)| view! { <line x1=left y1=*y x2={left + plot_width} y2=*y stroke="var(--chart-grid)" stroke-width="1"></line> }).collect::<Vec<_>>()}
                        {x_ticks.iter().map(|(index, _)| {
                            let x = to_x(*index);
                            view! { <line x1=x y1=top x2=x y2={top + price_height + volume_height + 18.0} stroke="var(--chart-grid)" stroke-width="1"></line> }
                        }).collect::<Vec<_>>()}
                        {data.iter().enumerate().map(|(index, candle)| {
                            let x = to_x(index);
                            let y_open = to_y(candle.open);
                            let y_close = to_y(candle.close);
                            let y_high = to_y(candle.high);
                            let y_low = to_y(candle.low);
                            let top_y = y_open.min(y_close);
                            let height_y = (y_open - y_close).abs().max(1.5);
                            let fill = if candle.close >= candle.open { "var(--buy)" } else { "var(--sell)" };
                            let volume_y = volume_top + volume_height - ((candle.volume / max_volume) * volume_height);
                            let volume_fill = if candle.close >= candle.open { "var(--buy-soft)" } else { "var(--sell-soft)" };
                            view! {
                                <g>
                                    <line x1=x y1=y_high x2=x y2=y_low stroke=fill stroke-width="1.4"></line>
                                    <rect x={x - candle_width / 2.0} y=top_y width=candle_width height=height_y fill=fill rx="2" ry="2"></rect>
                                    <rect x={x - candle_width / 2.0} y=volume_y width=candle_width height={(volume_top + volume_height - volume_y).max(1.0)} fill=volume_fill></rect>
                                </g>
                            }
                        }).collect::<Vec<_>>()}
                        <path d=ma_path fill="none" stroke="var(--accent)" stroke-width="2"></path>
                        {y_ticks.iter().map(|(price, y)| view! { <text x={left + plot_width + 8.0} y={*y + 4.0} fill="var(--text-muted)" font-size="11">{format!("{price:.4}")}</text> }).collect::<Vec<_>>()}
                        {x_ticks.iter().map(|(index, label)| {
                            let x = to_x(*index);
                            view! { <text x=x y={height - 10.0} text-anchor="middle" fill="var(--text-muted)" font-size="11">{label.clone()}</text> }
                        }).collect::<Vec<_>>()}
                        <text x={left + 6.0} y={top + 16.0} fill="var(--accent)" font-size="12">"MA20"</text>
                        <text x={left + 6.0} y={volume_top + 14.0} fill="var(--text-muted)" font-size="11">"Volume"</text>
                    </svg>
                }.into_any()
            }}
        </div>
    }
}

fn format_time(timestamp: u64, timeframe: &str) -> String {
    if timeframe.ends_with('d') || timeframe.ends_with('w') { format!("D{}", timestamp / 86_400) }
    else if timeframe.ends_with('h') { format!("H{}", timestamp / 3_600) }
    else { format!("M{}", timestamp / 60) }
}
