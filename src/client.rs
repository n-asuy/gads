use reqwest::Client;
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::auth::AuthProvider;
use crate::error::GoogleAdsError;
use crate::ids;
use crate::profile;

const API_VERSION: &str = "v21";
const BASE_URL: &str = "https://googleads.googleapis.com";

#[derive(Clone)]
pub struct GoogleAdsClient {
    http: Client,
    auth: AuthProvider,
    developer_token: String,
    login_customer_id: Option<String>,
    quota_project_id: Option<String>,
}

impl GoogleAdsClient {
    pub async fn new() -> Result<Self, GoogleAdsError> {
        Self::new_with_login_customer_id_override(None).await
    }

    pub async fn new_with_login_customer_id_override(
        login_customer_id_override: Option<Option<String>>,
    ) -> Result<Self, GoogleAdsError> {
        let developer_token = resolve_developer_token()?;

        let login_customer_id = match login_customer_id_override {
            Some(value) => value,
            None => resolve_login_customer_id()?,
        };
        let quota_project_id = resolve_quota_project_id();

        let auth = AuthProvider::new().await?;

        let http = Client::builder()
            .user_agent("gads/0.1.0")
            .build()
            .map_err(GoogleAdsError::Http)?;

        Ok(Self {
            http,
            auth,
            developer_token,
            login_customer_id,
            quota_project_id,
        })
    }

    pub async fn search_stream(
        &self,
        customer_id: &str,
        query: &str,
    ) -> Result<Value, GoogleAdsError> {
        let customer_id = ids::normalize_customer_id(customer_id, "customer_id")?;
        let url =
            format!("{BASE_URL}/{API_VERSION}/customers/{customer_id}/googleAds:searchStream");

        let token = self.auth.token().await?;

        let mut request = self
            .http
            .post(&url)
            .header("developer-token", &self.developer_token)
            .bearer_auth(&token)
            .json(&serde_json::json!({ "query": query }));

        if let Some(ref login_id) = self.login_customer_id {
            request = request.header("login-customer-id", login_id);
        }
        if let Some(ref quota_project_id) = self.quota_project_id {
            request = request.header("x-goog-user-project", quota_project_id);
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(GoogleAdsError::api(status.as_u16(), body));
        }

        let json: Value = response.json().await?;
        Ok(json)
    }

    pub async fn mutate(
        &self,
        customer_id: &str,
        service: &str,
        payload: Value,
    ) -> Result<Value, GoogleAdsError> {
        let customer_id = ids::normalize_customer_id(customer_id, "customer_id")?;
        let service = normalize_mutate_service(service)?;
        let url = format!("{BASE_URL}/{API_VERSION}/customers/{customer_id}/{service}:mutate");

        let token = self.auth.token().await?;

        let mut request = self
            .http
            .post(&url)
            .header("developer-token", &self.developer_token)
            .bearer_auth(&token)
            .json(&payload);

        if let Some(ref login_id) = self.login_customer_id {
            request = request.header("login-customer-id", login_id);
        }
        if let Some(ref quota_project_id) = self.quota_project_id {
            request = request.header("x-goog-user-project", quota_project_id);
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(GoogleAdsError::api(status.as_u16(), body));
        }

        let json: Value = response.json().await?;
        Ok(json)
    }

    pub async fn list_accessible_customers(&self) -> Result<Vec<String>, GoogleAdsError> {
        let url = format!("{BASE_URL}/{API_VERSION}/customers:listAccessibleCustomers");

        let token = self.auth.token().await?;

        let mut request = self
            .http
            .get(&url)
            .header("developer-token", &self.developer_token)
            .bearer_auth(&token);
        if let Some(ref quota_project_id) = self.quota_project_id {
            request = request.header("x-goog-user-project", quota_project_id);
        }

        let response = request.send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(GoogleAdsError::api(status.as_u16(), body));
        }

        // Response: { "resourceNames": ["customers/1234567890", ...] }
        let json: Value = response.json().await?;
        let resource_names = json["resourceNames"]
            .as_array()
            .ok_or_else(|| GoogleAdsError::InvalidResponse("missing resourceNames array".into()))?;

        let customer_ids: Vec<String> = resource_names
            .iter()
            .filter_map(|v| v.as_str())
            .map(|rn| rn.strip_prefix("customers/").unwrap_or(rn).to_owned())
            .collect();

        Ok(customer_ids)
    }
}

fn normalize_mutate_service(service: &str) -> Result<String, GoogleAdsError> {
    let trimmed = service.trim();

    if trimmed.is_empty() {
        return Err(GoogleAdsError::InvalidArgument(
            "mutate service must not be empty".to_owned(),
        ));
    }

    let mut chars = trimmed.chars();
    let starts_with_letter = chars
        .next()
        .map(|c| c.is_ascii_alphabetic())
        .unwrap_or(false);

    if !starts_with_letter || !chars.all(|c| c.is_ascii_alphanumeric()) {
        return Err(GoogleAdsError::InvalidArgument(format!(
            "invalid mutate service '{trimmed}' (expected something like campaigns, adGroups, adGroupAds)"
        )));
    }

    Ok(trimmed.to_owned())
}

fn resolve_developer_token() -> Result<String, GoogleAdsError> {
    if let Ok(value) = std::env::var("GOOGLE_ADS_DEVELOPER_TOKEN") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_owned());
        }
    }

    if let Ok(Some(token)) = profile::load_developer_token() {
        return Ok(token);
    }

    Err(GoogleAdsError::MissingEnvVar(
        "GOOGLE_ADS_DEVELOPER_TOKEN (or set via `gads config set developer-token <TOKEN>`)".into(),
    ))
}

fn resolve_login_customer_id() -> Result<Option<String>, GoogleAdsError> {
    if let Ok(value) = std::env::var("GOOGLE_ADS_LOGIN_CUSTOMER_ID") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return ids::normalize_customer_id(trimmed, "GOOGLE_ADS_LOGIN_CUSTOMER_ID").map(Some);
        }
    }

    if let Ok(Some(id)) = profile::load_login_customer_id() {
        return ids::normalize_customer_id(&id, "profile:login_customer_id").map(Some);
    }

    Ok(None)
}

fn resolve_quota_project_id() -> Option<String> {
    for key in ["GOOGLE_CLOUD_QUOTA_PROJECT", "GOOGLE_CLOUD_PROJECT"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_owned());
            }
        }
    }

    if let Ok(adc_path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        if let Some(quota_project_id) = read_quota_project_id(Path::new(&adc_path)) {
            return Some(quota_project_id);
        }
    }

    let home = std::env::var("HOME").ok()?;
    let default_adc_path: PathBuf =
        PathBuf::from(home).join(".config/gcloud/application_default_credentials.json");
    read_quota_project_id(&default_adc_path)
}

fn read_quota_project_id(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;
    let quota_project_id = json.get("quota_project_id")?.as_str()?.trim();
    if quota_project_id.is_empty() {
        None
    } else {
        Some(quota_project_id.to_owned())
    }
}
