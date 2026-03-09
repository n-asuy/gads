use std::borrow::Cow;

use rmcp::model::ErrorCode;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GoogleAdsError {
    #[error("authentication failed: {0}")]
    Auth(#[from] gcp_auth::Error),

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error (status {status}): {message}")]
    Api { status: u16, message: String },

    #[error("missing environment variable: {0}")]
    MissingEnvVar(String),

    #[error("invalid customer id for {field}: {value} (expected digits only, no hyphens)")]
    InvalidCustomerId { field: &'static str, value: String },

    #[error("invalid response: {0}")]
    InvalidResponse(String),

    #[error("query build error: {0}")]
    QueryBuild(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

impl GoogleAdsError {
    pub fn api(status: u16, body: String) -> Self {
        Self::Api {
            status,
            message: summarize_api_error(status, &body),
        }
    }
}

impl From<GoogleAdsError> for rmcp::model::ErrorData {
    fn from(err: GoogleAdsError) -> Self {
        Self {
            code: ErrorCode::INTERNAL_ERROR,
            message: Cow::Owned(err.to_string()),
            data: None,
        }
    }
}

#[derive(Default)]
struct ParsedApiError {
    status: Option<String>,
    message: Option<String>,
    code: Option<String>,
    request_id: Option<String>,
}

fn summarize_api_error(status: u16, body: &str) -> String {
    let Some(parsed) = parse_api_error(body) else {
        return format!("HTTP {status}: {}", truncate(body, 320));
    };

    let mut summary = format!("HTTP {status}");
    if let Some(api_status) = parsed.status.as_deref() {
        summary.push(' ');
        summary.push_str(api_status);
    }
    if let Some(message) = parsed.message.as_deref() {
        summary.push_str(": ");
        summary.push_str(message);
    }
    if let Some(code) = parsed.code.as_deref() {
        summary.push_str(" [code=");
        summary.push_str(code);
        summary.push(']');
    }
    if let Some(request_id) = parsed.request_id.as_deref() {
        summary.push_str(" [request_id=");
        summary.push_str(request_id);
        summary.push(']');
    }
    if let Some(hint) = hint_for_error(parsed.code.as_deref(), parsed.message.as_deref()) {
        summary.push_str(" Hint: ");
        summary.push_str(hint);
    }

    summary
}

fn parse_api_error(body: &str) -> Option<ParsedApiError> {
    let json: Value = serde_json::from_str(body).ok()?;
    let error_obj = extract_error_object(&json)?;

    let mut parsed = ParsedApiError {
        status: error_obj
            .get("status")
            .and_then(Value::as_str)
            .map(|s| s.to_owned()),
        message: error_obj
            .get("message")
            .and_then(Value::as_str)
            .map(|s| s.to_owned()),
        ..Default::default()
    };

    if let Some(details) = error_obj.get("details").and_then(Value::as_array) {
        for detail in details {
            if parsed.request_id.is_none() {
                parsed.request_id = detail
                    .get("requestId")
                    .and_then(Value::as_str)
                    .map(|s| s.to_owned());
            }

            if parsed.code.is_none() {
                parsed.code = extract_error_code(detail);
            }
        }
    }

    Some(parsed)
}

fn extract_error_object(json: &Value) -> Option<&Value> {
    if let Some(error) = json.get("error") {
        return Some(error);
    }

    json.as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("error"))
}

fn extract_error_code(detail: &Value) -> Option<String> {
    let first_error = detail
        .get("errors")
        .and_then(Value::as_array)
        .and_then(|errors| errors.first())?;
    let error_code_obj = first_error.get("errorCode")?.as_object()?;

    error_code_obj
        .values()
        .find_map(Value::as_str)
        .map(|s| s.to_owned())
}

fn hint_for_error(code: Option<&str>, message: Option<&str>) -> Option<&'static str> {
    if matches!(code, Some("INVALID_LOGIN_CUSTOMER_ID")) {
        return Some(
            "Set login-customer-id to digits only (no hyphens) and ensure the MCC is accessible.",
        );
    }

    if matches!(code, Some("USER_PERMISSION_DENIED")) {
        return Some(
            "If this is a direct account, unset login-customer-id. If it's an MCC path, use a linked manager account.",
        );
    }

    if message
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("requires a quota project")
    {
        return Some(
            "Run `gcloud auth application-default set-quota-project <PROJECT_ID>` or set GOOGLE_CLOUD_QUOTA_PROJECT.",
        );
    }

    None
}

fn truncate(s: &str, max_len: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max_len {
        return trimmed.to_owned();
    }

    let mut out = String::with_capacity(max_len + 1);
    for ch in trimmed.chars().take(max_len) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::summarize_api_error;

    #[test]
    fn summarizes_google_ads_failure_array_shape() {
        let body = r#"[{
          "error": {
            "code": 403,
            "message": "The caller does not have permission",
            "status": "PERMISSION_DENIED",
            "details": [{
              "@type": "type.googleapis.com/google.ads.googleads.v21.errors.GoogleAdsFailure",
              "errors": [{
                "errorCode": {
                  "authorizationError": "USER_PERMISSION_DENIED"
                }
              }],
              "requestId": "abc-123"
            }]
          }
        }]"#;

        let summary = summarize_api_error(403, body);
        assert!(summary.contains("HTTP 403 PERMISSION_DENIED"));
        assert!(summary.contains("USER_PERMISSION_DENIED"));
        assert!(summary.contains("request_id=abc-123"));
    }

    #[test]
    fn summarizes_quota_project_hint() {
        let body = r#"{
          "error": {
            "code": 403,
            "message": "The googleads.googleapis.com API requires a quota project, which is not set by default.",
            "status": "PERMISSION_DENIED",
            "details": []
          }
        }"#;

        let summary = summarize_api_error(403, body);
        assert!(summary.contains("quota project"));
        assert!(summary.contains("GOOGLE_CLOUD_QUOTA_PROJECT"));
    }
}
