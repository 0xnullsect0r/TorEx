use std::collections::HashMap;

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use serde_json::Value;
use web_sys::window;

use crate::components::nav::Nav;
use crate::core::{api, storage};
use crate::core::types::{AdminStats, AdminUserActivity, FeeTier, User, VolumeByPair, Withdrawal, format_compact, short_id};

const TABS: [&str; 5] = ["stats", "users", "fee_tiers", "withdrawals", "volume"];

#[component]
pub fn AdminDashboard() -> impl IntoView {
    let (active_tab, set_active_tab) = signal("stats".to_string());
    let (stats, set_stats) = signal(AdminStats::default());
    let (users, set_users) = signal(Vec::<User>::new());
    let (users_total, set_users_total) = signal(0_i64);
    let (fee_tiers, set_fee_tiers) = signal(Vec::<FeeTier>::new());
    let (withdrawals, set_withdrawals) = signal(Vec::<Withdrawal>::new());
    let (volume_rows, set_volume_rows) = signal(Vec::<VolumeByPair>::new());
    let (withdrawal_filter, set_withdrawal_filter) = signal("all".to_string());
    let (user_page, set_user_page) = signal(0_usize);
    let (expanded_user, set_expanded_user) = signal(None::<String>);
    let (activities, set_activities) = signal(HashMap::<String, AdminUserActivity>::new());
    let (flash, set_flash) = signal(None::<String>);
    let (error, set_error) = signal(None::<String>);
    let navigate = use_navigate();

    let refresh_stats = Action::new_local(move |_: &()| async move {
        match api::admin_get_value("/admin/api/stats").await {
            Ok(value) => set_stats.set(serde_json::from_value::<AdminStats>(value).unwrap_or_default()),
            Err(message) => set_error.set(Some(message)),
        }
    });

    let refresh_users = Action::new_local(move |page: &usize| {
        let page = *page;
        async move {
            let path = format!("/admin/api/users?page={}&per_page=50", page + 1);
            match api::admin_get_value(&path).await {
                Ok(value) => {
                    let parsed = serde_json::from_value::<Vec<User>>(value["users"].clone())
                        .or_else(|_| serde_json::from_value(value.clone()))
                        .unwrap_or_default();
                    let total = value["total"].as_i64().unwrap_or(parsed.len() as i64);
                    set_users.set(parsed);
                    set_users_total.set(total);
                }
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    let refresh_fee_tiers = Action::new_local(move |_: &()| async move {
        match api::admin_get_value("/admin/api/fee-tiers").await {
            Ok(value) => {
                let parsed = serde_json::from_value::<Vec<FeeTier>>(value["fee_tiers"].clone())
                    .or_else(|_| serde_json::from_value(value))
                    .unwrap_or_default();
                set_fee_tiers.set(parsed);
            }
            Err(message) => set_error.set(Some(message)),
        }
    });

    let refresh_withdrawals = Action::new_local(move |filter: &String| {
        let filter = filter.clone();
        async move {
            let path = if filter == "all" { "/admin/api/withdrawals".to_string() } else { format!("/admin/api/withdrawals?status={filter}") };
            match api::admin_get_value(&path).await {
                Ok(value) => {
                    let parsed = serde_json::from_value::<Vec<Withdrawal>>(value["withdrawals"].clone())
                        .or_else(|_| serde_json::from_value(value))
                        .unwrap_or_default();
                    set_withdrawals.set(parsed);
                }
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    let refresh_volume = Action::new_local(move |_: &()| async move {
        match api::admin_get_value("/admin/api/volume").await {
            Ok(value) => {
                let mut parsed = serde_json::from_value::<Vec<VolumeByPair>>(value["pairs"].clone())
                    .or_else(|_| serde_json::from_value(value))
                    .unwrap_or_default();
                parsed.sort_by(|left, right| right.volume_24h.partial_cmp(&left.volume_24h).unwrap_or(std::cmp::Ordering::Equal));
                set_volume_rows.set(parsed);
            }
            Err(message) => set_error.set(Some(message)),
        }
    });

    let create_user = Action::new_local(move |_: &()| async move {
        match api::admin_post_json::<_, Value>("/admin/api/users", &serde_json::json!({})).await {
            Ok(value) => {
                let created = value["user_id"].as_str().unwrap_or("new-user");
                set_flash.set(Some(format!("Created user {}", short_id(created))));
                refresh_users.dispatch_local(user_page.get());
                refresh_stats.dispatch_local(());
            }
            Err(message) => set_error.set(Some(message)),
        }
    });

    let fetch_activity = Action::new_local(move |user_id: &String| {
        let user_id = user_id.clone();
        async move {
            match api::admin_get_value(&format!("/admin/api/users/{user_id}/activity")).await {
                Ok(value) => {
                    set_activities.update(|map| {
                        map.insert(user_id.clone(), serde_json::from_value::<AdminUserActivity>(value).unwrap_or_default());
                    });
                }
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    let reset_sessions = Action::new_local(move |user_id: &String| {
        let user_id = user_id.clone();
        async move {
            match api::admin_post_json::<_, Value>(&format!("/admin/api/users/{user_id}/reset-sessions"), &serde_json::json!({})).await {
                Ok(_) => set_flash.set(Some(format!("Reset sessions for {}", short_id(&user_id)))),
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    let toggle_fee_free = Action::new_local(move |user_id: &String| {
        let user_id = user_id.clone();
        async move {
            match api::admin_post_json::<_, Value>(&format!("/admin/api/users/{user_id}/fee-free"), &serde_json::json!({})).await {
                Ok(_) => {
                    set_flash.set(Some(format!("Updated fee-free state for {}", short_id(&user_id))));
                    refresh_users.dispatch_local(user_page.get());
                }
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    let save_fee_tier = Action::new_local(move |tier: &FeeTier| {
        let tier = tier.clone();
        async move {
            match api::admin_put_json::<_, Value>(&format!("/admin/api/fee-tiers/{}", tier.id), &tier).await {
                Ok(_) => set_flash.set(Some(format!("Saved tier {}", tier.id))),
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    let approve_withdrawal = Action::new_local(move |withdrawal_id: &String| {
        let withdrawal_id = withdrawal_id.clone();
        async move {
            match api::admin_post_json::<_, Value>(&format!("/admin/api/withdrawals/{withdrawal_id}/approve"), &serde_json::json!({})).await {
                Ok(_) => {
                    set_flash.set(Some(format!("Approved withdrawal {}", short_id(&withdrawal_id))));
                    refresh_withdrawals.dispatch_local(withdrawal_filter.get());
                    refresh_stats.dispatch_local(());
                }
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    let create_user_cb = UnsyncCallback::new(move |_: ()| { create_user.dispatch_local(()); });
    let fetch_activity_cb = UnsyncCallback::new(move |id: String| { fetch_activity.dispatch_local(id); });
    let reset_sessions_cb = UnsyncCallback::new(move |id: String| { reset_sessions.dispatch_local(id); });
    let toggle_fee_free_cb = UnsyncCallback::new(move |id: String| { toggle_fee_free.dispatch_local(id); });
    let save_fee_tier_cb = UnsyncCallback::new(move |tier: FeeTier| { save_fee_tier.dispatch_local(tier); });
    let approve_withdrawal_cb = UnsyncCallback::new(move |id: String| { approve_withdrawal.dispatch_local(id); });
    let refresh_users_cb = UnsyncCallback::new(move |page: usize| { refresh_users.dispatch_local(page); });
    let refresh_withdrawals_cb = UnsyncCallback::new(move |filter: String| { refresh_withdrawals.dispatch_local(filter); });

    Effect::new(move |_| {
        refresh_stats.dispatch_local(());
        refresh_users.dispatch_local(user_page.get());
        refresh_fee_tiers.dispatch_local(());
        refresh_withdrawals.dispatch_local(withdrawal_filter.get());
        refresh_volume.dispatch_local(());
    });

    view! {
        <div class="shell">
            <Nav connected=Signal::derive(|| false) show_admin=Signal::derive(|| true) user_id=Signal::derive(storage::get_user_id) />
            <div class="page-wide admin-grid">
                <aside class="admin-sidebar panel">
                    <div class="panel-header"><div><div class="panel-title">"Admin Dashboard"</div><div class="text-muted">"Operational controls and monitoring"</div></div></div>
                    <div class="panel-body admin-tabs">
                        {TABS.into_iter().map(|tab| {
                            let label = match tab { "stats" => "Stats", "users" => "Users", "fee_tiers" => "Fee Tiers", "withdrawals" => "Withdrawals", _ => "Volume by Pair" };
                            view! { <button class=move || if active_tab.get() == tab { "admin-tab active" } else { "admin-tab" } on:click=move |_| set_active_tab.set(tab.to_string())>{label}</button> }
                        }).collect::<Vec<_>>()}
                        <div class="hr"></div>
                        <button class="btn-ghost" on:click=move |_| { storage::clear_admin_session(); navigate("/admin", Default::default()); }>"Sign Out"</button>
                    </div>
                </aside>

                <section class="right-stack">
                    {move || match active_tab.get().as_str() {
                        "stats" => render_stats_tab(stats.get()),
                        "users" => render_users_tab(users.get(), users_total.get(), user_page, set_user_page, expanded_user, set_expanded_user, activities.get(), create_user_cb, fetch_activity_cb, reset_sessions_cb, toggle_fee_free_cb, refresh_users_cb),
                        "fee_tiers" => render_fee_tiers_tab(fee_tiers, set_fee_tiers, save_fee_tier_cb),
                        "withdrawals" => render_withdrawals_tab(withdrawals.get(), withdrawal_filter, set_withdrawal_filter, refresh_withdrawals_cb, approve_withdrawal_cb),
                        _ => render_volume_tab(volume_rows.get()),
                    }}
                    {move || flash.get().map(|message| view! { <div class="form-success">{message}</div> })}
                    {move || error.get().map(|message| view! { <div class="form-error">{message}</div> })}
                </section>
            </div>
        </div>
    }
}

fn render_stats_tab(stats: AdminStats) -> AnyView {
    view! {
        <section class="panel">
            <div class="panel-header"><div class="panel-title">"Platform Stats"</div><div class=if stats.system_healthy { "badge-success" } else { "badge-danger" }>{if stats.system_healthy { "System Healthy" } else { "Attention Required" }}</div></div>
            <div class="panel-body stats-grid">
                {[("Total Users", stats.total_users.to_string()), ("Active Orders", stats.active_orders.to_string()), ("Pending Withdrawals", stats.pending_withdrawals.to_string()), ("24h Volume", format!("${}", format_compact(stats.volume_24h))), ("24h Trades", stats.trades_24h.to_string()), ("24h Deposits", format!("${}", format_compact(stats.deposits_24h)))].into_iter().map(|(label, value)| view! { <div class="metric-card"><div class="stat-label">{label}</div><div class="metric-value mono">{value}</div></div> }).collect::<Vec<_>>()}
            </div>
        </section>
    }.into_any()
}

fn render_users_tab(users: Vec<User>, total: i64, user_page: ReadSignal<usize>, set_user_page: WriteSignal<usize>, expanded_user: ReadSignal<Option<String>>, set_expanded_user: WriteSignal<Option<String>>, activities: HashMap<String, AdminUserActivity>, create_user: UnsyncCallback<()>, fetch_activity: UnsyncCallback<String>, reset_sessions: UnsyncCallback<String>, toggle_fee_free: UnsyncCallback<String>, refresh_users: UnsyncCallback<usize>) -> AnyView {
    let total_pages = ((total.max(users.len() as i64) as usize) + 49) / 50;
    view! {
        <section class="panel">
            <div class="panel-header"><div><div class="panel-title">"Users"</div><div class="text-muted">"50 users per page with administrative controls"</div></div><button class="btn-primary" on:click=move |_| create_user.run(())>"Create User"</button></div>
            <div class="panel-body">
                <table class="data-table mono">
                    <thead><tr><th>"GUID"</th><th>"Created"</th><th>"30d Volume"</th><th>"Orders"</th><th>"Fee Free"</th><th>"Actions"</th></tr></thead>
                    <tbody>
                        {users.into_iter().flat_map(|user| {
                            let user_id = user.user_id.clone();
                            let copy_id = user_id.clone();
                            let expand_id = user_id.clone();
                            let reset_id = user_id.clone();
                            let toggle_id = user_id.clone();
                            let is_expanded = expanded_user.get() == Some(user_id.clone());
                            let activity = activities.get(&user_id).cloned();
                            let mut rows = vec![view! {
                                <tr>
                                    <td><button class="copy-button" on:click=move |_| copy_text(&copy_id)>{short_id(&user.user_id)}</button></td>
                                    <td>{user.created_at.clone()}</td>
                                    <td>{format!("${}", format_compact(user.volume_30d))}</td>
                                    <td>{user.order_count}</td>
                                    <td><span class=if user.fee_free { "badge-success" } else { "badge-neutral" }>{if user.fee_free { "Enabled" } else { "Standard" }}</span></td>
                                    <td>
                                        <div class="table-actions">
                                            <button class="btn-ghost btn-small" on:click=move |_| {
                                                if expanded_user.get() == Some(expand_id.clone()) {
                                                    set_expanded_user.set(None);
                                                } else {
                                                    set_expanded_user.set(Some(expand_id.clone()));
                                                    fetch_activity.run(expand_id.clone());
                                                }
                                            }>"View Activity"</button>
                                            <button class="btn-ghost btn-small" on:click=move |_| reset_sessions.run(reset_id.clone())>"Reset Sessions"</button>
                                            <button class="btn-ghost btn-small" on:click=move |_| toggle_fee_free.run(toggle_id.clone())>"Toggle Fee Free"</button>
                                        </div>
                                    </td>
                                </tr>
                            }];
                            if is_expanded {
                                rows.push(view! {
                                    <tr class="expanded-row">
                                        <td colspan="6">
                                            <div class="two-column">
                                                <div><div class="label">"Last 5 Logins"</div><ul>{activity.clone().unwrap_or_default().last_logins.into_iter().take(5).map(|login| view! { <li>{format!("{} · {}", login.occurred_at, login.source)}</li> }).collect::<Vec<_>>()}</ul></div>
                                                <div><div class="label">"Last 5 Orders"</div><ul>{activity.unwrap_or_default().last_orders.into_iter().take(5).map(|order| view! { <li>{format!("{} {} {} @ {:.4}", order.side.label(), order.order_type.label(), order.pair, order.price)}</li> }).collect::<Vec<_>>()}</ul></div>
                                            </div>
                                        </td>
                                    </tr>
                                });
                            }
                            rows
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
                <div class="pagination">
                    <button class="btn-ghost btn-small" disabled=user_page.get() == 0 on:click=move |_| { let next = user_page.get().saturating_sub(1); set_user_page.set(next); refresh_users.run(next); }>"Previous"</button>
                    <span class="text-muted">{format!("Page {} of {}", user_page.get() + 1, total_pages.max(1))}</span>
                    <button class="btn-ghost btn-small" disabled=user_page.get() + 1 >= total_pages on:click=move |_| { let next = (user_page.get() + 1).min(total_pages.saturating_sub(1)); set_user_page.set(next); refresh_users.run(next); }>"Next"</button>
                </div>
            </div>
        </section>
    }.into_any()
}

fn render_fee_tiers_tab(fee_tiers: ReadSignal<Vec<FeeTier>>, set_fee_tiers: WriteSignal<Vec<FeeTier>>, save_fee_tier: UnsyncCallback<FeeTier>) -> AnyView {
    view! {
        <section class="panel">
            <div class="panel-header"><div><div class="panel-title">"Fee Tiers"</div><div class="text-muted">"Inline editing for maker and taker pricing"</div></div></div>
            <div class="panel-body">
                <table class="data-table mono">
                    <thead><tr><th>"Tier"</th><th>"Min Volume"</th><th>"Max Volume"</th><th>"Maker Fee"</th><th>"Taker Fee"</th><th></th></tr></thead>
                    <tbody>
                        {move || fee_tiers.get().into_iter().enumerate().map(|(index, tier)| {
                            let tier_id = tier.id;
                            view! {
                                <tr>
                                    <td>{tier_id}</td>
                                    <td><input class="inline-input mono" type="number" step="0.01" prop:value=tier.min_volume.to_string() on:input=move |event| { let value = event_target_value(&event).parse::<f64>().unwrap_or(0.0); set_fee_tiers.update(|tiers| if let Some(current) = tiers.get_mut(index) { current.min_volume = value; }); } /></td>
                                    <td><input class="inline-input mono" type="number" step="0.01" prop:value=tier.max_volume.map(|v| v.to_string()).unwrap_or_default() on:input=move |event| { let raw = event_target_value(&event); set_fee_tiers.update(|tiers| if let Some(current) = tiers.get_mut(index) { current.max_volume = raw.parse::<f64>().ok(); }); } /></td>
                                    <td><input class="inline-input mono" type="number" step="0.0001" prop:value=tier.maker_fee.to_string() on:input=move |event| { let value = event_target_value(&event).parse::<f64>().unwrap_or(0.0); set_fee_tiers.update(|tiers| if let Some(current) = tiers.get_mut(index) { current.maker_fee = value; }); } /></td>
                                    <td><input class="inline-input mono" type="number" step="0.0001" prop:value=tier.taker_fee.to_string() on:input=move |event| { let value = event_target_value(&event).parse::<f64>().unwrap_or(0.0); set_fee_tiers.update(|tiers| if let Some(current) = tiers.get_mut(index) { current.taker_fee = value; }); } /></td>
                                    <td><button class="btn-primary btn-small" on:click=move |_| { if let Some(current) = fee_tiers.get().into_iter().find(|candidate| candidate.id == tier_id) { save_fee_tier.run(current); } }>"Save"</button></td>
                                </tr>
                            }
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>
        </section>
    }.into_any()
}

fn render_withdrawals_tab(withdrawals: Vec<Withdrawal>, filter: ReadSignal<String>, set_filter: WriteSignal<String>, refresh_withdrawals: UnsyncCallback<String>, approve_withdrawal: UnsyncCallback<String>) -> AnyView {
    view! {
        <section class="panel">
            <div class="panel-header"><div><div class="panel-title">"Withdrawals"</div><div class="text-muted">"Approve cold-wallet requests and inspect queue status"</div></div><div class="filters-row"><select class="select" prop:value=move || filter.get() on:change=move |event| { let next = event_target_value(&event); set_filter.set(next.clone()); refresh_withdrawals.run(next); }><option value="all">"All"</option><option value="pending">"Pending"</option><option value="pending_cold_wallet">"Pending Cold Wallet"</option><option value="approved">"Approved"</option></select></div></div>
            <div class="panel-body">
                <table class="data-table mono">
                    <thead><tr><th>"ID"</th><th>"User"</th><th>"Chain"</th><th>"Amount"</th><th>"Fee"</th><th>"Status"</th><th>"Created"</th><th></th></tr></thead>
                    <tbody>
                        {withdrawals.into_iter().map(|withdrawal| {
                            let approve_id = withdrawal.withdrawal_id.clone();
                            let can_approve = withdrawal.status == "pending_cold_wallet";
                            view! {
                                <tr>
                                    <td>{short_id(&withdrawal.withdrawal_id)}</td>
                                    <td>{short_id(&withdrawal.user_id)}</td>
                                    <td>{withdrawal.chain}</td>
                                    <td>{format!("{:.4}", withdrawal.amount)}</td>
                                    <td>{format!("{:.4}", withdrawal.fee)}</td>
                                    <td>{withdrawal.status}</td>
                                    <td>{withdrawal.created_at}</td>
                                    <td>{if can_approve { view! { <button class="btn-primary btn-small" on:click=move |_| approve_withdrawal.run(approve_id.clone())>"Approve"</button> }.into_any() } else { view! { <span class="text-muted">"—"</span> }.into_any() }}</td>
                                </tr>
                            }
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>
        </section>
    }.into_any()
}

fn render_volume_tab(rows: Vec<VolumeByPair>) -> AnyView {
    view! {
        <section class="panel">
            <div class="panel-header"><div class="panel-title">"Volume by Pair"</div><div class="text-muted">"Sorted by 24h volume"</div></div>
            <div class="panel-body">
                <table class="data-table mono">
                    <thead><tr><th>"Pair"</th><th>"24h Volume"</th><th>"24h Trades"</th></tr></thead>
                    <tbody>{rows.into_iter().map(|row| view! { <tr><td>{row.pair}</td><td>{format!("${}", format_compact(row.volume_24h))}</td><td>{row.trades_24h}</td></tr> }).collect::<Vec<_>>()}</tbody>
                </table>
            </div>
        </section>
    }.into_any()
}

fn copy_text(value: &str) {
    if let Some(clipboard) = window().and_then(|window| window.navigator().clipboard()) {
        let _ = clipboard.write_text(value);
    }
}
