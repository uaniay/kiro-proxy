use futures::stream::{BoxStream, StreamExt};
use serde_json::json;
use std::sync::Arc;
use std::sync::Mutex;

use crate::error::ApiError;
use crate::streaming::{self, ToolUse, Usage};

pub async fn stream_kiro_to_responses(
    response: reqwest::Response,
    model: &str,
    first_token_timeout_secs: u64,
    input_tokens: i32,
    output_tokens_tracker: Option<Arc<std::sync::atomic::AtomicU64>>,
    response_collector: Option<Arc<Mutex<String>>>,
) -> Result<BoxStream<'static, Result<String, ApiError>>, ApiError> {
    let response_id = format!("resp_{}", uuid::Uuid::new_v4().simple());
    let created_at = chrono::Utc::now().timestamp();
    let model = model.to_string();

    let kiro_stream = streaming::parse_kiro_stream(response, first_token_timeout_secs).await?;

    struct StreamState {
        emitted_created: bool,
        emitted_message_added: bool,
        accumulated_text: String,
        tool_calls: Vec<ToolUse>,
        usage: Option<Usage>,
        thinking_started: bool,
    }

    let state = Arc::new(Mutex::new(StreamState {
        emitted_created: false,
        emitted_message_added: false,
        accumulated_text: String::new(),
        tool_calls: Vec::new(),
        usage: None,
        thinking_started: false,
    }));

    let state_for_final = state.clone();
    let response_id_clone = response_id.clone();
    let model_clone = model.clone();
    let tracker_for_stream = output_tokens_tracker.clone();
    let collector_for_stream = response_collector.clone();

    let event_stream = kiro_stream.filter_map(move |event_result| {
        let response_id = response_id_clone.clone();
        let model = model_clone.clone();
        let state = state.clone();
        let _tracker = tracker_for_stream.clone();
        let collector = collector_for_stream.clone();

        async move {
            match event_result {
                Ok(event) => {
                    let mut state = state.lock().unwrap();
                    let mut lines = Vec::new();

                    // Emit response.created on first event
                    if !state.emitted_created {
                        state.emitted_created = true;
                        let created = json!({
                            "type": "response.created",
                            "response": {
                                "id": response_id,
                                "object": "response",
                                "created_at": created_at,
                                "status": "in_progress",
                                "model": model,
                                "output": []
                            }
                        });
                        lines.push(format_sse("response.created", &created));
                    }

                    match event.event_type.as_str() {
                        "content" => {
                            if let Some(content) = event.content {
                                state.accumulated_text.push_str(&content);
                                if let Some(ref c) = collector {
                                    if let Ok(mut buf) = c.lock() {
                                        buf.push_str(&content);
                                    }
                                }

                                // Emit output_item.added for the message on first content
                                if !state.emitted_message_added {
                                    state.emitted_message_added = true;
                                    let added = json!({
                                        "type": "response.output_item.added",
                                        "item": {
                                            "type": "message",
                                            "role": "assistant",
                                            "content": []
                                        }
                                    });
                                    lines.push(format_sse("response.output_item.added", &added));
                                }

                                let delta = json!({
                                    "type": "response.output_text.delta",
                                    "delta": content
                                });
                                lines.push(format_sse("response.output_text.delta", &delta));
                            }
                        }
                        "thinking" => {
                            if let Some(thinking) = event.thinking_content {
                                state.accumulated_text.push_str(&thinking);

                                if !state.thinking_started {
                                    state.thinking_started = true;
                                    let added = json!({
                                        "type": "response.reasoning_summary_part.added",
                                        "summary_index": 0
                                    });
                                    lines.push(format_sse(
                                        "response.reasoning_summary_part.added",
                                        &added,
                                    ));
                                }

                                let delta = json!({
                                    "type": "response.reasoning_summary_text.delta",
                                    "delta": thinking,
                                    "summary_index": 0
                                });
                                lines.push(format_sse(
                                    "response.reasoning_summary_text.delta",
                                    &delta,
                                ));
                            }
                        }
                        "tool_use" => {
                            if let Some(tool) = event.tool_use {
                                let arguments =
                                    serde_json::to_string(&tool.input).unwrap_or_default();

                                // Emit output_item.added for the function call
                                let added = json!({
                                    "type": "response.output_item.added",
                                    "item": {
                                        "type": "function_call",
                                        "name": tool.name,
                                        "call_id": tool.tool_use_id,
                                        "arguments": ""
                                    }
                                });
                                lines.push(format_sse("response.output_item.added", &added));

                                // Emit the arguments as a delta
                                if !arguments.is_empty() && arguments != "null" {
                                    let arg_delta = json!({
                                        "type": "response.function_call_arguments.delta",
                                        "item_id": tool.tool_use_id,
                                        "call_id": tool.tool_use_id,
                                        "delta": arguments
                                    });
                                    lines.push(format_sse(
                                        "response.function_call_arguments.delta",
                                        &arg_delta,
                                    ));
                                }

                                // Emit output_item.done
                                let done = json!({
                                    "type": "response.output_item.done",
                                    "item": {
                                        "type": "function_call",
                                        "name": tool.name,
                                        "call_id": tool.tool_use_id,
                                        "arguments": arguments
                                    }
                                });
                                lines.push(format_sse("response.output_item.done", &done));

                                state.tool_calls.push(tool);
                            }
                        }
                        "usage" => {
                            state.usage = event.usage;
                        }
                        _ => {}
                    }

                    if lines.is_empty() {
                        None
                    } else {
                        Some(Ok(lines.join("")))
                    }
                }
                Err(e) => {
                    let failed = json!({
                        "type": "response.failed",
                        "response": {
                            "id": response_id,
                            "object": "response",
                            "status": "failed",
                            "error": {
                                "code": "server_error",
                                "message": e.to_string()
                            }
                        }
                    });
                    Some(Ok(format_sse("response.failed", &failed)))
                }
            }
        }
    });

    // Final stream that appends response.completed + output_item.done for message
    let response_id_final = response_id.clone();
    let model_final = model.clone();
    let final_stream = futures::stream::once(async move {
        let state = state_for_final.lock().unwrap();

        let output_tokens = crate::tokenizer::count_tokens(&state.accumulated_text, false);
        if let Some(ref tracker) = output_tokens_tracker {
            tracker.store(output_tokens as u64, std::sync::atomic::Ordering::Relaxed);
        }

        let usage = state.usage.as_ref();
        let in_tok = usage.map(|u| u.input_tokens).unwrap_or(input_tokens);
        let out_tok = usage.map(|u| u.output_tokens).unwrap_or(output_tokens);

        let mut lines = Vec::new();

        // Emit output_item.done for the text message if we had content
        if !state.accumulated_text.is_empty() && state.emitted_message_added {
            let msg_done = json!({
                "type": "response.output_item.done",
                "item": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": state.accumulated_text
                    }]
                }
            });
            lines.push(format_sse("response.output_item.done", &msg_done));
        }

        let completed = json!({
            "type": "response.completed",
            "response": {
                "id": response_id_final,
                "object": "response",
                "created_at": created_at,
                "status": "completed",
                "model": model_final,
                "end_turn": true,
                "usage": {
                    "input_tokens": in_tok,
                    "output_tokens": out_tok,
                    "total_tokens": in_tok + out_tok
                }
            }
        });
        lines.push(format_sse("response.completed", &completed));

        Ok::<String, ApiError>(lines.join(""))
    });

    Ok(event_stream.chain(final_stream).boxed())
}

pub async fn collect_responses_response(
    response: reqwest::Response,
    model: &str,
    first_token_timeout_secs: u64,
    input_tokens: i32,
) -> Result<serde_json::Value, ApiError> {
    let response_id = format!("resp_{}", uuid::Uuid::new_v4().simple());
    let created_at = chrono::Utc::now().timestamp();

    let mut kiro_stream = streaming::parse_kiro_stream(response, first_token_timeout_secs).await?;

    let mut full_content = String::new();
    let mut tool_calls: Vec<ToolUse> = Vec::new();
    let mut usage: Option<Usage> = None;

    while let Some(event_result) = kiro_stream.next().await {
        match event_result {
            Ok(event) => match event.event_type.as_str() {
                "content" => {
                    if let Some(content) = event.content {
                        full_content.push_str(&content);
                    }
                }
                "tool_use" => {
                    if let Some(tool_use) = event.tool_use {
                        tool_calls.push(tool_use);
                    }
                }
                "usage" => {
                    if let Some(u) = event.usage {
                        usage = Some(u);
                    }
                }
                _ => {}
            },
            Err(e) => {
                tracing::warn!("Error in stream: {:?}", e);
            }
        }
    }

    let tool_calls = streaming::deduplicate_tool_calls(tool_calls);

    let mut output = Vec::new();

    if !full_content.is_empty() {
        output.push(json!({
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": full_content}]
        }));
    }

    for tool in &tool_calls {
        let arguments = serde_json::to_string(&tool.input).unwrap_or_default();
        output.push(json!({
            "type": "function_call",
            "name": tool.name,
            "call_id": tool.tool_use_id,
            "arguments": arguments
        }));
    }

    let in_tok = usage.as_ref().map(|u| u.input_tokens).unwrap_or(input_tokens);
    let out_tok = usage.as_ref().map(|u| u.output_tokens).unwrap_or(0);

    Ok(json!({
        "id": response_id,
        "object": "response",
        "created_at": created_at,
        "status": "completed",
        "model": model,
        "output": output,
        "usage": {
            "input_tokens": in_tok,
            "output_tokens": out_tok,
            "total_tokens": in_tok + out_tok
        }
    }))
}

fn format_sse(event_type: &str, data: &serde_json::Value) -> String {
    let json = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    format!("event: {}\ndata: {}\n\n", event_type, json)
}
