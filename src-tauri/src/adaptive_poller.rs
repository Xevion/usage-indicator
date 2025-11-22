use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// Usage metrics with 1% resolution
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UsageMetrics {
    six_hour_pct: u8,
    weekly_pct: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageMetricsError {
    SixHourOutOfRange(u8),
    WeeklyOutOfRange(u8),
}

impl std::fmt::Display for UsageMetricsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SixHourOutOfRange(val) => write!(f, "six_hour_pct {} is out of range 0-100", val),
            Self::WeeklyOutOfRange(val) => write!(f, "weekly_pct {} is out of range 0-100", val),
        }
    }
}

impl std::error::Error for UsageMetricsError {}

impl UsageMetrics {
    /// Try to create new UsageMetrics, returning an error if values are out of range (0-100)
    pub fn try_new(six_hour_pct: u8, weekly_pct: u8) -> Result<Self, UsageMetricsError> {
        if six_hour_pct > 100 {
            return Err(UsageMetricsError::SixHourOutOfRange(six_hour_pct));
        }
        if weekly_pct > 100 {
            return Err(UsageMetricsError::WeeklyOutOfRange(weekly_pct));
        }

        Ok(Self {
            six_hour_pct,
            weekly_pct,
        })
    }

    /// Create new UsageMetrics, panicking if values are out of range (0-100)
    pub fn new(six_hour_pct: u8, weekly_pct: u8) -> Self {
        Self::try_new(six_hour_pct, weekly_pct).expect("UsageMetrics values must be in range 0-100")
    }

    pub fn six_hour_pct(&self) -> u8 {
        self.six_hour_pct
    }

    pub fn weekly_pct(&self) -> u8 {
        self.weekly_pct
    }
}

/// Temperature-based activity states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemperatureState {
    /// No changes detected for extended period
    Cold,
    /// Recently had activity, now cooling down
    Cool,
    /// 6h metric showing sustained increases
    Warm,
    /// Weekly metric actively changing
    Hot,
    /// Both metrics actively changing
    Blazing,
}

/// Configuration for adaptive polling behavior
#[derive(Debug, Clone)]
pub struct PollerConfig {
    // Interval bounds
    pub min_interval_secs: u64,
    pub max_interval_secs: u64,
    pub additive_increase_secs: u64,

    // Time windows for detection
    pub recency_window_secs: u64,
    pub context_window_secs: u64,
    pub idle_to_cold_secs: u64,

    // Change thresholds (percentage points)
    pub six_hour_sustained_threshold: u8,
    pub weekly_sustained_threshold: u8,
    pub six_hour_recent_threshold: u8,

    // AIMD multipliers
    pub warm_multiplier: f64,
    pub hot_multiplier: f64,
    pub blazing_multiplier: f64,
}

impl Default for PollerConfig {
    fn default() -> Self {
        Self {
            min_interval_secs: 180,     // 3 minutes
            max_interval_secs: 5400,    // 90 minutes
            additive_increase_secs: 90, // 1.5 minutes

            recency_window_secs: 600,  // 10 minutes
            context_window_secs: 3600, // 1 hour
            idle_to_cold_secs: 1800,   // 30 minutes

            six_hour_sustained_threshold: 4,
            weekly_sustained_threshold: 2,
            six_hour_recent_threshold: 2,

            warm_multiplier: 0.7,
            hot_multiplier: 0.4,
            blazing_multiplier: 0.25,
        }
    }
}

macro_rules! env_or_default {
    ($config:expr, $field:ident, $env_var:expr) => {
        if let Ok(val) = std::env::var($env_var) {
            if let Ok(parsed) = val.parse() {
                $config.$field = parsed;
            }
        }
    };
}

impl PollerConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        env_or_default!(config, min_interval_secs, "POLL_MIN_INTERVAL_SECS");
        env_or_default!(config, max_interval_secs, "POLL_MAX_INTERVAL_SECS");
        env_or_default!(config, recency_window_secs, "POLL_RECENCY_WINDOW_SECS");
        env_or_default!(config, context_window_secs, "POLL_CONTEXT_WINDOW_SECS");
        env_or_default!(config, idle_to_cold_secs, "POLL_IDLE_TO_COLD_SECS");

        config
    }
}

/// Time-windowed tracker for usage metrics history
struct TimeWindowedTracker {
    /// Time-ordered history of usage samples
    history: BTreeMap<Instant, UsageMetrics>,
    max_history_duration: Duration,
}

impl TimeWindowedTracker {
    fn new(max_duration: Duration) -> Self {
        Self {
            history: BTreeMap::new(),
            max_history_duration: max_duration,
        }
    }

    fn record_sample(&mut self, metrics: UsageMetrics, now: Instant) {
        self.history.insert(now, metrics);

        let cutoff = now.checked_sub(self.max_history_duration).unwrap_or(now);
        self.history = self.history.split_off(&cutoff);
    }

    fn calculate_momentum<F>(
        &self,
        window: Duration,
        now: Instant,
        extractor: F,
        max_change: Option<u8>,
    ) -> u8
    where
        F: Fn(&UsageMetrics) -> u8,
    {
        let cutoff = now.checked_sub(window).unwrap_or(now);
        let samples: Vec<u8> = self
            .history
            .range(cutoff..)
            .map(|(_, metrics)| extractor(metrics))
            .collect();

        if samples.len() < 2 {
            return 0;
        }

        samples.windows(2).fold(0u8, |total, w| {
            if w[1] > w[0] {
                let change = w[1] - w[0];
                if max_change.is_none_or(|max| change <= max) {
                    total.saturating_add(change)
                } else {
                    total
                }
            } else {
                total
            }
        })
    }

    fn calculate_six_hour_momentum(&self, window: Duration, now: Instant) -> u8 {
        self.calculate_momentum(window, now, |m| m.six_hour_pct(), Some(10))
    }

    fn calculate_weekly_momentum(&self, window: Duration, now: Instant) -> u8 {
        self.calculate_momentum(window, now, |m| m.weekly_pct(), None)
    }

    fn time_since_any_change(&self, now: Instant) -> Duration {
        if self.history.len() < 2 {
            return Duration::MAX;
        }

        let samples: Vec<_> = self.history.iter().rev().collect();
        let (latest_time, latest_metrics) = samples[0];

        // Find most recent change in either metric
        for (timestamp, metrics) in samples.iter().skip(1) {
            if metrics.six_hour_pct() != latest_metrics.six_hour_pct()
                || metrics.weekly_pct() != latest_metrics.weekly_pct()
            {
                return now.duration_since(**timestamp);
            }
        }

        // No change found in entire history
        let oldest_time = samples.last().map(|(t, _)| **t).unwrap_or(*latest_time);
        now.duration_since(oldest_time)
    }

    /// Two-tier state detection: recency gate + context analysis
    fn detect_state(&self, now: Instant, config: &PollerConfig) -> TemperatureState {
        let recency_window = Duration::from_secs(config.recency_window_secs);
        let context_window = Duration::from_secs(config.context_window_secs);

        // TIER 1: Recency gate (last 10 minutes)
        let recent_6h = self.calculate_six_hour_momentum(recency_window, now);
        let recent_weekly = self.calculate_weekly_momentum(recency_window, now);

        if recent_6h == 0 && recent_weekly == 0 {
            let idle_time = self.time_since_any_change(now);

            return if idle_time > Duration::from_secs(config.idle_to_cold_secs) {
                TemperatureState::Cold
            } else {
                TemperatureState::Cool
            };
        }

        // TIER 2: Recent activity detected - classify severity using context
        let context_6h = self.calculate_six_hour_momentum(context_window, now);
        let context_weekly = self.calculate_weekly_momentum(context_window, now);

        let weekly_active_now = recent_weekly > 0;
        let six_hour_active_now = recent_6h >= config.six_hour_recent_threshold;
        let weekly_sustained = context_weekly >= config.weekly_sustained_threshold;
        let six_hour_sustained = context_6h >= config.six_hour_sustained_threshold;

        if weekly_active_now && six_hour_active_now {
            TemperatureState::Blazing
        } else if weekly_active_now || weekly_sustained {
            TemperatureState::Hot
        } else if six_hour_active_now && six_hour_sustained {
            TemperatureState::Warm
        } else {
            TemperatureState::Cool
        }
    }
}

/// Adaptive polling engine with AIMD interval adjustment
pub struct AdaptivePoller {
    current_interval: Duration,
    current_state: TemperatureState,
    tracker: TimeWindowedTracker,
    config: PollerConfig,

    state_entered_at: Instant,
}

impl AdaptivePoller {
    pub fn new(config: PollerConfig) -> Self {
        let max_history =
            Duration::from_secs(config.context_window_secs.max(config.idle_to_cold_secs));

        Self {
            current_interval: Duration::from_secs(config.min_interval_secs),
            current_state: TemperatureState::Cold,
            tracker: TimeWindowedTracker::new(max_history),
            config,
            state_entered_at: Instant::now(),
        }
    }

    fn calculate_interval_for_state(&self, state: TemperatureState) -> Duration {
        match state {
            TemperatureState::Cold => {
                self.current_interval + Duration::from_secs(self.config.additive_increase_secs)
            }
            TemperatureState::Cool => {
                self.current_interval + Duration::from_secs(self.config.additive_increase_secs / 2)
            }
            TemperatureState::Warm => Duration::from_secs(
                (self.current_interval.as_secs() as f64 * self.config.warm_multiplier) as u64,
            ),
            TemperatureState::Hot => Duration::from_secs(
                (self.current_interval.as_secs() as f64 * self.config.hot_multiplier) as u64,
            ),
            TemperatureState::Blazing => Duration::from_secs(
                (self.current_interval.as_secs() as f64 * self.config.blazing_multiplier) as u64,
            ),
        }
    }

    fn get_smoothing_factor(state: TemperatureState) -> f64 {
        match state {
            TemperatureState::Blazing => 0.7,
            TemperatureState::Hot => 0.5,
            _ => 0.3,
        }
    }

    fn apply_smoothing(current: Duration, target: Duration, factor: f64) -> Duration {
        Duration::from_secs(
            (current.as_secs() as f64 * (1.0 - factor) + target.as_secs() as f64 * factor) as u64,
        )
    }

    pub fn next_interval(&mut self, metrics: UsageMetrics, now: Instant) -> Duration {
        self.tracker.record_sample(metrics, now);
        let new_state = self.tracker.detect_state(now, &self.config);

        if new_state != self.current_state {
            let time_in_state = now.saturating_duration_since(self.state_entered_at);
            info!(
                old_state = ?self.current_state,
                new_state = ?new_state,
                time_in_state_secs = time_in_state.as_secs(),
                "State transition"
            );
            self.current_state = new_state;
            self.state_entered_at = now;
        }

        let new_interval = self.calculate_interval_for_state(self.current_state).clamp(
            Duration::from_secs(self.config.min_interval_secs),
            Duration::from_secs(self.config.max_interval_secs),
        );
        self.current_interval = Self::apply_smoothing(
            self.current_interval,
            new_interval,
            Self::get_smoothing_factor(self.current_state),
        );

        debug!(
            state = ?self.current_state,
            interval_secs = self.current_interval.as_secs(),
            six_hour_pct = metrics.six_hour_pct(),
            weekly_pct = metrics.weekly_pct(),
            "Calculated next interval"
        );

        self.current_interval
    }

    /// Get current temperature state
    pub fn current_state(&self) -> TemperatureState {
        self.current_state
    }

    /// Get current interval
    pub fn current_interval(&self) -> Duration {
        self.current_interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::{assert, let_assert};
    use rstest::rstest;

    #[test]
    fn test_initial_state_is_cold() {
        let config = PollerConfig::default();
        let poller = AdaptivePoller::new(config);

        assert!(poller.current_state() == TemperatureState::Cold);
    }

    #[test]
    fn test_no_change_stays_cold() {
        let config = PollerConfig::default();
        let mut poller = AdaptivePoller::new(config);
        let now = Instant::now();

        let metrics = UsageMetrics::new(15, 5);
        poller.next_interval(metrics, now);

        assert!(poller.current_state() == TemperatureState::Cold);
    }

    #[rstest]
    #[case(TemperatureState::Cold, 0.3)]
    #[case(TemperatureState::Cool, 0.3)]
    #[case(TemperatureState::Warm, 0.3)]
    #[case(TemperatureState::Hot, 0.5)]
    #[case(TemperatureState::Blazing, 0.7)]
    fn test_smoothing_factors(#[case] state: TemperatureState, #[case] expected: f64) {
        let factor = AdaptivePoller::get_smoothing_factor(state);
        assert!(factor == expected);
    }

    #[rstest]
    #[case(
        Duration::from_secs(100),
        Duration::from_secs(200),
        0.5,
        Duration::from_secs(150)
    )]
    #[case(
        Duration::from_secs(100),
        Duration::from_secs(300),
        0.3,
        Duration::from_secs(160)
    )]
    #[case(
        Duration::from_secs(500),
        Duration::from_secs(100),
        0.7,
        Duration::from_secs(220)
    )]
    fn test_smoothing_application(
        #[case] current: Duration,
        #[case] target: Duration,
        #[case] factor: f64,
        #[case] expected: Duration,
    ) {
        let result = AdaptivePoller::apply_smoothing(current, target, factor);
        assert!(result == expected);
    }

    #[test]
    fn test_momentum_with_increasing_six_hour() {
        let mut tracker = TimeWindowedTracker::new(Duration::from_secs(3600));
        let now = Instant::now();

        tracker.record_sample(UsageMetrics::new(10, 5), now);
        tracker.record_sample(UsageMetrics::new(11, 5), now + Duration::from_secs(60));
        tracker.record_sample(UsageMetrics::new(13, 6), now + Duration::from_secs(120));

        let momentum = tracker
            .calculate_six_hour_momentum(Duration::from_secs(180), now + Duration::from_secs(120));
        assert!(momentum == 3); // 10 → 11 → 13 = +3 total
    }

    #[test]
    fn test_momentum_with_increasing_weekly() {
        let mut tracker = TimeWindowedTracker::new(Duration::from_secs(3600));
        let now = Instant::now();

        tracker.record_sample(UsageMetrics::new(10, 5), now);
        tracker.record_sample(UsageMetrics::new(11, 5), now + Duration::from_secs(60));
        tracker.record_sample(UsageMetrics::new(13, 6), now + Duration::from_secs(120));

        let weekly_momentum = tracker
            .calculate_weekly_momentum(Duration::from_secs(180), now + Duration::from_secs(120));
        assert!(weekly_momentum == 1); // 5 → 5 → 6 = +1 total
    }

    #[test]
    fn test_momentum_with_no_change() {
        let mut tracker = TimeWindowedTracker::new(Duration::from_secs(3600));
        let now = Instant::now();

        tracker.record_sample(UsageMetrics::new(10, 5), now);
        tracker.record_sample(UsageMetrics::new(10, 5), now + Duration::from_secs(60));

        let momentum = tracker
            .calculate_six_hour_momentum(Duration::from_secs(120), now + Duration::from_secs(60));
        assert!(momentum == 0);
    }

    #[rstest]
    #[case(50, 75)]
    #[case(0, 0)]
    #[case(100, 100)]
    #[case(25, 50)]
    fn test_valid_usage_metrics(#[case] six_hour: u8, #[case] weekly: u8) {
        let_assert!(Ok(metrics) = UsageMetrics::try_new(six_hour, weekly));
        assert!(metrics.six_hour_pct() == six_hour);
        assert!(metrics.weekly_pct() == weekly);
    }

    #[rstest]
    #[case(101, 50, UsageMetricsError::SixHourOutOfRange(101))]
    #[case(50, 101, UsageMetricsError::WeeklyOutOfRange(101))]
    #[case(255, 50, UsageMetricsError::SixHourOutOfRange(255))]
    #[case(50, 200, UsageMetricsError::WeeklyOutOfRange(200))]
    fn test_invalid_usage_metrics(
        #[case] six_hour: u8,
        #[case] weekly: u8,
        #[case] expected_error: UsageMetricsError,
    ) {
        let_assert!(Err(error) = UsageMetrics::try_new(six_hour, weekly));
        assert!(error == expected_error);
    }

    #[test]
    fn test_usage_metrics_new_succeeds() {
        let metrics = UsageMetrics::new(25, 50);
        assert!(metrics.six_hour_pct() == 25);
        assert!(metrics.weekly_pct() == 50);
    }

    #[test]
    #[should_panic(expected = "UsageMetrics values must be in range 0-100")]
    fn test_usage_metrics_new_panics_on_six_hour_overflow() {
        UsageMetrics::new(101, 50);
    }

    #[test]
    #[should_panic(expected = "UsageMetrics values must be in range 0-100")]
    fn test_usage_metrics_new_panics_on_weekly_overflow() {
        UsageMetrics::new(50, 101);
    }

    #[test]
    fn test_interval_clamping_to_min() {
        let config = PollerConfig {
            min_interval_secs: 180,
            max_interval_secs: 5400,
            ..Default::default()
        };
        let mut poller = AdaptivePoller::new(config.clone());

        poller.current_interval = Duration::from_secs(10); // Very low
        let interval = poller.calculate_interval_for_state(TemperatureState::Blazing);

        let clamped = interval.clamp(
            Duration::from_secs(config.min_interval_secs),
            Duration::from_secs(config.max_interval_secs),
        );
        assert!(clamped >= Duration::from_secs(config.min_interval_secs));
    }

    #[test]
    fn test_interval_clamping_to_max() {
        let config = PollerConfig {
            min_interval_secs: 180,
            max_interval_secs: 5400,
            ..Default::default()
        };
        let mut poller = AdaptivePoller::new(config.clone());

        poller.current_interval = Duration::from_secs(10000); // Very high
        let interval = poller.calculate_interval_for_state(TemperatureState::Cold);

        let clamped = interval.clamp(
            Duration::from_secs(config.min_interval_secs),
            Duration::from_secs(config.max_interval_secs),
        );
        assert!(clamped <= Duration::from_secs(config.max_interval_secs));
    }
}
