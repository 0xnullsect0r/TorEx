use leptos::*;
use leptos_router::use_navigate;
use serde_json::json;

use crate::core::{api, crypto, storage};

#[component]
pub fn Onboarding() -> impl IntoView {
    let (show_import, set_show_import) = create_signal(false);
    let (generated, set_generated) = create_signal(crypto::generate_mnemonic());
    let (confirmed, set_confirmed) = create_signal(false);
    let (import_val, set_import_val) = create_signal(String::new());
    let (error, set_error) = create_signal(Option::<String>::None);
    let (loading, set_loading) = create_signal(false);

    let navigate = use_navigate();

    let register = move |mnemonic: String| {
        if !crypto::validate_mnemonic(&mnemonic) {
            set_error.set(Some("Invalid recovery phrase".into()));
            return;
        }
        let Some(keys) = crypto::keys_from_mnemonic(&mnemonic) else {
            set_error.set(Some("Key derivation failed".into()));
            return;
        };
        let nav = navigate.clone();
        set_loading.set(true);
        set_error.set(None);

        let pubkey_bytes: [u8; 32] = hex::decode(&keys.pubkey_hex)
            .ok()
            .and_then(|b| b.try_into().ok())
            .unwrap_or([0u8; 32]);
        let (sig_hex, timestamp) = crypto::sign_registration(&keys.signing_key, &pubkey_bytes);

        wasm_bindgen_futures::spawn_local(async move {
            let body = json!({
                "pubkey":       keys.pubkey_hex,
                "view_pubkey":  keys.view_pubkey_hex,
                "spend_pubkey": keys.spend_pubkey_hex,
                "signature":    sig_hex,
                "timestamp":    timestamp,
            });
            match api::post_json("/api/auth/register", &body).await {
                Ok(resp) => {
                    if let Some(sid) = resp["session_id"].as_str() {
                        storage::set_session_id(sid);
                        storage::set_mnemonic(&mnemonic);
                        nav("/trading", Default::default());
                    } else {
                        set_error.set(Some("No session_id in response".into()));
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
                <div class="auth-title">"TorEx"</div>
                <div class="auth-sub">"Privacy-first USDT exchange · Tor hidden service"</div>

                <Show when=move || !show_import.get()>
                    <div class="card-title">"Your recovery phrase"</div>
                    <p class="text-muted" style="font-size:11px;margin-bottom:12px;">
                        "Write these 24 words down. They are your identity — no email, no password."
                    </p>

                    <MnemonicGrid mnemonic=generated.get()/>

                    <div class="flex gap-8 mt-12">
                        <input
                            type="checkbox"
                            id="confirm-cb"
                            prop:checked=move || confirmed.get()
                            on:change=move |_| set_confirmed.update(|v| *v = !*v)
                        />
                        <label for="confirm-cb" style="font-size:12px;">
                            "I have written down my recovery phrase"
                        </label>
                    </div>

                    <button
                        class="btn-primary mt-16"
                        disabled=move || !confirmed.get() || loading.get()
                        on:click=move |_| register(generated.get())
                    >
                        {move || if loading.get() { "Creating…" } else { "Create Wallet" }}
                    </button>
                    <button
                        class="btn-primary mt-8"
                        style="background:transparent;color:var(--muted);border:1px solid var(--border);"
                        on:click=move |_| set_show_import.set(true)
                    >
                        "Import existing wallet"
                    </button>
                </Show>

                <Show when=move || show_import.get()>
                    <div class="card-title">"Import wallet"</div>
                    <div class="form-group">
                        <textarea
                            class="form-input"
                            rows="4"
                            placeholder="Enter your 24-word recovery phrase…"
                            prop:value=move || import_val.get()
                            on:input=move |ev| set_import_val.set(event_target_value(&ev))
                        />
                    </div>
                    <button
                        class="btn-primary"
                        disabled=move || loading.get()
                        on:click=move |_| register(import_val.get().trim().to_string())
                    >
                        {move || if loading.get() { "Importing…" } else { "Import Wallet" }}
                    </button>
                    <button
                        class="btn-primary mt-8"
                        style="background:transparent;color:var(--muted);border:1px solid var(--border);"
                        on:click=move |_| { set_show_import.set(false); set_error.set(None); }
                    >
                        "← Back"
                    </button>
                </Show>

                {move || error.get().map(|e| view! {
                    <div class="form-error mt-8">{e}</div>
                })}
            </div>
        </div>
    }
}

#[component]
fn MnemonicGrid(mnemonic: String) -> impl IntoView {
    let words: Vec<(usize, String)> = mnemonic
        .split_whitespace()
        .enumerate()
        .map(|(i, w)| (i + 1, w.to_string()))
        .collect();
    view! {
        <div class="mnemonic-grid">
            <For
                each=move || words.clone()
                key=|(i, _)| *i
                children=move |(n, word)| view! {
                    <div class="mnemonic-word">
                        <span class="mnemonic-num">{n}"."</span>
                        <span class="mnemonic-text">{word}</span>
                    </div>
                }
            />
        </div>
    }
}
