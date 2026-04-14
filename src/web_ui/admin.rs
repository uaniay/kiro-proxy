use axum::extract::{Path, State};
use axum::http::Request;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::Ordering;

use crate::db;
use crate::error::ApiError;
use crate::routes::AppState;
use crate::web_ui::session::SessionUser;

fn require_admin(request: &Request<axum::body::Body>) -> Result<SessionUser, ApiError> {
    let user = request.extensions().get::<SessionUser>()
        .ok_or_else(|| ApiError::AuthError("Not authenticated".to_string()))?
        .clone();
    if user.role != "admin" {
        return Err(ApiError::Forbidden("Admin access required".to_string()));
    }
    Ok(user)
}

pub async fn list_users_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&request)?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let users = db::list_users(db_pool).await?;
    let users_json: Vec<serde_json::Value> = users.iter().map(|u| {
        json!({
            "id": u.id,
            "email": u.email,
            "name": u.name,
            "role": u.role,
            "status": u.status,
            "created_at": u.created_at,
            "last_login": u.last_login,
        })
    }).collect();

    Ok(Json(json!({ "users": users_json })))
}

pub async fn delete_user_handler(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let admin = require_admin(&request)?;
    if admin.user_id == user_id {
        return Err(ApiError::ValidationError("Cannot delete yourself".to_string()));
    }
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let deleted = db::delete_user(db_pool, &user_id).await?;
    if !deleted {
        return Err(ApiError::NotFound("User not found".to_string()));
    }

    // Evict caches
    state.kiro_token_cache.remove(&user_id);

    Ok(Json(json!({"status": "ok"})))
}

pub async fn approve_user_handler(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&request)?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let updated = db::approve_user(db_pool, &user_id).await?;
    if !updated {
        return Err(ApiError::NotFound("User not found or not pending".to_string()));
    }

    Ok(Json(json!({"status": "ok"})))
}

pub async fn reject_user_handler(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&request)?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let updated = db::reject_user(db_pool, &user_id).await?;
    if !updated {
        return Err(ApiError::NotFound("User not found or not pending".to_string()));
    }

    Ok(Json(json!({"status": "ok"})))
}

pub async fn list_pool_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&request)?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let entries = db::list_pool_entries(db_pool).await?;
    Ok(Json(json!({ "pool": entries })))
}

#[derive(Deserialize)]
pub struct AddPoolRequest {
    pub label: String,
    pub refresh_token: String,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub sso_region: Option<String>,
}

pub async fn add_pool_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&request)?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let body_bytes = axum::body::to_bytes(request.into_body(), 4096)
        .await
        .map_err(|e| ApiError::ValidationError(format!("Failed to read body: {}", e)))?;
    let body: AddPoolRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| ApiError::ValidationError(format!("Invalid JSON: {}", e)))?;

    if body.label.is_empty() || body.refresh_token.is_empty() {
        return Err(ApiError::ValidationError("label and refresh_token are required".to_string()));
    }

    let id = db::add_pool_entry(
        db_pool, &body.label, &body.refresh_token,
        body.client_id.as_deref(), body.client_secret.as_deref(), body.sso_region.as_deref(),
    ).await
    .map_err(|e| ApiError::ValidationError(e.to_string()))?;

    state.pool_scheduler.invalidate_cache().await;

    Ok(Json(json!({"id": id, "status": "ok"})))
}

pub async fn delete_pool_handler(
    State(state): State<AppState>,
    Path(pool_id): Path<String>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&request)?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let deleted = db::delete_pool_entry(db_pool, &pool_id).await?;
    if !deleted {
        return Err(ApiError::NotFound("Pool entry not found".to_string()));
    }

    state.pool_scheduler.invalidate_cache().await;

    Ok(Json(json!({"status": "ok"})))
}

#[derive(Deserialize)]
pub struct TogglePoolRequest {
    pub enabled: bool,
}

pub async fn toggle_pool_handler(
    State(state): State<AppState>,
    Path(pool_id): Path<String>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&request)?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let body_bytes = axum::body::to_bytes(request.into_body(), 1024)
        .await
        .map_err(|e| ApiError::ValidationError(format!("Failed to read body: {}", e)))?;
    let body: TogglePoolRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| ApiError::ValidationError(format!("Invalid JSON: {}", e)))?;

    let updated = db::toggle_pool_entry(db_pool, &pool_id, body.enabled).await?;
    if !updated {
        return Err(ApiError::NotFound("Pool entry not found".to_string()));
    }

    state.pool_scheduler.invalidate_cache().await;

    Ok(Json(json!({"status": "ok", "enabled": body.enabled})))
}

pub async fn usage_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&request)?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let stats = db::get_all_usage_stats(db_pool).await?;
    Ok(Json(json!({ "usage": stats })))
}

pub async fn list_accounts_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&request)?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let mut accounts: Vec<serde_json::Value> = Vec::new();

    // 1. Global account (from env)
    let (has_global, global_region) = {
        let config = state.config.read().unwrap_or_else(|p| p.into_inner());
        (config.kiro_refresh_token.is_some(), config.kiro_region.clone())
    };

    if has_global {
        accounts.push(json!({
            "id": "global",
            "type": "global",
            "label": "Global (env)",
            "enabled": state.global_kiro_enabled.load(Ordering::Relaxed),
            "region": global_region,
            "has_token": true,
            "last_used": null,
        }));
    }

    // 2. User tokens
    let user_tokens = db::list_all_kiro_tokens(db_pool).await
        .map_err(|e| ApiError::Internal(e))?;
    for t in user_tokens {
        accounts.push(json!({
            "id": t.user_id,
            "type": "user",
            "label": format!("{} ({})", t.email, t.name),
            "enabled": t.enabled,
            "region": t.sso_region,
            "has_token": t.has_token,
            "last_used": t.updated_at,
        }));
    }

    // 3. Pool entries
    let pool_entries = db::list_pool_entries(db_pool).await
        .map_err(|e| ApiError::Internal(e))?;
    for p in pool_entries {
        accounts.push(json!({
            "id": p.id,
            "type": "pool",
            "label": p.label,
            "enabled": p.enabled,
            "region": p.sso_region,
            "has_token": p.access_token.is_some(),
            "last_used": p.last_used,
        }));
    }

    Ok(Json(json!({ "accounts": accounts })))
}

#[derive(Deserialize)]
pub struct ToggleAccountRequest {
    #[serde(rename = "type")]
    pub account_type: String,
    pub enabled: bool,
}

pub async fn toggle_account_handler(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&request)?;

    let body_bytes = axum::body::to_bytes(request.into_body(), 1024)
        .await
        .map_err(|e| ApiError::ValidationError(format!("Failed to read body: {}", e)))?;
    let body: ToggleAccountRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| ApiError::ValidationError(format!("Invalid JSON: {}", e)))?;

    match body.account_type.as_str() {
        "global" => {
            state.global_kiro_enabled.store(body.enabled, Ordering::Relaxed);
        }
        "user" => {
            let db_pool = state.db.as_ref().ok_or_else(|| {
                ApiError::ConfigError("Database not configured".to_string())
            })?;
            let updated = db::toggle_kiro_token(db_pool, &account_id, body.enabled).await
                .map_err(|e| ApiError::Internal(e))?;
            if !updated {
                return Err(ApiError::NotFound("User token not found".to_string()));
            }
            state.kiro_token_cache.remove(&account_id);
        }
        "pool" => {
            let db_pool = state.db.as_ref().ok_or_else(|| {
                ApiError::ConfigError("Database not configured".to_string())
            })?;
            let updated = db::toggle_pool_entry(db_pool, &account_id, body.enabled).await
                .map_err(|e| ApiError::Internal(e))?;
            if !updated {
                return Err(ApiError::NotFound("Pool entry not found".to_string()));
            }
            state.pool_scheduler.invalidate_cache().await;
        }
        _ => {
            return Err(ApiError::ValidationError("Invalid account type".to_string()));
        }
    }

    Ok(Json(json!({ "status": "ok", "enabled": body.enabled })))
}
