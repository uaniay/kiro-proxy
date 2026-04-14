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
        let output_tracker = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let sse_stream = crate::streaming::stream_kiro_to_anthropic(
            response,
            &request.model,
            config.first_token_timeout,
            input_tokens,
            Some(output_tracker.clone()),
            config.truncation_recovery,
        )
        .await?;

        let byte_stream = sse_stream.map(|r| r.map(Bytes::from));

        // Wrap stream to record usage when streaming completes
        let byte_stream = if let (Some(ref db), Some(ref key_id), Some(ref uid)) =
            (&state.db, &creds.api_key_id, &creds.user_id)
        {
            let db = db.clone();
            let key_id = key_id.clone();
            let uid = uid.clone();
            let model = request.model.clone();
            let tracker = output_tracker;
            let in_tok = input_tokens as i64;
            Box::pin(super::OnCompleteStream::new(byte_stream, move || {
                let out_tok = tracker.load(std::sync::atomic::Ordering::Relaxed) as i64;
                tokio::spawn(async move {
                    let _ = crate::db::record_usage(&db, &key_id, &uid, &model, in_tok, out_tok).await;
                });
            })) as std::pin::Pin<Box<dyn futures::Stream<Item = _> + Send>>
        } else {
            Box::pin(byte_stream)
        };

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

        // Record usage
        if let (Some(ref db), Some(ref key_id), Some(ref uid)) = (&state.db, &creds.api_key_id, &creds.user_id) {
            let in_tok = body.get("usage").and_then(|u| u.get("input_tokens")).and_then(|v| v.as_i64()).unwrap_or(input_tokens as i64);
            let out_tok = body.get("usage").and_then(|u| u.get("output_tokens")).and_then(|v| v.as_i64()).unwrap_or(0);
            let model = request.model.clone();
            let db = db.clone();
            let key_id = key_id.clone();
            let uid = uid.clone();
            tokio::spawn(async move {
                let _ = crate::db::record_usage(&db, &key_id, &uid, &model, in_tok, out_tok).await;
            });
        }

        Ok(Json(body).into_response())
    }
}
