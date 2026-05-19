use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::core::{api, types::Candle};

#[component]
pub fn Chart(pair: ReadSignal<String>) -> impl IntoView {
    let (candles, set_candles) = create_signal(Vec::<Candle>::new());
    let (loading, set_loading) = create_signal(true);

    Effect::new(move || {
        let p = pair.get();
        set_loading.set(true);
        spawn_local(async move {
            let data = match api::get(&format!("/api/candles/{}?interval=1h&limit=60", p)).await {
                Ok(v) => serde_json::from_value::<Vec<Candle>>(v["candles"].clone()).unwrap_or_default(),
                Err(_) => vec![],
            };
            set_candles.set(data);
            set_loading.set(false);
        });
    });

    view! {
        <div class="chart-wrapper">
            {move || {
                if loading.get() {
                    return view! {
                        <div class="text-muted" style="padding:24px;text-align:center;">"Loading chart…"</div>
                    }.into_any();
                }

                let data = candles.get();
                if data.is_empty() {
                    return view! {
                        <div class="text-muted" style="padding:24px;text-align:center;">"No data"</div>
                    }.into_any();
                }

                let w = 600.0_f64;
                let h = 200.0_f64;
                let pad = 40.0_f64;

                let highs: Vec<f64> = data.iter().map(|c| c.high).collect();
                let lows:  Vec<f64> = data.iter().map(|c| c.low).collect();
                let closes: Vec<f64> = data.iter().map(|c| c.close).collect();

                let min_price = lows.iter().cloned().fold(f64::INFINITY, f64::min);
                let max_price = highs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let price_range = (max_price - min_price).max(1e-9);

                let n = data.len();
                let x_step = (w - pad * 2.0) / (n as f64 - 1.0).max(1.0);

                let to_x = |i: usize| pad + i as f64 * x_step;
                let to_y = |p: f64| h - pad - (p - min_price) / price_range * (h - pad * 2.0);

                // Build SVG path for close prices
                let _path_d = closes.iter().enumerate().map(|(i, &c)| {
                    let cmd = if i == 0 { "M" } else { "L" };
                    format!("{cmd} {:.1} {:.1}", to_x(i), to_y(c))
                }).collect::<Vec<_>>().join(" ");

                // Candle sticks
                let candle_views: Vec<_> = data.iter().enumerate().map(|(i, c)| {
                    let x = to_x(i);
                    let y_open  = to_y(c.open);
                    let y_close = to_y(c.close);
                    let y_high  = to_y(c.high);
                    let y_low   = to_y(c.low);
                    let top    = y_open.min(y_close);
                    let height = (y_open - y_close).abs().max(1.0);
                    let color  = if c.close >= c.open { "#00c853" } else { "#ff1744" };
                    view! {
                        <g>
                            // Wick
                            <line x1=x y1=y_high x2=x y2=y_low
                                stroke=color stroke-width="1"/>
                            // Body
                            <rect x=format!("{:.1}", x - 3.0) y=format!("{:.1}", top)
                                width="6" height=format!("{:.1}", height)
                                fill=color/>
                        </g>
                    }
                }).collect();

                view! {
                    <svg viewBox=format!("0 0 {w} {h}")
                        style="width:100%;height:200px;display:block;">
                        // Grid lines
                        <line x1=pad y1=pad x2=pad y2={h - pad}
                            stroke="#2a2a2a" stroke-width="1"/>
                        <line x1=pad y1={h - pad} x2={w - pad} y2={h - pad}
                            stroke="#2a2a2a" stroke-width="1"/>

                        // Candles
                        {candle_views}

                        // Price labels
                        <text x=format!("{:.1}", pad - 4.0)
                            y=format!("{:.1}", to_y(max_price))
                            text-anchor="end" font-size="9" fill="#666">
                            {format!("{max_price:.2}")}
                        </text>
                        <text x=format!("{:.1}", pad - 4.0)
                            y=format!("{:.1}", to_y(min_price))
                            text-anchor="end" font-size="9" fill="#666">
                            {format!("{min_price:.2}")}
                        </text>
                    </svg>
                }.into_any()
            }}
        </div>
    }
}
