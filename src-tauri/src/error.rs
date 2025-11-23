use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub enum FetchError {
    Network(String),
    Parse(String),
    Auth(String),
    RateLimited {
        message: String,
        retry_after: Option<u64>,
    },
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Network(msg) => write!(f, "Network error: {}", msg),
            FetchError::Parse(msg) => write!(f, "Parse error: {}", msg),
            FetchError::Auth(msg) => write!(f, "Auth error: {}", msg),
            FetchError::RateLimited {
                message,
                retry_after,
            } => {
                if let Some(seconds) = retry_after {
                    write!(f, "Rate limited: {} (retry after {}s)", message, seconds)
                } else {
                    write!(f, "Rate limited: {}", message)
                }
            }
        }
    }
}

impl std::error::Error for FetchError {}

impl FetchError {
    /// Returns true if the error is transient and should be retried
    pub fn is_transient(&self) -> bool {
        match self {
            FetchError::Network(msg) => {
                // Some network errors are not transient and should not be retried
                let msg_lower = msg.to_lowercase();

                // Non-retryable network errors:
                // - SSL/TLS certificate errors
                // - DNS resolution failures
                // - Invalid URLs or malformed requests
                let non_transient_patterns = [
                    "certificate",
                    "cert",
                    "ssl",
                    "tls",
                    "dns",
                    "invalid url",
                    "malformed",
                    "invalid header",
                ];

                // If the error message contains any non-transient pattern, it's not transient
                !non_transient_patterns
                    .iter()
                    .any(|pattern| msg_lower.contains(pattern))
            }
            FetchError::RateLimited { .. } => true,
            FetchError::Auth(_) => false,
            FetchError::Parse(_) => false,
        }
    }

    /// Get a user-friendly error category for display
    pub fn category(&self) -> &'static str {
        match self {
            FetchError::Network(_) => "Offline",
            FetchError::RateLimited { .. } => "Rate Limited",
            FetchError::Auth(_) => "Authentication Error",
            FetchError::Parse(_) => "Parse Error",
        }
    }
}

impl From<std::env::VarError> for FetchError {
    fn from(e: std::env::VarError) -> Self {
        FetchError::Auth(format!("Missing environment variable: {}", e))
    }
}

impl From<wreq::header::InvalidHeaderValue> for FetchError {
    fn from(e: wreq::header::InvalidHeaderValue) -> Self {
        FetchError::Network(format!("Invalid header value: {}", e))
    }
}

impl From<wreq::Error> for FetchError {
    fn from(e: wreq::Error) -> Self {
        FetchError::Network(format!("Request failed: {}", e))
    }
}

impl From<String> for FetchError {
    fn from(s: String) -> Self {
        FetchError::Network(s)
    }
}

/// Error indicator for visual feedback on icons
#[derive(Debug, Clone, Copy)]
pub enum ErrorIndicator {
    None,
    Offline,     // Gray border - network/transient errors
    AuthError,   // Yellow border - authentication failures
    RateLimited, // Orange border - rate limiting
}

impl ErrorIndicator {
    pub fn from_error(error: Option<&FetchError>) -> Self {
        match error {
            None => ErrorIndicator::None,
            Some(FetchError::Network(_)) => ErrorIndicator::Offline,
            Some(FetchError::Auth(_)) => ErrorIndicator::AuthError,
            Some(FetchError::RateLimited { .. }) => ErrorIndicator::RateLimited,
            Some(FetchError::Parse(_)) => ErrorIndicator::AuthError,
        }
    }

    pub fn border_color(&self) -> Option<[u8; 3]> {
        match self {
            ErrorIndicator::None => None,
            ErrorIndicator::Offline => Some([128, 128, 128]),
            ErrorIndicator::AuthError => Some([255, 193, 7]),
            ErrorIndicator::RateLimited => Some([255, 152, 0]),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ApiErrorResponse {
    #[serde(rename = "type")]
    pub response_type: String,
    pub error: ApiError,
    pub request_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ApiError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
    pub details: ErrorDetails,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ErrorDetails {
    pub error_visibility: String,
    pub error_code: String,
}
