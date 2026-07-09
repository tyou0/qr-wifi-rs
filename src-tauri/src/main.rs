// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri desktop GUI entry point.
//!
//! The frontend is a small vanilla HTML/CSS/JS app (`../../frontend`). Every
//! meaningful action is a Tauri command that delegates to [`qr_wifi_core`], so
//! the GUI shares one implementation with the CLI, TUI, and the browser-host.

use qr_wifi_core::{
    decode_qr_base64, default_adapter, networks, share_current as core_share_current,
    share_custom as core_share_custom, WifiCredentials, WifiNetwork,
};

fn to_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[tauri::command]
fn get_active_ssid() -> Result<String, String> {
    default_adapter().current_ssid().map_err(to_message)
}

#[tauri::command]
fn list_networks() -> Result<Vec<WifiNetwork>, String> {
    networks(default_adapter().as_ref()).map_err(to_message)
}

#[tauri::command]
fn get_credentials(ssid: String) -> Result<WifiCredentials, String> {
    default_adapter().credentials(&ssid).map_err(to_message)
}

#[tauri::command]
fn share_current() -> Result<qr_wifi_core::QrShare, String> {
    let adapter = default_adapter();
    core_share_current(adapter.as_ref()).map_err(to_message)
}

#[tauri::command]
fn share_custom(credentials: WifiCredentials) -> Result<qr_wifi_core::QrShare, String> {
    core_share_custom(&credentials).map_err(to_message)
}

#[tauri::command]
fn connect_network(credentials: WifiCredentials) -> Result<(), String> {
    qr_wifi_core::connect_credentials(default_adapter().as_ref(), &credentials).map_err(to_message)
}

#[tauri::command]
fn decode_qr(image_base64: String) -> Result<WifiCredentials, String> {
    decode_qr_base64(&image_base64).map_err(to_message)
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
