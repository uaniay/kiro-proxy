use axum::{
    body::Body,
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::stream::StreamExt;
use uuid::Uuid;

use crate::converters::anthropic_to_kiro::build_kiro_payload;
use crate::error::ApiError;
use crate::models::anthropic::AnthropicMessagesRequest;
use crate::routes::state::{AppState, KiroCreds};
use crate::tokenizer::count_tokens;

pub(crate) async fn anthropic_messages_handler(
    State(state): State<AppState>,
    raw_request: axum::http::Request<Body>,
) -> Result<Response, ApiError> {
    let creds = raw_request
        .extensions()
        .get::<KiroCreds>()
        .cloned()
        .ok_or_else(|| ApiError::AuthError("Missing credentials".to_string()))?;

    let body_bytes = axum::body::to_bytes(raw_request.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|e| ApiError::ValidationError(format!("Failed to read body: {}", e)))?;
    let request: AnthropicMessagesRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| ApiError::ValidationError(format!("Invalid JSON: {}", e)))?;

    tracing::info!(
        model = %request.model,
        stream = request.stream,
        messages = request.messages.len(),
        "POST /v1/messages"
    );

    if request.messages.is_empty() {
        return Err(ApiError::ValidationError(
            "messages cannot be empty".to_string(),
        ));
    }

    if request.max_tokens <= 0 {
        return Err(ApiError::ValidationError(
            "max_tokens must be positive".to_string(),
        ));
    }

    let config = state
        .config
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .clone();

    let conversation_id = Uuid::new_v4().to_string();
    let profile_arn = {
        let auth = state.auth_manager.read().await;
        auth.get_profile_arn().await.unwrap_or_default()
    };

    let kiro_result = build_kiro_payload(&request, &conversation_id, &profile_arn, &config)
        .map_err(ApiError::ValidationError)?;

    let kiro_api_url = format!(
        "https://codewhisperer.{}.amazonaws.com/generateAssistantResponse",
        creds.region
    );

    let req = state
        .http_client
        .client()
        .post(&kiro_api_url)
        .header("Authorization", format!("Bearer {}", creds.access_token))
        .header("Content-Type", "application/json")
        .json(&kiro_result.payload)
        .build()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("Failed to build request: {}", e)))?;

    let response = state.http_client.request_with_retry(req).await?;

    // Rough input token estimate from serialized body
    let input_tokens = count_tokens(&String::from_utf8_lossy(&body_bytes), false);

    if request.stream {
        let sse_stream = crate::streaming::stream_kiro_to_anthropic(
            response,
            &request.model,
            config.first_token_timeout,
            input_tokens,
            None,
            config.truncation_recovery,
        )
        .await?;

        let byte_stream = sse_stream.map(|r| r.map(Bytes::from));

        Ok(Response::builder()
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .header("connection", "keep-alive")
            .body(Body::from_stream(byte_stream))
            .unwrap())
    } else {
        let body = crate::streaming::collect_anthropic_response(
            response,
            &request.model,
            config.first_token_timeout,
            input_tokens,
            config.truncation_recovery,
        )
        .await?;

        Ok(Json(body).into_response())
    }
}
