use axum::extract::State;
use axum::http::Request;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db;
use crate::error::ApiError;
use crate::routes::AppState;
use crate::web_ui::session::SessionUser;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub name: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub user_id: String,
    pub email: String,
    pub role: String,
    pub status: String,
}

pub async fn register_handler(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    if body.email.is_empty() || body.password.is_empty() || body.name.is_empty() {
        return Err(ApiError::ValidationError("email, name, and password are required".to_string()));
    }

    if body.password.len() < 8 {
        return Err(ApiError::ValidationError("Password must be at least 8 characters".to_string()));
    }

    // Check if email already exists
    if db::get_user_by_email(db_pool, &body.email).await?.is_some() {
        return Err(ApiError::ValidationError("Email already registered".to_string()));
    }

    // Hash password with Argon2id
    let password_hash = tokio::task::spawn_blocking({
        let password = body.password.clone();
        move || hash_password(&password)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!("Password hashing failed: {}", e)))?
    .map_err(|e| ApiError::Internal(anyhow::anyhow!("Password hashing failed: {}", e)))?;

    let (user_id, role, status) = db::create_user(db_pool, &body.email, &body.name, &password_hash).await
        .map_err(|e| ApiError::Internal(e))?;

    // Create session
    let session_id = db::create_session(db_pool, &user_id).await?;
    db::update_last_login(db_pool, &user_id).await?;

    let cookie = format!(
        "kp_session={}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400",
        session_id
    );

    Ok((
        [("set-cookie", cookie)],
        Json(AuthResponse { user_id, email: body.email, role, status }),
    ))
}

pub async fn login_handler(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    let user = db::get_user_by_email(db_pool, &body.email).await?
        .ok_or(ApiError::InvalidCredentials)?;

    // Verify password
    let valid = tokio::task::spawn_blocking({
        let password = body.password.clone();
        let hash = user.password_hash.clone();
        move || verify_password(&password, &hash)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!("Password verification failed: {}", e)))?
    .map_err(|_| ApiError::InvalidCredentials)?;

    if !valid {
        return Err(ApiError::InvalidCredentials);
    }

    let session_id = db::create_session(db_pool, &user.id).await?;
    db::update_last_login(db_pool, &user.id).await?;

    let cookie = format!(
        "kp_session={}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400",
        session_id
    );

    Ok((
        [("set-cookie", cookie)],
        Json(AuthResponse { user_id: user.id, email: user.email, role: user.role, status: user.status }),
    ))
}

pub async fn logout_handler(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
) -> Result<impl IntoResponse, ApiError> {
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::ConfigError("Database not configured".to_string())
    })?;

    // Extract session cookie
    if let Some(session_id) = request
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|c| {
                c.trim().strip_prefix("kp_session=").map(|v| v.to_string())
            })
        })
    {
        let _ = db::delete_session(db_pool, &session_id).await;
    }

    let cookie = "kp_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";

    Ok((
        [("set-cookie", cookie.to_string())],
        Json(json!({"status": "ok"})),
    ))
}

pub async fn me_handler(
    request: Request<axum::body::Body>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = request.extensions().get::<SessionUser>()
        .ok_or_else(|| ApiError::AuthError("Not authenticated".to_string()))?;

    Ok(Json(json!({
        "user_id": user.user_id,
        "email": user.email,
        "role": user.role,
        "status": user.status,
    })))
}

fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    use argon2::{Argon2, PasswordHasher};
    use argon2::password_hash::SaltString;
    use argon2::password_hash::rand_core::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    use argon2::{Argon2, PasswordVerifier};
    use argon2::PasswordHash;

    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}
