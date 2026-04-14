use std::sync::Arc;
use std::sync::RwLock;

use crate::auth::AuthManager;
use crate::config::Config;
use crate::http_client::KiroHttpClient;

/// Kiro credentials injected into request extensions by auth middleware.
#[derive(Debug, Clone)]
pub struct KiroCreds {
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
}
