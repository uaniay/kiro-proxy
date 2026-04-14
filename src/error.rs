use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Authentication failed: {0}")]
    AuthError(String),

    #[error("Kiro API error: {status} - {message}")]
    KiroApiError { status: u16, message: String },

    #[error("Configuration error: {0}")]
    #[allow(dead_code)]
    ConfigError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Not found: {0}")]
    #[allow(dead_code)]
    NotFound(String),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match self {
            ApiError::AuthError(msg) => (StatusCode::UNAUTHORIZED, "auth_error", msg),
            ApiError::KiroApiError { status, message } => {
                let status_code =
                    StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status_code, "kiro_api_error", message)
            }
            ApiError::ConfigError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "config_error", msg),
            ApiError::ValidationError(msg) => (StatusCode::BAD_REQUEST, "validation_error", msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg),
            ApiError::Internal(err) => {
                tracing::error!("Internal error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "Internal server error".to_string(),
                )
            }
        };

        let body = Json(json!({
            "error": {
                "message": message,
                "type": error_type,
            }
        }));

        (status, body).into_response()
    }
}

#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, ApiError>;
