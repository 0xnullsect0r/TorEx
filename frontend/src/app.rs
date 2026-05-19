use leptos::prelude::*;
use leptos_router::components::*;
use leptos_router::hooks::use_navigate;

use crate::core::storage;
use crate::pages::{
    admin_dashboard::AdminDashboard, admin_login::AdminLogin, onboarding::Onboarding,
    trading::Trading, wallet::Wallet,
};

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <main>
                <Routes fallback=|| "Not found.">
                    <Route path=leptos_router::path!("/")          view=AuthGate/>
                    <Route path=leptos_router::path!("/onboarding") view=Onboarding/>
                    <Route path=leptos_router::path!("/trading")    view=Trading/>
                    <Route path=leptos_router::path!("/wallet")     view=Wallet/>
                    <Route path=leptos_router::path!("/admin/login") view=AdminLogin/>
                    <Route path=leptos_router::path!("/admin")      view=AdminDashboard/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn AuthGate() -> impl IntoView {
    let navigate = use_navigate();
    Effect::new(move || {
        let dest = if storage::get_session_id().is_some() {
            "/trading"
        } else {
            "/onboarding"
        };
        navigate(dest, Default::default());
    });
    view! { <div class="center-page"><div class="spinner"></div></div> }
}
