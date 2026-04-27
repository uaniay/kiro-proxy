use serde_json::Value;
use tracing::debug;

use crate::config::Config;
use crate::converters::core::normalize_model_name;
use crate::converters::openai_to_kiro::build_kiro_payload_core;
use crate::models::responses::ResponsesApiRequest;

use super::core::{
    KiroPayloadResult, MessageContent, ToolCall, ToolFunction, ToolResult, UnifiedMessage,
    UnifiedTool,
};

fn extract_text_from_response_content(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    if let Some(arr) = content.as_array() {
        let mut texts = Vec::new();
        for item in arr {
            let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match item_type {
                "input_text" | "output_text" | "text" => {
                    if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                        texts.push(t.to_string());
                    }
                }
                _ => {}
            }
        }
        return texts.join("");
    }
    String::new()
}

fn convert_responses_input_to_unified(input: &[Value]) -> Vec<UnifiedMessage> {
    let mut messages: Vec<UnifiedMessage> = Vec::new();

    for item in input {
        let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match item_type {
            "message" => {
                let role = item
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("user");
                let content = item
                    .get("content")
                    .map(|c| extract_text_from_response_content(c))
                    .unwrap_or_default();
                messages.push(UnifiedMessage {
                    role: role.to_string(),
                    content: MessageContent::Text(content),
                    tool_calls: None,
                    tool_results: None,
                    images: None,
                });
            }
            "function_call" => {
                let name = item
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = item
                    .get("arguments")
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}")
                    .to_string();
                let call_id = item
                    .get("call_id")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();

                let tc = ToolCall {
                    id: call_id.clone(),
                    call_type: "function".to_string(),
                    function: ToolFunction {
                        name,
                        arguments,
                    },
                };

                // Try to merge into previous assistant message
                if let Some(last) = messages.last_mut() {
                    if last.role == "assistant" {
                        match last.tool_calls {
                            Some(ref mut calls) => {
                                calls.push(tc);
                                continue;
                            }
                            None => {
                                last.tool_calls = Some(vec![tc]);
                                continue;
                            }
                        }
                    }
                }
                messages.push(UnifiedMessage {
                    role: "assistant".to_string(),
                    content: MessageContent::Text(String::new()),
                    tool_calls: Some(vec![tc]),
                    tool_results: None,
                    images: None,
                });
            }
            "function_call_output" | "custom_tool_call_output" => {
                let call_id = item
                    .get("call_id")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                let output = item
                    .get("output")
                    .and_then(|o| o.as_str())
                    .unwrap_or("(empty result)")
                    .to_string();

                let tr = ToolResult {
                    result_type: "tool_result".to_string(),
                    tool_use_id: call_id,
                    content: if output.is_empty() {
                        "(empty result)".to_string()
                    } else {
                        output
                    },
                };

                // Try to merge into previous user message that has tool_results
                if let Some(last) = messages.last_mut() {
                    if last.role == "user" {
                        if let Some(ref mut results) = last.tool_results {
                            results.push(tr);
                            continue;
                        }
                    }
                }
                messages.push(UnifiedMessage {
                    role: "user".to_string(),
                    content: MessageContent::Text(String::new()),
                    tool_calls: None,
                    tool_results: Some(vec![tr]),
                    images: None,
                });
            }
            "custom_tool_call" => {
                let name = item
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let input_str = item
                    .get("input")
                    .and_then(|i| i.as_str())
                    .unwrap_or("{}")
                    .to_string();
                let call_id = item
                    .get("call_id")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();

                let tc = ToolCall {
                    id: call_id,
                    call_type: "function".to_string(),
                    function: ToolFunction {
                        name,
                        arguments: input_str,
                    },
                };

                if let Some(last) = messages.last_mut() {
                    if last.role == "assistant" {
                        if let Some(ref mut calls) = last.tool_calls {
                            calls.push(tc);
                            continue;
                        }
                    }
                }
                messages.push(UnifiedMessage {
                    role: "assistant".to_string(),
                    content: MessageContent::Text(String::new()),
                    tool_calls: Some(vec![tc]),
                    tool_results: None,
                    images: None,
                });
            }
            // Skip reasoning, compaction, ghost_snapshot, etc.
            _ => {
                debug!("Skipping Responses API input item type: {}", item_type);
            }
        }
    }

    messages
}

fn convert_responses_tools_to_unified(tools: &[Value]) -> Option<Vec<UnifiedTool>> {
    if tools.is_empty() {
        return None;
    }

    let mut unified = Vec::new();
    for tool in tools {
        let tool_type = tool.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match tool_type {
            "function" => {
                let name = tool
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let description = tool
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string());
                let parameters = tool.get("parameters").cloned();
                unified.push(UnifiedTool {
                    name,
                    description,
                    input_schema: parameters,
                });
            }
            "namespace" => {
                // Flatten namespace tools
                if let Some(inner_tools) = tool.get("tools").and_then(|t| t.as_array()) {
                    let ns_name = tool
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    for inner in inner_tools {
                        let inner_type =
                            inner.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        if inner_type == "function" {
                            let name = inner
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string();
                            let full_name = if ns_name.is_empty() {
                                name
                            } else {
                                format!("{}{}", ns_name, name)
                            };
                            let description = inner
                                .get("description")
                                .and_then(|d| d.as_str())
                                .map(|s| s.to_string());
                            let parameters = inner.get("parameters").cloned();
                            unified.push(UnifiedTool {
                                name: full_name,
                                description,
                                input_schema: parameters,
                            });
                        }
                    }
                }
            }
            _ => {
                debug!(
                    "Skipping unsupported Responses API tool type: {}",
                    tool_type
                );
            }
        }
    }

    if unified.is_empty() {
        None
    } else {
        Some(unified)
    }
}

pub fn build_kiro_payload(
    request: &ResponsesApiRequest,
    conversation_id: &str,
    profile_arn: &str,
    config: &Config,
) -> Result<KiroPayloadResult, String> {
    let system_prompt = request.instructions.clone().unwrap_or_default();
    let unified_messages = convert_responses_input_to_unified(&request.input);
    let unified_tools = convert_responses_tools_to_unified(&request.tools);
    let model_id = normalize_model_name(&request.model);

    debug!(
        "Converting Responses API request: model={} -> {}, input_items={}, tools={}, instructions_len={}",
        request.model,
        model_id,
        request.input.len(),
        request.tools.len(),
        system_prompt.len()
    );

    if unified_messages.is_empty() && system_prompt.is_empty() {
        return Err("No input items to send".to_string());
    }

    // If no messages but we have instructions, create a synthetic user message
    let unified_messages = if unified_messages.is_empty() {
        vec![UnifiedMessage {
            role: "user".to_string(),
            content: MessageContent::Text(String::new()),
            tool_calls: None,
            tool_results: None,
            images: None,
        }]
    } else {
        unified_messages
    };

    build_kiro_payload_core(
        unified_messages,
        system_prompt,
        &model_id,
        unified_tools,
        conversation_id,
        profile_arn,
        true,
        config,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_config() -> Config {
        Config {
            fake_reasoning_enabled: false,
            ..Config::with_defaults()
        }
    }

    #[test]
    fn test_simple_user_message() {
        let request = ResponsesApiRequest {
            model: "claude-sonnet-4".to_string(),
            instructions: Some("You are helpful.".to_string()),
            input: vec![json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "Hello!"}]
            })],
            tools: vec![],
            tool_choice: "auto".to_string(),
            parallel_tool_calls: false,
            reasoning: None,
            store: false,
            stream: true,
            include: vec![],
            service_tier: None,
            text: None,
            prompt_cache_key: None,
            client_metadata: None,
            previous_response_id: None,
        };

        let result = build_kiro_payload(&request, "conv-1", "arn", &test_config());
        assert!(result.is_ok());
        let payload = result.unwrap().payload;
        let content = payload["conversationState"]["currentMessage"]["userInputMessage"]["content"]
            .as_str()
            .unwrap();
        assert!(content.contains("Hello!"));
        assert!(content.contains("You are helpful."));
    }

    #[test]
    fn test_function_call_roundtrip() {
        let request = ResponsesApiRequest {
            model: "claude-sonnet-4".to_string(),
            instructions: None,
            input: vec![
                json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Read test.py"}]}),
                json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Reading..."}]}),
                json!({"type": "function_call", "name": "Read", "arguments": "{\"path\":\"test.py\"}", "call_id": "call_1"}),
                json!({"type": "function_call_output", "call_id": "call_1", "output": "print('hello')"}),
            ],
            tools: vec![json!({"type": "function", "name": "Read", "description": "Read a file", "parameters": {"type": "object"}})],
            tool_choice: "auto".to_string(),
            parallel_tool_calls: false,
            reasoning: None,
            store: false,
            stream: true,
            include: vec![],
            service_tier: None,
            text: None,
            prompt_cache_key: None,
            client_metadata: None,
            previous_response_id: None,
        };

        let result = build_kiro_payload(&request, "conv-1", "arn", &test_config());
        assert!(result.is_ok());
        let payload_str = result.unwrap().payload.to_string();
        assert!(payload_str.contains("print('hello')"));
    }

    #[test]
    fn test_empty_input_with_instructions() {
        let request = ResponsesApiRequest {
            model: "claude-sonnet-4".to_string(),
            instructions: Some("System prompt".to_string()),
            input: vec![],
            tools: vec![],
            tool_choice: "auto".to_string(),
            parallel_tool_calls: false,
            reasoning: None,
            store: false,
            stream: true,
            include: vec![],
            service_tier: None,
            text: None,
            prompt_cache_key: None,
            client_metadata: None,
            previous_response_id: None,
        };

        let result = build_kiro_payload(&request, "conv-1", "arn", &test_config());
        // Should fail — no input and no instructions means nothing to send
        // But we have instructions, so it should create a synthetic message
        assert!(result.is_ok());
    }

    #[test]
    fn test_namespace_tools_flattened() {
        let tools = vec![json!({
            "type": "namespace",
            "name": "mcp__demo__",
            "description": "Demo tools",
            "tools": [
                {"type": "function", "name": "lookup", "description": "Look up", "parameters": {"type": "object"}}
            ]
        })];
        let unified = convert_responses_tools_to_unified(&tools);
        assert!(unified.is_some());
        let tools = unified.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "mcp__demo__lookup");
    }
}
