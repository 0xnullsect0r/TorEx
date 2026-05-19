use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::hooks::use_navigate;

use crate::core::storage;
use crate::pages::{admin_dashboard::AdminDashboard, admin_login::AdminLogin, onboarding::Onboarding, trading::TradingPage, wallet::WalletPage};

#[component]
pub fn App() -> impl IntoView {
    Effect::new(move |_| { storage::apply_theme(storage::get_theme()); });
    view! {
        <Router>
            <main>
                <Routes fallback=|| view! { <div class="center-page"><div class="panel-title">"Not Found"</div></div> }>
                    <Route path=leptos_router::path!("/") view=RootGate />
                    <Route path=leptos_router::path!("/onboarding") view=Onboarding />
                    <Route path=leptos_router::path!("/trade") view=TradingPage />
                    <Route path=leptos_router::path!("/wallet") view=WalletPage />
                    <Route path=leptos_router::path!("/admin") view=AdminLogin />
                    <Route path=leptos_router::path!("/admin/dashboard") view=AdminDashboard />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn RootGate() -> impl IntoView {
    let navigate = use_navigate();
    Effect::new(move |_| {
        if storage::has_mnemonic() { navigate("/trade", Default::default()); }
        else { navigate("/onboarding", Default::default()); }
    });
    view! { <div class="center-page"><div class="spinner"></div></div> }
}
