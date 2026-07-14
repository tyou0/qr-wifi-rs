// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri desktop GUI entry point.
//!
//! The frontend is a small vanilla HTML/CSS/JS app (`../../frontend`). Every
//! meaningful action is a Tauri command that delegates to [`qr_wifi_core`], so
//! the GUI shares one implementation with the CLI, TUI, and the browser-host.

mod command_names;

use qr_wifi_core::{
    decode_qr_base64, default_adapter, networks, share_current as core_share_current,
    share_custom as core_share_custom, WifiCredentials, WifiNetwork,
};
use std::net::TcpListener;
use tauri::{
    ipc::CapabilityBuilder, Manager, WebviewUrl, WebviewWindowBuilder,
};
use tiny_http::{Header, Method, Response, Server, StatusCode};

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
    // WKWebView rejects camera access from Tauri's custom `tauri://` scheme.
    // Bind first, before creating the webview, so another local process cannot
    // claim the selected port between discovery and use.
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("failed to bind UI server");
    let port = listener
        .local_addr()
        .expect("failed to read UI server address")
        .port();
    let server = Server::from_listener(listener, None).expect("failed to start UI server");

    tauri::Builder::default()
        .setup(move |app| {
            let asset_resolver = app.asset_resolver();
            std::thread::Builder::new()
                .name("qr-wifi-assets".into())
                .spawn(move || {
                    for request in server.incoming_requests() {
                        if request.method() != &Method::Get {
                            let _ = request.respond(Response::empty(StatusCode(405)));
                            continue;
                        }

                        let path = request
                            .url()
                            .split_once('?')
                            .map_or(request.url(), |(path, _)| path);
                        let Some(asset) = asset_resolver.get(path.to_string()) else {
                            let _ = request.respond(Response::empty(StatusCode(404)));
                            continue;
                        };

                        let mut response = Response::from_data(asset.bytes);
                        if let Ok(header) = Header::from_bytes("Content-Type", asset.mime_type) {
                            response.add_header(header);
                        }
                        if let Some(csp) = asset.csp_header {
                            if let Ok(header) =
                                Header::from_bytes("Content-Security-Policy", csp)
                            {
                                response.add_header(header);
                            }
                        }
                        if let Ok(header) = Header::from_bytes("Cache-Control", "no-store") {
                            response.add_header(header);
                        }
                        let _ = request.respond(response);
                    }
            })?;

            let url: tauri::Url = format!("http://localhost:{port}/").parse()?;
            let capability = command_names::GUI_COMMANDS.iter().fold(
                CapabilityBuilder::new("loopback-ui")
                    .remote(url.to_string())
                    .window("main"),
                |capability, command| {
                    capability.permission(format!("allow-{}", command.replace('_', "-")))
                },
            );
            app.add_capability(capability)?;
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("QR Wi-Fi RS")
                .inner_size(480.0, 720.0)
                .resizable(true)
                .build()?;
            Ok(())
        })
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
