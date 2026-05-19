use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use crate::core::{api, crypto, storage};
use crate::core::types::RegisterResponse;

#[component]
pub fn Onboarding() -> impl IntoView {
    let (mode, set_mode) = signal("create".to_string());
    let (generated, set_generated) = signal(crypto::generate_mnemonic());
    let (confirmed, set_confirmed) = signal(false);
    let (import_value, set_import_value) = signal(String::new());
    let (error, set_error) = signal(None::<String>);
    let navigate = use_navigate();

    let register = Action::new_local(move |mnemonic: &String| {
        let mnemonic = mnemonic.clone();
        let navigate = navigate.clone();
        async move {
            if !crypto::validate_mnemonic(&mnemonic) {
                set_error.set(Some("Recovery phrase is invalid.".to_string()));
                return;
            }
            let Some(keys) = crypto::keys_from_mnemonic(&mnemonic) else {
                set_error.set(Some("Failed to derive account keys.".to_string()));
                return;
            };
            let pubkey_bytes: [u8; 32] = hex::decode(&keys.pubkey_hex).ok().and_then(|bytes| bytes.try_into().ok()).unwrap_or([0u8; 32]);
            let (signature, timestamp) = crypto::sign_registration(&keys.signing_key, &pubkey_bytes);
            let body = serde_json::json!({
                "pubkey": keys.pubkey_hex,
                "view_pubkey": keys.view_pubkey_hex,
                "spend_pubkey": keys.spend_pubkey_hex,
                "signature": signature,
                "timestamp": timestamp,
            });
            match api::post_json::<_, RegisterResponse>("/api/auth/register", &body).await {
                Ok(response) => {
                    storage::set_mnemonic(&mnemonic);
                    if !response.session_id.is_empty() { storage::set_session_id(&response.session_id); }
                    if !response.user_id.is_empty() { storage::set_user_id(&response.user_id); }
                    set_error.set(None);
                    navigate("/trade", Default::default());
                }
                Err(message) => set_error.set(Some(message)),
            }
        }
    });

    view! {
        <div class="center-page">
            <div class="auth-card">
                <div class="auth-title">"TorEx Trading Platform"</div>
                <div class="auth-subtitle">"Create or import your private trading identity."</div>
                <div class="tab-row" style="margin-bottom: 18px;">
                    <button class=move || if mode.get() == "create" { "tab-button active" } else { "tab-button" } on:click=move |_| set_mode.set("create".to_string())>"Create Account"</button>
                    <button class=move || if mode.get() == "import" { "tab-button active" } else { "tab-button" } on:click=move |_| set_mode.set("import".to_string())>"Import Account"</button>
                </div>
                {move || if mode.get() == "create" {
                    view! {
                        <>
                            <div class="label">"Recovery Phrase"</div>
                            <div class="text-muted">"Write this down. It controls your account and cannot be recovered."</div>
                            <MnemonicGrid mnemonic=generated />
                            <div class="inline-actions" style="justify-content: space-between; margin: 14px 0 18px;">
                                <label class="inline-actions"><input type="checkbox" prop:checked=move || confirmed.get() on:change=move |_| set_confirmed.update(|value| *value = !*value) /><span>I saved my phrase securely.</span></label>
                                <button class="btn-ghost btn-small" on:click=move |_| { set_generated.set(crypto::generate_mnemonic()); set_confirmed.set(false); }>"Regenerate"</button>
                            </div>
                            <button class="btn-primary" style="width: 100%;" disabled=move || !confirmed.get() || register.pending().get() on:click=move |_| { register.dispatch_local(generated.get()); }>{move || if register.pending().get() { "Creating…" } else { "Create Wallet" }}</button>
                        </>
                    }.into_any()
                } else {
                    view! {
                        <>
                            <label class="label">"Import Recovery Phrase"</label>
                            <textarea class="textarea" placeholder="Enter your 24-word recovery phrase" prop:value=move || import_value.get() on:input=move |event| set_import_value.set(event_target_value(&event))></textarea>
                            <button class="btn-primary" style="width: 100%; margin-top: 16px;" disabled=move || register.pending().get() on:click=move |_| { register.dispatch_local(import_value.get().trim().to_string()); }>{move || if register.pending().get() { "Importing…" } else { "Import Wallet" }}</button>
                        </>
                    }.into_any()
                }}
                {move || error.get().map(|message| view! { <div class="form-error">{message}</div> })}
            </div>
        </div>
    }
}

#[component]
fn MnemonicGrid(mnemonic: ReadSignal<String>) -> impl IntoView {
    view! {
        <div class="mnemonic-grid">
            {move || mnemonic.get().split_whitespace().enumerate().map(|(index, word)| view! { <div class="mnemonic-word"><span class="mnemonic-index">{format!("{:02}", index + 1)}</span><span class="mono">{word.to_string()}</span></div> }).collect::<Vec<_>>()}
        </div>
    }
}
