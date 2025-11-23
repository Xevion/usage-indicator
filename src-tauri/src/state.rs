use crate::error::FetchError;
use crate::poller::UsageMetrics;
use serde::{Deserialize, Serialize};

/// Represents the application's data state with error tracking and last-known-good support
#[derive(Debug, Clone, Default)]
pub struct AppState {
    /// Last successfully fetched data (None if never succeeded)
    pub last_success: Option<SuccessfulFetch>,
    /// Current error state (None if no active error)
    pub current_error: Option<FetchError>,
}

#[derive(Debug, Clone)]
pub struct SuccessfulFetch {
    pub metrics: UsageMetrics,
    pub usage_data: UsageData,
    pub timestamp: std::time::SystemTime,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_success(&mut self, metrics: UsageMetrics, usage_data: UsageData) {
        self.last_success = Some(SuccessfulFetch {
            metrics,
            usage_data,
            timestamp: std::time::SystemTime::now(),
        });
        self.current_error = None;
    }

    pub fn update_error(&mut self, error: FetchError) {
        self.current_error = Some(error);
    }

    pub fn is_stale(&self, threshold_secs: u64) -> bool {
        if let Some(success) = &self.last_success
            && let Ok(elapsed) = std::time::SystemTime::now().duration_since(success.timestamp)
        {
            return elapsed.as_secs() > threshold_secs;
        }
        false
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct UsageData {
    pub five_hour: UsagePeriod,
    pub seven_day: UsagePeriod,
    pub seven_day_oauth_apps: Option<UsagePeriod>,
    pub seven_day_opus: UsagePeriod,
    pub iguana_necktie: Option<UsagePeriod>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct UsagePeriod {
    pub utilization: f64,
    pub resets_at: Option<String>,
}
