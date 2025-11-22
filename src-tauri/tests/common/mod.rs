// Common test utilities and fixtures

use mockito::{Mock, Server, ServerGuard};
use serde_json::json;

/// Mock Claude API server for testing
pub struct MockClaudeApi {
    pub server: ServerGuard,
    pub org_id: String,
}

impl MockClaudeApi {
    /// Create a new mock API server (async)
    pub async fn new() -> Self {
        let server = Server::new_async().await;
        Self {
            server,
            org_id: "test-org-123".to_string(),
        }
    }

    /// Get the base URL for the mock server
    pub fn url(&self) -> String {
        self.server.url()
    }

    /// Create a mock for successful usage response
    pub fn mock_success_response(&mut self, six_hour_pct: f64, weekly_pct: f64) -> Mock {
        let body = json!({
            "five_hour": {
                "utilization": six_hour_pct,
                "resets_at": "2025-11-22T18:00:00Z"
            },
            "seven_day": {
                "utilization": weekly_pct,
                "resets_at": "2025-11-29T00:00:00Z"
            },
            "seven_day_oauth_apps": null,
            "seven_day_opus": {
                "utilization": 0.0,
                "resets_at": "2025-11-29T00:00:00Z"
            },
            "iguana_necktie": null
        });

        self.server
            .mock(
                "GET",
                format!("/api/organizations/{}/usage", self.org_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body.to_string())
            .create()
    }

    /// Create a mock for 401 authentication error
    pub fn mock_auth_error(&mut self) -> Mock {
        let body = json!({
            "type": "error",
            "error": {
                "type": "authentication_error",
                "message": "Invalid session key",
                "details": {
                    "error_visibility": "user",
                    "error_code": "invalid_session"
                }
            },
            "request_id": "req_123456"
        });

        self.server
            .mock(
                "GET",
                format!("/api/organizations/{}/usage", self.org_id).as_str(),
            )
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(body.to_string())
            .create()
    }

    /// Create a mock for 429 rate limit error
    pub fn mock_rate_limit_error(&mut self) -> Mock {
        self.server
            .mock(
                "GET",
                format!("/api/organizations/{}/usage", self.org_id).as_str(),
            )
            .with_status(429)
            .with_header("content-type", "application/json")
            .with_body(json!({"error": "Too many requests"}).to_string())
            .create()
    }

    /// Create a mock for network/server error (5xx)
    pub fn mock_server_error(&mut self) -> Mock {
        let body = json!({
            "type": "error",
            "error": {
                "type": "server_error",
                "message": "Internal server error",
                "details": {
                    "error_visibility": "user",
                    "error_code": "internal_error"
                }
            },
            "request_id": "req_123456"
        });

        self.server
            .mock(
                "GET",
                format!("/api/organizations/{}/usage", self.org_id).as_str(),
            )
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(body.to_string())
            .create()
    }

    /// Create a mock for invalid JSON response (parse error)
    pub fn mock_invalid_json(&mut self) -> Mock {
        self.server
            .mock(
                "GET",
                format!("/api/organizations/{}/usage", self.org_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("not valid json {{{")
            .create()
    }
}
