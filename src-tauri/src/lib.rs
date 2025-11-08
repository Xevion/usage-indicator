mod adaptive_poller;

use adaptive_poller::{AdaptivePoller, PollerConfig, UsageMetrics};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use wreq::header::{HeaderMap, HeaderValue, COOKIE, USER_AGENT};
use wreq::ClientBuilder;

#[derive(Debug)]
enum FetchError {
    Network(String),
    Parse(String),
    Auth(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Network(msg) => write!(f, "Network error: {}", msg),
            FetchError::Parse(msg) => write!(f, "Parse error: {}", msg),
            FetchError::Auth(msg) => write!(f, "Auth error: {}", msg),
        }
    }
}

impl std::error::Error for FetchError {}

#[derive(Debug, Clone)]
enum DataState {
    Fresh {
        metrics: UsageMetrics,
        usage_data: UsageData,
        timestamp: std::time::SystemTime,
    },
    Unknown,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
struct UsageData {
    five_hour: UsagePeriod,
    seven_day: UsagePeriod,
    seven_day_oauth_apps: Option<UsagePeriod>,
    seven_day_opus: UsagePeriod,
    iguana_necktie: Option<UsagePeriod>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct UsagePeriod {
    utilization: f64,
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ApiErrorResponse {
    #[serde(rename = "type")]
    response_type: String,
    error: ApiError,
    request_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ApiError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
    details: ErrorDetails,
}

#[derive(Debug, Deserialize, Serialize)]
struct ErrorDetails {
    error_visibility: String,
    error_code: String,
}

/// Calculate color based on usage percentage with gradient:
/// 0-50%: Green → Yellow
/// 50-100%: Yellow → Red
fn usage_to_color(percentage: u8) -> [u8; 3] {
    let pct = percentage.min(100) as f32 / 100.0;

    // Define color stops
    const GREEN: [f32; 3] = [0.0, 200.0, 83.0]; // #00C853
    const YELLOW: [f32; 3] = [255.0, 214.0, 0.0]; // #FFD600
    const RED: [f32; 3] = [211.0, 47.0, 47.0]; // #D32F2F

    let rgb = if pct <= 0.5 {
        // Interpolate between GREEN and YELLOW (0-50%)
        let t = pct * 2.0; // Normalize to 0-1 range
        [
            GREEN[0] + (YELLOW[0] - GREEN[0]) * t,
            GREEN[1] + (YELLOW[1] - GREEN[1]) * t,
            GREEN[2] + (YELLOW[2] - GREEN[2]) * t,
        ]
    } else {
        // Interpolate between YELLOW and RED (50-100%)
        let t = (pct - 0.5) * 2.0; // Normalize to 0-1 range
        [
            YELLOW[0] + (RED[0] - YELLOW[0]) * t,
            YELLOW[1] + (RED[1] - YELLOW[1]) * t,
            YELLOW[2] + (RED[2] - YELLOW[2]) * t,
        ]
    };

    [rgb[0] as u8, rgb[1] as u8, rgb[2] as u8]
}

/// Calculate relative luminance and return appropriate text color for contrast
/// Returns (r, g, b) where each component is 0 or 255
fn contrast_text_color(bg_rgb: [u8; 3]) -> [u8; 3] {
    // Calculate relative luminance using sRGB formula
    let r = bg_rgb[0] as f32 / 255.0;
    let g = bg_rgb[1] as f32 / 255.0;
    let b = bg_rgb[2] as f32 / 255.0;

    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;

    // Use white text on dark backgrounds, black on light backgrounds
    if luminance > 0.5 {
        [0, 0, 0] // Black text
    } else {
        [255, 255, 255] // White text
    }
}

// Icon rendering configuration
const ICON_SIZE: u32 = 32; // Final tray icon size
const RENDER_SCALE: u32 = 4; // Render at 4x for quality
const RENDER_SIZE: u32 = ICON_SIZE * RENDER_SCALE; // 128px

// Font sizes (scaled for render resolution)
const PERCENTAGE_FONT_SIZE: f32 = 124.0; // 31.0 * 4
const UNKNOWN_FONT_SIZE: f32 = 80.0; // 20.0 * 4

/// Measure text dimensions using ab_glyph metrics
/// Returns (width, height, baseline_offset)
fn measure_text_bounds(
    text: &str,
    font: &ab_glyph::FontRef,
    scale: ab_glyph::PxScale,
) -> (f32, f32, f32) {
    use ab_glyph::{Font, ScaleFont};

    let scaled_font = font.as_scaled(scale);

    // Calculate text width by summing glyph advances
    let mut width = 0.0;
    for ch in text.chars() {
        let glyph_id = font.glyph_id(ch);
        width += scaled_font.h_advance(glyph_id);
    }

    // Calculate height from font metrics
    let ascent = scaled_font.ascent();
    let descent = scaled_font.descent();
    let height = ascent - descent;
    let baseline_offset = ascent;

    (width, height, baseline_offset)
}

/// Calculate position to center text on canvas
fn calculate_centered_position(
    text_width: f32,
    text_height: f32,
    _baseline_offset: f32,
    canvas_size: u32,
) -> (i32, i32) {
    let canvas_f = canvas_size as f32;

    // Center horizontally
    let x = ((canvas_f - text_width) / 2.0) as i32;

    // Center vertically (accounting for baseline)
    let y = ((canvas_f - text_height) / 2.0) as i32;

    (x, y)
}

/// Generate icon with usage percentage displayed on color gradient background
fn generate_usage_icon(percentage: u8) -> Vec<u8> {
    use ab_glyph::{FontRef, PxScale};
    use image::{imageops, Rgba, RgbaImage};
    use imageproc::drawing::draw_text_mut;

    // Get background color based on usage
    let bg_color = usage_to_color(percentage);
    let mut img = RgbaImage::from_pixel(
        RENDER_SIZE,
        RENDER_SIZE,
        Rgba([bg_color[0], bg_color[1], bg_color[2], 255]),
    );

    // Get contrasting text color
    let text_color = contrast_text_color(bg_color);
    let text_rgba = Rgba([text_color[0], text_color[1], text_color[2], 255]);

    // Load embedded font
    let font_data = include_bytes!("../fonts/Roboto-Bold.ttf");
    let font = FontRef::try_from_slice(font_data).expect("Failed to load font");

    // Format percentage text
    let text = format!("{:2}", percentage);

    // Use scaled font size for high-resolution rendering
    let scale = PxScale::from(PERCENTAGE_FONT_SIZE);

    // Measure text dimensions
    let (text_width, text_height, baseline_offset) = measure_text_bounds(&text, &font, scale);

    // Calculate centered position
    let (x, y) = calculate_centered_position(text_width, text_height, baseline_offset, RENDER_SIZE);

    // Draw text at calculated position
    draw_text_mut(&mut img, text_rgba, x, y, scale, &font, &text);

    // Downscale to final icon size for better quality
    let final_img = imageops::resize(
        &img,
        ICON_SIZE,
        ICON_SIZE,
        imageops::FilterType::Lanczos3,
    );

    final_img.into_raw()
}

/// Generate icon with question mark for unknown state
fn generate_unknown_icon() -> Vec<u8> {
    use ab_glyph::{FontRef, PxScale};
    use image::{imageops, Rgba, RgbaImage};
    use imageproc::drawing::draw_text_mut;

    // Gray background for unknown state
    let mut img = RgbaImage::from_pixel(
        RENDER_SIZE,
        RENDER_SIZE,
        Rgba([128, 128, 128, 255]),
    );

    // White question mark
    let text_rgba = Rgba([255, 255, 255, 255]);

    // Load embedded font
    let font_data = include_bytes!("../fonts/Roboto-Bold.ttf");
    let font = FontRef::try_from_slice(font_data).expect("Failed to load font");

    // Use scaled font size for high-resolution rendering
    let scale = PxScale::from(UNKNOWN_FONT_SIZE);
    let text = "?";

    // Measure text dimensions
    let (text_width, text_height, baseline_offset) = measure_text_bounds(text, &font, scale);

    // Calculate centered position
    let (x, y) = calculate_centered_position(text_width, text_height, baseline_offset, RENDER_SIZE);

    // Draw text at calculated position
    draw_text_mut(&mut img, text_rgba, x, y, scale, &font, text);

    // Downscale to final icon size for better quality
    let final_img = imageops::resize(
        &img,
        ICON_SIZE,
        ICON_SIZE,
        imageops::FilterType::Lanczos3,
    );

    final_img.into_raw()
}

async fn fetch_usage_data() -> Result<UsageData, FetchError> {
    let org_id = std::env::var("CLAUDE_ORG_ID")?;
    let session_key = std::env::var("CLAUDE_SESSION_KEY")?;

    let mut headers = HeaderMap::new();
    headers.insert(
        COOKIE,
        HeaderValue::from_str(&format!("sessionKey={}", session_key))?,
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"),
    );

    let client = ClientBuilder::new().default_headers(headers).build()?;

    let url = format!("https://claude.ai/api/organizations/{}/usage", org_id);
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
    } else {
        let error_msg = match serde_json::from_str::<ApiErrorResponse>(&response_text) {
            Ok(error_data) => format!(
                "{} - {}",
                error_data.error.error_type, error_data.error.message
            ),
            Err(_) => format!("HTTP {}", status),
        };
        Err(FetchError::Auth(error_msg))
    }
}

fn update_tray_icon(
    app: &AppHandle,
    state: &DataState,
    poller: &AdaptivePoller,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::SystemTime;

    let tray = app.tray_by_id("main").ok_or("Tray not found")?;

    // Generate icon based on state
    let icon_bytes = match state {
        DataState::Fresh { metrics, .. } => generate_usage_icon(metrics.weekly_pct()),
        DataState::Unknown => generate_unknown_icon(),
    };

    let icon = tauri::image::Image::new_owned(icon_bytes, 32, 32);
    tray.set_icon(Some(icon))?;

    // Build comprehensive tooltip
    let tooltip = match state {
        DataState::Fresh {
            metrics,
            usage_data,
            timestamp,
        } => {
            let elapsed = SystemTime::now()
                .duration_since(*timestamp)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let format_reset_time = |resets_at: &Option<String>| -> String {
                resets_at
                    .as_ref()
                    .map(|s| {
                        // Parse ISO 8601 timestamp and format it nicely
                        s.split('T')
                            .next()
                            .map(|date| date.to_string())
                            .unwrap_or_else(|| s.clone())
                    })
                    .unwrap_or_else(|| "Unknown".to_string())
            };

            format!(
                "Claude Usage Indicator\n\
                \n\
                Weekly: {}% (resets {})\n\
                6-hour: {}% (resets {})\n\
                \n\
                State: {:?}\n\
                Next poll: {}s\n\
                Last update: {}s ago",
                metrics.weekly_pct(),
                format_reset_time(&usage_data.seven_day.resets_at),
                metrics.six_hour_pct(),
                format_reset_time(&usage_data.five_hour.resets_at),
                poller.current_state(),
                poller.current_interval().as_secs(),
                elapsed
            )
        }
        DataState::Unknown => {
            format!(
                "Claude Usage Indicator\n\
                \n\
                Status: Unable to fetch usage data\n\
                Next poll: {}s",
                poller.current_interval().as_secs()
            )
        }
    };

    tray.set_tooltip(Some(tooltip))?;

    Ok(())
}

async fn start_polling(app: AppHandle, cancel_token: CancellationToken) {
    use std::time::SystemTime;

    // Initialize adaptive poller with config from environment
    let config = PollerConfig::from_env();
    info!("Adaptive poller initialized with config: {:?}", config);

    let mut poller = AdaptivePoller::new(config);
    let mut current_state = DataState::Unknown;

    loop {
        // Check for cancellation signal
        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("Shutdown signal received, stopping polling gracefully");
                break;
            }
            _ = async {
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
                    "Usage data fetched - 6h: {}%, weekly: {}%",
                    metrics.six_hour_pct(),
                    metrics.weekly_pct()
                );

                // Update state with fresh data
                current_state = DataState::Fresh {
                    metrics,
                    usage_data: data,
                    timestamp: SystemTime::now(),
                };

                // Calculate next interval using adaptive algorithm
                let next_interval = poller.next_interval(metrics, now);

                info!(
                    state = ?poller.current_state(),
                    next_interval_secs = next_interval.as_secs(),
                    next_interval_mins = next_interval.as_secs() / 60,
                    "Adaptive polling cycle complete"
                );

                // Update tray icon with current state
                if let Err(e) = update_tray_icon(&app, &current_state, &poller) {
                    error!("Failed to update tray icon: {}", e);
                }

                // Sleep for adaptive duration
                sleep(next_interval).await;
            }
            Err(e) => {
                error!("Failed to fetch usage data: {}", e);

                // Update state to unknown
                current_state = DataState::Unknown;

                // Update tray icon to show unknown state
                if let Err(e) = update_tray_icon(&app, &current_state, &poller) {
                    error!("Failed to update tray icon: {}", e);
                }

                // On error, wait minimum interval before retrying
                let min_interval =
                    Duration::from_secs(poller.current_interval().as_secs().max(180));
                info!(
                    "Retrying in {} seconds due to error",
                    min_interval.as_secs()
                );
                sleep(min_interval).await;
            }
        }
            } => {}
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Load environment variables from .env file in repository root
    dotenvy::from_filename("../.env").ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Create tray menu
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let menu = MenuBuilder::new(app).item(&quit_item).build()?;

            // Create initial tray icon with unknown state
            let icon_bytes = generate_unknown_icon();
            let icon = tauri::image::Image::new_owned(icon_bytes, 32, 32);

            TrayIconBuilder::with_id("main")
                .icon(icon)
                .menu(&menu)
                .on_menu_event(|app, event| if event.id.as_ref() == "quit" {
                    app.exit(0);
                })
                .build(app)?;

            info!("Tray icon created successfully");

            // Create cancellation token for graceful shutdown
            let cancel_token = CancellationToken::new();
            let cancel_clone = cancel_token.clone();

            // Create shutdown flag to prevent infinite exit loop
            let shutdown_started = Arc::new(AtomicBool::new(false));

            // Start background polling task
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(start_polling(app_handle, cancel_clone));

            // Store state for shutdown handling
            app.manage(cancel_token);
            app.manage(shutdown_started);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                // Check if shutdown has already been initiated
                let shutdown_flag = app_handle.state::<Arc<AtomicBool>>();

                if shutdown_flag.swap(true, Ordering::SeqCst) {
                    // Shutdown already initiated, allow exit to proceed
                    return;
                }

                info!("Exit requested, initiating graceful shutdown");

                // Prevent immediate exit to perform cleanup
                api.prevent_exit();

                // Get cancellation token and trigger shutdown
                let token = app_handle.state::<CancellationToken>();
                token.cancel();

                info!("Graceful shutdown complete, tray icon will be cleaned up automatically");

                // Trigger exit again - this time the flag is set so it won't prevent
                app_handle.exit(0);
            }
        });
}
