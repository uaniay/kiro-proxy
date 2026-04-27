#![allow(dead_code)]

use serde::Deserialize;
use serde::Deserializer;

fn default_true() -> bool {
    true
}

fn default_tool_choice() -> String {
    "auto".to_string()
}

fn deserialize_input<'de, D>(deserializer: D) -> Result<Vec<serde_json::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Array(arr) => Ok(arr),
        serde_json::Value::String(s) => Ok(vec![serde_json::json!({
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": s}]
        })]),
        _ => Ok(vec![]),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponsesApiRequest {
    pub model: String,
    #[serde(default)]
    pub instructions: Option<String>,
    #[serde(default, deserialize_with = "deserialize_input")]
    pub input: Vec<serde_json::Value>,
    #[serde(default)]
    pub tools: Vec<serde_json::Value>,
    #[serde(default = "default_tool_choice")]
    pub tool_choice: String,
    #[serde(default)]
    pub parallel_tool_calls: bool,
    #[serde(default)]
    pub reasoning: Option<Reasoning>,
    #[serde(default)]
    pub store: bool,
    #[serde(default = "default_true")]
    pub stream: bool,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub service_tier: Option<String>,
    // Accepted but not forwarded
    #[serde(default)]
    pub text: Option<serde_json::Value>,
    #[serde(default)]
    pub prompt_cache_key: Option<String>,
    #[serde(default)]
    pub client_metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub previous_response_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Reasoning {
    pub effort: Option<String>,
    pub summary: Option<String>,
}
