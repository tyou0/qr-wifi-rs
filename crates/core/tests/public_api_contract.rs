//! Public API contract tests for the shared core.
//!
//! These tests model what every frontend depends on: one adapter trait, one
//! service layer, one payload/QR implementation.

use std::sync::Mutex;

use qr_wifi_core::{
    build_payload, connect_payload, decode_image_base64, networks, parse_payload, share_current,
    Result, WifiAdapter, WifiCredentials, WifiNetwork, WifiSecurity,
};

#[derive(Debug)]
struct ContractAdapter {
    current: String,
    connected: Mutex<Vec<WifiCredentials>>,
}

impl ContractAdapter {
    fn new() -> Self {
        Self {
            current: "Home".to_string(),
            connected: Mutex::new(Vec::new()),
        }
    }

    fn connected(&self) -> Vec<WifiCredentials> {
        self.connected.lock().expect("mutex poisoned").clone()
    }
}

impl WifiAdapter for ContractAdapter {
    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        Ok(vec![
            WifiNetwork {
                ssid: "zeta".to_string(),
                security: WifiSecurity::Wpa,
                signal: None,
                active: false,
            },
            WifiNetwork {
                ssid: self.current.clone(),
                security: WifiSecurity::Wpa2,
                signal: Some(90),
                active: true,
            },
            WifiNetwork {
                ssid: "alpha".to_string(),
                security: WifiSecurity::Nopass,
                signal: Some(40),
                active: false,
            },
        ])
    }

    fn current_ssid(&self) -> Result<String> {
        Ok(self.current.clone())
    }

    fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
        Ok(
            WifiCredentials::new(ssid, WifiSecurity::Wpa2)
                .with_password(format!("{ssid}-password")),
        )
    }

    fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
        self.connected
            .lock()
            .expect("mutex poisoned")
            .push(credentials.clone());
        Ok(())
    }
}

#[test]
fn public_networks_contract_is_active_first_then_alphabetical() {
    let adapter = ContractAdapter::new();

    let ssids: Vec<String> = networks(&adapter)
        .unwrap()
        .into_iter()
        .map(|network| network.ssid)
        .collect();

    assert_eq!(ssids, vec!["Home", "alpha", "zeta"]);
}

#[test]
fn public_share_current_contract_returns_payload_and_decodable_png() {
    let adapter = ContractAdapter::new();

    let share = share_current(&adapter).unwrap();
    let from_payload = parse_payload(&share.payload).unwrap();
    let decoded_payload = decode_image_base64(&share.png_base64).unwrap();
    let from_png = parse_payload(&decoded_payload).unwrap();

    assert_eq!(from_payload.ssid, "Home");
    assert_eq!(from_payload.password.as_deref(), Some("Home-password"));
    assert_eq!(from_png, from_payload);
}

#[test]
fn public_connect_payload_contract_parses_then_connects() {
    let adapter = ContractAdapter::new();
    let expected = WifiCredentials::new("Guest", WifiSecurity::Wpa3)
        .with_password("guest-password")
        .hidden(true);
    let payload = build_payload(&expected);

    let connected = connect_payload(&adapter, &payload).unwrap();

    assert_eq!(connected, expected);
    assert_eq!(adapter.connected(), vec![expected]);
}
