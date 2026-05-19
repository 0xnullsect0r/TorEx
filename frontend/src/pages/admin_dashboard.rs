use leptos::*;
use leptos_router::use_navigate;
use serde_json::{json, Value};
use wasm_bindgen_futures::spawn_local;

use crate::core::{api, storage, types::{AdminStats, FeeTier, Withdrawal}};

#[component]
pub fn AdminDashboard() -> impl IntoView {
    let (stats, set_stats) = create_signal(Option::<AdminStats>::None);
    let (fee_tiers, set_fee_tiers) = create_signal(Vec::<FeeTier>::new());
    let (withdrawals, set_withdrawals) = create_signal(Vec::<Withdrawal>::new());
    let (active_tab, set_active_tab) = create_signal("stats");
    let navigate = use_navigate();

    let refresh_stats = create_action(move |_: &()| async move {
        if let Ok(v) = api::admin_get("/admin/api/stats").await {
            if let Ok(s) = serde_json::from_value::<AdminStats>(v) {
                set_stats.set(Some(s));
            }
        }
    });

    let refresh_fees = create_action(move |_: &()| async move {
        if let Ok(v) = api::admin_get("/admin/api/fees").await {
            if let Some(arr) = v.as_array() {
                if let Ok(tiers) = serde_json::from_value::<Vec<FeeTier>>(Value::Array(arr.clone())) {
                    set_fee_tiers.set(tiers);
                }
            }
        }
    });

    let refresh_withdrawals = create_action(move |_: &()| async move {
        if let Ok(v) = api::admin_get("/admin/api/withdrawals?status=pending").await {
            if let Some(arr) = v["withdrawals"].as_array() {
                if let Ok(ws) = serde_json::from_value::<Vec<Withdrawal>>(Value::Array(arr.clone())) {
                    set_withdrawals.set(ws);
                }
            }
        }
    });

    let approve_withdrawal = create_action(move |id: &String| {
        let id = id.clone();
        let rw = refresh_withdrawals.clone();
        async move {
            let body = json!({ "action": "approve" });
            let _ = api::admin_put(&format!("/admin/api/withdrawals/{id}"), &body).await;
            rw.dispatch(());
        }
    });

    let logout = move |_| {
        storage::clear_admin_session();
        navigate("/admin/login", Default::default());
    };

    // Load on mount
    create_effect(move |_| {
        refresh_stats.dispatch(());
        refresh_fees.dispatch(());
        refresh_withdrawals.dispatch(());
    });

    view! {
        <div class="admin-page">
            <div class="admin-header">
                <div class="admin-title">"TorEx Admin"</div>
                <div class="flex gap-12">
                    <button class="tab-btn"
                        class:active=move || active_tab.get()=="stats"
                        on:click=move |_| set_active_tab.set("stats")>"Stats"</button>
                    <button class="tab-btn"
                        class:active=move || active_tab.get()=="fees"
                        on:click=move |_| set_active_tab.set("fees")>"Fee Tiers"</button>
                    <button class="tab-btn"
                        class:active=move || active_tab.get()=="withdrawals"
                        on:click=move |_| set_active_tab.set("withdrawals")>"Withdrawals"</button>
                    <button class="btn-primary" style="padding:4px 12px;font-size:11px;"
                        on:click=logout>"Logout"</button>
                </div>
            </div>

            // Stats
            <Show when=move || active_tab.get()=="stats">
                {move || stats.get().map(|s| view! {
                    <div class="stats-grid">
                        <StatCard label="Total Users"     value=s.total_users.to_string()/>
                        <StatCard label="Active 24h"      value=s.active_users_24h.to_string()/>
                        <StatCard label="24h Volume"      value=format!("${:.0}", s.volume_24h)/>
                        <StatCard label="Total Volume"    value=format!("${:.0}", s.total_volume)/>
                        <StatCard label="Fee Revenue 24h" value=format!("${:.2}", s.fee_revenue_24h)/>
                        <StatCard label="Total Fee Rev"   value=format!("${:.2}", s.total_fee_revenue)/>
                        <StatCard label="Open Orders"     value=s.open_orders.to_string()/>
                        <StatCard label="Pending W/D"     value=s.pending_withdrawals.to_string()/>
                    </div>
                }).unwrap_or_else(|| view! {
                    <div class="text-muted">"Loading…"</div>
                })}
            </Show>

            // Fee tiers
            <Show when=move || active_tab.get()=="fees">
                <table class="data-table admin-table">
                    <thead>
                        <tr>
                            <th>"Min Vol (30d)"</th>
                            <th>"Max Vol"</th>
                            <th>"Maker %"</th>
                            <th>"Taker %"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || fee_tiers.get()
                            key=|t| t.tier_id
                            children=move |t| view! {
                                <tr>
                                    <td>{format!("${}", t.min_volume)}</td>
                                    <td>{t.max_volume.map(|v| format!("${v}")).unwrap_or("∞".into())}</td>
                                    <td>{format!("{:.3}%", t.maker_fee * 100.0)}</td>
                                    <td>{format!("{:.3}%", t.taker_fee * 100.0)}</td>
                                </tr>
                            }
                        />
                    </tbody>
                </table>
            </Show>

            // Pending withdrawals
            <Show when=move || active_tab.get()=="withdrawals">
                <table class="data-table admin-table">
                    <thead>
                        <tr>
                            <th>"ID"</th><th>"Amount"</th><th>"Chain"</th>
                            <th>"Dest"</th><th>"Status"</th><th></th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || withdrawals.get()
                            key=|w| w.withdrawal_id.clone()
                            children=move |w| {
                                let wid = w.withdrawal_id.clone();
                                let w_id_short = w.withdrawal_id.chars().take(8).collect::<String>();
                                let w_amount = format!("{:.2}", w.amount);
                                let w_chain = w.chain.clone();
                                let w_dest = w.dest_address.clone();
                                let w_status = w.status.clone();
                                let w_status2 = w.status.clone();
                                view! {
                                    <tr>
                                        <td style="font-size:10px;">{w_id_short}"…"</td>
                                        <td>{w_amount}</td>
                                        <td>{w_chain}</td>
                                        <td style="font-size:10px;max-width:120px;overflow:hidden;text-overflow:ellipsis;">
                                            {w_dest}
                                        </td>
                                        <td>{w_status.clone()}</td>
                                        <td>
                                            <Show when=move || w_status2=="pending">
                                                <button class="btn-primary"
                                                    style="padding:2px 8px;font-size:11px;"
                                                    on:click=move |_| approve_withdrawal.dispatch(wid.clone())>
                                                    "Approve"
                                                </button>
                                            </Show>
                                        </td>
                                    </tr>
                                }
                            }
                        />
                    </tbody>
                </table>
            </Show>
        </div>
    }
}

#[component]
fn StatCard(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="stat-card">
            <div class="stat-label">{label}</div>
            <div class="stat-value">{value}</div>
        </div>
    }
}
