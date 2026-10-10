//! Requires only inactive fixture profiles in an isolated NetworkManager
//! container, not Wi-Fi hardware. See scripts/test-linux-nmcli.sh.
#![cfg(target_os = "linux")]

use qr_wifi_core::{default_adapter, parse_payload, share_ssid, WifiCredentials, WifiSecurity};

#[test]
#[ignore = "requires the isolated NetworkManager fixture setup"]
fn saved_profiles_preserve_ssids_keys_security_and_hidden_settings() {
    assert_eq!(std::env::var("QR_WIFI_NMCLI_FIXTURES").as_deref(), Ok("1"));
    let adapter = default_adapter();
    let cases = [
        WifiCredentials::new("  카페:Guest\\WiFi  ", WifiSecurity::Wpa2)
            .with_password("  fixture:p\\ass  ")
            .hidden(true),
        WifiCredentials::new("Guest:Open", WifiSecurity::Nopass),
        WifiCredentials::new("Legacy", WifiSecurity::Wep).with_password("abcde"),
        WifiCredentials::new("WPA3:Fixture", WifiSecurity::Wpa3).with_password("fixture-password"),
    ];
    for expected in cases {
        let actual = adapter.credentials(&expected.ssid).unwrap();
        assert_eq!(actual, expected);
        let share = share_ssid(adapter.as_ref(), &expected.ssid).unwrap();
        assert_eq!(parse_payload(&share.payload).unwrap(), expected);
    }
}
