use gloo_net::http::{Request, RequestBuilder};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::core::storage;

fn attach_user_headers(builder: RequestBuilder) -> RequestBuilder {
    if let Some(sid) = storage::get_session_id() { builder.header("X-Session-Id", &sid) } else { builder }
}

fn attach_admin_headers(builder: RequestBuilder) -> RequestBuilder {
    if let Some(tok) = storage::get_admin_session() { builder.header("X-Admin-Session", &tok) } else { builder }
}

async fn send_builder(builder: RequestBuilder) -> Result<Value, String> {
    let response = builder.send().await.map_err(|e| e.to_string())?;
    let ok = response.ok();
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if !ok { return Err(format!("HTTP {status}: {}", if text.trim().is_empty() { "<empty>" } else { &text })); }
    if text.trim().is_empty() { Ok(Value::Null) } else { serde_json::from_str(&text).map_err(|e| e.to_string()) }
}

async fn send_request(request: Request) -> Result<Value, String> {
    let response = request.send().await.map_err(|e| e.to_string())?;
    let ok = response.ok();
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if !ok { return Err(format!("HTTP {status}: {}", if text.trim().is_empty() { "<empty>" } else { &text })); }
    if text.trim().is_empty() { Ok(Value::Null) } else { serde_json::from_str(&text).map_err(|e| e.to_string()) }
}

fn with_json_body<T: Serialize>(builder: RequestBuilder, body: &T) -> Result<Request, String> {
    let json = serde_json::to_string(body).map_err(|e| e.to_string())?;
    builder
        .header("Content-Type", "application/json")
        .body(json)
        .map_err(|e| format!("{e:?}"))
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| e.to_string())
}

// ── User auth endpoints ──────────────────────────────────────────────────────

pub async fn get_value(path: &str) -> Result<Value, String> {
    send_builder(attach_user_headers(Request::get(path))).await
}
pub async fn get_json<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    decode(get_value(path).await?)
}

pub async fn post_json<T: Serialize, R: DeserializeOwned>(path: &str, body: &T) -> Result<R, String> {
    decode(send_request(with_json_body(attach_user_headers(Request::post(path)), body)?).await?)
}

pub async fn put_json<T: Serialize, R: DeserializeOwned>(path: &str, body: &T) -> Result<R, String> {
    decode(send_request(with_json_body(attach_user_headers(Request::put(path)), body)?).await?)
}

pub async fn delete_value(path: &str) -> Result<Value, String> {
    send_builder(attach_user_headers(Request::delete(path))).await
}

// ── Admin endpoints ──────────────────────────────────────────────────────────

pub async fn admin_get_value(path: &str) -> Result<Value, String> {
    send_builder(attach_admin_headers(Request::get(path))).await
}
pub async fn admin_get_json<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    decode(admin_get_value(path).await?)
}

pub async fn admin_post_json<T: Serialize, R: DeserializeOwned>(path: &str, body: &T) -> Result<R, String> {
    decode(send_request(with_json_body(attach_admin_headers(Request::post(path)), body)?).await?)
}

pub async fn admin_put_json<T: Serialize, R: DeserializeOwned>(path: &str, body: &T) -> Result<R, String> {
    decode(send_request(with_json_body(attach_admin_headers(Request::put(path)), body)?).await?)
}

// ── Convenience aliases ──────────────────────────────────────────────────────

pub async fn get(path: &str) -> Result<Value, String> { get_value(path).await }
pub async fn post(path: &str, body: &Value) -> Result<Value, String> {
    send_request(with_json_body(attach_user_headers(Request::post(path)), body)?).await
}
pub async fn delete(path: &str) -> Result<Value, String> { delete_value(path).await }
pub async fn admin_get(path: &str) -> Result<Value, String> { admin_get_value(path).await }
pub async fn admin_post(path: &str, body: &Value) -> Result<Value, String> {
    send_request(with_json_body(attach_admin_headers(Request::post(path)), body)?).await
}
