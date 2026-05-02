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
use tracing::info;

pub(crate) async fn get_models_handler() -> Result<Json<ModelList>, ApiError> {
    // Static model list for proxy mode
    let models = vec![
        OpenAIModel::new("claude-sonnet-4".to_string()),
        OpenAIModel::new("claude-sonnet-4-5".to_string()),
        OpenAIModel::new("claude-sonnet-4-6".to_string()),
        OpenAIModel::new("claude-haiku-4".to_string()),
        OpenAIModel::new("claude-haiku-4-5".to_string()),
        OpenAIModel::new("claude-haiku-4-6".to_string()),
        OpenAIModel::new("claude-opus-4".to_string()),
        OpenAIModel::new("claude-opus-4-6".to_string()),
        OpenAIModel::new("claude-opus-4-7".to_string()),
    ];
    Ok(Json(ModelList::new(models)))
}

pub(crate) async fn chat_completions_handler(
    State(state): State<AppState>,
    raw_request: axum::http::Request<Body>,
) -> Result<Response, ApiError> {
    let start_time = std::time::Instant::now();
    let creds = raw_request
        .extensions()
        .get::<KiroCreds>()
        .cloned()
        .ok_or_else(|| ApiError::AuthError("Missing credentials".to_string()))?;

    let req_headers = raw_request.headers().clone();
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

    // Lazy-load model cache on first request if empty
    if state.model_cache.is_empty() {
        state.model_cache.refresh_with_token(&state.http_client, &creds.access_token, &creds.region).await;
    }

    let model_id = state.model_cache.resolve(&request.model);
    let kiro_result = build_kiro_payload(&request, &conversation_id, &profile_arn, &config, Some(&model_id))
        .map_err(ApiError::ValidationError)?;

    let kiro_api_url = format!(
        "https://codewhisperer.{}.amazonaws.com/generateAssistantResponse",
        creds.region
    );

    info!(
        original_model = %request.model,
        kiro_model = %kiro_result.payload
            .get("conversationState")
            .and_then(|s| s.get("currentMessage"))
            .and_then(|m| m.get("userInputMessage"))
            .and_then(|u| u.get("modelId"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown"),
        user_id = %creds.user_id.as_deref().unwrap_or("unknown"),
        api_key_id = %creds.api_key_id.as_deref().unwrap_or("unknown"),
        region = %creds.region,
        "Forwarding to Kiro API"
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

    let log_conversations = config.enable_conversation_log;

    let resp_headers_json = if log_conversations {
        let mut map = serde_json::Map::new();
        for (name, value) in response.headers().iter() {
            if let Ok(v) = value.to_str() {
                map.insert(name.as_str().to_string(), serde_json::Value::String(v.to_string()));
            }
        }
        Some(serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_default())
    } else {
        None
    };

    if request.stream {
        let include_usage = request
            .stream_options
            .as_ref()
            .and_then(|opts| opts.get("include_usage"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let output_tracker = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let response_collector = if log_conversations {
            Some(std::sync::Arc::new(std::sync::Mutex::new(String::new())))
        } else {
            None
        };
        let sse_stream = crate::streaming::stream_kiro_to_openai(
            response,
            &request.model,
            config.first_token_timeout,
            input_tokens,
            Some(output_tracker.clone()),
            include_usage,
            config.truncation_recovery,
            response_collector.clone(),
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
            let conv_log = log_conversations;
            let conv_db = db.clone();
            let conv_key_id = key_id.clone();
            let conv_uid = uid.clone();
            let conv_model = model.clone();
            let conv_request = String::from_utf8_lossy(&body_bytes).to_string();
            let conv_headers = crate::conversation_log::sanitize_headers(&req_headers);
            let conv_start = start_time;
            let conv_collector = response_collector;
            let conv_resp_headers = resp_headers_json.clone();
            Box::pin(super::OnCompleteStream::new(byte_stream, move || {
                let out_tok = tracker.load(std::sync::atomic::Ordering::Relaxed) as i64;
                tokio::spawn(async move {
                    let _ = crate::db::record_usage(&db, &key_id, &uid, &model, in_tok, out_tok).await;
                });
                if conv_log {
                    let resp_text = conv_collector
                        .map(|c| c.lock().map(|g| g.clone()).unwrap_or_default())
                        .unwrap_or_default();
                    let resp_json = serde_json::json!({"content": resp_text}).to_string();
                    let headers_str = serde_json::to_string(&conv_headers).unwrap_or_default();
                    let duration = conv_start.elapsed().as_millis() as i64;
                    let conv_id = uuid::Uuid::new_v4().to_string();
                    tokio::spawn(async move {
                        let _ = crate::db::record_conversation(
                            &conv_db, &conv_id, &conv_key_id, &conv_uid,
                            "openai", &conv_model, true,
                            &conv_request, Some(&resp_json),
                            Some(&headers_str), conv_resp_headers.as_deref(),
                            in_tok, out_tok, Some(duration),
                        ).await;
                    });
                }
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
            let conv_log = log_conversations;
            let conv_db = db.clone();
            let conv_key_id = key_id.clone();
            let conv_uid = uid.clone();
            let conv_model = model.clone();
            let conv_request = String::from_utf8_lossy(&body_bytes).to_string();
            let conv_response = serde_json::to_string(&body).unwrap_or_default();
            let conv_headers = serde_json::to_string(&crate::conversation_log::sanitize_headers(&req_headers)).unwrap_or_default();
            let conv_duration = start_time.elapsed().as_millis() as i64;
            let conv_resp_headers = resp_headers_json.clone();
            tokio::spawn(async move {
                let _ = crate::db::record_usage(&db, &key_id, &uid, &model, in_tok, out_tok).await;
                if conv_log {
                    let conv_id = uuid::Uuid::new_v4().to_string();
                    let _ = crate::db::record_conversation(
                        &conv_db, &conv_id, &conv_key_id, &conv_uid,
                        "openai", &conv_model, false,
                        &conv_request, Some(&conv_response),
                        Some(&conv_headers), conv_resp_headers.as_deref(),
                        in_tok, out_tok, Some(conv_duration),
                    ).await;
                }
            });
        }

        Ok(Json(body).into_response())
    }
}
