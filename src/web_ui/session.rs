use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use chrono::Utc;

use crate::db;
use crate::error::ApiError;
use crate::routes::AppState;

/// Session info injected into request extensions.
#[derive(Debug, Clone)]
pub struct SessionUser {
    pub user_id: String,
    pub email: String,
    pub role: String,
    pub status: String,
}

pub async fn session_middleware(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    // Extract session cookie
    let session_id = request
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|c| {
                let c = c.trim();
                c.strip_prefix("kp_session=").map(|v| v.to_string())
            })
        })
        .ok_or_else(|| ApiError::AuthError("Not authenticated".to_string()))?;

    // Look up session
    let session = db::get_session(db_pool, &session_id).await
        .map_err(|e| ApiError::Internal(e))?
        .ok_or_else(|| ApiError::AuthError("Invalid session".to_string()))?;

    // Check expiry
    let expires_at = chrono::DateTime::parse_from_rfc3339(&session.expires_at)
        .map_err(|_| ApiError::AuthError("Invalid session".to_string()))?;
    if Utc::now() > expires_at {
        let _ = db::delete_session(db_pool, &session_id).await;
        return Err(ApiError::AuthError("Session expired".to_string()));
    }

    request.extensions_mut().insert(SessionUser {
        user_id: session.user_id,
        email: session.email,
        role: session.role,
        status: session.status,
    });

    Ok(next.run(request).await)
}
