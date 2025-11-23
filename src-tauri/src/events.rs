use std::time::Duration;

/// Cross-platform system events for adaptive polling behavior
///
/// Platform-specific event detection and listening should be implemented separately.
/// This enum provides a unified interface for system state changes that affect
/// polling behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEvent {
    /// User logged into the system
    UserLogin,

    /// User logged out of the system
    UserLogout,

    /// Screen/display turned on
    ScreenOn,

    /// Screen/display turned off
    ScreenOff,

    /// User actively using the system (keyboard/mouse input)
    UserActive,

    /// User has been idle for specified duration
    UserIdle { duration: Duration },

    /// System entering sleep/suspend state
    SystemSleep,

    /// System waking from sleep/suspend
    SystemWake,
}

/// Action to take in response to a system event
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollAction {
    /// Immediately fetch usage data (skip waiting for next interval)
    FetchImmediately,

    /// Pause all polling until further notice
    Pause,

    /// Continue normal polling behavior
    Continue,
}

impl SystemEvent {
    /// Determine what action should be taken for this event
    pub fn recommended_action(&self) -> PollAction {
        match self {
            // Immediately check usage when user logs in or wakes system
            SystemEvent::UserLogin | SystemEvent::SystemWake | SystemEvent::ScreenOn => {
                PollAction::FetchImmediately
            }

            // Stop polling when user logs out or system sleeps
            SystemEvent::UserLogout | SystemEvent::SystemSleep | SystemEvent::ScreenOff => {
                PollAction::Pause
            }

            // Continue normally for activity/idle events
            SystemEvent::UserActive | SystemEvent::UserIdle { .. } => PollAction::Continue,
        }
    }

    /// Check if this event indicates the system is entering an inactive state
    pub fn is_inactive_state(&self) -> bool {
        matches!(
            self,
            SystemEvent::UserLogout | SystemEvent::SystemSleep | SystemEvent::ScreenOff
        )
    }

    /// Check if this event indicates the system is entering an active state
    pub fn is_active_state(&self) -> bool {
        matches!(
            self,
            SystemEvent::UserLogin | SystemEvent::SystemWake | SystemEvent::ScreenOn
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recommended_actions() {
        assert_eq!(
            SystemEvent::UserLogin.recommended_action(),
            PollAction::FetchImmediately
        );
        assert_eq!(
            SystemEvent::UserLogout.recommended_action(),
            PollAction::Pause
        );
        assert_eq!(
            SystemEvent::UserActive.recommended_action(),
            PollAction::Continue
        );
    }

    #[test]
    fn test_state_classification() {
        assert!(SystemEvent::UserLogin.is_active_state());
        assert!(SystemEvent::UserLogout.is_inactive_state());
        assert!(!SystemEvent::UserActive.is_active_state());
        assert!(!SystemEvent::UserActive.is_inactive_state());
    }
}

#[cfg(windows)]
pub mod windows {
    use super::SystemEvent;
    use tokio::sync::mpsc;
    use tracing::{debug, error};
    use windows::core::w;

    // Power broadcast event constants (not exposed by windows crate)
    const PBT_APMSUSPEND: u32 = 0x0004;
    const PBT_APMRESUMEAUTOMATIC: u32 = 0x0012;
    const PBT_APMRESUMESUSPEND: u32 = 0x0007;

    /// Start listening for Windows power management events
    /// Returns a receiver channel that will receive SystemEvent::SystemSleep and SystemEvent::SystemWake
    pub fn start_power_listener() -> mpsc::UnboundedReceiver<SystemEvent> {
        let (tx, rx) = mpsc::unbounded_channel();

        std::thread::spawn(move || {
            use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DispatchMessageW,
                GetMessageW, MSG, RegisterClassW, TranslateMessage, WM_POWERBROADCAST, WNDCLASSW,
                WS_OVERLAPPEDWINDOW,
            };

            unsafe extern "system" fn wnd_proc(
                hwnd: HWND,
                msg: u32,
                wparam: WPARAM,
                lparam: LPARAM,
            ) -> LRESULT {
                match msg {
                    WM_POWERBROADCAST => {
                        let event_type = wparam.0 as u32;
                        match event_type {
                            PBT_APMSUSPEND => {
                                debug!("Windows power event: System entering sleep");
                                // SAFETY: The pointer stored in GWLP_USERDATA is valid because:
                                // 1. It was created from a Box and stored before the message loop started
                                // 2. Windows guarantees no messages are dispatched to this window after GetMessage returns 0
                                // 3. The pointer is only freed after the message loop exits (line 246)
                                // 4. as_ref() safely converts the raw pointer to Option<&T>, returning None if null
                                if let Some(tx) = unsafe {
                                    (GetWindowLongPtrW(hwnd, GWLP_USERDATA)
                                        as *const mpsc::UnboundedSender<SystemEvent>)
                                        .as_ref()
                                } {
                                    let _ = tx.send(SystemEvent::SystemSleep);
                                }
                            }
                            PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => {
                                debug!("Windows power event: System waking from sleep");
                                // SAFETY: Same invariants as PBT_APMSUSPEND case above
                                if let Some(tx) = unsafe {
                                    (GetWindowLongPtrW(hwnd, GWLP_USERDATA)
                                        as *const mpsc::UnboundedSender<SystemEvent>)
                                        .as_ref()
                                } {
                                    let _ = tx.send(SystemEvent::SystemWake);
                                }
                            }
                            _ => {}
                        }
                        LRESULT(0)
                    }
                    // SAFETY: DefWindowProcW is safe to call with any valid window handle and message parameters
                    _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
                }
            }

            use windows::Win32::UI::WindowsAndMessaging::{
                GWLP_USERDATA, GetWindowLongPtrW, SetWindowLongPtrW,
            };
            use windows::core::PCWSTR;

            // SAFETY: This entire block performs Win32 API calls that require unsafe.
            // Safety invariants are documented inline for each operation.
            unsafe {
                let class_name = w!("UsageIndicatorPowerListener");

                let wc = WNDCLASSW {
                    style: CS_HREDRAW | CS_VREDRAW,
                    lpfnWndProc: Some(wnd_proc),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: Default::default(),
                    hIcon: Default::default(),
                    hCursor: Default::default(),
                    hbrBackground: Default::default(),
                    lpszMenuName: PCWSTR::null(),
                    lpszClassName: class_name,
                };

                // SAFETY: RegisterClassW is safe with a valid WNDCLASSW structure
                if RegisterClassW(&wc) == 0 {
                    error!("Failed to register window class for power events");
                    return;
                }

                // SAFETY: CreateWindowExW is safe with valid parameters.
                // The window is message-only (size 0x0) and never shown.
                let hwnd = match CreateWindowExW(
                    Default::default(),
                    class_name,
                    w!(""),
                    WS_OVERLAPPEDWINDOW,
                    0,
                    0,
                    0,
                    0,
                    None,
                    Default::default(),
                    Default::default(),
                    None,
                ) {
                    Ok(hwnd) => hwnd,
                    Err(e) => {
                        error!("Failed to create window for power events: {}", e);
                        return;
                    }
                };

                // SAFETY: We create a raw pointer from Box to store in window user data.
                // Lifetime invariants:
                // 1. The pointer is created here and stored in GWLP_USERDATA immediately
                // 2. The pointer remains valid throughout the message loop's lifetime
                // 3. Windows guarantees the message loop owns the window
                // 4. The pointer is freed via Box::from_raw after the loop exits
                // 5. No window messages can be dispatched after GetMessage returns 0
                let tx_ptr = Box::into_raw(Box::new(tx));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, tx_ptr as isize);

                debug!("Windows power event listener started");

                // Message loop
                let mut msg = MSG::default();
                loop {
                    // SAFETY: GetMessageW is safe with a valid MSG pointer and window handle.
                    // Returns -1 on error, 0 when WM_QUIT received, >0 for normal messages.
                    match GetMessageW(&mut msg, None, 0, 0).0 {
                        -1 => {
                            error!("GetMessage failed");
                            break;
                        }
                        0 => break,
                        _ => {
                            // SAFETY: TranslateMessage and DispatchMessageW are safe with valid MSG
                            let _ = TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                }

                // SAFETY: We're reconstructing the Box from the raw pointer to properly drop it.
                // This is safe because:
                // 1. The pointer was created from Box::into_raw above
                // 2. The message loop has exited, so no more window procedure calls
                // 3. We're about to return, so the pointer won't be used again
                let _ = Box::from_raw(tx_ptr);
                debug!("Windows power event listener stopped");
            }
        });

        rx
    }
}

#[cfg(target_os = "macos")]
pub mod platform {
    use super::SystemEvent;
    use tokio::sync::mpsc;
    use tracing::{debug, error};

    /// Start listening for macOS power management events using IOKit
    /// Returns a receiver channel that will receive SystemEvent::SystemSleep and SystemEvent::SystemWake
    pub fn start_power_listener() -> mpsc::UnboundedReceiver<SystemEvent> {
        let (tx, rx) = mpsc::unbounded_channel();

        std::thread::spawn(move || {
            use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
            use io_kit_sys::*;
            use std::ffi::c_void;
            use std::ptr;

            // Message type constants from IOMessage.h
            const kIOMessageSystemWillSleep: u32 = 0xE0000280;
            const kIOMessageSystemHasPoweredOn: u32 = 0xE0000300;

            unsafe extern "C" fn sleep_callback(
                refcon: *mut c_void,
                _service: io_service_t,
                message_type: u32,
                _message_argument: *mut c_void,
            ) {
                let tx = &*(refcon as *const mpsc::UnboundedSender<SystemEvent>);

                match message_type {
                    kIOMessageSystemWillSleep => {
                        debug!("macOS power event: System will sleep");
                        let _ = tx.send(SystemEvent::SystemSleep);
                    }
                    kIOMessageSystemHasPoweredOn => {
                        debug!("macOS power event: System has powered on");
                        let _ = tx.send(SystemEvent::SystemWake);
                    }
                    _ => {}
                }
            }

            unsafe {
                let mut root_port: io_connect_t = 0;
                let mut notifier_port: IONotificationPortRef = ptr::null_mut();

                // Register for power notifications
                root_port = IORegisterForSystemPower(
                    &tx as *const _ as *mut c_void,
                    &mut notifier_port,
                    sleep_callback,
                    &mut 0,
                );

                if root_port == 0 {
                    error!("Failed to register for macOS power events");
                    return;
                }

                // Add notification port to run loop
                let run_loop_source = IONotificationPortGetRunLoopSource(notifier_port);
                if run_loop_source.is_null() {
                    error!("Failed to get run loop source for power notifications");
                    IODeregisterForSystemPower(&mut notifier_port);
                    IOServiceClose(root_port);
                    return;
                }

                let run_loop = CFRunLoop::get_current();
                core_foundation::runloop::CFRunLoopAddSource(
                    run_loop.as_concrete_TypeRef(),
                    run_loop_source,
                    kCFRunLoopDefaultMode,
                );

                debug!("macOS power event listener started");

                // Run the event loop
                run_loop.run();

                // Cleanup (this won't be reached unless run loop is stopped)
                IODeregisterForSystemPower(&mut notifier_port);
                IOServiceClose(root_port);
                debug!("macOS power event listener stopped");
            }
        });

        rx
    }
}

#[cfg(target_os = "linux")]
pub mod platform {
    use super::SystemEvent;
    use tokio::sync::mpsc;
    use tracing::{debug, error};

    /// Start listening for Linux power management events using D-Bus
    /// Returns a receiver channel that will receive SystemEvent::SystemSleep and SystemEvent::SystemWake
    pub fn start_power_listener() -> mpsc::UnboundedReceiver<SystemEvent> {
        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            use futures_util::stream::StreamExt;
            use zbus::{Connection, proxy};

            #[proxy(
                interface = "org.freedesktop.login1.Manager",
                default_service = "org.freedesktop.login1",
                default_path = "/org/freedesktop/login1"
            )]
            trait Login1Manager {
                #[zbus(signal)]
                fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;
            }

            match Connection::system().await {
                Ok(connection) => {
                    debug!("Connected to D-Bus system bus");

                    match Login1ManagerProxy::new(&connection).await {
                        Ok(proxy) => {
                            match proxy.receive_prepare_for_sleep().await {
                                Ok(mut stream) => {
                                    debug!("Linux power event listener started");

                                    // Listen for sleep/wake signals
                                    while let Some(signal) = stream.next().await {
                                        if let Ok(args) = signal.args() {
                                            if args.start {
                                                debug!(
                                                    "Linux power event: System preparing for sleep"
                                                );
                                                let _ = tx.send(SystemEvent::SystemSleep);
                                            } else {
                                                debug!(
                                                    "Linux power event: System resuming from sleep"
                                                );
                                                let _ = tx.send(SystemEvent::SystemWake);
                                            }
                                        }
                                    }

                                    debug!("Linux power event listener stopped");
                                }
                                Err(e) => {
                                    error!("Failed to subscribe to PrepareForSleep signal: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to create D-Bus proxy for login1: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to connect to D-Bus system bus: {}", e);
                }
            }
        });

        rx
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub mod platform {
    use super::SystemEvent;
    use tokio::sync::mpsc;

    /// Placeholder for unsupported platforms
    /// Returns a receiver that will never receive events
    pub fn start_power_listener() -> mpsc::UnboundedReceiver<SystemEvent> {
        let (_tx, rx) = mpsc::unbounded_channel();
        rx
    }
}
