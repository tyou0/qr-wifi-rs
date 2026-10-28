//! Platform Wi-Fi adapters.
//!
//! Each operating system has its own module implementing [`WifiAdapter`]. The
//! macOS uses CoreWLAN. Linux uses NetworkManager's D-Bus API;
//! Windows uses the Native Wi-Fi API. Neither parses localized CLI output.

// Exercise every adapter's parsing and command construction on every test host.
#[cfg(any(test, target_os = "linux"))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(test, target_os = "windows"))]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
mod windows;

use crate::error::Result;
use crate::types::{WifiCredentials, WifiNetwork};

/// OS-level Wi-Fi operations used by every frontend (CLI/TUI/GUI).
pub trait WifiAdapter: Send + Sync {
    /// List known/visible networks, marking the active one.
    fn list_networks(&self) -> Result<Vec<WifiNetwork>>;

    /// The SSID of the currently connected network.
    fn current_ssid(&self) -> Result<String>;

    /// Read saved credentials for a known SSID (the password may be `None`
    /// when the OS refuses to reveal it).
    fn credentials(&self, ssid: &str) -> Result<WifiCredentials>;

    /// Connect to a network described by `credentials`.
    fn connect(&self, credentials: &WifiCredentials) -> Result<()>;
}

/// Pick the right adapter for the current operating system.
pub fn default_adapter() -> Box<dyn WifiAdapter> {
    #[cfg(target_os = "macos")]
    let adapter: Box<dyn WifiAdapter> = Box::new(macos::MacosAdapter::new());
    #[cfg(target_os = "linux")]
    let adapter: Box<dyn WifiAdapter> = Box::new(linux::LinuxAdapter);
    #[cfg(target_os = "windows")]
    let adapter: Box<dyn WifiAdapter> = Box::new(windows::WindowsAdapter);

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    compile_error!("qr-wifi-core only supports macOS, Linux, and Windows");

    adapter
}
