//! Shared error type for all of QR Wi-Fi.
//!
//! Rust has no exceptions: fallible functions return [`Result`]. Instead of
//! passing strings around, every error in this crate is a strongly typed
//! [`CoreError`] (built with [`thiserror`]), so callers can `match` on the
//! concrete cause. This is the idiomatic Rust pattern for a library's public
//! error type.

use thiserror::Error;

/// Every fallible operation in the crate returns [`CoreError`].
///
/// Variants ending in a wrapped value (e.g. [`CoreError::Payload`]) carry a
/// human-readable detail; the `#[from]` variants convert automatically from
/// common third-party errors so `?` works out of the box.
#[derive(Debug, Error)]
pub enum CoreError {
    /// A `WIFI:` payload could not be built or parsed.
    #[error("Wi-Fi payload error: {0}")]
    Payload(String),

    /// QR matrix/image generation failed (e.g. payload too large to encode).
    #[error("QR generation failed: {0}")]
    QrGen(String),

    /// QR decoding from an image failed (no code found, corrupt data, ...).
    #[error("QR decode failed: {0}")]
    QrDecode(String),

    /// A spawned OS command (`networksetup`, `nmcli`, `netsh`, ...) failed.
    #[error("OS command failed ({command}): {message}")]
    Command {
        /// The program + arguments that were invoked, for diagnostics.
        command: String,
        /// Trimmed stderr / status detail from the command.
        message: String,
    },

    /// A platform Wi-Fi operation failed in a way not covered by another variant.
    #[error("Platform Wi-Fi error: {0}")]
    Platform(String),

    /// The active Wi-Fi network could not be determined.
    #[error("Could not detect the active Wi-Fi network")]
    NoActiveNetwork,

    /// A requested SSID is not present in the OS network list.
    #[error("Network not found: {0}")]
    NetworkNotFound(String),

    /// Wraps [`std::io::Error`] (file/network/native-messaging I/O).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Wraps [`serde_json::Error`] (JSON parse/serialize).
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Wraps [`base64::DecodeError`] (malformed base64 input).
    #[error("Base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
}

/// Convenience alias so call sites read as `Result<T>` (with [`CoreError`] as
/// the default error).
pub type Result<T, E = CoreError> = std::result::Result<T, E>;
