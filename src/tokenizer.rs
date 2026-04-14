use crate::models::anthropic::AnthropicTool;
use crate::models::openai::{ChatMessage, Tool};
use serde_json::Value;

/// Approximate token count using character-based estimation.
/// Uses ~4 chars per token as a rough heuristic (no tiktoken dependency).
pub fn count_tokens(text: &str, _apply_claude_correction: bool) -> i32 {
    if text.is_empty() {
        return 0;
    }
    (text.len() as f64 / 4.0).ceil() as i32
}

/// Count tokens in a list of OpenAI messages
pub fn count_message_tokens(messages: &[ChatMessage]) -> i32 {
    let mut total = 0;
    for msg in messages {
        total += 4; // per-message overhead
        total += count_tokens(&msg.role, false);
        if let Some(ref content) = msg.content {
            match content {
                Value::String(s) => total += count_tokens(s, false),
                other => total += count_tokens(&other.to_string(), false),
            }
        }
        if let Some(ref name) = msg.name {
            total += count_tokens(name, false);
        }
    }
    total += 3; // final overhead
    total
}

/// Count tokens in tool definitions (OpenAI format)
pub fn count_tools_tokens(tools: &[Tool]) -> i32 {
    let mut total = 0;
    for tool in tools {
        total += 4; // per-tool overhead
        let json = serde_json::to_string(tool).unwrap_or_default();
        total += count_tokens(&json, false);
    }
    total
}

/// Count tokens in Anthropic tool definitions
pub fn count_anthropic_tools_tokens(tools: &[AnthropicTool]) -> i32 {
    let mut total = 0;
    for tool in tools {
        total += 4;
        let json = serde_json::to_string(tool).unwrap_or_default();
        total += count_tokens(&json, false);
    }
    total
}
