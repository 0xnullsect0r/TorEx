use leptos::*;
use leptos_router::*;

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
                <Routes>
                    <Route path="/"                 view=AuthGate/>
                    <Route path="/onboarding"       view=Onboarding/>
                    <Route path="/trading"          view=Trading/>
                    <Route path="/wallet"           view=Wallet/>
                    <Route path="/admin/login"      view=AdminLogin/>
                    <Route path="/admin"            view=AdminDashboard/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn AuthGate() -> impl IntoView {
    let navigate = use_navigate();
    create_effect(move |_| {
        let dest = if storage::get_session_id().is_some() {
            "/trading"
        } else {
            "/onboarding"
        };
        navigate(dest, Default::default());
    });
    view! { <div class="center-page"><div class="spinner"></div></div> }
}
