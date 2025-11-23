use crate::icon::generate_unknown_icon;
use crate::polling::start_polling;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tokio_util::sync::CancellationToken;
use tracing::info;

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
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "quit" {
                        app.exit(0);
                    }
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
