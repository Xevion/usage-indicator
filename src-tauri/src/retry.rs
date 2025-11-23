use crate::error::FetchError;
use tokio::time::Duration;

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    min_delay_secs: u64,
    max_delay_secs: u64,
    multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            min_delay_secs: 5,   // 5 seconds
            max_delay_secs: 300, // 5 minutes
            multiplier: 2.0,     // Double each time
        }
    }
}

impl RetryConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(val) = std::env::var("RETRY_MIN_DELAY_SECS")
            && let Ok(parsed) = val.parse()
        {
            config.min_delay_secs = parsed;
        }
        if let Ok(val) = std::env::var("RETRY_MAX_DELAY_SECS")
            && let Ok(parsed) = val.parse()
        {
            config.max_delay_secs = parsed;
        }
        if let Ok(val) = std::env::var("RETRY_MULTIPLIER")
            && let Ok(parsed) = val.parse()
        {
            config.multiplier = parsed;
        }

        config
    }
}

/// Tracks retry state with exponential backoff
#[derive(Debug)]
pub struct RetryState {
    current_delay: Duration,
    consecutive_failures: u32,
    config: RetryConfig,
}

impl RetryState {
    pub fn new(config: RetryConfig) -> Self {
        Self {
            current_delay: Duration::from_secs(config.min_delay_secs),
            consecutive_failures: 0,
            config,
        }
    }

    /// Record a successful fetch - resets backoff
    pub fn record_success(&mut self) {
        self.current_delay = Duration::from_secs(self.config.min_delay_secs);
        self.consecutive_failures = 0;
    }

    /// Record a failure and calculate next delay with exponential backoff
    pub fn record_failure(&mut self, error: &FetchError) -> Duration {
        self.consecutive_failures += 1;

        let delay = if error.is_transient() {
            // Exponential backoff for transient errors
            let next_delay_secs =
                (self.current_delay.as_secs() as f64 * self.config.multiplier) as u64;
            let clamped = next_delay_secs
                .max(self.config.min_delay_secs)
                .min(self.config.max_delay_secs);

            self.current_delay = Duration::from_secs(clamped);
            self.current_delay
        } else {
            // Permanent errors: use minimum interval (don't spam, but stay responsive)
            Duration::from_secs(self.config.min_delay_secs)
        };

        // Extra backoff for rate limiting
        if matches!(error, FetchError::RateLimited { .. }) {
            // Use max delay for rate limits to avoid further limiting
            Duration::from_secs(self.config.max_delay_secs)
        } else {
            delay
        }
    }

    pub fn current_delay(&self) -> Duration {
        self.current_delay
    }
}
