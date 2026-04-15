use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::db::{self, PoolEntryRow};

/// Round-robin pool scheduler for admin token pool.
pub struct PoolScheduler {
    /// Cached enabled pool entries
    cache: Arc<RwLock<Vec<PoolEntryRow>>>,
    /// When the cache was last refreshed
    cache_updated: Arc<RwLock<Instant>>,
    /// Round-robin index
    index: AtomicUsize,
    /// Cache TTL in seconds
    cache_ttl: u64,
}

/// A selected pool entry with access token and region.
#[derive(Debug, Clone)]
pub struct PoolToken {
    pub pool_id: String,
    pub access_token: String,
    pub region: String,
}

impl PoolScheduler {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(Vec::new())),
            cache_updated: Arc::new(RwLock::new(Instant::now() - std::time::Duration::from_secs(600))),
            index: AtomicUsize::new(0),
            cache_ttl: 300, // 5 minutes
        }
    }

    /// Get the next available pool token via round-robin.
    /// Returns None if no enabled pool entries with valid access tokens exist.
    pub async fn next_token(
        &self,
        pool: &sqlx::sqlite::SqlitePool,
        default_region: &str,
    ) -> Option<PoolToken> {
        self.refresh_cache_if_stale(pool).await;

        let entries = self.cache.read().await;
        if entries.is_empty() {
            return None;
        }

        let len = entries.len();
        let idx = self.index.fetch_add(1, Ordering::Relaxed) % len;
        let entry = &entries[idx];

        let access_token = entry.access_token.as_ref()?;
        let region = entry.sso_region.as_deref().unwrap_or(default_region);

        // Update last_used in background
        let pool_clone = pool.clone();
        let entry_id = entry.id.clone();
        tokio::spawn(async move {
            let _ = db::touch_pool_entry(&pool_clone, &entry_id).await;
        });

        Some(PoolToken {
            pool_id: entry.id.clone(),
            access_token: access_token.clone(),
            region: region.to_string(),
        })
    }

    async fn refresh_cache_if_stale(&self, pool: &sqlx::sqlite::SqlitePool) {
        let updated = *self.cache_updated.read().await;
        if updated.elapsed().as_secs() < self.cache_ttl {
            return;
        }

        match db::get_enabled_pool_entries(pool).await {
            Ok(mut entries) => {
                // Also include shared user tokens
                if let Ok(shared) = db::get_shared_kiro_tokens(pool).await {
                    for t in shared {
                        entries.push(PoolEntryRow {
                            id: format!("user:{}", t.user_id),
                            label: format!("shared:{}", t.user_id),
                            refresh_token: t.refresh_token,
                            access_token: t.access_token,
                            token_expiry: t.token_expiry,
                            client_id: t.client_id,
                            client_secret: t.client_secret,
                            sso_region: t.sso_region,
                            enabled: true,
                            last_used: None,
                            created_at: t.updated_at,
                        });
                    }
                }
                let count = entries.len();
                *self.cache.write().await = entries;
                *self.cache_updated.write().await = Instant::now();
                tracing::debug!("Pool cache refreshed: {} entries", count);
            }
            Err(e) => {
                tracing::error!("Failed to refresh pool cache: {}", e);
            }
        }
    }

    /// Force refresh the cache (called after admin adds/removes pool entries).
    pub async fn invalidate_cache(&self) {
        *self.cache_updated.write().await =
            Instant::now() - std::time::Duration::from_secs(self.cache_ttl + 1);
    }
}
