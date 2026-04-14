use std::sync::Arc;
use std::sync::RwLock;

use dashmap::DashMap;
use uuid::Uuid;

use crate::auth::AuthManager;
use crate::config::Config;
use crate::http_client::KiroHttpClient;
use crate::pool::PoolScheduler;

/// Kiro credentials injected into request extensions by auth middleware.
#[derive(Debug, Clone)]
pub struct KiroCreds {
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    pub access_token: String,
    pub region: String,
}

/// Application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    pub proxy_api_key_hash: [u8; 32],
    pub auth_manager: Arc<tokio::sync::RwLock<AuthManager>>,
    pub http_client: Arc<KiroHttpClient>,
    pub config: Arc<RwLock<Config>>,
    // Multi-user (None in proxy-only mode)
    pub db: Option<sqlx::sqlite::SqlitePool>,
    /// api_key_hash → (key_id, user_id)
    pub api_key_cache: Arc<DashMap<String, (String, String)>>,
    /// user_id → (access_token, region, cached_at)
    pub kiro_token_cache: Arc<DashMap<String, (String, String, std::time::Instant)>>,
    /// Pool scheduler for admin token pool
    pub pool_scheduler: Arc<PoolScheduler>,
}
