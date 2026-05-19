use leptos::prelude::*;
use serde_json::Value;
use web_sys::window;

use crate::components::nav::Nav;
use crate::core::{api, storage};
use crate::core::types::{Balance, DepositAddressResponse, WalletHistoryResponse, WalletTransaction};

const CHAINS: [&str; 3] = ["TRC-20", "ERC-20", "BEP-20"];

#[component]
pub fn WalletPage() -> impl IntoView {
    let (balance, set_balance) = signal(0.0_f64);
    let (deposit_chain, set_deposit_chain) = signal(CHAINS[0].to_string());
    let (withdraw_chain, set_withdraw_chain) = signal(CHAINS[0].to_string());
    let (deposit_address, set_deposit_address) = signal(None::<DepositAddressResponse>);
    let (withdraw_address, set_withdraw_address) = signal(String::new());
    let (withdraw_amount, set_withdraw_amount) = signal(String::new());
    let (transactions, set_transactions) = signal(Vec::<WalletTransaction>::new());
    let (message, set_message) = signal(None::<String>);
    let (error, set_error) = signal(None::<String>);
    let user_id = Signal::derive(storage::get_user_id);

    let refresh_wallet = Action::new_local(move |_: &()| async move {
        if let Ok(balance_response) = api::get_json::<Balance>("/api/wallet/balance").await { set_balance.set(balance_response.usdt); }
        if let Ok(history) = api::get_json::<WalletHistoryResponse>("/api/wallet/history").await {
            let mut rows = history.deposits;
            rows.extend(history.withdrawals);
            rows.sort_by(|left, right| right.created_at.cmp(&left.created_at));
            rows.truncate(40);
            set_transactions.set(rows);
        }
    });

    let generate_address = Action::new_local(move |chain: &String| {
        let chain = chain.clone();
        async move {
            let body = serde_json::json!({ "chain": chain });
            match api::post_json::<_, DepositAddressResponse>("/api/wallet/deposit-address", &body).await {
                Ok(response) => { set_deposit_address.set(Some(response)); set_error.set(None); }
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    let withdraw_action = Action::new_local(move |_: &()| {
        let chain = withdraw_chain.get();
        let address = withdraw_address.get();
        let amount = withdraw_amount.get();
        async move {
            let parsed_amount = match amount.parse::<f64>() { Ok(value) if value > 0.0 => value, _ => { set_error.set(Some("Enter a valid withdrawal amount.".to_string())); return; } };
            let body = serde_json::json!({ "chain": chain, "dest_address": address, "amount": parsed_amount });
            match api::post_json::<_, Value>("/api/wallet/withdraw", &body).await {
                Ok(_) => { set_message.set(Some("Withdrawal submitted.".to_string())); set_error.set(None); set_withdraw_address.set(String::new()); set_withdraw_amount.set(String::new()); refresh_wallet.dispatch_local(()); }
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    Effect::new(move |_| { refresh_wallet.dispatch_local(()); });

    view! {
        <div class="shell">
            <Nav connected=Signal::derive(|| false) show_admin=Signal::derive(|| storage::get_admin_session().is_some()) user_id=user_id />
            <div class="page-wide">
                <div class="wallet-grid">
                    <div class="left-stack">
                        <section class="panel"><div class="panel-header"><div><div class="panel-title">"Wallet Overview"</div><div class="text-muted">"Private settlement balance"</div></div><button class="btn-ghost btn-small" on:click=move |_| { refresh_wallet.dispatch_local(()); }>"Refresh"</button></div><div class="panel-body balance-hero"><span class="label">"Available Balance"</span><span class="balance-value">{move || format!("{:.2} USDT", balance.get())}</span><span class="text-muted">"Deposit, withdraw, and monitor settlement history."</span></div></section>
                        <section class="panel"><div class="panel-header"><div><div class="panel-title">"Deposit"</div><div class="text-muted">"Generate a fresh address per chain."</div></div></div><div class="panel-body form-grid"><div><label class="label">"Chain"</label><select class="select" prop:value=move || deposit_chain.get() on:change=move |event| set_deposit_chain.set(event_target_value(&event))>{CHAINS.into_iter().map(|chain| view! { <option value=chain>{chain}</option> }).collect::<Vec<_>>()}</select></div><button class="btn-primary" on:click=move |_| { generate_address.dispatch_local(deposit_chain.get()); } disabled=move || generate_address.pending().get()>{move || if generate_address.pending().get() { "Generating…" } else { "Generate Address" }}</button>{move || if let Some(address) = deposit_address.get() { let display_value = address.address.clone(); let copy_value = display_value.clone(); view! { <div class="two-column"><div class="qr-placeholder"></div><div class="form-grid"><div><span class="label">"Deposit Address"</span><div class="address-box">{display_value}</div></div><button class="btn-ghost" on:click=move |_| copy_to_clipboard(&copy_value)>"Copy Address"</button><div class="placeholder-note">"QR placeholder shown here. Address is ready to use now."</div></div></div> }.into_any() } else { view! { <div class="placeholder-note">"Select a chain and generate a deposit address."</div> }.into_any() }}</div></section>
                    </div>
                    <div class="right-stack">
                        <section class="panel"><div class="panel-header"><div><div class="panel-title">"Withdraw"</div><div class="text-muted">"Submit a withdrawal request with estimated fee preview."</div></div></div><div class="panel-body form-grid"><div><label class="label">"Chain"</label><select class="select" prop:value=move || withdraw_chain.get() on:change=move |event| set_withdraw_chain.set(event_target_value(&event))>{CHAINS.into_iter().map(|chain| view! { <option value=chain>{chain}</option> }).collect::<Vec<_>>()}</select></div><div><label class="label">"Destination Address"</label><input class="input mono" type="text" prop:value=move || withdraw_address.get() on:input=move |event| set_withdraw_address.set(event_target_value(&event)) /></div><div><label class="label">"Amount"</label><input class="input mono" type="number" step="0.0001" prop:value=move || withdraw_amount.get() on:input=move |event| set_withdraw_amount.set(event_target_value(&event)) /></div><div class="order-summary mono"><div class="summary-row"><span class="text-muted">"Estimated fee"</span><span>{move || format!("{:.4} USDT", withdraw_amount.get().parse::<f64>().unwrap_or(0.0) * 0.0015)}</span></div><div class="summary-row"><span class="text-muted">"Net amount"</span><span>{move || { let gross = withdraw_amount.get().parse::<f64>().unwrap_or(0.0); let fee = gross * 0.0015; format!("{:.4} USDT", (gross - fee).max(0.0)) }}</span></div></div><button class="btn-primary" on:click=move |_| { withdraw_action.dispatch_local(()); } disabled=move || withdraw_action.pending().get()>{move || if withdraw_action.pending().get() { "Submitting…" } else { "Submit Withdrawal" }}</button></div></section>
                        <section class="panel"><div class="panel-header"><div class="panel-title">"Transaction History"</div><div class="text-muted">"Last 20 deposits and withdrawals"</div></div><div class="panel-body"><table class="data-table mono"><thead><tr><th>"Type"</th><th>"Chain"</th><th>"Amount"</th><th>"Fee"</th><th>"Status"</th><th>"Time"</th></tr></thead><tbody>{move || { let rows = transactions.get().into_iter().take(40).collect::<Vec<_>>(); if rows.is_empty() { view! { <tr><td colspan="6" class="text-muted">"No transactions available yet."</td></tr> }.into_any() } else { view! { <>{rows.into_iter().map(|tx| view! { <tr><td>{tx.tx_type}</td><td>{tx.chain}</td><td>{format!("{:.4}", tx.amount)}</td><td>{format!("{:.4}", tx.fee)}</td><td>{tx.status}</td><td>{tx.created_at}</td></tr> }).collect::<Vec<_>>()}</> }.into_any() } }}</tbody></table></div></section>
                    </div>
                </div>
                {move || message.get().map(|message| view! { <div class="form-success">{message}</div> })}
                {move || error.get().map(|message| view! { <div class="form-error">{message}</div> })}
            </div>
        </div>
    }
}

fn copy_to_clipboard(value: &str) {
    if let Some(clipboard) = window().and_then(|window| window.navigator().clipboard()) { let _ = clipboard.write_text(value); }
}
