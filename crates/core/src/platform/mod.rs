//! Platform Wi-Fi adapters.
//!
//! Each operating system has its own module implementing [`WifiAdapter`]. The
//! adapters shell out to native tooling (`networksetup`/`ipconfig`,
//! `nmcli`, `netsh`) so no C bindings are required.

mod command;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use crate::error::Result;
use crate::types::{WifiCredentials, WifiNetwork};

pub(crate) use command::{run, try_capture};

/// OS-level Wi-Fi operations used by every frontend (CLI/TUI/GUI/IPC).
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
