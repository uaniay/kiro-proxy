CREATE TABLE IF NOT EXISTS conversation_logs (
    id TEXT PRIMARY KEY,
    api_key_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    api_type TEXT NOT NULL,
    model TEXT NOT NULL,
    is_stream INTEGER NOT NULL DEFAULT 0,
    request_body TEXT NOT NULL,
    response_body TEXT,
    request_headers TEXT,
    response_headers TEXT,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversation_logs_api_key_id ON conversation_logs(api_key_id);
CREATE INDEX IF NOT EXISTS idx_conversation_logs_user_id ON conversation_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_conversation_logs_created_at ON conversation_logs(created_at);
