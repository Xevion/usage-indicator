use crate::api::fetch_usage_data;
use crate::poller::{AdaptivePoller, PollerConfig, UsageMetrics};
use crate::retry::{RetryConfig, RetryState};
use crate::state::AppState;
use crate::tray::update_tray_icon;
use std::time::Instant;
use tauri::AppHandle;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

#[cfg(not(windows))]
use crate::events::platform::start_power_listener;
#[cfg(windows)]
use crate::events::windows::start_power_listener;

use crate::events::PollAction;

pub async fn start_polling(app: AppHandle, cancel_token: CancellationToken) {
    // Initialize adaptive poller with config from environment
    let poller_config = PollerConfig::from_env();
    info!(
        config = ?poller_config,
        "Adaptive poller initialized"
    );

    let retry_config = RetryConfig::from_env();
    info!(
        config = ?retry_config,
        "Retry config initialized"
    );

    let mut poller = AdaptivePoller::new(poller_config);
    let mut retry_state = RetryState::new(retry_config);
    let mut app_state = AppState::new();

    // Start system event listener (Windows power management)
    let mut event_rx = start_power_listener();
    let mut paused = false;

    loop {
        // Check for cancellation signal and system events
        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("Shutdown signal received, stopping polling gracefully");
                break;
            }
            Some(event) = event_rx.recv() => {
                let action = event.recommended_action();
                info!(?event, ?action, "System event received");

                match action {
                    PollAction::Pause => {
                        info!("Pausing polling due to system event");
                        paused = true;
                    }
                    PollAction::FetchImmediately => {
                        if paused {
                            info!("Resuming polling due to system event");
                            paused = false;
                        }
                        // Trigger immediate fetch by continuing to next iteration
                        continue;
                    }
                    PollAction::Continue => {
                        // No action needed
                    }
                }
            }
            _ = async {
                // Skip polling if paused
                if paused {
                    sleep(tokio::time::Duration::from_secs(60)).await;
                    return;
                }

                let now = Instant::now();

                info!("Fetching usage data...");

                match fetch_usage_data().await {
                    Ok(data) => {
                        // Convert API response to metrics (rounding to 1% resolution)
                        let metrics = UsageMetrics::new(
                            data.five_hour.utilization.round() as u8,
                            data.seven_day.utilization.round() as u8,
                        );

                        info!(
                            five_hour_pct = metrics.five_hour_pct(),
                            weekly_pct = metrics.weekly_pct(),
                            "Usage data fetched"
                        );

                        // Update state with fresh data
                        app_state.update_success(metrics, data);
                        retry_state.record_success();

                        // Calculate next interval using adaptive algorithm
                        let next_interval = poller.next_interval(metrics, now);

                        info!(
                            state = ?poller.current_state(),
                            next_interval_secs = next_interval.as_secs(),
                            next_interval_mins = next_interval.as_secs() / 60,
                            "Adaptive polling cycle complete"
                        );

                        // Update tray icon with current state
                        if let Err(e) = update_tray_icon(&app, &app_state, &poller, &retry_state) {
                            error!("Failed to update tray icon: {}", e);
                        }

                        // Sleep for adaptive duration
                        sleep(next_interval).await;
                    }
                    Err(e) => {
                        error!("Failed to fetch usage data: {}", e);

                        // Calculate retry delay with exponential backoff
                        let retry_delay = retry_state.record_failure(&e);

                        // Update state with error (keeps last-known-good data)
                        app_state.update_error(e.clone());

                        // Update tray icon to show error state
                        if let Err(icon_err) = update_tray_icon(&app, &app_state, &poller, &retry_state) {
                            error!("Failed to update tray icon: {}", icon_err);
                        }

                        info!(
                            error_category = e.category(),
                            is_transient = e.is_transient(),
                            retry_delay_secs = retry_delay.as_secs(),
                            "Retrying after error"
                        );

                        // Sleep for calculated retry delay
                        sleep(retry_delay).await;
                    }
                }
            } => {}
        }
    }
}
