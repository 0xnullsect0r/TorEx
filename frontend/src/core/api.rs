use gloo_net::http::{Request, RequestBuilder};
use serde::Serialize;
use serde_json::Value;
use wasm_bindgen::JsValue;

use crate::core::storage;

fn build(method: &str, path: &str) -> RequestBuilder {
    let rb = Request::new(path).method(method.parse().expect("method"));
    // Attach user session
    if let Some(sid) = storage::get_session_id() {
        rb.header("X-Session-Id", &sid)
    } else {
        rb
    }
}

fn build_admin(method: &str, path: &str) -> RequestBuilder {
    let rb = Request::new(path).method(method.parse().expect("method"));
    if let Some(tok) = storage::get_admin_session() {
        rb.header("X-Admin-Session", &tok)
    } else {
        rb
    }
}

pub async fn get(path: &str) -> Result<Value, String> {
    let resp = build("GET", path)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<Value>().await.map_err(|e| e.to_string())
}

pub async fn post_json<T: Serialize>(path: &str, body: &T) -> Result<Value, String> {
    let resp = build("POST", path)
        .header("Content-Type", "application/json")
        .body(JsValue::from_str(
            &serde_json::to_string(body).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", resp.status(), text));
    }
    resp.json::<Value>().await.map_err(|e| e.to_string())
}

pub async fn delete(path: &str) -> Result<Value, String> {
    let resp = build("DELETE", path)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<Value>().await.map_err(|e| e.to_string())
}

pub async fn admin_get(path: &str) -> Result<Value, String> {
    let resp = build_admin("GET", path)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<Value>().await.map_err(|e| e.to_string())
}

pub async fn admin_post<T: Serialize>(path: &str, body: &T) -> Result<Value, String> {
    let resp = build_admin("POST", path)
        .header("Content-Type", "application/json")
        .body(JsValue::from_str(
            &serde_json::to_string(body).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", resp.status(), text));
    }
    resp.json::<Value>().await.map_err(|e| e.to_string())
}

pub async fn admin_put<T: Serialize>(path: &str, body: &T) -> Result<Value, String> {
    let resp = build_admin("PUT", path)
        .header("Content-Type", "application/json")
        .body(JsValue::from_str(
            &serde_json::to_string(body).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", resp.status(), text));
    }
    resp.json::<Value>().await.map_err(|e| e.to_string())
}
