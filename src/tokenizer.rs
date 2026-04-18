use crate::models::openai::ChatMessage;
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
