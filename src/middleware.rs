use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

use crate::db;
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

    // 1. Check PROXY_API_KEY (backward compat, always works)
    let incoming_hash: [u8; 32] = Sha256::digest(raw_key.as_bytes()).into();
    if bool::from(incoming_hash.ct_eq(&state.proxy_api_key_hash)) {
        // Check if global kiro is enabled
        if !state.global_kiro_enabled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(ApiError::Forbidden("Global Kiro account is disabled".to_string()));
        }
        // Proxy-only path: use global AuthManager
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
            user_id: None,
            api_key_id: None,
            access_token,
            region,
        });
        return Ok(next.run(request).await);
    }

    // 2. Multi-user path: look up API key in DB
    let db_pool = state.db.as_ref().ok_or_else(|| {
        ApiError::AuthError("Invalid API key".to_string())
    })?;

    let key_hash = hex::encode(incoming_hash);

    // Check cache first
    let cached = state.api_key_cache.get(&key_hash).map(|v| v.clone());
    let (key_id, user_id) = if let Some((key_id, user_id)) = cached {
        (key_id, user_id)
    } else {
        // DB fallback
        let result = db::get_api_key_by_hash(db_pool, &key_hash).await
            .map_err(|e| ApiError::Internal(e))?
            .ok_or_else(|| ApiError::AuthError("Invalid API key".to_string()))?;

        // Cache (bounded)
        if state.api_key_cache.len() < 10_000 {
            state.api_key_cache.insert(key_hash, (result.0.clone(), result.1.clone()));
        }
        result
    };

    // Check user status — only active users can use the API
    let user_info = db::get_user_status(db_pool, &user_id).await
        .map_err(|e| ApiError::Internal(e))?;
    let pool_allowed = match user_info.as_ref().map(|(s, _)| s.as_str()) {
        Some("active") => user_info.unwrap().1,
        Some("pending") => return Err(ApiError::Forbidden("Account pending admin approval".to_string())),
        Some("rejected") => return Err(ApiError::Forbidden("Account has been rejected".to_string())),
        _ => return Err(ApiError::AuthError("Invalid API key".to_string())),
    };

    // 3. Resolve Kiro token for this user
    let default_region = state.config.read().unwrap_or_else(|p| p.into_inner()).kiro_region.clone();

    // Check kiro_token_cache (4-min TTL)
    let cached_token = state.kiro_token_cache.get(&user_id).and_then(|entry| {
        let (ref token, ref region, cached_at) = *entry;
        if cached_at.elapsed().as_secs() < 240 {
            Some((token.clone(), region.clone()))
        } else {
            None
        }
    });

    if let Some((access_token, region)) = cached_token {
        request.extensions_mut().insert(KiroCreds {
            user_id: Some(user_id),
            api_key_id: Some(key_id.clone()),
            access_token,
            region,
        });
        return Ok(next.run(request).await);
    }

    // DB lookup for user's Kiro token
    if let Some(kiro_row) = db::get_kiro_token(db_pool, &user_id).await
        .map_err(|e| ApiError::Internal(e))?
    {
        if kiro_row.enabled {
            if let Some(ref access_token) = kiro_row.access_token {
                let region = kiro_row.sso_region.as_deref().unwrap_or(&default_region).to_string();

                // Cache it
                if state.kiro_token_cache.len() < 10_000 {
                    state.kiro_token_cache.insert(
                        user_id.clone(),
                        (access_token.clone(), region.clone(), std::time::Instant::now()),
                    );
                }

                request.extensions_mut().insert(KiroCreds {
                    user_id: Some(user_id),
                    api_key_id: Some(key_id.clone()),
                    access_token: access_token.clone(),
                    region,
                });
                return Ok(next.run(request).await);
            }
        }
    }

    // 4. Fallback: pool scheduler (only if user is allowed)
    if pool_allowed {
        if let Some(pool_token) = state.pool_scheduler.next_token(db_pool, &default_region).await {
            request.extensions_mut().insert(KiroCreds {
                user_id: Some(user_id),
                api_key_id: Some(key_id.clone()),
                access_token: pool_token.access_token,
                region: pool_token.region,
            });
            return Ok(next.run(request).await);
        }
    }

    // 5. Last resort: try global AuthManager (if enabled)
    if state.global_kiro_enabled.load(std::sync::atomic::Ordering::Relaxed) {
        let auth = state.auth_manager.read().await;
        if let Ok(token) = auth.get_access_token().await {
            let region = auth.get_region().await;
            request.extensions_mut().insert(KiroCreds {
                user_id: Some(user_id),
                api_key_id: Some(key_id),
                access_token: token,
                region,
            });
            return Ok(next.run(request).await);
        }
    }

    Err(ApiError::KiroTokenRequired)
}

fn extract_api_key(request: &Request<Body>) -> Option<String> {
    if let Some(auth_header) = request.headers().get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(key) = auth_str.strip_prefix("Bearer ") {
                if !key.is_empty() {
                    return Some(key.to_string());
                }
            }
        }
    }

    if let Some(api_key_header) = request.headers().get("x-api-key") {
        if let Ok(key_str) = api_key_header.to_str() {
            if !key_str.is_empty() {
                return Some(key_str.to_string());
            }
        }
    }

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
