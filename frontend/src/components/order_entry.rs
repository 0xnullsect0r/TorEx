use leptos::prelude::*;
use serde_json::Value;

use crate::core::api;
use crate::core::types::{CreateOrderRequest, OrderSide, OrderType, TimeInForce};

const ORDER_TYPES: [OrderType; 11] = [OrderType::Limit, OrderType::Market, OrderType::StopLimit, OrderType::StopMarket, OrderType::TrailingStop, OrderType::Oco, OrderType::Iceberg, OrderType::Twap, OrderType::Fok, OrderType::Ioc, OrderType::PostOnly];

#[component]
pub fn OrderEntry(
    pair: ReadSignal<String>,
    #[prop(into)] available_balance: Signal<f64>,
    #[prop(into)] last_price: Signal<f64>,
    on_placed: UnsyncCallback<()>,
) -> impl IntoView {
    let (side, set_side) = signal(OrderSide::Buy);
    let (order_type, set_order_type) = signal(OrderType::Limit);
    let (price, set_price) = signal(String::new());
    let (stop_price, set_stop_price) = signal(String::new());
    let (limit_price, set_limit_price) = signal(String::new());
    let (amount, set_amount) = signal(String::new());
    let (total, set_total) = signal(String::new());
    let (visible_amount, set_visible_amount) = signal(String::new());
    let (duration_minutes, set_duration_minutes) = signal("30".to_string());
    let (trailing_offset, set_trailing_offset) = signal("1.0".to_string());
    let (error, set_error) = signal(None::<String>);
    let (success, set_success) = signal(None::<String>);

    let submit_action = Action::new_local(move |payload: &CreateOrderRequest| {
        let payload = payload.clone();
        async move {
            let response = api::post_json::<_, Value>("/api/orders", &payload).await;
            match response {
                Ok(_) => { set_success.set(Some(format!("{} order submitted.", payload.order_type.label()))); set_error.set(None); set_amount.set(String::new()); set_total.set(String::new()); set_visible_amount.set(String::new()); on_placed.run(()); }
                Err(message) => { set_success.set(None); set_error.set(Some(message)); }
            }
        }
    });

    let apply_fill = move |ratio: f64| {
        let balance = available_balance.get();
        let effective_price = price.get().parse::<f64>().ok().filter(|value| *value > 0.0).or_else(|| limit_price.get().parse::<f64>().ok().filter(|value| *value > 0.0)).unwrap_or_else(|| last_price.get().max(1.0));
        let slice = balance * ratio;
        if side.get() == OrderSide::Buy { set_total.set(format!("{slice:.4}")); set_amount.set(format!("{:.6}", slice / effective_price.max(1e-9))); } else { set_amount.set(format!("{slice:.6}")); set_total.set(format!("{:.4}", slice * effective_price)); }
    };

    let order_summary = move || {
        let amount_value = amount.get().parse::<f64>().unwrap_or(0.0);
        let explicit_total = total.get().parse::<f64>().unwrap_or(0.0);
        let effective_price = price.get().parse::<f64>().ok().filter(|value| *value > 0.0).or_else(|| limit_price.get().parse::<f64>().ok().filter(|value| *value > 0.0)).unwrap_or_else(|| last_price.get());
        let gross_total = if explicit_total > 0.0 { explicit_total } else { amount_value * effective_price };
        let fee_rate = if matches!(order_type.get(), OrderType::PostOnly) { 0.001 } else { 0.0018 };
        let fees = gross_total * fee_rate;
        (amount_value, gross_total, fees)
    };

    view! {
        <div class="order-entry">
            <div class="order-entry-header" style="justify-content: space-between; margin-bottom: 14px;">
                <div><div class="panel-title">"Place Order"</div><div class="text-muted">{move || format!("Available balance: {:.2} USDT", available_balance.get())}</div></div>
                <div class="badge-neutral mono">{move || pair.get()}</div>
            </div>
            <div class="section-block"><div class="buy-sell-toggle"><button class=move || if side.get() == OrderSide::Buy { "side-button buy active" } else { "side-button buy" } on:click=move |_| set_side.set(OrderSide::Buy)>"Buy"</button><button class=move || if side.get() == OrderSide::Sell { "side-button sell active" } else { "side-button sell" } on:click=move |_| set_side.set(OrderSide::Sell)>"Sell"</button></div></div>
            <div class="section-block"><label class="label">"Order Type"</label><select class="select" prop:value=move || format!("{:?}", order_type.get()) on:change=move |event| { let selected = event_target_value(&event); let next = ORDER_TYPES.into_iter().find(|candidate| format!("{:?}", candidate) == selected).unwrap_or(OrderType::Limit); set_order_type.set(next); set_error.set(None); set_success.set(None); }>{ORDER_TYPES.into_iter().map(|candidate| view! { <option value=format!("{:?}", candidate)>{candidate.label()}</option> }).collect::<Vec<_>>()}</select></div>
            <div class="section-block form-grid two">{move || render_order_fields(order_type.get(), price, set_price, stop_price, set_stop_price, limit_price, set_limit_price, amount, set_amount, total, set_total, visible_amount, set_visible_amount, duration_minutes, set_duration_minutes, trailing_offset, set_trailing_offset)}</div>
            <div class="section-block"><div class="label">"Quick Fill"</div><div class="quick-fill-row">{[0.25_f64, 0.50, 0.75, 1.0].into_iter().map(|value| view! { <button class="quick-fill-btn" on:click=move |_| apply_fill(value)>{format!("{}%", (value * 100.0) as i32)}</button> }).collect::<Vec<_>>()}</div></div>
            <div class="section-block"><div class="order-summary mono"><div class="summary-row"><span class="text-muted">"Estimated total"</span><span>{move || format!("{:.4} USDT", order_summary().1)}</span></div><div class="summary-row"><span class="text-muted">"Estimated fees"</span><span>{move || format!("{:.4} USDT", order_summary().2)}</span></div><div class="summary-row"><span class="text-muted">"Estimated fill"</span><span>{move || format!("{:.6} {}", order_summary().0, pair.get().split('/').next().unwrap_or("BASE"))}</span></div></div></div>
            {move || error.get().map(|message| view! { <div class="form-error">{message}</div> })}
            {move || success.get().map(|message| view! { <div class="form-success">{message}</div> })}
            <button class=move || if side.get() == OrderSide::Buy { "btn-buy" } else { "btn-sell" } style="width: 100%; margin-top: 16px;" disabled=move || submit_action.pending().get() on:click=move |_| {
                match build_payload(pair.get(), side.get(), order_type.get(), price.get(), stop_price.get(), limit_price.get(), amount.get(), total.get(), visible_amount.get(), duration_minutes.get(), trailing_offset.get()) {
                    Ok(payload) => { set_error.set(None); set_success.set(None); submit_action.dispatch_local(payload); }
                    Err(message) => set_error.set(Some(message)),
                }
            }>{move || if submit_action.pending().get() { "Submitting…".to_string() } else { format!("{} {}", side.get().label(), order_type.get().label()) }}</button>
        </div>
    }
}

fn render_order_fields(order_type: OrderType, price: ReadSignal<String>, set_price: WriteSignal<String>, stop_price: ReadSignal<String>, set_stop_price: WriteSignal<String>, limit_price: ReadSignal<String>, set_limit_price: WriteSignal<String>, amount: ReadSignal<String>, set_amount: WriteSignal<String>, total: ReadSignal<String>, set_total: WriteSignal<String>, visible_amount: ReadSignal<String>, set_visible_amount: WriteSignal<String>, duration_minutes: ReadSignal<String>, set_duration_minutes: WriteSignal<String>, trailing_offset: ReadSignal<String>, set_trailing_offset: WriteSignal<String>) -> AnyView {
    let price_input = move || labeled_input("Price", "e.g. 68000", price, set_price, "number", "0.0001");
    let amount_input = move || labeled_input("Amount", "e.g. 0.25", amount, set_amount, "number", "0.000001");
    match order_type {
        OrderType::Limit | OrderType::Fok | OrderType::Ioc | OrderType::PostOnly => view! { <>{price_input()}{amount_input()}{labeled_input("Total", "Auto / optional", total, set_total, "number", "0.0001")}</> }.into_any(),
        OrderType::Market => view! { <>{amount_input()}{labeled_input("Total (optional)", "USDT budget", total, set_total, "number", "0.0001")}</> }.into_any(),
        OrderType::StopLimit => view! { <>{labeled_input("Stop Price", "Trigger", stop_price, set_stop_price, "number", "0.0001")}{labeled_input("Limit Price", "Execution", limit_price, set_limit_price, "number", "0.0001")}{amount_input()}</> }.into_any(),
        OrderType::StopMarket => view! { <>{labeled_input("Stop Price", "Trigger", stop_price, set_stop_price, "number", "0.0001")}{amount_input()}</> }.into_any(),
        OrderType::TrailingStop => view! { <>{labeled_input("Trailing Offset %", "e.g. 1.5", trailing_offset, set_trailing_offset, "number", "0.1")}{amount_input()}</> }.into_any(),
        OrderType::Oco => view! { <>{price_input()}{labeled_input("Stop Price", "Stop trigger", stop_price, set_stop_price, "number", "0.0001")}{labeled_input("Limit Price", "Stop limit", limit_price, set_limit_price, "number", "0.0001")}{amount_input()}</> }.into_any(),
        OrderType::Iceberg => view! { <>{price_input()}{labeled_input("Total Amount", "Total size", amount, set_amount, "number", "0.000001")}{labeled_input("Visible Amount", "Displayed size", visible_amount, set_visible_amount, "number", "0.000001")}</> }.into_any(),
        OrderType::Twap => view! { <>{price_input()}{amount_input()}{labeled_input("Duration (minutes)", "e.g. 30", duration_minutes, set_duration_minutes, "number", "1")}</> }.into_any(),
    }
}

fn labeled_input(label: &'static str, placeholder: &'static str, value: ReadSignal<String>, setter: WriteSignal<String>, input_type: &'static str, step: &'static str) -> impl IntoView {
    view! { <div><label class="label">{label}</label><input class="input mono" type=input_type step=step placeholder=placeholder prop:value=move || value.get() on:input=move |event| setter.set(event_target_value(&event)) /></div> }
}

fn build_payload(pair: String, side: OrderSide, order_type: OrderType, price: String, stop_price: String, limit_price: String, amount: String, total: String, visible_amount: String, duration_minutes: String, trailing_offset: String) -> Result<CreateOrderRequest, String> {
    let parse_optional = |value: String| -> Result<Option<f64>, String> {
        let trimmed = value.trim();
        if trimmed.is_empty() { Ok(None) } else { trimmed.parse::<f64>().map(Some).map_err(|_| format!("Invalid numeric value: {trimmed}")) }
    };
    let amount_value = amount.trim().parse::<f64>().map_err(|_| "Amount is required".to_string())?;
    let price_value = parse_optional(price)?;
    let stop_value = parse_optional(stop_price)?;
    let limit_value = parse_optional(limit_price)?;
    let total_value = parse_optional(total)?;
    let visible_value = parse_optional(visible_amount)?;
    let trailing_value = parse_optional(trailing_offset)?;
    let duration_value = if duration_minutes.trim().is_empty() { None } else { Some(duration_minutes.trim().parse::<i64>().map_err(|_| "Invalid duration".to_string())?) };
    match order_type {
        OrderType::Limit if price_value.is_none() => return Err("Limit price is required".into()),
        OrderType::StopLimit if stop_value.is_none() || limit_value.is_none() => return Err("Stop and limit prices are required".into()),
        OrderType::StopMarket if stop_value.is_none() => return Err("Stop price is required".into()),
        OrderType::TrailingStop if trailing_value.is_none() => return Err("Trailing offset is required".into()),
        OrderType::Oco if price_value.is_none() || stop_value.is_none() || limit_value.is_none() => return Err("Primary, stop, and limit prices are required".into()),
        OrderType::Iceberg if price_value.is_none() || visible_value.is_none() => return Err("Price and visible amount are required".into()),
        OrderType::Twap if price_value.is_none() || duration_value.is_none() => return Err("Price and duration are required".into()),
        OrderType::Fok | OrderType::Ioc | OrderType::PostOnly if price_value.is_none() => return Err("Price is required".into()),
        _ => {}
    }
    let time_in_force = match order_type { OrderType::Fok => Some(TimeInForce::Fok), OrderType::Ioc => Some(TimeInForce::Ioc), OrderType::PostOnly => Some(TimeInForce::Gtx), _ => Some(TimeInForce::Gtc) };
    Ok(CreateOrderRequest { pair, side, order_type, time_in_force, price: price_value, stop_price: stop_value, limit_price: limit_value, trailing_offset_pct: trailing_value, visible_amount: visible_value, duration_minutes: duration_value, amount: amount_value, total: total_value })
}
