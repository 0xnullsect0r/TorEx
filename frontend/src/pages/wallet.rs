use leptos::*;
use leptos_router::A;
use serde_json::json;
use wasm_bindgen::JsValue;
use web_sys::window;

use crate::core::api;
use crate::components::nav::Nav;

const CHAINS: &[(&str, &str)] = &[
    ("tron", "TRC-20 (TRON)"),
    ("eth",  "ERC-20 (Ethereum)"),
    ("bsc",  "BEP-20 (BSC)"),
];

#[component]
pub fn Wallet() -> impl IntoView {
    let (pair, set_pair) = create_signal("BTC/USDT".to_string());
    let (balance, set_balance) = create_signal("0.00".to_string());
    let (chain, set_chain) = create_signal("tron".to_string());
    let (deposit_addr, set_deposit_addr) = create_signal(Option::<String>::None);
    let (withdraw_addr, set_withdraw_addr) = create_signal(String::new());
    let (withdraw_amount, set_withdraw_amount) = create_signal(String::new());
    let (withdraw_error, set_withdraw_error) = create_signal(Option::<String>::None);
    let (withdraw_ok, set_withdraw_ok) = create_signal(false);
    let (loading_deposit, set_loading_deposit) = create_signal(false);
    let (loading_withdraw, set_loading_withdraw) = create_signal(false);

    // Load balance on mount
    create_effect(move |_| {
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(v) = api::get("/api/wallet/balance").await {
                let usdt = v["usdt"].as_str().unwrap_or("0.00").to_string();
                set_balance.set(usdt);
            }
        });
    });

    let get_deposit = move |_| {
        let ch = chain.get();
        set_loading_deposit.set(true);
        set_deposit_addr.set(None);
        wasm_bindgen_futures::spawn_local(async move {
            let body = json!({ "chain": ch });
            if let Ok(v) = api::post_json("/api/wallet/deposit-address", &body).await {
                if let Some(addr) = v["address"].as_str() {
                    set_deposit_addr.set(Some(addr.to_string()));
                }
            }
            set_loading_deposit.set(false);
        });
    };

    let do_withdraw = move |_| {
        let addr = withdraw_addr.get();
        let amt = withdraw_amount.get();
        let ch = chain.get();
        set_withdraw_error.set(None);
        set_withdraw_ok.set(false);
        if addr.is_empty() || amt.is_empty() {
            set_withdraw_error.set(Some("Address and amount are required".into()));
            return;
        }
        let amount: f64 = match amt.parse() {
            Ok(v) => v,
            Err(_) => {
                set_withdraw_error.set(Some("Invalid amount".into()));
                return;
            }
        };
        set_loading_withdraw.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let body = json!({
                "dest_address": addr,
                "amount": amount,
                "chain": ch,
                "zk_proof": vec![0u8; 32],
            });
            match api::post_json("/api/wallet/withdraw", &body).await {
                Ok(_) => {
                    set_withdraw_ok.set(true);
                    set_withdraw_addr.set(String::new());
                    set_withdraw_amount.set(String::new());
                }
                Err(e) => set_withdraw_error.set(Some(e)),
            }
            set_loading_withdraw.set(false);
        });
    };

    let copy_addr = move |_| {
        if let Some(addr) = deposit_addr.get() {
            if let Some(win) = window() {
                let _ = win.navigator().clipboard().write_text(&addr);
            }
        }
    };

    view! {
        <Nav pair=pair set_pair=set_pair pairs=&["BTC/USDT"]/>
        <div class="page">
            <div style="max-width:520px;margin:0 auto;">

                // Balance
                <div class="card">
                    <div class="card-title">"Balance"</div>
                    <div class="balance-big">{move || format!("{} USDT", balance.get())}</div>
                    <button class="btn-primary" style="width:auto;padding:4px 12px;font-size:11px;"
                        on:click=move |_| {
                            wasm_bindgen_futures::spawn_local(async move {
                                if let Ok(v) = api::get("/api/wallet/balance").await {
                                    let u = v["usdt"].as_str().unwrap_or("0.00").to_string();
                                    set_balance.set(u);
                                }
                            });
                        }>
                        "Refresh"
                    </button>
                </div>

                // Chain selector
                <div class="flex gap-8 mt-12">
                    <span class="text-muted">"Chain:"</span>
                    <select class="chain-select"
                        on:change=move |ev| {
                            set_chain.set(event_target_value(&ev));
                            set_deposit_addr.set(None);
                        }>
                        {CHAINS.iter().map(|(val, label)| view! {
                            <option value=*val>{*label}</option>
                        }).collect::<Vec<_>>()}
                    </select>
                </div>

                // Deposit
                <div class="card mt-12">
                    <div class="card-title">"Deposit"</div>
                    <Show when=move || deposit_addr.get().is_none()>
                        <button class="btn-primary"
                            disabled=move || loading_deposit.get()
                            on:click=get_deposit>
                            {move || if loading_deposit.get() {
                                "Generating…"
                            } else {
                                "Generate Deposit Address"
                            }}
                        </button>
                    </Show>
                    {move || deposit_addr.get().map(|addr| {
                        let addr2 = addr.clone();
                        view! {
                            <div>
                                <div class="qr-container">
                                    // Simple text QR placeholder — real QR needs a JS lib or pure-Rust impl
                                    <div style="width:160px;height:160px;display:flex;align-items:center;
                                                justify-content:center;background:#fff;color:#000;
                                                font-size:9px;word-break:break-all;padding:4px;">
                                        {addr.clone()}
                                    </div>
                                </div>
                                <div class="address-text" on:click=copy_addr>{addr2}</div>
                                <div class="text-muted mt-8" style="font-size:11px;">
                                    "This stealth address expires in 48 hours. Click address to copy."
                                </div>
                            </div>
                        }
                    })}
                </div>

                // Withdraw
                <div class="card mt-12">
                    <div class="card-title">"Withdraw"</div>
                    <div class="form-group">
                        <label class="form-label">"Destination address"</label>
                        <input class="form-input" type="text"
                            prop:value=move || withdraw_addr.get()
                            on:input=move |ev| set_withdraw_addr.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="form-group">
                        <label class="form-label">"Amount (USDT)"</label>
                        <input class="form-input" type="number" step="0.01"
                            prop:value=move || withdraw_amount.get()
                            on:input=move |ev| set_withdraw_amount.set(event_target_value(&ev))
                        />
                    </div>
                    {move || withdraw_error.get().map(|e| view! {
                        <div class="form-error">{e}</div>
                    })}
                    {move || withdraw_ok.get().then(|| view! {
                        <div style="color:var(--buy);font-size:12px;margin-bottom:8px;">
                            "Withdrawal submitted successfully."
                        </div>
                    })}
                    <button class="btn-primary"
                        disabled=move || loading_withdraw.get()
                        on:click=do_withdraw>
                        {move || if loading_withdraw.get() { "Submitting…" } else { "Withdraw USDT" }}
                    </button>
                </div>

            </div>
        </div>
    }
}
