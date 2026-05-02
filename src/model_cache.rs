use dashmap::DashMap;
use std::sync::Arc;

use crate::auth::AuthManager;
use crate::converters::core::normalize_model_name;
use crate::http_client::KiroHttpClient;

/// Thread-safe cache mapping normalized model names to valid Kiro API model IDs.
#[derive(Clone)]
pub struct ModelCache {
    /// normalized_name → actual Kiro model ID
    inner: Arc<DashMap<String, String>>,
}

impl ModelCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Resolve a model name: normalize it, then look up in cache.
    /// Falls back to the normalized name if not found (pass-through).
    pub fn resolve(&self, name: &str) -> String {
        let normalized = normalize_model_name(name);
        if let Some(id) = self.inner.get(&normalized) {
            return id.clone();
        }
        // Also try exact match on the original name
        if let Some(id) = self.inner.get(name) {
            return id.clone();
        }
        normalized
    }

    /// Populate cache from Kiro's ListAvailableModels API.
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
        let url = format!("https://q.{}.amazonaws.com/ListAvailableModels", region);

        let req = match http_client
            .client()
            .get(&url)
            .query(&[("origin", "AI_EDITOR")])
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("ModelCache: failed to build request: {}", e);
                return;
            }
        };

        let response = match http_client.request_no_retry(req).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("ModelCache: ListAvailableModels request failed: {}", e);
                return;
            }
        };

        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("ModelCache: failed to read response body: {}", e);
                return;
            }
        };

        let json: serde_json::Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("ModelCache: failed to parse response: {}", e);
                return;
            }
        };

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
                // Index by the model ID itself
                self.inner.insert(model_id.to_string(), model_id.to_string());
                // Also index by normalized name so lookups work
                let normalized = normalize_model_name(model_id);
                self.inner.insert(normalized, model_id.to_string());
                count += 1;
            }
        }
        tracing::info!("ModelCache: loaded {} models from ListAvailableModels", count);
    }
}
