use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, window};

use crate::core::{api, storage};

#[component]
pub fn AdminLogin() -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (totp, set_totp) = signal(String::new());
    let (totp_required, set_totp_required) = signal(false);
    let (error, set_error) = signal(None::<String>);
    let navigate = use_navigate();

    let login = Action::new_local(move |_: &()| {
        let username = username.get();
        let password = password.get();
        let totp = totp.get();
        let navigate = navigate.clone();
        async move {
            let captcha = window().and_then(|win| win.document()).and_then(|document| document.get_element_by_id("hcaptcha-response")).and_then(|node| node.dyn_into::<HtmlInputElement>().ok()).map(|input| input.value()).unwrap_or_default();
            let mut body = serde_json::json!({ "username": username, "password": password, "hcaptcha_token": captcha });
            if !totp.is_empty() { body["totp_code"] = serde_json::json!(totp); }
            match api::admin_post_json::<_, serde_json::Value>("/admin/api/auth/login", &body).await {
                Ok(response) => {
                    if response["totp_required"].as_bool().unwrap_or(false) { set_totp_required.set(true); set_error.set(Some("Enter your TOTP code to continue.".to_string())); }
                    else if let Some(token) = response["session_token"].as_str() { storage::set_admin_session(token); set_error.set(None); navigate("/admin/dashboard", Default::default()); }
                    else { set_error.set(Some("Unexpected admin login response.".to_string())); }
                }
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    view! {
        <div class="center-page">
            <div class="auth-card">
                <div class="auth-title">"Admin Console"</div>
                <div class="auth-subtitle">"Protected access for platform operations."</div>
                <div class="form-grid">
                    <div><label class="label">"Username"</label><input class="input" type="text" prop:value=move || username.get() on:input=move |event| set_username.set(event_target_value(&event)) /></div>
                    <div><label class="label">"Password"</label><input class="input" type="password" prop:value=move || password.get() on:input=move |event| set_password.set(event_target_value(&event)) /></div>
                    <div class="h-captcha" data-sitekey="HCAPTCHA_SITE_KEY"></div>
                    <input type="hidden" id="hcaptcha-response" />
                    {move || if totp_required.get() { view! { <div><label class="label">"TOTP"</label><input class="input" type="text" maxlength="6" prop:value=move || totp.get() on:input=move |event| set_totp.set(event_target_value(&event)) /></div> }.into_any() } else { view! { <></> }.into_any() }}
                </div>
                {move || error.get().map(|message| view! { <div class="form-error">{message}</div> })}
                <button class="btn-primary" style="width: 100%; margin-top: 18px;" disabled=move || login.pending().get() on:click=move |_| { login.dispatch_local(()); }>{move || if login.pending().get() { "Signing in…" } else { "Sign In" }}</button>
            </div>
        </div>
    }
}
