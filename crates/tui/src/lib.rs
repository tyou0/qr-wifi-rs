//! Menu-driven terminal UI for QR Wi-Fi.
//!
//! This is intentionally **not** a full-screen dashboard: each screen is a
//! short text menu that reads from stdin, performs an action, prints the
//! result, then returns to the main menu. Network selection uses a built-in
//! interactive fuzzy finder (falls back to a numbered list when `fzf` is not
//! available).
//!
//! The CLI (`qr-wifi`) calls [`run_menu`] when launched with no flags, so the
//! menu is shared between `qr-wifi` and `qr-wifi-tui`.

mod fuzzy;

use std::io::{self, BufRead, Write};

use qr_wifi_core::{
    build_payload, connect_credentials, connect_payload, decode_qr_path, networks,
    share_current as core_share_current, share_custom as core_share_custom,
    share_ssid as core_share_ssid, to_unicode, WifiAdapter, WifiCredentials, WifiNetwork,
    WifiSecurity,
};

/// Print a raw `WIFI:` payload as terminal QR art.
pub fn print_payload(payload: &str) -> Result<(), String> {
    let art = to_unicode(payload).map_err(|e| e.to_string())?;
    println!("\n{art}");
    println!("Payload: {payload}");
    Ok(())
}

/// Print a QR code as terminal art followed by the raw `WIFI:` payload string.
///
/// Shared by the interactive menu and the one-shot CLI actions.
pub fn print_qr(credentials: &WifiCredentials) -> Result<(), String> {
    let payload = build_payload(credentials);
    print_payload(&payload)
}

/// Run the interactive main menu until the user quits.
pub fn run_menu(adapter: &dyn WifiAdapter) {
    loop {
        clear();
        println!("╭─ QR Wi-Fi RS ───────────────────────────────╮");
        println!("│                                            │");
        println!("│  1) Share current Wi-Fi                    │");
        println!("│  2) Share by SSID  (fzf finder)            │");
        println!("│  3) Custom QR code                         │");
        println!("│  4) Connect / scan QR                      │");
        println!("│  5) Quit                                   │");
        println!("│                                            │");
        println!("╰────────────────────────────────────────────╯");

        match prompt("\nChoice").as_str() {
            "1" => share_current(adapter),
            "2" => share_by_ssid(adapter),
            "3" => custom_qr(),
            "4" => scan_connect(adapter),
            "5" | "q" | "quit" | "exit" => break,
            other => println!("\nInvalid choice {other:?}."),
        }
        pause();
    }
    clear();
    println!("Bye.");
}

fn share_current(adapter: &dyn WifiAdapter) {
    match core_share_current(adapter) {
        Ok(share) => {
            let _ = print_payload(&share.payload);
        }
        Err(error) => println!("\n{error}"),
    }
}

fn share_by_ssid(adapter: &dyn WifiAdapter) {
    let networks = match networks(adapter) {
        Ok(networks) => networks,
        Err(error) => {
            println!("\nCould not list networks: {error}");
            return;
        }
    };
    if networks.is_empty() {
        println!("\nNo networks found.");
        return;
    }
    let Some(ssid) = pick_network(&networks) else {
        println!("\nCancelled.");
        return;
    };
    match core_share_ssid(adapter, &ssid) {
        Ok(share) => {
            let _ = print_payload(&share.payload);
        }
        Err(error) => println!("\nCould not read credentials for {ssid}: {error}"),
    }
}

fn custom_qr() {
    let ssid = prompt("\nSSID");
    if ssid.is_empty() {
        println!("\nSSID is required.");
        return;
    }
    let security = {
        let raw = prompt("Security [WPA/WPA2/WPA3/WEP/nopass] (default WPA)");
        if raw.is_empty() {
            WifiSecurity::Wpa
        } else {
            WifiSecurity::parse(&raw)
        }
    };
    let password = if security == WifiSecurity::Nopass {
        None
    } else {
        let raw = prompt("Password");
        if raw.is_empty() {
            None
        } else {
            Some(raw)
        }
    };
    let hidden = prompt("Hidden? [y/N]").eq_ignore_ascii_case("y");

    let creds = WifiCredentials {
        ssid,
        security,
        password,
        hidden,
    };
    match core_share_custom(&creds) {
        Ok(share) => {
            let _ = print_payload(&share.payload);
        }
        Err(error) => println!("\nCould not build QR: {error}"),
    }
}

fn scan_connect(adapter: &dyn WifiAdapter) {
    println!("\nConnect options:");
    println!("  1) Decode a QR from an image file");
    println!("  2) Paste a WIFI: payload");
    match prompt("Choice").as_str() {
        "1" => {
            let path = prompt("Image path");
            if path.is_empty() {
                println!("\nCancelled.");
                return;
            }
            match decode_qr_path(std::path::Path::new(&path)) {
                Ok(creds) => connect_from_credentials(adapter, &creds),
                Err(error) => println!("\nDecode failed: {error}"),
            }
        }
        "2" => {
            let payload = prompt("WIFI: payload");
            if payload.is_empty() {
                println!("\nCancelled.");
                return;
            }
            connect_from_payload(adapter, &payload);
        }
        _ => println!("\nCancelled."),
    }
}

fn connect_from_credentials(adapter: &dyn WifiAdapter, creds: &WifiCredentials) {
    match connect_credentials(adapter, creds) {
        Ok(()) => println!("Connected to {}.", creds.ssid),
        Err(error) => println!("Connect failed: {error}"),
    }
}

fn connect_from_payload(adapter: &dyn WifiAdapter, payload: &str) {
    println!("\nPayload: {payload}");
    match connect_payload(adapter, payload) {
        Ok(creds) => println!("Connected to {}.", creds.ssid),
        Err(error) => println!("Connect failed: {error}"),
    }
}

/// Open the built-in fuzzy finder over the network list and return the chosen
/// SSID (or `None` if cancelled).
fn pick_network(networks: &[WifiNetwork]) -> Option<String> {
    let entries: Vec<fuzzy::Entry> = networks
        .iter()
        .map(|network| fuzzy::Entry {
            label: if network.active {
                format!("* {}", network.ssid)
            } else {
                network.ssid.clone()
            },
            value: network.ssid.clone(),
        })
        .collect();
    fuzzy::pick(&entries, "Select SSID")
}

fn clear() {
    // ANSI clear-screen + move cursor home. Not an alternate-screen takeover.
    print!("\x1b[2J\x1b[H");
    let _ = io::stdout().flush();
}

fn prompt(label: &str) -> String {
    print!("{label}: ");
    let _ = io::stdout().flush();
    read_line()
}

fn read_line() -> String {
    let mut buffer = String::new();
    let _ = io::stdin().lock().read_line(&mut buffer);
    buffer.trim().to_string()
}

fn pause() {
    println!("\n— press Enter to return to the menu —");
    read_line();
}
