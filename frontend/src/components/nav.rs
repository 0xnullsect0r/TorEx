use leptos::*;
use leptos_router::A;

const PAIRS: &[&str] = &[
    "BTC/USDT", "ETH/USDT", "BNB/USDT", "SOL/USDT", "XRP/USDT",
    "DOGE/USDT", "ADA/USDT", "AVAX/USDT", "DOT/USDT", "MATIC/USDT",
    "LTC/USDT", "SHIB/USDT", "TRX/USDT", "LINK/USDT", "UNI/USDT",
    "ATOM/USDT", "XLM/USDT", "BCH/USDT", "FIL/USDT", "APT/USDT",
];

#[component]
pub fn Nav(
    pair: ReadSignal<String>,
    set_pair: WriteSignal<String>,
    #[prop(optional)] pairs: Option<&'static [&'static str]>,
) -> impl IntoView {
    let pair_list = pairs.unwrap_or(PAIRS);
    view! {
        <nav class="top-nav">
            <div class="nav-logo">
                <A href="/trading">"TorEx"</A>
            </div>

            <select class="pair-select"
                prop:value=move || pair.get()
                on:change=move |ev| set_pair.set(event_target_value(&ev))>
                {pair_list.iter().map(|p| view! {
                    <option value=*p>{*p}</option>
                }).collect::<Vec<_>>()}
            </select>

            <div class="nav-links">
                <A href="/trading">"Trade"</A>
                <A href="/wallet">"Wallet"</A>
                <A href="/admin/login">"Admin"</A>
            </div>
        </nav>
    }
}
