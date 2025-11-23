use crate::error::{ApiErrorResponse, FetchError};
use crate::state::UsageData;
use wreq::ClientBuilder;
use wreq::header::{COOKIE, HeaderMap, HeaderValue, USER_AGENT};

/// Fetch usage data from the Claude API using a custom base URL (for testing)
#[doc(hidden)]
pub async fn fetch_usage_data_with_base_url(
    base_url: &str,
    org_id: &str,
    session_key: &str,
) -> Result<UsageData, FetchError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        COOKIE,
        HeaderValue::from_str(&format!("sessionKey={}", session_key))
            .map_err(|e| FetchError::Network(format!("Invalid header value: {}", e)))?,
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"),
    );

    let client = ClientBuilder::new()
        .default_headers(headers)
        .build()
        .map_err(|e| FetchError::Network(format!("Failed to build client: {}", e)))?;

    let url = format!("{}/api/organizations/{}/usage", base_url, org_id);
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| FetchError::Network(format!("Failed to send request: {}", e)))?;

    let status = response.status();
    let response_text = response
        .text()
        .await
        .map_err(|e| FetchError::Network(format!("Failed to read response: {}", e)))?;

    if status.is_success() {
        serde_json::from_str::<UsageData>(&response_text)
            .map_err(|e| FetchError::Parse(format!("Failed to parse response: {}", e)))
    } else if status.as_u16() == 429 {
        // Rate limited - basic detection without Retry-After parsing
        Err(FetchError::RateLimited {
            message: "Too many requests".to_string(),
            retry_after: None,
        })
    } else if status.as_u16() == 401 || status.as_u16() == 403 {
        // Authentication/authorization errors
        let error_msg = match serde_json::from_str::<ApiErrorResponse>(&response_text) {
            Ok(error_data) => format!(
                "{} - {}",
                error_data.error.error_type, error_data.error.message
            ),
            Err(_) => format!("HTTP {}", status),
        };
        Err(FetchError::Auth(error_msg))
    } else {
        // Other errors (5xx, etc.) - treat as network/transient
        let error_msg = match serde_json::from_str::<ApiErrorResponse>(&response_text) {
            Ok(error_data) => format!(
                "{} - {}",
                error_data.error.error_type, error_data.error.message
            ),
            Err(_) => format!("HTTP {}", status),
        };
        Err(FetchError::Network(error_msg))
    }
}

pub async fn fetch_usage_data() -> Result<UsageData, FetchError> {
    let org_id = std::env::var("CLAUDE_ORG_ID")?;
    let session_key = std::env::var("CLAUDE_SESSION_KEY")?;
    fetch_usage_data_with_base_url("https://claude.ai", &org_id, &session_key).await
}
