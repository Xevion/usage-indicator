use crate::error::ErrorIndicator;
use crate::icon::{STALENESS_THRESHOLD_SECS, generate_unknown_icon, generate_usage_icon};
use crate::poller::AdaptivePoller;
use crate::retry::RetryState;
use crate::state::AppState;
use std::time::SystemTime;
use tauri::AppHandle;

pub fn update_tray_icon(
    app: &AppHandle,
    state: &AppState,
    poller: &AdaptivePoller,
    retry_state: &RetryState,
) -> Result<(), Box<dyn std::error::Error>> {
    let tray = app.tray_by_id("main").ok_or("Tray not found")?;

    // Determine error indicator from current error
    let error_indicator = ErrorIndicator::from_error(state.current_error.as_ref());

    // Generate icon based on state
    let icon_bytes = if let Some(success) = &state.last_success {
        generate_usage_icon(success.metrics.weekly_pct(), error_indicator)
    } else {
        generate_unknown_icon()
    };

    let icon = tauri::image::Image::new_owned(icon_bytes, 32, 32);
    tray.set_icon(Some(icon))?;

    // Build comprehensive tooltip
    let tooltip = if let Some(success) = &state.last_success {
        let elapsed = SystemTime::now()
            .duration_since(success.timestamp)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let format_reset_time = |resets_at: &Option<String>| -> String {
            resets_at
                .as_ref()
                .map(|s| {
                    s.split('T')
                        .next()
                        .map(|date| date.to_string())
                        .unwrap_or_else(|| s.clone())
                })
                .unwrap_or_else(|| "Unknown".to_string())
        };

        let mut tooltip = format!(
            "Claude Usage Indicator\n\
            \n\
            Weekly: {}% (resets {})\n\
            5-hour: {}% (resets {})\n\
            \n\
            State: {:?}\n\
            Next poll: {}s\n\
            Last update: {}s ago",
            success.metrics.weekly_pct(),
            format_reset_time(&success.usage_data.seven_day.resets_at),
            success.metrics.five_hour_pct(),
            format_reset_time(&success.usage_data.five_hour.resets_at),
            poller.current_state(),
            poller.current_interval().as_secs(),
            elapsed
        );

        // Add error information if present
        if let Some(error) = &state.current_error {
            let is_stale = state.is_stale(STALENESS_THRESHOLD_SECS);
            tooltip.push_str(&format!(
                "\n\n⚠ {}: {}\n\
                Retry in: {}s{}",
                error.category(),
                error,
                retry_state.current_delay().as_secs(),
                if is_stale { " (data is stale)" } else { "" }
            ));
        }

        tooltip
    } else {
        // No data available yet
        let mut tooltip = String::from("Claude Usage Indicator\n\nStatus: No data available yet");

        if let Some(error) = &state.current_error {
            tooltip.push_str(&format!(
                "\n\n⚠ {}: {}\n\
                Retry in: {}s",
                error.category(),
                error,
                retry_state.current_delay().as_secs()
            ));
        } else {
            tooltip.push_str(&format!(
                "\nNext poll: {}s",
                poller.current_interval().as_secs()
            ));
        }

        tooltip
    };

    tray.set_tooltip(Some(tooltip))?;

    Ok(())
}
