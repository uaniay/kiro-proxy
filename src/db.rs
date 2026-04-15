use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use uuid::Uuid;

pub async fn create_pool(database_url: &str) -> Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .context("Failed to connect to SQLite database")?;
    Ok(pool)
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::query("PRAGMA journal_mode=WAL").execute(pool).await?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(pool).await?;

    for schema in &[
        include_str!("../migrations/001_init.sql"),
        include_str!("../migrations/002_usage.sql"),
        include_str!("../migrations/003_user_status.sql"),
        include_str!("../migrations/004_token_enabled.sql"),
    ] {
        for statement in schema.split(';') {
            let trimmed = statement.trim();
            if !trimmed.is_empty() {
                match sqlx::query(trimmed).execute(pool).await {
                    Ok(_) => {}
                    Err(e) => {
                        let err_str = e.to_string();
                        // Ignore "duplicate column" errors from ALTER TABLE re-runs
                        if err_str.contains("duplicate column name") {
                            tracing::debug!("Migration skipped (column already exists): {}", &trimmed[..trimmed.len().min(80)]);
                        } else {
                            return Err(e).with_context(|| format!("Failed to execute migration: {}", &trimmed[..trimmed.len().min(80)]));
                        }
                    }
                }
            }
        }
    }
    tracing::info!("Database migrations completed");
    Ok(())
}

// ── Users ────────────────────────────────────────────────────

pub async fn create_user(pool: &SqlitePool, email: &str, name: &str, password_hash: &str) -> Result<(String, String, String)> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    // First user becomes admin
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool).await?;
    let (role, status) = if count.0 == 0 { ("admin", "active") } else { ("user", "pending") };

    sqlx::query("INSERT INTO users (id, email, name, role, status, password_hash, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(&id).bind(email).bind(name).bind(role).bind(status).bind(password_hash).bind(&now)
        .execute(pool).await
        .context("Failed to create user")?;

    Ok((id, role.to_string(), status.to_string()))
}

pub async fn get_user_by_email(pool: &SqlitePool, email: &str) -> Result<Option<UserRow>> {
    let row = sqlx::query_as::<_, UserRow>("SELECT id, email, name, role, status, password_hash, created_at, last_login FROM users WHERE email = ?")
        .bind(email)
        .fetch_optional(pool).await?;
    Ok(row)
}

pub async fn get_user_by_id(pool: &SqlitePool, id: &str) -> Result<Option<UserRow>> {
    let row = sqlx::query_as::<_, UserRow>("SELECT id, email, name, role, status, password_hash, created_at, last_login FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(pool).await?;
    Ok(row)
}

pub async fn list_users(pool: &SqlitePool) -> Result<Vec<UserRow>> {
    let rows = sqlx::query_as::<_, UserRow>("SELECT id, email, name, role, status, password_hash, created_at, last_login FROM users ORDER BY created_at")
        .fetch_all(pool).await?;
    Ok(rows)
}

pub async fn delete_user(pool: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn update_last_login(pool: &SqlitePool, user_id: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE users SET last_login = ? WHERE id = ?")
        .bind(&now).bind(user_id).execute(pool).await?;
    Ok(())
}

pub async fn approve_user(pool: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("UPDATE users SET status = 'active' WHERE id = ? AND status = 'pending'")
        .bind(id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn reject_user(pool: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("UPDATE users SET status = 'rejected' WHERE id = ? AND status = 'pending'")
        .bind(id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_user_status(pool: &SqlitePool, user_id: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT status FROM users WHERE id = ?")
        .bind(user_id).fetch_optional(pool).await?;
    Ok(row.map(|r| r.0))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub status: String,
    pub password_hash: String,
    pub created_at: String,
    pub last_login: Option<String>,
}

// ── Sessions ─────────────────────────────────────────────────

pub async fn create_session(pool: &SqlitePool, user_id: &str) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let expires_at = (now + chrono::Duration::hours(24)).to_rfc3339();
    let created_at = now.to_rfc3339();

    sqlx::query("INSERT INTO sessions (id, user_id, expires_at, created_at) VALUES (?, ?, ?, ?)")
        .bind(&id).bind(user_id).bind(&expires_at).bind(&created_at)
        .execute(pool).await?;
    Ok(id)
}

pub async fn get_session(pool: &SqlitePool, session_id: &str) -> Result<Option<SessionRow>> {
    let row = sqlx::query_as::<_, SessionRow>("SELECT s.id, s.user_id, s.expires_at, u.email, u.role, u.status FROM sessions s JOIN users u ON s.user_id = u.id WHERE s.id = ?")
        .bind(session_id)
        .fetch_optional(pool).await?;
    Ok(row)
}

pub async fn delete_session(pool: &SqlitePool, session_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(session_id).execute(pool).await?;
    Ok(())
}

pub async fn cleanup_expired_sessions(pool: &SqlitePool) -> Result<u64> {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at < ?")
        .bind(&now).execute(pool).await?;
    Ok(result.rows_affected())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionRow {
    pub id: String,
    pub user_id: String,
    pub expires_at: String,
    pub email: String,
    pub role: String,
    pub status: String,
}

// ── API Keys ─────────────────────────────────────────────────

pub async fn create_api_key(pool: &SqlitePool, user_id: &str, key_hash: &str, key_prefix: &str, name: &str) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    // Max 10 keys per user
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM api_keys WHERE user_id = ?")
        .bind(user_id).fetch_one(pool).await?;
    if count.0 >= 10 {
        anyhow::bail!("Maximum 10 API keys per user");
    }

    sqlx::query("INSERT INTO api_keys (id, user_id, key_hash, key_prefix, name, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&id).bind(user_id).bind(key_hash).bind(key_prefix).bind(name).bind(&now)
        .execute(pool).await?;
    Ok(id)
}

pub async fn list_api_keys(pool: &SqlitePool, user_id: &str) -> Result<Vec<ApiKeyRow>> {
    let rows = sqlx::query_as::<_, ApiKeyRow>("SELECT id, user_id, key_prefix, name, last_used, created_at FROM api_keys WHERE user_id = ? ORDER BY created_at")
        .bind(user_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn get_api_key_by_hash(pool: &SqlitePool, key_hash: &str) -> Result<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as("SELECT id, user_id FROM api_keys WHERE key_hash = ?")
        .bind(key_hash).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn delete_api_key(pool: &SqlitePool, key_id: &str, user_id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM api_keys WHERE id = ? AND user_id = ?")
        .bind(key_id).bind(user_id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn touch_api_key(pool: &SqlitePool, key_id: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE api_keys SET last_used = ? WHERE id = ?")
        .bind(&now).bind(key_id).execute(pool).await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ApiKeyRow {
    pub id: String,
    pub user_id: String,
    pub key_prefix: String,
    pub name: String,
    pub last_used: Option<String>,
    pub created_at: String,
}

// ── User Kiro Tokens ─────────────────────────────────────────

pub async fn upsert_kiro_token(
    pool: &SqlitePool, user_id: &str, refresh_token: &str,
    access_token: Option<&str>, token_expiry: Option<&str>,
    client_id: Option<&str>, client_secret: Option<&str>,
    sso_region: Option<&str>, sso_start_url: Option<&str>,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO user_kiro_tokens (user_id, refresh_token, access_token, token_expiry, client_id, client_secret, sso_region, sso_start_url, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(user_id) DO UPDATE SET refresh_token=excluded.refresh_token, access_token=excluded.access_token, token_expiry=excluded.token_expiry, \
         client_id=excluded.client_id, client_secret=excluded.client_secret, sso_region=excluded.sso_region, sso_start_url=excluded.sso_start_url, updated_at=excluded.updated_at"
    )
    .bind(user_id).bind(refresh_token).bind(access_token).bind(token_expiry)
    .bind(client_id).bind(client_secret).bind(sso_region).bind(sso_start_url).bind(&now)
    .execute(pool).await?;
    Ok(())
}

pub async fn get_kiro_token(pool: &SqlitePool, user_id: &str) -> Result<Option<KiroTokenRow>> {
    let row = sqlx::query_as::<_, KiroTokenRow>(
        "SELECT user_id, refresh_token, access_token, token_expiry, client_id, client_secret, sso_region, sso_start_url, enabled, updated_at FROM user_kiro_tokens WHERE user_id = ?"
    ).bind(user_id).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn delete_kiro_token(pool: &SqlitePool, user_id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM user_kiro_tokens WHERE user_id = ?")
        .bind(user_id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_expiring_kiro_tokens(pool: &SqlitePool, threshold: &str) -> Result<Vec<KiroTokenRow>> {
    let rows = sqlx::query_as::<_, KiroTokenRow>(
        "SELECT user_id, refresh_token, access_token, token_expiry, client_id, client_secret, sso_region, sso_start_url, enabled, updated_at \
         FROM user_kiro_tokens WHERE token_expiry IS NOT NULL AND token_expiry < ?"
    ).bind(threshold).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn update_kiro_token_access(pool: &SqlitePool, user_id: &str, access_token: &str, token_expiry: &str, refresh_token: Option<&str>) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    if let Some(rt) = refresh_token {
        sqlx::query("UPDATE user_kiro_tokens SET access_token = ?, token_expiry = ?, refresh_token = ?, updated_at = ? WHERE user_id = ?")
            .bind(access_token).bind(token_expiry).bind(rt).bind(&now).bind(user_id)
            .execute(pool).await?;
    } else {
        sqlx::query("UPDATE user_kiro_tokens SET access_token = ?, token_expiry = ?, updated_at = ? WHERE user_id = ?")
            .bind(access_token).bind(token_expiry).bind(&now).bind(user_id)
            .execute(pool).await?;
    }
    Ok(())
}

pub async fn mark_kiro_token_expired(pool: &SqlitePool, user_id: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE user_kiro_tokens SET access_token = NULL, token_expiry = NULL, updated_at = ? WHERE user_id = ?")
        .bind(&now).bind(user_id).execute(pool).await?;
    Ok(())
}

pub async fn list_all_kiro_tokens(pool: &SqlitePool) -> Result<Vec<KiroTokenWithUser>> {
    let rows = sqlx::query_as::<_, KiroTokenWithUser>(
        "SELECT t.user_id, u.email, u.name, t.sso_region, t.enabled, \
         (t.access_token IS NOT NULL) as has_token, t.updated_at \
         FROM user_kiro_tokens t JOIN users u ON t.user_id = u.id ORDER BY u.email"
    ).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn toggle_kiro_token(pool: &SqlitePool, user_id: &str, enabled: bool) -> Result<bool> {
    let result = sqlx::query("UPDATE user_kiro_tokens SET enabled = ? WHERE user_id = ?")
        .bind(enabled as i32).bind(user_id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KiroTokenRow {
    pub user_id: String,
    pub refresh_token: String,
    pub access_token: Option<String>,
    pub token_expiry: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub sso_region: Option<String>,
    pub sso_start_url: Option<String>,
    pub enabled: bool,
    pub updated_at: String,
}

/// Kiro token with user info, for admin listing.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KiroTokenWithUser {
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub sso_region: Option<String>,
    pub enabled: bool,
    pub has_token: bool,
    pub updated_at: String,
}

// ── Admin Token Pool ─────────────────────────────────────────

pub async fn add_pool_entry(
    pool: &SqlitePool, label: &str, refresh_token: &str,
    client_id: Option<&str>, client_secret: Option<&str>, sso_region: Option<&str>,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO admin_token_pool (id, label, refresh_token, client_id, client_secret, sso_region, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id).bind(label).bind(refresh_token).bind(client_id).bind(client_secret).bind(sso_region).bind(&now)
    .execute(pool).await?;
    Ok(id)
}

pub async fn get_pool_entry(pool: &SqlitePool, id: &str) -> Result<Option<PoolEntryRow>> {
    let row = sqlx::query_as::<_, PoolEntryRow>(
        "SELECT id, label, refresh_token, access_token, token_expiry, client_id, client_secret, sso_region, enabled, last_used, created_at FROM admin_token_pool WHERE id = ?"
    ).bind(id).fetch_optional(pool).await?;
    Ok(row)
}

pub async fn update_pool_entry_tokens(
    pool: &SqlitePool, id: &str, refresh_token: &str,
    access_token: &str, token_expiry: &str,
    client_id: &str, client_secret: &str,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE admin_token_pool SET refresh_token = ?, access_token = ?, token_expiry = ?, client_id = ?, client_secret = ? WHERE id = ?"
    )
    .bind(refresh_token).bind(access_token).bind(token_expiry)
    .bind(client_id).bind(client_secret).bind(id)
    .execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_pool_entries(pool: &SqlitePool) -> Result<Vec<PoolEntryRow>> {
    let rows = sqlx::query_as::<_, PoolEntryRow>(
        "SELECT id, label, refresh_token, access_token, token_expiry, client_id, client_secret, sso_region, enabled, last_used, created_at FROM admin_token_pool ORDER BY label"
    ).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn get_enabled_pool_entries(pool: &SqlitePool) -> Result<Vec<PoolEntryRow>> {
    let rows = sqlx::query_as::<_, PoolEntryRow>(
        "SELECT id, label, refresh_token, access_token, token_expiry, client_id, client_secret, sso_region, enabled, last_used, created_at \
         FROM admin_token_pool WHERE enabled = 1 AND access_token IS NOT NULL ORDER BY last_used ASC NULLS FIRST"
    ).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn delete_pool_entry(pool: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM admin_token_pool WHERE id = ?")
        .bind(id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn toggle_pool_entry(pool: &SqlitePool, id: &str, enabled: bool) -> Result<bool> {
    let result = sqlx::query("UPDATE admin_token_pool SET enabled = ? WHERE id = ?")
        .bind(enabled as i32).bind(id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn touch_pool_entry(pool: &SqlitePool, id: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE admin_token_pool SET last_used = ? WHERE id = ?")
        .bind(&now).bind(id).execute(pool).await?;
    Ok(())
}

pub async fn update_pool_entry_access(pool: &SqlitePool, id: &str, access_token: &str, token_expiry: &str, refresh_token: Option<&str>) -> Result<()> {
    if let Some(rt) = refresh_token {
        sqlx::query("UPDATE admin_token_pool SET access_token = ?, token_expiry = ?, refresh_token = ? WHERE id = ?")
            .bind(access_token).bind(token_expiry).bind(rt).bind(id)
            .execute(pool).await?;
    } else {
        sqlx::query("UPDATE admin_token_pool SET access_token = ?, token_expiry = ? WHERE id = ?")
            .bind(access_token).bind(token_expiry).bind(id)
            .execute(pool).await?;
    }
    Ok(())
}

pub async fn mark_pool_entry_expired(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("UPDATE admin_token_pool SET access_token = NULL, token_expiry = NULL WHERE id = ?")
        .bind(id).execute(pool).await?;
    Ok(())
}

pub async fn get_expiring_pool_entries(pool: &SqlitePool, threshold: &str) -> Result<Vec<PoolEntryRow>> {
    let rows = sqlx::query_as::<_, PoolEntryRow>(
        "SELECT id, label, refresh_token, access_token, token_expiry, client_id, client_secret, sso_region, enabled, last_used, created_at \
         FROM admin_token_pool WHERE enabled = 1 AND token_expiry IS NOT NULL AND token_expiry < ?"
    ).bind(threshold).fetch_all(pool).await?;
    Ok(rows)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct PoolEntryRow {
    pub id: String,
    pub label: String,
    #[serde(skip)]
    pub refresh_token: String,
    #[serde(skip)]
    pub access_token: Option<String>,
    pub token_expiry: Option<String>,
    #[serde(skip)]
    pub client_id: Option<String>,
    #[serde(skip)]
    pub client_secret: Option<String>,
    pub sso_region: Option<String>,
    pub enabled: bool,
    pub last_used: Option<String>,
    pub created_at: String,
}

// ── Usage Tracking ───────────────────────────────────────────

pub async fn record_usage(
    pool: &SqlitePool, api_key_id: &str, user_id: &str,
    model: &str, input_tokens: i64, output_tokens: i64,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO usage_logs (id, api_key_id, user_id, model, input_tokens, output_tokens, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id).bind(api_key_id).bind(user_id).bind(model)
    .bind(input_tokens).bind(output_tokens).bind(&now)
    .execute(pool).await?;

    sqlx::query("UPDATE api_keys SET last_used = ? WHERE id = ?")
        .bind(&now).bind(api_key_id).execute(pool).await?;

    Ok(())
}

pub async fn get_key_usage_stats(pool: &SqlitePool, user_id: &str) -> Result<Vec<KeyUsageStats>> {
    let rows = sqlx::query_as::<_, KeyUsageStats>(
        "SELECT k.id, k.key_prefix, k.name, k.last_used, k.created_at, k.user_id, \
         COALESCE(SUM(u.input_tokens), 0) as total_input_tokens, \
         COALESCE(SUM(u.output_tokens), 0) as total_output_tokens, \
         COUNT(u.id) as request_count \
         FROM api_keys k LEFT JOIN usage_logs u ON k.id = u.api_key_id \
         WHERE k.user_id = ? GROUP BY k.id ORDER BY k.created_at"
    ).bind(user_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn get_all_usage_stats(pool: &SqlitePool) -> Result<Vec<KeyUsageStats>> {
    let rows = sqlx::query_as::<_, KeyUsageStats>(
        "SELECT k.id, k.key_prefix, k.name, k.last_used, k.created_at, k.user_id, \
         COALESCE(SUM(u.input_tokens), 0) as total_input_tokens, \
         COALESCE(SUM(u.output_tokens), 0) as total_output_tokens, \
         COUNT(u.id) as request_count \
         FROM api_keys k LEFT JOIN usage_logs u ON k.id = u.api_key_id \
         GROUP BY k.id ORDER BY k.created_at"
    ).fetch_all(pool).await?;
    Ok(rows)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct KeyUsageStats {
    pub id: String,
    pub key_prefix: String,
    pub name: String,
    pub last_used: Option<String>,
    pub created_at: String,
    pub user_id: String,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub request_count: i64,
}
