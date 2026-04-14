use axum::extract::State;
use axum::http::Request;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::oauth;
use crate::db;
use crate::error::ApiError;
use crate::routes::AppState;
use crate::web_ui::session::SessionUser;

#[derive(Deserialize)]
pub struct SetupRequest {
    pub sso_start_url: Option<String>,
    pub sso_region: Option<String>,
}

#[derive(Deserialize)]
pub struct PollRequest {
    pub device_code: String,
}

pub async fn setup_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = request.extensions().get::<SessionUser>()
        .ok_or_else(|| ApiError::AuthError("Not authenticated".to_string()))?
        .clone();
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let body_bytes = axum::body::to_bytes(request.into_body(), 4096)
        .await
        .map_err(|e| ApiError::ValidationError(format!("Failed to read body: {}", e)))?;
    let body: SetupRequest = serde_json::from_slice(&body_bytes)
        .unwrap_or(SetupRequest { sso_start_url: None, sso_region: None });

    let region = body.sso_region.as_deref().unwrap_or("us-east-1");
    let start_url = body.sso_start_url.as_deref().unwrap_or("");

    // Register OAuth client
    let http_client = reqwest::Client::new();
    let registration = oauth::register_client(&http_client, region, "device", None, Some(start_url))
        .await
        .map_err(|e| ApiError::Internal(e))?;

    // Start device authorization
    let device_auth = oauth::start_device_authorization(
        &http_client, region, &registration.client_id, &registration.client_secret, start_url,
    )
    .await
    .map_err(|e| ApiError::Internal(e))?;

    // Store client credentials for polling
    db::upsert_kiro_token(
        db_pool, &user.user_id, "",
        None, None,
        Some(&registration.client_id), Some(&registration.client_secret),
        Some(region), Some(start_url),
    ).await?;

    Ok(Json(json!({
        "device_code": device_auth.device_code,
        "user_code": device_auth.user_code,
        "verification_uri": device_auth.verification_uri,
        "verification_uri_complete": device_auth.verification_uri_complete,
        "expires_in": device_auth.expires_in,
        "interval": device_auth.interval,
    })))
}

pub async fn poll_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = request.extensions().get::<SessionUser>()
        .ok_or_else(|| ApiError::AuthError("Not authenticated".to_string()))?
        .clone();
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let body_bytes = axum::body::to_bytes(request.into_body(), 4096)
        .await
        .map_err(|e| ApiError::ValidationError(format!("Failed to read body: {}", e)))?;
    let body: PollRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| ApiError::ValidationError(format!("Invalid JSON: {}", e)))?;

    // Get stored client credentials
    let kiro_row = db::get_kiro_token(db_pool, &user.user_id).await?
        .ok_or_else(|| ApiError::ValidationError("No setup in progress. Call /api/kiro/setup first.".to_string()))?;

    let client_id = kiro_row.client_id.as_deref()
        .ok_or_else(|| ApiError::ValidationError("Missing client_id".to_string()))?;
    let client_secret = kiro_row.client_secret.as_deref()
        .ok_or_else(|| ApiError::ValidationError("Missing client_secret".to_string()))?;
    let region = kiro_row.sso_region.as_deref().unwrap_or("us-east-1");

    let http_client = reqwest::Client::new();
    let result = oauth::poll_device_token(&http_client, region, client_id, client_secret, &body.device_code)
        .await
        .map_err(|e| ApiError::Internal(e))?;

    match result {
        crate::auth::PollResult::Pending => {
            Ok(Json(json!({"status": "pending", "message": "Waiting for user authorization"})))
        }
        crate::auth::PollResult::SlowDown => {
            Ok(Json(json!({"status": "slow_down", "message": "Polling too fast"})))
        }
        crate::auth::PollResult::Success(token) => {
            let expires_in = token.expires_in.unwrap_or(3600);
            let expiry = (chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64 - 60)).to_rfc3339();

            db::upsert_kiro_token(
                db_pool, &user.user_id, &token.refresh_token.unwrap_or_default(),
                Some(&token.access_token), Some(&expiry),
                Some(client_id), Some(client_secret),
                Some(region), kiro_row.sso_start_url.as_deref(),
            ).await?;

            // Invalidate token cache
            state.kiro_token_cache.remove(&user.user_id);

            Ok(Json(json!({"status": "success", "message": "Kiro token bound successfully"})))
        }
    }
}

pub async fn status_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = request.extensions().get::<SessionUser>()
        .ok_or_else(|| ApiError::AuthError("Not authenticated".to_string()))?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let kiro_row = db::get_kiro_token(db_pool, &user.user_id).await?;

    match kiro_row {
        Some(row) => {
            let has_token = row.access_token.is_some();
            let expired = row.token_expiry.as_ref().map_or(true, |exp| {
                chrono::DateTime::parse_from_rfc3339(exp)
                    .map(|dt| chrono::Utc::now() > dt)
                    .unwrap_or(true)
            });
            Ok(Json(json!({
                "has_token": has_token,
                "expired": expired,
                "sso_region": row.sso_region,
                "sso_start_url": row.sso_start_url,
            })))
        }
        None => Ok(Json(json!({
            "has_token": false,
            "expired": true,
        }))),
    }
}

pub async fn delete_token_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = request.extensions().get::<SessionUser>()
        .ok_or_else(|| ApiError::AuthError("Not authenticated".to_string()))?;
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    db::delete_kiro_token(db_pool, &user.user_id).await?;
    state.kiro_token_cache.remove(&user.user_id);

    Ok(Json(json!({"status": "ok"})))
}
