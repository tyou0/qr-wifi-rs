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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_provides_helpful_message() {
        let err = CoreError::NetworkNotFound("TestNetwork".to_string());
        assert!(err.to_string().contains("TestNetwork"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn command_error_includes_command_name() {
        let err = CoreError::Command {
            command: "networksetup -getairportnetwork".to_string(),
            message: "Invalid argument".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("networksetup"));
        assert!(display.contains("Invalid argument"));
    }

    #[test]
    fn payload_error_includes_detail() {
        let err = CoreError::Payload("Invalid character at position 5".to_string());
        assert!(err.to_string().contains("payload"));
        assert!(err.to_string().contains("position 5"));
    }

    #[test]
    fn no_active_network_has_clear_message() {
        let err = CoreError::NoActiveNetwork;
        assert!(err.to_string().contains("active"));
        assert!(err.to_string().contains("Wi-Fi"));
    }

    #[test]
    fn errors_are_send_and_sync() {
        // This test confirms CoreError can be used across threads
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CoreError>();
    }

    #[test]
    fn io_error_converts_from_std_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let core_err: CoreError = io_err.into();
        assert!(matches!(core_err, CoreError::Io(_)));
        assert!(core_err.to_string().contains("file not found"));
    }

    #[test]
    fn json_error_converts_from_serde() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let core_err: CoreError = json_err.into();
        assert!(matches!(core_err, CoreError::Json(_)));
    }

    #[test]
    fn base64_error_converts_from_base64() {
        use base64::DecodeError;
        let b64_err = DecodeError::InvalidByte(0, b'!');
        let core_err: CoreError = b64_err.into();
        assert!(matches!(core_err, CoreError::Base64(_)));
    }
}
