use axum::extract::{Path, State};
use axum::http::Request;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::db;
use crate::error::ApiError;
use crate::routes::AppState;
use crate::web_ui::session::SessionUser;

#[derive(Deserialize)]
pub struct CreateKeyRequest {
    pub name: Option<String>,
}

pub async fn list_keys_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = request.extensions().get::<SessionUser>()
        .ok_or_else(|| ApiError::AuthError("Not authenticated".to_string()))?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let keys = db::get_key_usage_stats(db_pool, &user.user_id).await?;
    Ok(Json(json!({ "keys": keys })))
}

pub async fn create_key_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = request.extensions().get::<SessionUser>()
        .ok_or_else(|| ApiError::AuthError("Not authenticated".to_string()))?
        .clone();
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    // Parse optional body
    let body_bytes = axum::body::to_bytes(request.into_body(), 1024)
        .await
        .unwrap_or_default();
    let name = if body_bytes.is_empty() {
        String::new()
    } else {
        serde_json::from_slice::<CreateKeyRequest>(&body_bytes)
            .map(|r| r.name.unwrap_or_default())
            .unwrap_or_default()
    };

    // Generate key: sk- + 64 hex chars (32 bytes)
    let mut key_bytes = [0u8; 32];
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(&mut key_bytes);
    let raw_key = format!("sk-{}", hex::encode(key_bytes));
    let key_prefix = format!("sk-{}", &hex::encode(key_bytes)[..8]);
    let key_hash = hex::encode(Sha256::digest(raw_key.as_bytes()));

    let key_id = db::create_api_key(db_pool, &user.user_id, &key_hash, &key_prefix, &name).await
        .map_err(|e| ApiError::ValidationError(e.to_string()))?;

    Ok(Json(json!({
        "id": key_id,
        "key": raw_key,
        "prefix": key_prefix,
        "name": name,
    })))
}

pub async fn delete_key_handler(
    State(state): State<AppState>,
    Path(key_id): Path<String>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = request.extensions().get::<SessionUser>()
        .ok_or_else(|| ApiError::AuthError("Not authenticated".to_string()))?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let deleted = db::delete_api_key(db_pool, &key_id, &user.user_id).await?;
    if !deleted {
        return Err(ApiError::NotFound("API key not found".to_string()));
    }

    Ok(Json(json!({"status": "ok"})))
}
