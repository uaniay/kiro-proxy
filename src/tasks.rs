use chrono::{Duration, Utc};
use sqlx::sqlite::SqlitePool;

use crate::auth::refresh::refresh_aws_sso_oidc;
use crate::auth::types::Credentials;
use crate::db;

/// Spawn background tasks for token refresh and session cleanup.
pub fn spawn_background_tasks(db_pool: SqlitePool) {
    let pool1 = db_pool.clone();
    let pool2 = db_pool;

    // Token refresh: every 5 minutes
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            if let Err(e) = refresh_all_tokens(&pool1).await {
                tracing::error!("Background token refresh failed: {}", e);
            }
        }
    });

    // Session cleanup: every hour
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            match db::cleanup_expired_sessions(&pool2).await {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!("Cleaned up {} expired sessions", count);
                    }
                }
                Err(e) => tracing::error!("Session cleanup failed: {}", e),
            }
        }
    });
}

async fn refresh_all_tokens(pool: &SqlitePool) -> anyhow::Result<()> {
    let threshold = (Utc::now() + Duration::minutes(5)).to_rfc3339();
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Refresh user tokens
    let expiring_users = db::get_expiring_kiro_tokens(pool, &threshold).await?;
    for row in &expiring_users {
        if row.client_id.is_none() || row.client_secret.is_none() {
            continue;
        }
        let creds = Credentials {
            refresh_token: row.refresh_token.clone(),
            access_token: row.access_token.clone(),
            expires_at: None,
            profile_arn: None,
            region: row.sso_region.clone().unwrap_or_else(|| "us-east-1".to_string()),
            client_id: row.client_id.clone(),
            client_secret: row.client_secret.clone(),
            sso_region: row.sso_region.clone(),
            scopes: None,
        };

        match refresh_aws_sso_oidc(&http_client, &creds).await {
            Ok(token_data) => {
                let expiry = token_data.expires_at.to_rfc3339();
                db::update_kiro_token_access(
                    pool, &row.user_id, &token_data.access_token, &expiry,
                    token_data.refresh_token.as_deref(),
                ).await?;
                tracing::debug!("Refreshed token for user {}", row.user_id);
            }
            Err(e) => {
                tracing::warn!("Failed to refresh token for user {}: {}", row.user_id, e);
                db::mark_kiro_token_expired(pool, &row.user_id).await?;
            }
        }
    }

    // Refresh pool tokens
    let expiring_pool = db::get_expiring_pool_entries(pool, &threshold).await?;
    for entry in &expiring_pool {
        if entry.client_id.is_none() || entry.client_secret.is_none() {
            continue;
        }
        let creds = Credentials {
            refresh_token: entry.refresh_token.clone(),
            access_token: entry.access_token.clone(),
            expires_at: None,
            profile_arn: None,
            region: entry.sso_region.clone().unwrap_or_else(|| "us-east-1".to_string()),
            client_id: entry.client_id.clone(),
            client_secret: entry.client_secret.clone(),
            sso_region: entry.sso_region.clone(),
            scopes: None,
        };

        match refresh_aws_sso_oidc(&http_client, &creds).await {
            Ok(token_data) => {
                let expiry = token_data.expires_at.to_rfc3339();
                db::update_pool_entry_access(
                    pool, &entry.id, &token_data.access_token, &expiry,
                    token_data.refresh_token.as_deref(),
                ).await?;
                tracing::debug!("Refreshed pool token {}", entry.label);
            }
            Err(e) => {
                tracing::warn!("Failed to refresh pool token {}: {}", entry.label, e);
                db::mark_pool_entry_expired(pool, &entry.id).await?;
            }
        }
    }

    let total = expiring_users.len() + expiring_pool.len();
    if total > 0 {
        tracing::info!("Token refresh: {} user + {} pool tokens processed", expiring_users.len(), expiring_pool.len());
    }

    Ok(())
}
