use anyhow::{Context, Result};
use std::sync::{Arc, RwLock};

mod auth;
mod config;
mod converters;
mod error;
mod http_client;
mod middleware;
mod models;
mod routes;
mod streaming;
mod thinking_parser;
mod tokenizer;
mod truncation;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::Config::load()?;
    config.validate()?;

    // Logging
    let log_level = config.log_level.to_lowercase();
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&log_level));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    tracing::info!("Kiro Proxy starting...");

    // Auth manager
    let auth_manager = auth::AuthManager::new_from_env(&config)
        .context("Failed to create auth manager")?;
    let auth_manager = Arc::new(tokio::sync::RwLock::new(auth_manager));

    // Bootstrap credentials (refresh token → access token)
    {
        let mut mgr = auth_manager.write().await;
        if let Err(e) = mgr.bootstrap_proxy_credentials().await {
            tracing::warn!("Failed to bootstrap Kiro credentials: {}. Will retry on first request.", e);
        }
    }

    // HTTP client
    let http_client = Arc::new(
        http_client::KiroHttpClient::new(
            config.http_max_connections,
            config.http_connect_timeout,
            config.http_request_timeout,
            config.http_max_retries,
        )
        .context("Failed to create HTTP client")?,
    );

    // Compute API key hash
    let proxy_api_key_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(config.proxy_api_key.as_bytes());
        let result: [u8; 32] = hasher.finalize().into();
        result
    };

    let state = routes::AppState {
        proxy_api_key_hash,
        auth_manager,
        http_client,
        config: Arc::new(RwLock::new(config.clone())),
    };

    // Router
    let app = routes::health_routes()
        .merge(routes::openai_routes(state.clone()))
        .merge(routes::anthropic_routes(state))
        .layer(middleware::cors_layer());

    let addr = format!("{}:{}", config.server_host, config.server_port);
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .context("Failed to bind address")?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    tracing::info!("Shutdown signal received");
}
