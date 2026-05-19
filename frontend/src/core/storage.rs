use gloo_storage::{LocalStorage, SessionStorage, Storage};
use web_sys::window;

use crate::core::types::Theme;

const MNEMONIC_KEY: &str = "torex_mnemonic";
const SESSION_KEY: &str = "torex_session_id";
const ADMIN_TOKEN_KEY: &str = "torex_admin_token";
const THEME_KEY: &str = "torex_theme";
const USER_ID_KEY: &str = "torex_user_id";

pub fn get_mnemonic() -> Option<String> { LocalStorage::get(MNEMONIC_KEY).ok() }
pub fn has_mnemonic() -> bool { get_mnemonic().is_some() }
pub fn set_mnemonic(value: &str) { let _ = LocalStorage::set(MNEMONIC_KEY, value); }
pub fn get_session_id() -> Option<String> { SessionStorage::get(SESSION_KEY).ok() }
pub fn set_session_id(id: &str) { let _ = SessionStorage::set(SESSION_KEY, id); }
pub fn clear_session_id() { SessionStorage::delete(SESSION_KEY); }
pub fn get_admin_session() -> Option<String> { SessionStorage::get(ADMIN_TOKEN_KEY).ok() }
pub fn set_admin_session(token: &str) { let _ = SessionStorage::set(ADMIN_TOKEN_KEY, token); }
pub fn clear_admin_session() { SessionStorage::delete(ADMIN_TOKEN_KEY); }
pub fn get_user_id() -> Option<String> { LocalStorage::get(USER_ID_KEY).ok() }
pub fn set_user_id(user_id: &str) { let _ = LocalStorage::set(USER_ID_KEY, user_id); }
pub fn clear_user_id() { LocalStorage::delete(USER_ID_KEY); }

pub fn get_theme() -> Theme {
    LocalStorage::get::<String>(THEME_KEY).map(Theme::from).unwrap_or_default()
}

pub fn set_theme(theme: Theme) {
    let _ = LocalStorage::set(THEME_KEY, theme.as_str());
    apply_theme(theme);
}

pub fn apply_theme(theme: Theme) {
    if let Some(document) = window().and_then(|w| w.document()) {
        if let Some(root) = document.document_element() {
            let _ = root.set_attribute("data-theme", theme.as_str());
        }
    }
}

pub fn clear_all_user_state() {
    clear_session_id();
    clear_user_id();
}

pub fn get(key: &str) -> Option<String> { LocalStorage::get::<String>(key).ok() }
pub fn set(key: &str, value: &str) { let _ = LocalStorage::set(key, value); }
pub fn remove(key: &str) { LocalStorage::delete(key); SessionStorage::delete(key); }
