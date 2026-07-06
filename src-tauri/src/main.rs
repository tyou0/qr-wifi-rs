// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri desktop GUI entry point.
//!
//! The frontend is a small vanilla HTML/CSS/JS app (`../../frontend`). Every
//! meaningful action is a Tauri command that delegates to [`qr_wifi_core`], so
//! the GUI shares one implementation with the CLI, TUI, and the browser-host.

use qr_wifi_core::{
    credentials_to_qr, decode_image_base64, default_adapter, parse_payload, WifiCredentials,
    WifiNetwork,
};
use serde::Serialize;

/// QR result returned to the frontend: the matrix image (base64 PNG) plus the
/// raw `WIFI:` payload string shown beneath it.
#[derive(Debug, Serialize)]
struct QrResult {
    payload: String,
    png_base64: String,
}

fn to_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[tauri::command]
fn get_active_ssid() -> Result<String, String> {
    default_adapter().current_ssid().map_err(to_message)
}

#[tauri::command]
fn list_networks() -> Result<Vec<WifiNetwork>, String> {
    default_adapter().list_networks().map_err(to_message)
}

#[tauri::command]
fn get_credentials(ssid: String) -> Result<WifiCredentials, String> {
    default_adapter().credentials(&ssid).map_err(to_message)
}

#[tauri::command]
fn share_current() -> Result<QrResult, String> {
    let adapter = default_adapter();
    let ssid = adapter.current_ssid().map_err(to_message)?;
    let credentials = adapter.credentials(&ssid).map_err(to_message)?;
    render(credentials)
}

#[tauri::command]
fn share_custom(credentials: WifiCredentials) -> Result<QrResult, String> {
    render(credentials)
}

fn render(credentials: WifiCredentials) -> Result<QrResult, String> {
    let (payload, png_base64) = credentials_to_qr(&credentials).map_err(to_message)?;
    Ok(QrResult {
        payload,
        png_base64,
    })
}

#[tauri::command]
fn connect_network(credentials: WifiCredentials) -> Result<(), String> {
    default_adapter().connect(&credentials).map_err(to_message)
}

#[tauri::command]
fn decode_qr(image_base64: String) -> Result<WifiCredentials, String> {
    let payload = decode_image_base64(&image_base64).map_err(to_message)?;
    parse_payload(&payload).map_err(to_message)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_active_ssid,
            list_networks,
            get_credentials,
            share_current,
            share_custom,
            connect_network,
            decode_qr,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
