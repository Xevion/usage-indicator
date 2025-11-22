# Roadmap

This document outlines the long-term vision and planned features for `usage-indicator`.

## Vision

Build a polished, open-source system tray application for monitoring usage statistics of rate-limited cloud services. The app should provide minimal-friction setup, intelligent behavior, and a professional user experience suitable for public distribution.

## Core Principles

- 🎯 **Tray-first interface** - Pure system tray with rich tooltips and right-click menu, no persistent window
- 🚀 **Zero-friction setup** - Auto-extract credentials from browser with guided manual fallback
- 🧠 **Intelligent polling** - Adaptive algorithm that respects both user activity and API limits
- 🖥️ **System-aware** - Integrates with OS events (sleep/wake, network changes)
- 💎 **Polished UX** - Professional error handling, visual states, notifications
- 📦 **Wide distribution** - Installers, package managers, auto-updates

## v0.2 - Foundation & Distribution

Establish automated build and release infrastructure to make the app easily installable across all platforms, with basic error handling to ensure users understand when things go wrong.

#### CI/CD Pipeline

- [x] GitHub Actions workflow for automated builds
- [x] Matrix builds for Windows, macOS, and Linux
- [x] Automated testing on all platforms
- [ ] Release artifact generation and signing
- [x] Automated GitHub Releases publishing
- [x] Version bumping automation

#### Platform-Specific Installers

- [x] **Windows:** `.msi` installer with proper registry integration
- [x] **macOS:** `.dmg` with drag-to-Applications UX
- [x] **Linux:** `.deb` and `.rpm` packages

#### Auto-launch on Startup

- [ ] Windows: Registry entry or Startup folder
- [ ] macOS: Launch Agent (plist)
- [ ] Linux: `.desktop` file in autostart directory
- [ ] User-configurable (enable/disable in settings)

#### Basic Error Handling

- [ ] **Core Error States**
  - Distinguish transient vs. permanent errors
  - Automatic retry with backoff for transient errors
  - Maintain last-known-good state during outages
- [ ] **Visual Feedback**
  - Error state in tray icon (simple indicator)
  - Tooltip shows basic error information
  - Clear distinction between "working" and "broken" states
- [ ] **Network Resilience**
  - Handle network disconnection gracefully
  - Avoid spamming failed requests
  - Automatic recovery when connectivity returns

#### Documentation

- [ ] Installation instructions for each platform
- [ ] Basic troubleshooting guide
- [ ] Platform-specific setup notes

### Success Criteria

- Automated release builds for all three platforms
- Users can install without building from source
- App launches automatically on system startup (configurable)
- Users can tell when the app is broken and why (basic error feedback)

## v0.3 - Authentication & Configuration

Eliminate manual credential extraction and implement secure credential storage using OS-native keychains.

#### Hybrid Configuration System

- [ ] **OS Keychain Integration**
  - Windows: Credential Manager API
  - macOS: Keychain Services
  - Linux: Secret Service API (libsecret)
- [ ] **TOML Config File**
  - Polling intervals and tuning parameters
  - UI preferences (dual icons, notifications)
  - Service selection
- [ ] **Platform-Specific Config Directories**
  - Windows: `%APPDATA%\usage-indicator\config.toml`
  - macOS: `~/Library/Application Support/usage-indicator/config.toml`
  - Linux: `~/.config/usage-indicator/config.toml`

#### Browser Credential Extraction (Best-Effort)

- [ ] Chrome/Chromium cookie database reading
- [ ] Firefox cookie database reading
- [ ] Edge (Chromium) support
- [ ] Automatic browser detection (prioritize most recently used)
- [ ] Secure cookie decryption (platform-specific)
- [ ] Graceful fallback if extraction fails (clear instructions, copy-paste helpers)

#### First-Launch Configuration UI

- [ ] Minimal Tauri window shown only on first launch
- [ ] Auto-detection attempt with progress indicator
- [ ] Guided manual setup if auto-detection fails
  - Step-by-step instructions with screenshots
  - Copy-paste helpers for credentials
  - Real-time validation of entered credentials
- [ ] Success confirmation with test API call
- [ ] Window closes automatically after setup, leaving only tray icon

#### Automatic Organization ID Detection

- [ ] Parse org ID from Claude API response headers
- [ ] Store detected org ID in config
- [ ] Eliminate need for users to manually find org ID

#### Migration from .env

- [ ] Detect existing `.env` file on first launch
- [ ] Migrate credentials to new storage system
- [ ] Prompt user to delete `.env` after migration
- [ ] Backwards compatibility fallback for development

### Success Criteria

- New users can install and run without ever opening browser DevTools
- Credentials stored securely in OS keychain
- Smooth migration path for existing users
- Clear error messages if credential extraction fails

## v0.4 - System Intelligence

Make the app responsive to system state changes and network conditions, improving battery life, responsiveness and reliability.

#### System Event Integration

- [ ] **Sleep/Suspend Events**
  - Detect system sleep/suspend
  - Pause all polling during sleep
  - Cancel in-flight requests gracefully
- [ ] **Wake/Resume Events**
  - Detect system wake/resume
  - Immediate usage check on wake (user likely just used Claude)
  - Resume normal adaptive polling
- [ ] **Screen Lock/Unlock**
  - Pause polling on lock (optional, configurable)
  - Resume on unlock
- [ ] **Platform-Specific Implementations**
  - Windows: WM_POWERBROADCAST messages
  - macOS: IOKit power notifications
  - Linux: systemd-logind or UPower D-Bus signals

#### Advanced Network Awareness

- [ ] Platform-specific network change listeners
- [ ] Immediate check when network returns (builds on v0.2 basics)

#### Rate Limit Handling

- [ ] Detect HTTP 429 (Rate Limited) responses
- [ ] Exponential backoff with jitter
- [ ] Configurable backoff parameters
- [ ] Visual indication in tray icon tooltip
- [ ] Automatic recovery when rate limit lifts
- [ ] Respect Retry-After headers from API

### Success Criteria

- No wasted polling during system sleep
- Immediate responsiveness on wake/reconnect
- Graceful handling of network interruptions
- No API spam during rate limiting
- Battery-efficient behavior on laptops

## v0.5 - Polish & User Feedback

Enhance visual feedback and add professional polish on top of the basic error handling from v0.2.

#### Enhanced Visual States

- [ ] **Expanded Icon States** (beyond v0.2 basics)
  - Normal: Color-coded usage percentage (current)
  - Offline: Gray icon with network symbol
  - Auth Error: Yellow warning icon
  - Rate Limited: Orange caution icon
  - API Error: Red error icon
  - Unknown/Stale: Question mark icon
- [ ] Smooth icon transitions
- [ ] Rich tooltip with current state explanation

#### Tray Menu Diagnostics

- [ ] **Right-Click Context Menu**
  - Usage overview (current values)
  - Connection status indicator
  - Last successful update timestamp
  - Last error message (if any)
  - "Retry Now" action
  - "Open Settings" action (future)
  - "Check for Updates" action (v0.6)
  - "About" dialog
  - "Quit" action
- [ ] Menu items update in real-time
- [ ] Disabled states for unavailable actions

#### System Notifications

- [ ] **Critical Issues** (requires user action)
  - Authentication expired (requires re-login)
  - Credential extraction failed
  - Persistent API errors
- [ ] **Usage Warnings** (optional, configurable)
  - Approaching limit (80%, 90%, 95%)
  - Configurable thresholds
  - Option to disable notifications
- [ ] **Platform-Native Notifications**
  - Windows: Toast notifications
  - macOS: Notification Center
  - Linux: libnotify/D-Bus notifications
- [ ] Action buttons in notifications (e.g., "Open Settings")
- [ ] Silent resilience: transient errors don't trigger notifications

#### Logging System

- [ ] Structured logging to file
- [ ] Configurable log levels
- [ ] Log rotation (max size/age)
- [ ] Logs accessible from tray menu
- [ ] Include in bug reports

### Success Criteria

- Professional, polished error UX
- Users always know the current state of the app
- Clear, actionable error messages
- No noise from transient issues
- Easy diagnosis when problems occur

## v0.6 - Advanced Features

Add advanced features for power users and establish initial distribution channels.

#### Configurable Icon Metric

- [ ] **Metric Selection**
  - Choose which metric to display in tray icon
  - Option 1: Weekly (7-day) usage (default)
  - Option 2: 6-hour usage
  - User-configurable via settings
  - Tooltip always shows both metrics regardless of selection

#### Relative Timestamps

- [ ] Replace absolute times with relative ("2h ago", "5m ago")
- [ ] Hover tooltip shows full timestamp
- [ ] Configurable format preferences
- [ ] Locale-aware formatting

#### Auto-Updater

- [ ] Built-in update checker using Tauri updater
- [ ] Background check for updates (configurable interval)
- [ ] System notification when update available
- [ ] In-tray menu: "Update Available" with changelog
- [ ] One-click download and install
- [ ] Automatic restart after update (with confirmation)
- [ ] Rollback capability if update fails

#### Package Manager Distribution

- [ ] **winget (Windows)**
  - Create winget manifest
  - Submit to winget-pkgs repository
  - Automated manifest updates
- [ ] **Homebrew (macOS)** - Post-v0.6
- [ ] **AUR (Arch Linux)** - Post-v0.6

#### Advanced Configuration

- [ ] Settings UI for all config options
- [ ] Import/export configuration
- [ ] Configuration validation
- [ ] Reset to defaults option

### Success Criteria

- Auto-update works seamlessly on all platforms
- Available via winget for Windows users
- Power users can customize icon metric and behavior
- Smooth upgrade path from v0.5 to v0.6

## v1.0 - Polish & Public Release

Final polish, comprehensive testing, and official public launch.

#### Documentation

- [ ] **End-User Documentation**
  - Comprehensive installation guide
  - Setup wizard walkthrough
  - Feature explanations with screenshots
  - FAQ section
  - Troubleshooting guide
- [ ] **Developer Documentation**
  - Architecture overview
  - Build instructions
  - Contribution guidelines
  - Code style guide
  - Testing guide
- [ ] **API Documentation**
  - Document Claude API integration
  - Rate limiting behavior
  - Error handling patterns

#### Security Review

- [ ] Internal security review of credential storage
- [ ] Internal review of network communication
- [ ] Automated dependency vulnerability scanning (cargo audit, dependabot)
- [ ] Static analysis (clippy pedantic mode)
- [ ] Address all findings

#### Performance Optimization

- [ ] Memory profiling and leak detection
- [ ] CPU usage optimization
- [ ] Binary size reduction
- [ ] Startup time optimization
- [ ] Icon rendering performance
- [ ] Network request efficiency

#### Quality Assurance

- [ ] Comprehensive test suite (unit + integration)
- [ ] Manual testing on all platforms

### Success Criteria

- All critical bugs resolved
- Internal security review complete with no major issues
- Comprehensive documentation complete
- Available via installers and winget

## Future (Post-v1.0)

### Multi-Service Support

- [ ] **Service Abstraction Layer**
  - Generic usage monitoring interface
  - Pluggable service implementations
  - Unified configuration schema
- [ ] **Cursor Integration**
  - API endpoint discovery
  - Authentication flow
  - Usage metric mapping
- [ ] **Gemini Integration**
  - Authentication discovery
  - Quota/Usage monitoring
- [ ] **OpenAI Integration**
  - Usage tracking for ChatGPT, API
  - Usage limit monitoring
- [ ] **Other Services**
  - Perplexity
  - GitHub Copilot
  - Any usage-limited API service

### Usage History & Predictive Analytics

- [ ] **Data Persistence**
  - Persist usage data to local database (SQLite)
  - Historical data collection
  - Automatic data retention policies
- [ ] **Trend Analysis**
  - Calculate usage rate (requests per hour)
  - Identify usage patterns
  - Detect high activity periods
- [ ] **Predictive Estimates**
  - Predict when usage will reach 100% based on current rate
  - Show estimated time to limit in tooltip
  - Warn when projected to exceed limit before reset
  - Adjust predictions based on historical patterns

### Multi-Account Support

- [ ] Track multiple Claude organizations
- [ ] Track multiple services simultaneously
- [ ] Account switcher in tray menu
- [ ] Separate icons or unified composite view
- [ ] Per-account configuration

## Technical Debt & Maintenance

### Ongoing

- Regular dependency updates
- Security patch releases
- Bug fix releases
- Performance monitoring
- User feedback incorporation

### Known Technical Debt

- [ ] Frontend is mostly unused (React app shows only instructions)
  - Consider removing frontend entirely or repurposing for settings UI
- [ ] `system_events.rs` defined but not integrated
  - Complete integration in v0.4
- [ ] `.env` file in project root (awkward)
  - Remove in favor of proper config system (v0.3)
- [ ] No persistent state between runs
  - Add in post-v1.0 for usage history
