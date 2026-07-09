//! # qr-wifi-core
//!
//! Cross-platform core for the QR Wi-Fi project. Everything that is not UI
//! lives here so the CLI, TUI, Tauri GUI, and the browser-extension host all
//! share one implementation:
//!
//! - [`types`] — domain models (`WifiSecurity`, `WifiNetwork`, `WifiCredentials`).
//! - [`payload`] — build/parse `WIFI:` QR payloads.
//! - [`qr`] — QR matrix → PNG / terminal art, plus image decoding.
//! - [`platform`] — OS Wi-Fi adapters (`networksetup`, `nmcli`, `netsh`).
//! - [`service`] — high-level feature functions shared by every frontend.
//! - [`ipc`] — shared request/response protocol and Native Messaging framing.

// Project-wide lint policy: every public item is documented, every public type
// implements `Debug`, and we follow Rust 2018+ idioms. This keeps the library a
// clean, explorable API surface for learners. Add `#![deny(...)]` in a real
// release to turn these into hard errors.
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]

pub mod error;
pub mod ipc;
pub mod payload;
pub mod platform;
pub mod qr;
pub mod service;
pub mod types;

pub use error::{CoreError, Result};
pub use ipc::{
    handle_request, read_message, run_loop, write_message, Request, Response, ResponseData,
};
pub use payload::{build_payload, parse_payload};
pub use platform::{default_adapter, WifiAdapter};
pub use qr::{
    credentials_to_qr, decode_image, decode_image_base64, decode_image_path, to_png, to_png_base64,
    to_unicode,
};
pub use service::{
    connect_credentials, connect_payload, current_credentials, decode_qr_base64, decode_qr_path,
    networks, share_current, share_custom, share_ssid, QrShare,
};
pub use types::{sort_networks, WifiCredentials, WifiNetwork, WifiSecurity};
