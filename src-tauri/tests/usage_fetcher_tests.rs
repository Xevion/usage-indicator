mod common;

use assert2::{assert, let_assert};
use common::MockClaudeApi;
use rstest::rstest;
use usage_indicator_lib::{FetchError, fetch_usage_data_with_base_url};

#[rstest]
#[case(15.0, 45.0)]
#[case(0.0, 0.0)]
#[case(100.0, 100.0)]
#[case(50.5, 75.3)]
#[tokio::test]
async fn test_successful_fetch(#[case] six_hour_pct: f64, #[case] weekly_pct: f64) {
    let mut mock_api = MockClaudeApi::new().await;
    let _mock = mock_api.mock_success_response(six_hour_pct, weekly_pct);

    let result =
        fetch_usage_data_with_base_url(&mock_api.url(), &mock_api.org_id, "test-session-key").await;

    let_assert!(Ok(data) = result);
    assert!(data.five_hour.utilization == six_hour_pct);
    assert!(data.seven_day.utilization == weekly_pct);
    assert!(data.five_hour.resets_at.is_some());
    assert!(data.seven_day.resets_at.is_some());
}

#[tokio::test]
async fn test_auth_error_returns_auth_fetch_error() {
    let mut mock_api = MockClaudeApi::new().await;
    let _mock = mock_api.mock_auth_error();

    let result =
        fetch_usage_data_with_base_url(&mock_api.url(), &mock_api.org_id, "invalid-session-key")
            .await;

    let_assert!(Err(error) = result);
    assert!(matches!(error, FetchError::Auth(_)));
    assert!(!error.is_transient());
    assert!(error.category() == "Authentication Error");
}

#[tokio::test]
async fn test_rate_limit_error_returns_rate_limited_fetch_error() {
    let mut mock_api = MockClaudeApi::new().await;
    let _mock = mock_api.mock_rate_limit_error();

    let result =
        fetch_usage_data_with_base_url(&mock_api.url(), &mock_api.org_id, "test-session-key").await;

    let_assert!(Err(error) = result);
    assert!(matches!(
        error,
        FetchError::RateLimited {
            message: _,
            retry_after: None
        }
    ));
    assert!(error.is_transient());
    assert!(error.category() == "Rate Limited");
}

#[tokio::test]
async fn test_server_error_returns_network_fetch_error() {
    let mut mock_api = MockClaudeApi::new().await;
    let _mock = mock_api.mock_server_error();

    let result =
        fetch_usage_data_with_base_url(&mock_api.url(), &mock_api.org_id, "test-session-key").await;

    let_assert!(Err(error) = result);
    assert!(matches!(error, FetchError::Network(_)));
    assert!(error.is_transient());
    assert!(error.category() == "Offline");
}

#[tokio::test]
async fn test_invalid_json_returns_parse_error() {
    let mut mock_api = MockClaudeApi::new().await;
    let _mock = mock_api.mock_invalid_json();

    let result =
        fetch_usage_data_with_base_url(&mock_api.url(), &mock_api.org_id, "test-session-key").await;

    let_assert!(Err(error) = result);
    assert!(matches!(error, FetchError::Parse(_)));
    assert!(!error.is_transient());
    assert!(error.category() == "Parse Error");
}

#[rstest]
#[case(FetchError::Network("Connection failed".to_string()), true, "Offline")]
#[case(FetchError::Auth("Invalid credentials".to_string()), false, "Authentication Error")]
#[case(
    FetchError::RateLimited {
        message: "Too many requests".to_string(),
        retry_after: Some(60)
    },
    true,
    "Rate Limited"
)]
#[case(FetchError::Parse("Invalid JSON".to_string()), false, "Parse Error")]
#[test]
fn test_fetch_error_properties(
    #[case] error: FetchError,
    #[case] expected_transient: bool,
    #[case] expected_category: &str,
) {
    assert!(error.is_transient() == expected_transient);
    assert!(error.category() == expected_category);
}

#[tokio::test]
async fn test_usage_data_deserialization() {
    let mut mock_api = MockClaudeApi::new().await;
    let _mock = mock_api.mock_success_response(25.0, 75.0);

    let result =
        fetch_usage_data_with_base_url(&mock_api.url(), &mock_api.org_id, "test-session-key").await;

    let_assert!(Ok(data) = result);

    // Verify all fields are properly deserialized
    assert!(data.five_hour.utilization == 25.0);
    assert!(data.seven_day.utilization == 75.0);
    assert!(data.seven_day_oauth_apps.is_none());
    assert!(data.seven_day_opus.utilization == 0.0);
    assert!(data.iguana_necktie.is_none());
}

#[tokio::test]
async fn test_fetch_with_empty_org_id() {
    let mock_api = MockClaudeApi::new().await;

    let result = fetch_usage_data_with_base_url(&mock_api.url(), "", "test-session-key").await;

    // Should succeed with empty org_id (API will handle validation)
    // The mock won't match, so it should return a network error
    let_assert!(Err(error) = result);
    assert!(matches!(error, FetchError::Network(_)));
}
