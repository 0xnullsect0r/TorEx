use gloo_net::http::{Request, RequestBuilder};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::core::storage;

fn attach_user_headers(builder: RequestBuilder) -> RequestBuilder {
    if let Some(session_id) = storage::get_session_id() { builder.header("X-Session-Id", &session_id) } else { builder }
}

fn attach_admin_headers(builder: RequestBuilder) -> RequestBuilder {
    if let Some(token) = storage::get_admin_session() { builder.header("X-Admin-Session", &token) } else { builder }
}

async fn send_value(builder: RequestBuilder) -> Result<Value, String> {
    let response = builder.send().await.map_err(|error| error.to_string())?;
    let status = response.status();
    let ok = response.ok();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !ok {
        return Err(format!("HTTP {status}: {}", if text.trim().is_empty() { "<empty>".to_string() } else { text }));
    }
    if text.trim().is_empty() { Ok(Value::Null) } else { serde_json::from_str(&text).map_err(|error| error.to_string()) }
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

pub async fn get_value(path: &str) -> Result<Value, String> { send_value(attach_user_headers(Request::get(path))).await }
pub async fn get_json<T: DeserializeOwned>(path: &str) -> Result<T, String> { decode(get_value(path).await?) }

pub async fn post_json<T: Serialize, R: DeserializeOwned>(path: &str, body: &T) -> Result<R, String> {
    let builder = attach_user_headers(Request::post(path)).header("Content-Type", "application/json").json(body).map_err(|error| error.to_string())?;
    decode(send_value(builder).await?)
}

pub async fn put_json<T: Serialize, R: DeserializeOwned>(path: &str, body: &T) -> Result<R, String> {
    let builder = attach_user_headers(Request::put(path)).header("Content-Type", "application/json").json(body).map_err(|error| error.to_string())?;
    decode(send_value(builder).await?)
}

pub async fn delete_value(path: &str) -> Result<Value, String> { send_value(attach_user_headers(Request::delete(path))).await }
pub async fn admin_get_value(path: &str) -> Result<Value, String> { send_value(attach_admin_headers(Request::get(path))).await }
pub async fn admin_get_json<T: DeserializeOwned>(path: &str) -> Result<T, String> { decode(admin_get_value(path).await?) }

pub async fn admin_post_json<T: Serialize, R: DeserializeOwned>(path: &str, body: &T) -> Result<R, String> {
    let builder = attach_admin_headers(Request::post(path)).header("Content-Type", "application/json").json(body).map_err(|error| error.to_string())?;
    decode(send_value(builder).await?)
}

pub async fn admin_put_json<T: Serialize, R: DeserializeOwned>(path: &str, body: &T) -> Result<R, String> {
    let builder = attach_admin_headers(Request::put(path)).header("Content-Type", "application/json").json(body).map_err(|error| error.to_string())?;
    decode(send_value(builder).await?)
}

// Convenience aliases
pub async fn get(path: &str) -> Result<Value, String> { get_value(path).await }
pub async fn post(path: &str, body: &Value) -> Result<Value, String> {
    let builder = attach_user_headers(Request::post(path)).header("Content-Type", "application/json").json(body).map_err(|e| e.to_string())?;
    send_value(builder).await
}
pub async fn delete(path: &str) -> Result<Value, String> { delete_value(path).await }
pub async fn admin_get(path: &str) -> Result<Value, String> { admin_get_value(path).await }
pub async fn admin_post(path: &str, body: &Value) -> Result<Value, String> {
    let builder = attach_admin_headers(Request::post(path)).header("Content-Type", "application/json").json(body).map_err(|e| e.to_string())?;
    send_value(builder).await
}
