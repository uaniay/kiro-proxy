use axum::{
    body::Body,
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::stream::StreamExt;
use uuid::Uuid;

use crate::converters::openai_to_kiro::build_kiro_payload;
use crate::error::ApiError;
use crate::models::openai::{ChatCompletionRequest, ModelList, OpenAIModel};
use crate::routes::state::{AppState, KiroCreds};
use crate::tokenizer::count_message_tokens;

pub(crate) async fn get_models_handler() -> Result<Json<ModelList>, ApiError> {
    // Static model list for proxy mode
    let models = vec![
        OpenAIModel::new("claude-sonnet-4".to_string()),
        OpenAIModel::new("claude-sonnet-4-5".to_string()),
        OpenAIModel::new("claude-haiku-4".to_string()),
        OpenAIModel::new("claude-haiku-4-5".to_string()),
        OpenAIModel::new("claude-opus-4".to_string()),
        OpenAIModel::new("claude-opus-4-6".to_string()),
    ];
    Ok(Json(ModelList::new(models)))
}

pub(crate) async fn chat_completions_handler(
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
    let request: ChatCompletionRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| ApiError::ValidationError(format!("Invalid JSON: {}", e)))?;

    tracing::info!(
        model = %request.model,
        stream = request.stream,
        messages = request.messages.len(),
        "POST /v1/chat/completions"
    );

    if request.messages.is_empty() {
        return Err(ApiError::ValidationError(
            "messages cannot be empty".to_string(),
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

    let input_tokens = count_message_tokens(&request.messages);

    if request.stream {
        let include_usage = request
            .stream_options
            .as_ref()
            .and_then(|opts| opts.get("include_usage"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let sse_stream = crate::streaming::stream_kiro_to_openai(
            response,
            &request.model,
            config.first_token_timeout,
            input_tokens,
            None,
            include_usage,
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
        let body = crate::streaming::collect_openai_response(
            response,
            &request.model,
            config.first_token_timeout,
            input_tokens,
            config.truncation_recovery,
        )
        .await?;

        // Record usage
        if let (Some(ref db), Some(ref key_id), Some(ref uid)) = (&state.db, &creds.api_key_id, &creds.user_id) {
            let in_tok = body.get("usage").and_then(|u| u.get("prompt_tokens")).and_then(|v| v.as_i64()).unwrap_or(input_tokens as i64);
            let out_tok = body.get("usage").and_then(|u| u.get("completion_tokens")).and_then(|v| v.as_i64()).unwrap_or(0);
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
