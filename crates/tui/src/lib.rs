//! Menu-driven terminal UI for QR Wi-Fi.
//!
//! This is intentionally **not** a full-screen dashboard: each screen is a
//! short text menu that reads from stdin, performs an action, prints the
//! result, then returns to the main menu. Network selection uses the built-in
//! interactive fuzzy finder in [`fuzzy`].
//!
//! The CLI (`qr-wifi`) calls [`run_menu`] when launched with no flags, so the
//! menu is shared between `qr-wifi` and `qr-wifi-tui`.

mod fuzzy;

use std::io::{self, BufRead, Write};

use qr_wifi_core::{
    build_payload, decode_image_path, parse_payload, to_unicode, WifiAdapter, WifiCredentials,
    WifiNetwork, WifiSecurity,
};

/// Print a QR code as terminal art followed by the raw `WIFI:` payload string.
///
/// Shared by the interactive menu and the one-shot CLI actions.
pub fn print_qr(credentials: &WifiCredentials) -> Result<(), String> {
    let payload = build_payload(credentials);
    let art = to_unicode(&payload).map_err(|e| e.to_string())?;
    println!("\n{art}");
    println!("Payload: {payload}");
    Ok(())
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
    match current_credentials(adapter) {
        Ok(creds) => {
            let _ = print_qr(&creds);
        }
        Err(message) => println!("\n{message}"),
    }
}

fn share_by_ssid(adapter: &dyn WifiAdapter) {
    let networks = match adapter.list_networks() {
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
    match adapter.credentials(&ssid) {
        Ok(creds) => {
            let _ = print_qr(&creds);
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
    let _ = print_qr(&creds);
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
            match decode_image_path(std::path::Path::new(&path)) {
                Ok(payload) => connect_from_payload(adapter, &payload),
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

fn connect_from_payload(adapter: &dyn WifiAdapter, payload: &str) {
    println!("\nPayload: {payload}");
    let creds = match parse_payload(payload) {
        Ok(creds) => creds,
        Err(error) => {
            println!("Invalid payload: {error}");
            return;
        }
    };
    match adapter.connect(&creds) {
        Ok(()) => println!("Connected to {}.", creds.ssid),
        Err(error) => println!("Connect failed: {error}"),
    }
}

/// Resolve credentials for the active network, mapping OS errors to messages.
fn current_credentials(adapter: &dyn WifiAdapter) -> Result<WifiCredentials, String> {
    let ssid = adapter.current_ssid().map_err(|e| e.to_string())?;
    adapter.credentials(&ssid).map_err(|e| e.to_string())
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
