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
            SystemEvent::UserActive | SystemEvent::UserIdle { .. } => {
                PollAction::Continue
            }
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
        assert_eq!(SystemEvent::UserLogin.recommended_action(), PollAction::FetchImmediately);
        assert_eq!(SystemEvent::UserLogout.recommended_action(), PollAction::Pause);
        assert_eq!(SystemEvent::UserActive.recommended_action(), PollAction::Continue);
    }

    #[test]
    fn test_state_classification() {
        assert!(SystemEvent::UserLogin.is_active_state());
        assert!(SystemEvent::UserLogout.is_inactive_state());
        assert!(!SystemEvent::UserActive.is_active_state());
        assert!(!SystemEvent::UserActive.is_inactive_state());
    }
}
