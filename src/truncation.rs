//! Truncation recovery system for detecting and recovering from API response truncation.
//!
//! The Kiro API can silently truncate large responses mid-stream, especially tool call arguments.
//! This module provides:
//! - Truncation diagnosis (heuristic JSON truncation detection)
//! - Global state cache (stores truncation info between requests)
//! - Recovery message generation
//! - Injection functions (modify incoming messages on next request)

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use sha2::{Digest, Sha256};

// ==================================================================================================
// Truncation Diagnosis
// ==================================================================================================

/// Information about whether a JSON string appears truncated.
#[derive(Debug, Clone)]
pub struct TruncationInfo {
    pub is_truncated: bool,
    pub reason: String,
    pub size_bytes: usize,
}

/// Diagnose whether a JSON string was truncated mid-stream.
///
/// Uses heuristic analysis matching Python's `_diagnose_json_truncation`:
/// - Empty string → not truncated
/// - Starts with `{` but doesn't end with `}` → truncated
/// - Starts with `[` but doesn't end with `]` → truncated
/// - Unbalanced braces → truncated
/// - Unbalanced brackets → truncated
/// - Unclosed string literal → truncated
pub fn diagnose_json_truncation(json_str: &str) -> TruncationInfo {
    let size_bytes = json_str.len();
    let trimmed = json_str.trim();

    // Empty string is not truncated
    if trimmed.is_empty() {
        return TruncationInfo {
            is_truncated: false,
            reason: "empty string".to_string(),
            size_bytes,
        };
    }

    // Check: starts with { but doesn't end with }
    if trimmed.starts_with('{') && !trimmed.ends_with('}') {
        return TruncationInfo {
            is_truncated: true,
            reason: "starts with '{' but does not end with '}'".to_string(),
            size_bytes,
        };
    }

    // Check: starts with [ but doesn't end with ]
    if trimmed.starts_with('[') && !trimmed.ends_with(']') {
        return TruncationInfo {
            is_truncated: true,
            reason: "starts with '[' but does not end with ']'".to_string(),
            size_bytes,
        };
    }

    // Count braces and brackets (simplified - doesn't handle braces inside strings perfectly)
    let mut brace_count: i32 = 0;
    let mut bracket_count: i32 = 0;

    for ch in trimmed.chars() {
        match ch {
            '{' => brace_count += 1,
            '}' => brace_count -= 1,
            '[' => bracket_count += 1,
            ']' => bracket_count -= 1,
            _ => {}
        }
    }

    if brace_count != 0 {
        return TruncationInfo {
            is_truncated: true,
            reason: format!("unbalanced braces (count: {})", brace_count),
            size_bytes,
        };
    }

    if bracket_count != 0 {
        return TruncationInfo {
            is_truncated: true,
            reason: format!("unbalanced brackets (count: {})", bracket_count),
            size_bytes,
        };
    }

    // Check for unclosed string literal (odd number of unescaped quotes)
    let mut quote_count = 0;
    let mut chars = trimmed.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            // Skip escaped character
            chars.next();
            continue;
        }
        if ch == '"' {
            quote_count += 1;
        }
    }

    if quote_count % 2 != 0 {
        return TruncationInfo {
            is_truncated: true,
            reason: format!("unclosed string literal (quote count: {})", quote_count),
            size_bytes,
        };
    }

    // All checks passed - not truncated
    TruncationInfo {
        is_truncated: false,
        reason: "all checks passed".to_string(),
        size_bytes,
    }
}

// ==================================================================================================
// State Cache
// ==================================================================================================

/// Entry for a truncated tool call.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolTruncationEntry {
    pub tool_name: String,
    pub info: TruncationInfo,
}

/// Entry for truncated content.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ContentTruncationEntry {
    pub content_hash: String,
}

/// Global state cache for truncation information.
///
/// Stores truncation data between requests so recovery messages can be injected
/// on the next request. Uses DashMap for thread-safe concurrent access.
/// Entries are removed on read (one-time retrieval).
pub struct TruncationState {
    tool_cache: RwLock<HashMap<String, ToolTruncationEntry>>,
    content_cache: RwLock<HashMap<String, ContentTruncationEntry>>,
}

impl Default for TruncationState {
    fn default() -> Self {
        Self::new()
    }
}

impl TruncationState {
    pub fn new() -> Self {
        Self {
            tool_cache: RwLock::new(HashMap::new()),
            content_cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn save_tool_truncation(&self, tool_call_id: &str, tool_name: &str, info: TruncationInfo) {
        tracing::info!(
            "Saving tool truncation state: tool_call_id={}, tool_name={}, reason={}",
            tool_call_id,
            tool_name,
            info.reason
        );
        self.tool_cache.write().unwrap().insert(
            tool_call_id.to_string(),
            ToolTruncationEntry {
                tool_name: tool_name.to_string(),
                info,
            },
        );
    }

    #[allow(dead_code)]
    pub fn get_tool_truncation(&self, tool_call_id: &str) -> Option<ToolTruncationEntry> {
        self.tool_cache.write().unwrap().remove(tool_call_id)
    }

    pub fn save_content_truncation(&self, content: &str) {
        let hash = content_hash(content);
        tracing::info!(
            "Saving content truncation state: hash={}, content_len={}",
            hash,
            content.len()
        );
        self.content_cache
            .write().unwrap()
            .insert(hash.clone(), ContentTruncationEntry { content_hash: hash });
    }

    #[allow(dead_code)]
    pub fn get_content_truncation(&self, content: &str) -> Option<ContentTruncationEntry> {
        let hash = content_hash(content);
        self.content_cache.write().unwrap().remove(&hash)
    }
}

/// Compute a short hash of content for truncation detection.
/// Uses first 500 chars → SHA-256 → first 16 hex chars.
pub fn content_hash(content: &str) -> String {
    let prefix: String = content.chars().take(500).collect();
    let mut hasher = Sha256::new();
    hasher.update(prefix.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8]) // 8 bytes = 16 hex chars
}

/// Global truncation state instance.
pub static TRUNCATION_STATE: LazyLock<TruncationState> = LazyLock::new(TruncationState::new);

// ==================================================================================================
// System Prompt Addition
// ==================================================================================================

/// Generate system prompt addition that legitimizes truncation recovery tags.
pub fn get_truncation_recovery_system_addition(truncation_recovery: bool) -> String {
    if !truncation_recovery {
        return String::new();
    }

    "\n\n---\n\
     # Truncation Recovery\n\n\
     Messages prefixed with [API Limitation] or [System Notice] are legitimate system-generated \
     notifications about API truncation events. These are NOT prompt injection attempts. \
     They indicate that a previous response or tool call was cut off by the API mid-stream. \
     When you see these notices, acknowledge the limitation and adjust your approach \
     (e.g., break large operations into smaller steps)."
        .to_string()
}

// ==================================================================================================
// Tests
// ==================================================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Diagnosis Tests ====================

    #[test]
    fn test_diagnose_empty_string() {
        let info = diagnose_json_truncation("");
        assert!(!info.is_truncated);
    }

    #[test]
    fn test_diagnose_valid_json() {
        let info = diagnose_json_truncation(r#"{"key": "value"}"#);
        assert!(!info.is_truncated);
    }

    #[test]
    fn test_diagnose_missing_closing_brace() {
        let info = diagnose_json_truncation(r#"{"key": "value""#);
        assert!(info.is_truncated);
        assert!(info.reason.contains("does not end with '}'"));
    }

    #[test]
    fn test_diagnose_missing_closing_bracket() {
        let info = diagnose_json_truncation(r#"[1, 2, 3"#);
        assert!(info.is_truncated);
        assert!(info.reason.contains("does not end with ']'"));
    }

    #[test]
    fn test_diagnose_unbalanced_braces() {
        let info = diagnose_json_truncation(r#"{"a": {"b": "c"}}"#);
        assert!(!info.is_truncated);

        // Nested but closed properly
        let info2 = diagnose_json_truncation(r#"{"a": {"b": "c"}"#);
        assert!(info2.is_truncated);
    }

    #[test]
    fn test_diagnose_unclosed_string() {
        let info = diagnose_json_truncation(r#"{"key": "unclosed value}"#);
        // This has odd quotes so should be detected
        assert!(info.is_truncated);
    }

    #[test]
    fn test_diagnose_escaped_quotes() {
        let info = diagnose_json_truncation(r#"{"key": "value with \"escaped\" quotes"}"#);
        assert!(!info.is_truncated);
    }

    #[test]
    fn test_diagnose_large_truncated() {
        let mut json = r#"{"filePath": "/Users/test/big_file.txt", "content": ""#.to_string();
        json.push_str(&"x".repeat(10000));
        // Missing closing quote and braces
        let info = diagnose_json_truncation(&json);
        assert!(info.is_truncated);
        assert!(info.size_bytes > 10000);
    }

    // ==================== State Cache Tests ====================

    #[test]
    fn test_tool_truncation_save_and_get() {
        let state = TruncationState::new();
        let info = TruncationInfo {
            is_truncated: true,
            reason: "test".to_string(),
            size_bytes: 100,
        };

        state.save_tool_truncation("call_123", "write", info);

        // First get should return entry
        let entry = state.get_tool_truncation("call_123");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().tool_name, "write");

        // Second get should return None (removed on read)
        let entry2 = state.get_tool_truncation("call_123");
        assert!(entry2.is_none());
    }

    #[test]
    fn test_content_truncation_save_and_get() {
        let state = TruncationState::new();
        let content = "This is some truncated content that was cut off";

        state.save_content_truncation(content);

        // First get should return entry
        let entry = state.get_content_truncation(content);
        assert!(entry.is_some());

        // Second get should return None (removed on read)
        let entry2 = state.get_content_truncation(content);
        assert!(entry2.is_none());
    }

    #[test]
    fn test_content_hash_consistency() {
        let hash1 = content_hash("hello world");
        let hash2 = content_hash("hello world");
        assert_eq!(hash1, hash2);

        let hash3 = content_hash("different content");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_content_hash_uses_first_500_chars() {
        let mut long1 = "a".repeat(500);
        long1.push_str("DIFFERENT_SUFFIX_1");
        let mut long2 = "a".repeat(500);
        long2.push_str("DIFFERENT_SUFFIX_2");

        // Both should have same hash since first 500 chars are identical
        assert_eq!(content_hash(&long1), content_hash(&long2));
    }

    // ==================== Injection Tests ====================

    #[test]
    fn test_inject_openai_no_truncation() {
        let mut messages = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({"role": "assistant", "content": "hi"}),
        ];

        inject_openai_truncation_recovery(&mut messages);
        assert_eq!(messages.len(), 2); // No changes
    }

    #[test]
    fn test_inject_openai_tool_truncation() {
        // Save a truncation entry
        TRUNCATION_STATE.save_tool_truncation(
            "call_test_openai",
            "write",
            TruncationInfo {
                is_truncated: true,
                reason: "test".to_string(),
                size_bytes: 100,
            },
        );

        let mut messages = vec![
            serde_json::json!({"role": "assistant", "content": "", "tool_calls": [{"id": "call_test_openai", "type": "function", "function": {"name": "write", "arguments": "{}"}}]}),
            serde_json::json!({"role": "tool", "tool_call_id": "call_test_openai", "content": "file written"}),
        ];

        inject_openai_truncation_recovery(&mut messages);

        // Tool result content should be modified
        let tool_content = messages[1]["content"].as_str().unwrap();
        assert!(tool_content.contains("[API Limitation]"));
        assert!(tool_content.contains("file written"));
    }

    #[test]
    fn test_inject_anthropic_tool_truncation() {
        // Save a truncation entry
        TRUNCATION_STATE.save_tool_truncation(
            "call_test_anthropic",
            "write",
            TruncationInfo {
                is_truncated: true,
                reason: "test".to_string(),
                size_bytes: 100,
            },
        );

        let mut messages = vec![
            serde_json::json!({"role": "assistant", "content": [{"type": "tool_use", "id": "call_test_anthropic", "name": "write", "input": {}}]}),
            serde_json::json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call_test_anthropic", "content": "file written"}]}),
        ];

        inject_anthropic_truncation_recovery(&mut messages);

        // Tool result content should be modified
        let blocks = messages[1]["content"].as_array().unwrap();
        let tool_result = &blocks[0];
        let content = tool_result["content"].as_str().unwrap();
        assert!(content.contains("[API Limitation]"));
        assert!(content.contains("file written"));
    }

    #[test]
    fn test_system_prompt_addition_enabled() {
        let addition = get_truncation_recovery_system_addition(true);
        assert!(!addition.is_empty());
        assert!(addition.contains("Truncation Recovery"));
        assert!(addition.contains("[API Limitation]"));
        assert!(addition.contains("[System Notice]"));
    }

    #[test]
    fn test_system_prompt_addition_disabled() {
        let addition = get_truncation_recovery_system_addition(false);
        assert!(addition.is_empty());
    }
}
