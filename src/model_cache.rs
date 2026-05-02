use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::auth::AuthManager;
use crate::converters::core::normalize_model_name;
use crate::http_client::KiroHttpClient;

/// Thread-safe cache mapping normalized model names to valid Kiro API model IDs.
#[derive(Clone)]
pub struct ModelCache {
    /// normalized_name → actual Kiro model ID
    inner: Arc<DashMap<String, String>>,
    /// Prevent concurrent refreshes
    refreshing: Arc<AtomicBool>,
}

impl ModelCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            refreshing: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Resolve a model name: normalize it, then look up in cache.
    /// Falls back to the normalized name if not found (pass-through).
    pub fn resolve(&self, name: &str) -> String {
        let normalized = normalize_model_name(name);
        if let Some(id) = self.inner.get(&normalized) {
            return id.clone();
        }
        if let Some(id) = self.inner.get(name) {
            return id.clone();
        }
        normalized
    }

    /// Populate cache from Kiro's ListAvailableModels API using global auth.
    pub async fn refresh(
        &self,
        http_client: &KiroHttpClient,
        auth_manager: &AuthManager,
    ) {
        let access_token = match auth_manager.get_access_token().await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("ModelCache: cannot refresh, no access token: {}", e);
                return;
            }
        };
        let region = auth_manager.get_region().await;
        self.refresh_with_token(http_client, &access_token, &region).await;
    }

    /// Populate cache using a specific access token and region.
    /// Used for lazy refresh on first request when global token is unavailable.
    pub async fn refresh_with_token(
        &self,
        http_client: &KiroHttpClient,
        access_token: &str,
        region: &str,
    ) {
        // Prevent concurrent refreshes
        if self.refreshing.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }

        let url = format!("https://q.{}.amazonaws.com/ListAvailableModels", region);

        let result = async {
            let req = http_client
                .client()
                .get(&url)
                .query(&[("origin", "AI_EDITOR")])
                .header("Authorization", format!("Bearer {}", access_token))
                .header("Content-Type", "application/json")
                .build()
                .map_err(|e| format!("build request: {}", e))?;

            let response = http_client.request_no_retry(req).await
                .map_err(|e| format!("request failed: {}", e))?;

            let body = response.text().await
                .map_err(|e| format!("read body: {}", e))?;

            let json: serde_json::Value = serde_json::from_str(&body)
                .map_err(|e| format!("parse JSON: {}", e))?;

            Ok::<_, String>(json)
        }.await;

        self.refreshing.store(false, Ordering::SeqCst);

        match result {
            Err(e) => {
                tracing::warn!("ModelCache: ListAvailableModels failed: {}", e);
            }
            Ok(json) => {
                let models = match json.get("models").and_then(|v| v.as_array()) {
                    Some(m) => m.clone(),
                    None => {
                        tracing::warn!("ModelCache: no 'models' field in response");
                        return;
                    }
                };

                self.inner.clear();
                let mut count = 0;
                for model in &models {
                    if let Some(model_id) = model.get("modelId").and_then(|v| v.as_str()) {
                        self.inner.insert(model_id.to_string(), model_id.to_string());
                        let normalized = normalize_model_name(model_id);
                        self.inner.insert(normalized, model_id.to_string());
                        count += 1;
                    }
                }
                tracing::info!("ModelCache: loaded {} models from ListAvailableModels", count);
            }
        }
    }
}
