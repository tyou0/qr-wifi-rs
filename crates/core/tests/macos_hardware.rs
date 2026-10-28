//! Opt-in, read-only scan and absent-network regression on real macOS.
#![cfg(target_os = "macos")]
use qr_wifi_core::{default_adapter, CoreError, WifiCredentials, WifiSecurity};

#[test]
#[ignore = "requires a real Wi-Fi radio and macOS privacy authorization"]
fn absent_network_must_not_report_connection_success() {
    let adapter = default_adapter();
    let result = adapter.connect(&WifiCredentials::new(
        "qr-wifi-missing-93",
        WifiSecurity::Nopass,
    ));
    assert!(
        matches!(result, Err(CoreError::NetworkUnavailable(_))),
        "absent network must be rejected by a real scan"
    );
}
