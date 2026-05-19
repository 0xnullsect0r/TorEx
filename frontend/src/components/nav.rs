use leptos::prelude::*;

use crate::core::storage;
use crate::core::types::{Theme, short_id};

#[component]
pub fn Nav(
    #[prop(optional, into)] connected: Option<Signal<bool>>,
    #[prop(optional, into)] show_admin: Option<Signal<bool>>,
    #[prop(optional, into)] user_id: Option<Signal<Option<String>>>,
) -> impl IntoView {
    let (theme, set_theme) = signal(storage::get_theme());

    let is_connected = move || connected.map(|signal| signal.get()).unwrap_or(false);
    let admin_visible = move || show_admin.map(|signal| signal.get()).unwrap_or(storage::get_admin_session().is_some());
    let user_label = move || user_id.and_then(|signal| signal.get()).or_else(storage::get_user_id).map(|value| short_id(&value)).unwrap_or_else(|| "guest".to_string());

    view! {
        <nav class="top-nav">
            <a href="/trade" class="brand">
                <span class="brand-mark">"T"</span>
                <span class="brand-name">"TorEx"</span>
            </a>
            <div class="nav-links">
                <a href="/trade" class="nav-link">"Trade"</a>
                <a href="/wallet" class="nav-link">"Wallet"</a>
                {move || if admin_visible() { view! { <a href="/admin/dashboard" class="nav-link">"Admin"</a> }.into_any() } else { view! { <a href="/admin" class="nav-link">"Admin"</a> }.into_any() }}
            </div>
            <div class="nav-actions">
                <div class="connection-pill"><span class=move || if is_connected() { "status-dot online" } else { "status-dot" }></span><span>{move || if is_connected() { "Realtime connected" } else { "Offline" }}</span></div>
                <select class="theme-select" prop:value=move || theme.get().as_str().to_string() on:change=move |event| { let next = Theme::from(event_target_value(&event)); set_theme.set(next); storage::set_theme(next); }>
                    {Theme::all().into_iter().map(|candidate| view! { <option value=candidate.as_str()>{candidate.label()}</option> }).collect::<Vec<_>>()}
                </select>
                <div class="user-pill"><span class="text-muted">"User"</span><span>{user_label}</span></div>
            </div>
        </nav>
    }
}
