mod adaptive_poller;
mod system_events;

use adaptive_poller::{AdaptivePoller, PollerConfig, UsageMetrics};
use serde::{Deserialize, Serialize};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;
use tokio::time::{sleep, Duration};
use std::time::Instant;
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

#[derive(Debug, Deserialize, Serialize)]
struct UsageData {
    five_hour: UsagePeriod,
    seven_day: UsagePeriod,
    seven_day_oauth_apps: Option<UsagePeriod>,
    seven_day_opus: UsagePeriod,
    iguana_necktie: Option<UsagePeriod>,
}

#[derive(Debug, Deserialize, Serialize)]
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

fn generate_colored_square(size: u32, color: [u8; 4]) -> Vec<u8> {
    use image::{Rgba, RgbaImage};

    let mut img = RgbaImage::new(size, size);
    for pixel in img.pixels_mut() {
        *pixel = Rgba(color);
    }

    img.into_raw()
}

fn random_color() -> [u8; 4] {
    use rand::Rng;
    let mut rng = rand::rng();
    [
        rng.random_range(0..=255),
        rng.random_range(0..=255),
        rng.random_range(0..=255),
        255, // fully opaque
    ]
}

async fn fetch_usage_data() -> Result<UsageData, FetchError> {
    let org_id = std::env::var("CLAUDE_ORG_ID")?;
    let session_key = std::env::var("CLAUDE_SESSION_KEY")?;

    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(&format!("sessionKey={}", session_key))?);
    headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"));

    let client = ClientBuilder::new()
        .default_headers(headers)
        .build()?;

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
                error_data.error.error_type,
                error_data.error.message
            ),
            Err(_) => format!("HTTP {}", status),
        };
        Err(FetchError::Auth(error_msg))
    }
}

fn update_tray_icon(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let color = random_color();
    let icon_bytes = generate_colored_square(32, color);

    let tray = app.tray_by_id("main").ok_or("Tray not found")?;
    let icon = tauri::image::Image::new_owned(icon_bytes, 32, 32);
    tray.set_icon(Some(icon))?;

    Ok(())
}

async fn start_polling(app: AppHandle) {
    // Initialize adaptive poller with config from environment
    let config = PollerConfig::from_env();
    info!("Adaptive poller initialized with config: {:?}", config);

    let mut poller = AdaptivePoller::new(config);

    loop {
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

                // Calculate next interval using adaptive algorithm
                let next_interval = poller.next_interval(metrics, now);

                info!(
                    state = ?poller.current_state(),
                    next_interval_secs = next_interval.as_secs(),
                    next_interval_mins = next_interval.as_secs() / 60,
                    "Adaptive polling cycle complete"
                );

                if let Err(e) = update_tray_icon(&app) {
                    error!("Failed to update tray icon: {}", e);
                }

                // Sleep for adaptive duration
                sleep(next_interval).await;
            }
            Err(e) => {
                error!("Failed to fetch usage data: {}", e);

                // On error, wait minimum interval before retrying
                let min_interval = Duration::from_secs(poller.current_interval().as_secs().max(180));
                info!("Retrying in {} seconds due to error", min_interval.as_secs());
                sleep(min_interval).await;
            }
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

            let menu = MenuBuilder::new(app)
                .item(&quit_item)
                .build()?;

            // Create initial tray icon with random color
            let color = random_color();
            let icon_bytes = generate_colored_square(32, color);
            let icon = tauri::image::Image::new_owned(icon_bytes, 32, 32);

            TrayIconBuilder::with_id("main")
                .icon(icon)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            info!("Tray icon created successfully");

            // Start background polling task
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(start_polling(app_handle));

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
