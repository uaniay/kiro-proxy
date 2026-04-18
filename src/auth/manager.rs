use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::refresh;
use super::types::Credentials;

pub struct AuthManager {
    credentials: Arc<RwLock<Credentials>>,
    access_token: Arc<RwLock<Option<String>>>,
    expires_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    client: Client,
    refresh_threshold: i64,
}

impl AuthManager {
    pub fn new_from_env(config: &crate::config::Config) -> Result<Self> {
        let refresh_token = match &config.kiro_refresh_token {
            Some(t) if !t.is_empty() => t.clone(),
            _ => {
                tracing::warn!("No KIRO_REFRESH_TOKEN set — will need device code flow to authenticate");
                String::new()
            }
        };

        let credentials = Credentials {
            refresh_token,
            access_token: None,
            expires_at: None,
            profile_arn: None,
            region: config.kiro_region.clone(),
            client_id: config.kiro_client_id.clone(),
            client_secret: config.kiro_client_secret.clone(),
            sso_region: config.kiro_sso_region.clone(),
            scopes: None,
        };

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            credentials: Arc::new(RwLock::new(credentials)),
            access_token: Arc::new(RwLock::new(None)),
            expires_at: Arc::new(RwLock::new(None)),
            client,
            refresh_threshold: config.token_refresh_threshold as i64,
        })
    }

    pub async fn bootstrap_proxy_credentials(&mut self) -> Result<()> {
        // Skip bootstrap if no refresh token configured
        let has_refresh = {
            let creds = self.credentials.read().await;
            !creds.refresh_token.is_empty() && creds.client_id.is_some()
        };

        if !has_refresh {
            tracing::info!("No refresh token + client credentials — skipping bootstrap");
            return Ok(());
        }

        self.get_access_token()
            .await
            .context("Failed to bootstrap proxy credentials")?;
        tracing::info!("Kiro credentials bootstrapped successfully");
        Ok(())
    }

    async fn is_token_expiring_soon(&self) -> bool {
        let expires_at = self.expires_at.read().await;
        match *expires_at {
            None => true,
            Some(exp) => {
                let threshold = Utc::now() + Duration::seconds(self.refresh_threshold);
                exp <= threshold
            }
        }
    }

    async fn is_token_expired(&self) -> bool {
        let expires_at = self.expires_at.read().await;
        match *expires_at {
            None => true,
            Some(exp) => Utc::now() >= exp,
        }
    }

    async fn refresh_token(&self) -> Result<()> {
        tracing::debug!("Refreshing access token...");

        if !self.is_token_expiring_soon().await {
            return Ok(());
        }

        let mut creds = self.credentials.write().await;
        let token_data = refresh::refresh_aws_sso_oidc(&self.client, &creds).await?;

        {
            let mut access_token = self.access_token.write().await;
            *access_token = Some(token_data.access_token.clone());
        }

        {
            let mut expires_at = self.expires_at.write().await;
            *expires_at = Some(token_data.expires_at);
        }

        if let Some(ref new_refresh_token) = token_data.refresh_token {
            creds.refresh_token = new_refresh_token.clone();
        }

        if let Some(ref new_profile_arn) = token_data.profile_arn {
            creds.profile_arn = Some(new_profile_arn.clone());
        }

        Ok(())
    }

    pub async fn get_access_token(&self) -> Result<String> {
        if self.is_token_expiring_soon().await {
            if let Err(e) = self.refresh_token().await {
                tracing::error!("Token refresh failed: {}", e);

                if !self.is_token_expired().await {
                    tracing::warn!("Using existing token despite refresh failure (not yet expired)");
                    let token = self.access_token.read().await;
                    if let Some(ref t) = *token {
                        return Ok(t.clone());
                    }
                }

                return Err(e).context("Failed to refresh token and no valid token available");
            }
        }

        let token = self.access_token.read().await;
        token.as_ref().cloned().context("No access token available")
    }

    pub async fn get_region(&self) -> String {
        let creds = self.credentials.read().await;
        creds.region.clone()
    }

    pub async fn get_profile_arn(&self) -> Option<String> {
        let creds = self.credentials.read().await;
        creds.profile_arn.clone()
    }
}
