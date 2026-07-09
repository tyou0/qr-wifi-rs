//! High-level feature functions shared by every frontend.
//!
//! This module is the "application layer" of the crate: it combines the small
//! pure helpers (`payload`, `qr`) with a [`WifiAdapter`] so CLI, TUI, GUI, and
//! browser extension code call the same feature entry points.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::platform::WifiAdapter;
use crate::qr::{credentials_to_qr, decode_image_base64, decode_image_path};
use crate::types::{sort_networks, WifiCredentials, WifiNetwork};
use crate::{parse_payload, Result};

/// A shareable Wi-Fi QR result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QrShare {
    /// Raw `WIFI:` payload embedded in the QR.
    pub payload: String,
    /// PNG image bytes encoded as base64 for web/IPC frontends.
    pub png_base64: String,
}

/// List networks in the canonical order: active first, then alphabetical.
pub fn networks(adapter: &dyn WifiAdapter) -> Result<Vec<WifiNetwork>> {
    let mut networks = adapter.list_networks()?;
    sort_networks(&mut networks);
    Ok(networks)
}

/// Resolve credentials for the currently connected Wi-Fi network.
pub fn current_credentials(adapter: &dyn WifiAdapter) -> Result<WifiCredentials> {
    let ssid = adapter.current_ssid()?;
    adapter.credentials(&ssid)
}

/// Build a QR for the currently connected Wi-Fi network.
pub fn share_current(adapter: &dyn WifiAdapter) -> Result<QrShare> {
    let credentials = current_credentials(adapter)?;
    share_custom(&credentials)
}

/// Build a QR for a saved network by SSID.
pub fn share_ssid(adapter: &dyn WifiAdapter, ssid: &str) -> Result<QrShare> {
    let credentials = adapter.credentials(ssid)?;
    share_custom(&credentials)
}

/// Build a QR from explicit credentials.
pub fn share_custom(credentials: &WifiCredentials) -> Result<QrShare> {
    let (payload, png_base64) = credentials_to_qr(credentials)?;
    Ok(QrShare {
        payload,
        png_base64,
    })
}

/// Parse a raw `WIFI:` payload and connect to that network.
pub fn connect_payload(adapter: &dyn WifiAdapter, payload: &str) -> Result<WifiCredentials> {
    let credentials = parse_payload(payload)?;
    adapter.connect(&credentials)?;
    Ok(credentials)
}

/// Connect to a network from explicit credentials.
pub fn connect_credentials(adapter: &dyn WifiAdapter, credentials: &WifiCredentials) -> Result<()> {
    adapter.connect(credentials)
}

/// Decode a base64 image containing a QR code into Wi-Fi credentials.
pub fn decode_qr_base64(image_base64: &str) -> Result<WifiCredentials> {
    let payload = decode_image_base64(image_base64)?;
    parse_payload(&payload)
}

/// Decode an image file containing a QR code into Wi-Fi credentials.
pub fn decode_qr_path(path: &Path) -> Result<WifiCredentials> {
    let payload = decode_image_path(path)?;
    parse_payload(&payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::WifiSecurity;
    use std::sync::Mutex;

    struct FakeAdapter {
        networks: Vec<WifiNetwork>,
        current: String,
        calls: Mutex<Vec<String>>,
        connected: Mutex<Vec<WifiCredentials>>,
    }

    impl FakeAdapter {
        fn new() -> Self {
            Self {
                networks: vec![
                    network("zeta", false),
                    network("Beta", true),
                    network("alpha", false),
                ],
                current: "Home".into(),
                calls: Mutex::new(Vec::new()),
                connected: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            let guard = match self.calls.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.clone()
        }

        fn connected(&self) -> Vec<WifiCredentials> {
            let guard = match self.connected.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.clone()
        }

        fn push_call(&self, call: impl Into<String>) {
            let mut guard = match self.calls.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.push(call.into());
        }
    }

    impl WifiAdapter for FakeAdapter {
        fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
            Ok(self.networks.clone())
        }

        fn current_ssid(&self) -> Result<String> {
            self.push_call("current_ssid");
            Ok(self.current.clone())
        }

        fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
            self.push_call(format!("credentials:{ssid}"));
            Ok(credentials(ssid))
        }

        fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
            self.push_call(format!("connect:{}", credentials.ssid));
            let mut guard = match self.connected.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.push(credentials.clone());
            Ok(())
        }
    }

    fn credentials(ssid: &str) -> WifiCredentials {
        WifiCredentials::new(ssid, WifiSecurity::Wpa2).with_password(format!("{ssid}-password"))
    }

    fn network(ssid: &str, active: bool) -> WifiNetwork {
        WifiNetwork {
            ssid: ssid.into(),
            security: WifiSecurity::Wpa2,
            signal: None,
            active,
        }
    }

    fn assert_share_matches(share: &QrShare, expected: &WifiCredentials) {
        assert_eq!(share.payload, crate::build_payload(expected));
        assert_eq!(crate::parse_payload(&share.payload).unwrap(), *expected);
        assert_eq!(decode_qr_base64(&share.png_base64).unwrap(), *expected);
    }

    #[test]
    fn networks_sorts_active_first_then_alphabetical() {
        let adapter = FakeAdapter::new();

        let ssids: Vec<_> = networks(&adapter)
            .unwrap()
            .into_iter()
            .map(|network| network.ssid)
            .collect();

        assert_eq!(ssids, vec!["Beta", "alpha", "zeta"]);
    }

    #[test]
    fn share_current_resolves_current_ssid_and_returns_qr() {
        let adapter = FakeAdapter::new();
        let expected = credentials("Home");

        let share = share_current(&adapter).unwrap();

        assert_share_matches(&share, &expected);
        assert_eq!(adapter.calls(), vec!["current_ssid", "credentials:Home"]);
    }

    #[test]
    fn share_ssid_uses_requested_ssid() {
        let adapter = FakeAdapter::new();
        let expected = credentials("Cafe");

        let share = share_ssid(&adapter, "Cafe").unwrap();

        assert_share_matches(&share, &expected);
        assert_eq!(adapter.calls(), vec!["credentials:Cafe"]);
    }

    #[test]
    fn connect_payload_parses_connects_and_returns_credentials() {
        let adapter = FakeAdapter::new();
        let expected = WifiCredentials::new("Guest", WifiSecurity::Wpa3)
            .with_password("guest-password")
            .hidden(true);
        let payload = crate::build_payload(&expected);

        let actual = connect_payload(&adapter, &payload).unwrap();

        assert_eq!(actual, expected);
        assert_eq!(adapter.connected(), vec![expected]);
        assert_eq!(adapter.calls(), vec!["connect:Guest"]);
    }

    #[test]
    fn decode_qr_base64_decodes_generated_qr_to_credentials() {
        let expected = WifiCredentials::new("Guest;Net", WifiSecurity::Wep)
            .with_password("p;w")
            .hidden(true);
        let payload = crate::build_payload(&expected);
        let png_base64 = crate::to_png_base64(&payload).unwrap();

        assert_eq!(decode_qr_base64(&png_base64).unwrap(), expected);
    }
}
