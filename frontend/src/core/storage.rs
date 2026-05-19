use gloo_storage::{LocalStorage, Storage};

const SESSION_KEY: &str = "torex_session_id";
const MNEMONIC_KEY: &str = "torex_mnemonic";
const ADMIN_SESSION_KEY: &str = "torex_admin_session";

pub fn get_session_id() -> Option<String> {
    LocalStorage::get::<String>(SESSION_KEY).ok()
}

pub fn set_session_id(id: &str) {
    let _ = LocalStorage::set(SESSION_KEY, id);
}

pub fn clear_session_id() {
    LocalStorage::delete(SESSION_KEY);
}

pub fn get_mnemonic() -> Option<String> {
    LocalStorage::get::<String>(MNEMONIC_KEY).ok()
}

pub fn set_mnemonic(mnemonic: &str) {
    let _ = LocalStorage::set(MNEMONIC_KEY, mnemonic);
}

pub fn get_admin_session() -> Option<String> {
    LocalStorage::get::<String>(ADMIN_SESSION_KEY).ok()
}

pub fn set_admin_session(token: &str) {
    let _ = LocalStorage::set(ADMIN_SESSION_KEY, token);
}

pub fn clear_admin_session() {
    LocalStorage::delete(ADMIN_SESSION_KEY);
}

pub fn clear_all() {
    LocalStorage::clear();
}
