use leptos::*;
use leptos_router::use_navigate;
use serde_json::json;
use wasm_bindgen::JsCast;
use web_sys::{window, HtmlInputElement};

use crate::core::{api, storage};

#[component]
pub fn AdminLogin() -> impl IntoView {
    let (username, set_username) = create_signal(String::new());
    let (password, set_password) = create_signal(String::new());
    let (totp, set_totp) = create_signal(String::new());
    let (totp_required, set_totp_required) = create_signal(false);
    let (error, set_error) = create_signal(Option::<String>::None);
    let (loading, set_loading) = create_signal(false);

    let navigate = use_navigate();

    let do_login = move |_| {
        let user = username.get();
        let pass = password.get();
        let totp_code = totp.get();
        let nav = navigate.clone();
        set_error.set(None);
        set_loading.set(true);

        wasm_bindgen_futures::spawn_local(async move {
            // hCaptcha token from the DOM (widget should be on the page)
            let hcaptcha_token = window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("hcaptcha-response"))
                .and_then(|el| el.dyn_into::<HtmlInputElement>().ok())
                .map(|inp| inp.value())
                .unwrap_or_default();

            let mut body = json!({
                "username": user,
                "password": pass,
                "hcaptcha_token": hcaptcha_token,
            });
            if !totp_code.is_empty() {
                body["totp_code"] = json!(totp_code);
            }

            match api::admin_post("/admin/api/auth/login", &body).await {
                Ok(resp) => {
                    if resp["totp_required"].as_bool().unwrap_or(false) {
                        set_totp_required.set(true);
                    } else if let Some(token) = resp["session_token"].as_str() {
                        storage::set_admin_session(token);
                        nav("/admin", Default::default());
                    } else {
                        set_error.set(Some("Unexpected response".into()));
                    }
                }
                Err(e) => set_error.set(Some(e)),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="center-page">
            <div class="auth-card">
                <div class="auth-title">"Admin Login"</div>

                <div class="form-group">
                    <label class="form-label">"Username"</label>
                    <input class="form-input" type="text"
                        prop:value=move || username.get()
                        on:input=move |ev| set_username.set(event_target_value(&ev))
                    />
                </div>
                <div class="form-group">
                    <label class="form-label">"Password"</label>
                    <input class="form-input" type="password"
                        prop:value=move || password.get()
                        on:input=move |ev| set_password.set(event_target_value(&ev))
                    />
                </div>

                // hCaptcha widget (rendered by the external script in index.html)
                <div class="h-captcha" data-sitekey="HCAPTCHA_SITE_KEY" style="margin:12px 0;"></div>
                // Hidden input that hCaptcha SDK fills in:
                <input type="hidden" id="hcaptcha-response"/>

                <Show when=move || totp_required.get()>
                    <div class="form-group">
                        <label class="form-label">"Authenticator code"</label>
                        <input class="form-input" type="text" maxlength="6"
                            placeholder="6-digit code"
                            prop:value=move || totp.get()
                            on:input=move |ev| set_totp.set(event_target_value(&ev))
                        />
                    </div>
                </Show>

                {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}

                <button class="btn-primary"
                    disabled=move || loading.get()
                    on:click=do_login>
                    {move || if loading.get() { "Signing in…" } else { "Sign In" }}
                </button>
            </div>
        </div>
    }
}
