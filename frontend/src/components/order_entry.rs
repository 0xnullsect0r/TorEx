use leptos::prelude::*;
use serde_json::json;

use crate::core::api;

#[component]
pub fn OrderEntry(
    pair: ReadSignal<String>,
    on_placed: UnsyncCallback<()>,
) -> impl IntoView {
    let (side, set_side) = create_signal("buy");
    let (order_type, set_order_type) = create_signal("limit");
    let (price, set_price) = create_signal(String::new());
    let (quantity, set_quantity) = create_signal(String::new());
    let (error, set_error) = create_signal(Option::<String>::None);
    let (loading, set_loading) = create_signal(false);

    let submit = move |_| {
        let p = pair.get();
        let sd = side.get();
        let ot = order_type.get();
        let qty_str = quantity.get();
        let price_str = price.get();

        let qty: f64 = match qty_str.parse() {
            Ok(v) if v > 0.0 => v,
            _ => {
                set_error.set(Some("Invalid quantity".into()));
                return;
            }
        };
        let pr: Option<f64> = if ot == "limit" {
            match price_str.parse::<f64>() {
                Ok(v) if v > 0.0 => Some(v),
                _ => {
                    set_error.set(Some("Invalid price".into()));
                    return;
                }
            }
        } else {
            None
        };

        set_error.set(None);
        set_loading.set(true);

        wasm_bindgen_futures::spawn_local(async move {
            let mut body = json!({
                "pair":       p,
                "side":       sd,
                "order_type": ot,
                "quantity":   qty,
            });
            if let Some(price_val) = pr {
                body["price"] = json!(price_val);
            }

            match api::post_json("/api/orders", &body).await {
                Ok(_) => {
                    set_quantity.set(String::new());
                    set_price.set(String::new());
                    on_placed.run(());
                }
                Err(e) => set_error.set(Some(e)),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="order-entry">
            // Side tabs
            <div class="side-tabs">
                <button
                    class="side-tab"
                    class:active-buy=move || side.get()=="buy"
                    on:click=move |_| set_side.set("buy")>
                    "Buy"
                </button>
                <button
                    class="side-tab"
                    class:active-sell=move || side.get()=="sell"
                    on:click=move |_| set_side.set("sell")>
                    "Sell"
                </button>
            </div>

            // Type toggle
            <div class="flex gap-8 mt-8 mb-12">
                <label style="font-size:12px;">
                    <input type="radio" name="otype" value="limit"
                        prop:checked=move || order_type.get()=="limit"
                        on:change=move |_| set_order_type.set("limit")/>
                    " Limit"
                </label>
                <label style="font-size:12px;">
                    <input type="radio" name="otype" value="market"
                        prop:checked=move || order_type.get()=="market"
                        on:change=move |_| set_order_type.set("market")/>
                    " Market"
                </label>
            </div>

            // Price (limit only)
            <Show when=move || order_type.get()=="limit">
                <div class="form-group">
                    <label class="form-label">"Price (USDT)"</label>
                    <input class="form-input" type="number" step="0.0001"
                        prop:value=move || price.get()
                        on:input=move |ev| set_price.set(event_target_value(&ev))
                    />
                </div>
            </Show>

            // Quantity
            <div class="form-group">
                <label class="form-label">{move || format!("Quantity ({})", pair.get().split('/').next().unwrap_or(""))}</label>
                <input class="form-input" type="number" step="0.000001"
                    prop:value=move || quantity.get()
                    on:input=move |ev| set_quantity.set(event_target_value(&ev))
                />
            </div>

            {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}

            <button
                class=move || format!("btn-primary btn-{}", side.get())
                disabled=move || loading.get()
                on:click=submit>
                {move || {
                    let sd = side.get();
                    let ot = order_type.get();
                    if loading.get() { "Placing…".into() }
                    else { format!("{} {} {}", sd.to_uppercase(), ot, pair.get().split('/').next().unwrap_or("")) }
                }}
            </button>
        </div>
    }
}
