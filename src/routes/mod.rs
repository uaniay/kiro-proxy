mod anthropic;
mod openai;
pub mod state;

pub use state::{AppState, KiroCreds};

use axum::{
    middleware as axum_middleware,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde_json::{json, Value};

use crate::middleware;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn health_routes() -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler))
}

pub fn openai_routes(state: AppState) -> Router {
    let authed = Router::new()
        .route(
            "/v1/chat/completions",
            post(openai::chat_completions_handler),
        )
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::auth_middleware,
        ))
        .with_state(state);

    Router::new()
        .route("/v1/models", get(openai::get_models_handler))
        .merge(authed)
}

pub fn anthropic_routes(state: AppState) -> Router {
    Router::new()
        .route("/v1/messages", post(anthropic::anthropic_messages_handler))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::auth_middleware,
        ))
        .with_state(state)
}

async fn root_handler() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "message": "Kiro Proxy is running",
        "version": VERSION
    }))
}

async fn health_handler() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "timestamp": Utc::now().to_rfc3339(),
        "version": VERSION
    }))
}
