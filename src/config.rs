use anyhow::Result;

#[derive(Clone, Debug, PartialEq)]
pub enum FakeReasoningHandling {
    AsReasoningContent,
    Remove,
    Pass,
    StripTags,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DebugMode {
    Off,
    Errors,
    All,
}

#[derive(Clone)]
pub struct Config {
    // Server
    pub server_host: String,
    pub server_port: u16,

    // Proxy auth
    pub proxy_api_key: String,

    // Database (optional — enables multi-user mode)
    pub database_url: Option<String>,

    // Kiro
    pub kiro_region: String,
    pub kiro_sso_region: Option<String>,
    pub kiro_refresh_token: Option<String>,
    pub kiro_client_id: Option<String>,
    pub kiro_client_secret: Option<String>,

    // Timeouts
    pub token_refresh_threshold: u64,
    pub first_token_timeout: u64,

    // HTTP client
    pub http_max_connections: usize,
    pub http_connect_timeout: u64,
    pub http_request_timeout: u64,
    pub http_max_retries: u32,

    // Debug
    pub debug_mode: DebugMode,
    pub log_level: String,

    // Converter settings (referenced by converters)
    pub tool_description_max_length: usize,
    pub fake_reasoning_enabled: bool,
    pub fake_reasoning_max_tokens: u32,
    pub fake_reasoning_handling: FakeReasoningHandling,

    // Truncation recovery
    pub truncation_recovery: bool,

    // Conversation logging
    pub enable_conversation_log: bool,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("server_host", &self.server_host)
            .field("server_port", &self.server_port)
            .field("proxy_api_key", &"[REDACTED]")
            .field("kiro_region", &self.kiro_region)
            .field("kiro_sso_region", &self.kiro_sso_region)
            .field("kiro_refresh_token", &self.kiro_refresh_token.as_ref().map(|_| "[REDACTED]"))
            .field("debug_mode", &self.debug_mode)
            .field("log_level", &self.log_level)
            .finish()
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        dotenvy::dotenv().ok();

        let proxy_api_key = std::env::var("PROXY_API_KEY")
            .unwrap_or_default();

        let config = Config {
            server_host: std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: std::env::var("SERVER_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(9199),
            proxy_api_key,
            database_url: std::env::var("DATABASE_URL").ok().filter(|s| !s.is_empty()),
            kiro_region: std::env::var("KIRO_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            kiro_sso_region: std::env::var("KIRO_SSO_REGION").ok().filter(|s| !s.is_empty()),
            kiro_refresh_token: std::env::var("KIRO_REFRESH_TOKEN").ok().filter(|s| !s.is_empty()),
            kiro_client_id: std::env::var("KIRO_CLIENT_ID").ok().filter(|s| !s.is_empty()),
            kiro_client_secret: std::env::var("KIRO_CLIENT_SECRET").ok().filter(|s| !s.is_empty()),
            token_refresh_threshold: 300,
            first_token_timeout: 15,
            http_max_connections: 20,
            http_connect_timeout: 30,
            http_request_timeout: 300,
            http_max_retries: 3,
            debug_mode: std::env::var("DEBUG_MODE")
                .map(|v| parse_debug_mode(&v))
                .unwrap_or(DebugMode::Off),
            log_level: std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            tool_description_max_length: 10000,
            fake_reasoning_enabled: true,
            fake_reasoning_max_tokens: 4000,
            fake_reasoning_handling: FakeReasoningHandling::AsReasoningContent,
            truncation_recovery: true,
            enable_conversation_log: std::env::var("ENABLE_CONVERSATION_LOG")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
        };

        Ok(config)
    }

    pub fn with_defaults() -> Self {
        Config {
            server_host: "0.0.0.0".to_string(),
            server_port: 9199,
            proxy_api_key: String::new(),
            database_url: None,
            kiro_region: "us-east-1".to_string(),
            kiro_sso_region: None,
            kiro_refresh_token: None,
            kiro_client_id: None,
            kiro_client_secret: None,
            token_refresh_threshold: 300,
            first_token_timeout: 15,
            http_max_connections: 20,
            http_connect_timeout: 30,
            http_request_timeout: 300,
            http_max_retries: 3,
            debug_mode: DebugMode::Off,
            log_level: "info".to_string(),
            tool_description_max_length: 10000,
            fake_reasoning_enabled: false,
            fake_reasoning_max_tokens: 4000,
            fake_reasoning_handling: FakeReasoningHandling::AsReasoningContent,
            truncation_recovery: true,
            enable_conversation_log: false,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.proxy_api_key.len() < 16 {
            anyhow::bail!("PROXY_API_KEY must be at least 16 characters for security");
        }
        Ok(())
    }
}

pub fn parse_debug_mode(s: &str) -> DebugMode {
    match s.to_lowercase().as_str() {
        "errors" => DebugMode::Errors,
        "all" => DebugMode::All,
        _ => DebugMode::Off,
    }
}
