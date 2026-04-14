use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

use crate::error::ApiError;
use crate::routes::{AppState, KiroCreds};

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let raw_key = extract_api_key(&request).ok_or_else(|| {
        tracing::warn!(
            method = %request.method(),
            path = %request.uri().path(),
            "Access attempt with invalid or missing API key"
        );
        ApiError::AuthError("Invalid or missing API Key".to_string())
    })?;

    // Constant-time hash comparison
    let incoming_hash: [u8; 32] = Sha256::digest(raw_key.as_bytes()).into();
    if !bool::from(incoming_hash.ct_eq(&state.proxy_api_key_hash)) {
        return Err(ApiError::AuthError("Invalid API key".to_string()));
    }

    // Get access token from auth manager
    let (access_token, region) = {
        let auth = state.auth_manager.read().await;
        let token = auth.get_access_token().await.map_err(|e| {
            tracing::error!(error = %e, "Proxy auth token refresh failed");
            ApiError::AuthError("Proxy authentication unavailable".to_string())
        })?;
        let region = auth.get_region().await;
        (token, region)
    };

    request.extensions_mut().insert(KiroCreds {
        access_token,
        region,
    });

    Ok(next.run(request).await)
}

fn extract_api_key(request: &Request<Body>) -> Option<String> {
    // Authorization: Bearer <key>
    if let Some(auth_header) = request.headers().get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(key) = auth_str.strip_prefix("Bearer ") {
                if !key.is_empty() {
                    return Some(key.to_string());
                }
            }
        }
    }

    // x-api-key header
    if let Some(api_key_header) = request.headers().get("x-api-key") {
        if let Ok(key_str) = api_key_header.to_str() {
            if !key_str.is_empty() {
                return Some(key_str.to_string());
            }
        }
    }

    // Query parameter: api_key=<key>
    if let Some(query) = request.uri().query() {
        for param in query.split('&') {
            if let Some(key) = param.strip_prefix("api_key=") {
                let decoded = urlencoding::decode(key).unwrap_or_default();
                if !decoded.is_empty() {
                    return Some(decoded.into_owned());
                }
            }
        }
    }

    None
}

pub fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::any())
        .allow_methods(AllowMethods::any())
        .allow_headers(AllowHeaders::any())
}
