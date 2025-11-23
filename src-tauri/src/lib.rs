mod api;
mod app;
mod error;
mod events;
mod icon;
mod poller;
mod polling;
mod retry;
mod state;
mod tray;

// Public re-exports
pub use app::run;
pub use error::{ErrorIndicator, FetchError};
pub use events::{PollAction, SystemEvent};
pub use poller::{AdaptivePoller, PollerConfig, TemperatureState, UsageMetrics};
pub use state::{UsageData, UsagePeriod};

// Re-export for testing
#[doc(hidden)]
pub use api::fetch_usage_data_with_base_url;
