use std::sync::Arc;

use gcp_auth::TokenProvider;

use crate::error::GoogleAdsError;

const ADS_SCOPE: &str = "https://www.googleapis.com/auth/adwords";

#[derive(Clone)]
pub struct AuthProvider {
    provider: Arc<dyn TokenProvider>,
}

impl AuthProvider {
    pub async fn new() -> Result<Self, GoogleAdsError> {
        let provider = gcp_auth::provider().await?;
        Ok(Self { provider })
    }

    pub async fn token(&self) -> Result<String, GoogleAdsError> {
        let token = self.provider.token(&[ADS_SCOPE]).await?;
        Ok(token.as_str().to_owned())
    }
}
