-- kiro-proxy v2: multi-user schema

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_login TEXT
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key_hash TEXT NOT NULL,
    key_prefix TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    last_used TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS user_kiro_tokens (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    refresh_token TEXT NOT NULL,
    access_token TEXT,
    token_expiry TEXT,
    client_id TEXT,
    client_secret TEXT,
    sso_region TEXT,
    sso_start_url TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS admin_token_pool (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL DEFAULT 'pool-1',
    refresh_token TEXT NOT NULL,
    access_token TEXT,
    token_expiry TEXT,
    client_id TEXT,
    client_secret TEXT,
    sso_region TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_used TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(label)
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash ON api_keys(key_hash);
