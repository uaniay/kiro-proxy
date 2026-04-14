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
use futures::stream::Stream;
use serde_json::{json, Value};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::middleware;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A stream wrapper that calls a callback when the inner stream ends.
/// Used to record usage after streaming completes.
pub(crate) struct OnCompleteStream<S> {
    inner: S,
    on_complete: Option<Box<dyn FnOnce() + Send>>,
}

impl<S> OnCompleteStream<S> {
    pub fn new(inner: S, on_complete: impl FnOnce() + Send + 'static) -> Self {
        Self {
            inner,
            on_complete: Some(Box::new(on_complete)),
        }
    }
}

impl<S: Stream + Unpin> Stream for OnCompleteStream<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(None) => {
                if let Some(f) = self.on_complete.take() {
                    f();
                }
                Poll::Ready(None)
            }
            other => other,
        }
    }
}

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
