use axum::http::HeaderMap;
use serde_json::Value;

const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "x-api-key",
    "cookie",
    "set-cookie",
    "proxy-authorization",
];

pub fn sanitize_headers(headers: &HeaderMap) -> Value {
    let mut map = serde_json::Map::new();
    for (name, value) in headers.iter() {
        let key = name.as_str().to_lowercase();
        if SENSITIVE_HEADERS.contains(&key.as_str()) {
            continue;
        }
        if let Ok(v) = value.to_str() {
            map.insert(key, Value::String(v.to_string()));
        }
    }
    Value::Object(map)
}
