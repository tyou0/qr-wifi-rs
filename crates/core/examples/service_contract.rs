//! Demonstrate the shared service layer without touching real Wi-Fi state.
//!
//! This is the key architecture lesson in executable form: CLI, TUI, GUI, and
//! browser extension code should all call the same functions shown here.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p qr-wifi-core --example service_contract
//! ```

use std::sync::Mutex;

use qr_wifi_core::{
    build_payload, connect_payload, networks, share_current, Result, WifiAdapter, WifiCredentials,
    WifiNetwork, WifiSecurity,
};

#[derive(Debug)]
struct LessonAdapter {
    current: String,
    connected: Mutex<Vec<WifiCredentials>>,
}

impl LessonAdapter {
    fn new() -> Self {
        Self {
            current: "Home".to_string(),
            connected: Mutex::new(Vec::new()),
        }
    }
}

impl WifiAdapter for LessonAdapter {
    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        Ok(vec![
            WifiNetwork {
                ssid: "Guest".to_string(),
                security: WifiSecurity::Wpa2,
                signal: Some(80),
                active: false,
            },
            WifiNetwork {
                ssid: self.current.clone(),
                security: WifiSecurity::Wpa2,
                signal: Some(95),
                active: true,
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

fn main() -> Result<()> {
    let adapter = LessonAdapter::new();

    let names: Vec<String> = networks(&adapter)?
        .into_iter()
        .map(|network| network.ssid)
        .collect();
    println!("networks: {names:?}");

    let share = share_current(&adapter)?;
    println!("share payload: {}", share.payload);
    println!(
        "share png bytes are exposed as base64: {} chars",
        share.png_base64.len()
    );

    let guest = WifiCredentials::new("Guest", WifiSecurity::Wpa2).with_password("Guest-password");
    let payload = build_payload(&guest);
    let connected = connect_payload(&adapter, &payload)?;
    println!("connected from payload: {}", connected.ssid);

    Ok(())
}
