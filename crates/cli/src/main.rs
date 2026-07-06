//! QR Wi-Fi command-line interface.
//!
//! A single command with flags for one-shot actions. With no flags it drops
//! into the shared interactive menu (`qr_wifi_tui::run_menu`). Run
//! `qr-wifi --help` for the full flag list.

use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use qr_wifi_core::{
    decode_image_path, default_adapter, parse_payload, WifiAdapter, WifiCredentials, WifiSecurity,
};
use qr_wifi_tui::print_qr;

#[derive(Parser)]
#[command(
    name = "qr-wifi",
    version,
    about = "Share/connect Wi-Fi with QR codes. No flags opens the interactive menu."
)]
struct Cli {
    /// List all Wi-Fi networks known to the OS (active network first).
    #[arg(long)]
    list: bool,

    /// Share the currently connected Wi-Fi.
    #[arg(long)]
    share: bool,

    /// Share a saved network by SSID, or the custom SSID when used with --custom.
    #[arg(long, value_name = "SSID")]
    ssid: Option<String>,

    /// Build a custom QR (requires --ssid; --password optional).
    #[arg(long)]
    custom: bool,

    /// Password for --custom.
    #[arg(long, value_name = "PASSWORD")]
    password: Option<String>,

    /// Security for --custom (WPA | WEP | nopass).
    #[arg(long, default_value = "WPA")]
    security: String,

    /// Mark the custom network as hidden.
    #[arg(long)]
    hidden: bool,

    /// Scan a QR (via --image) and connect. (Live camera scan is in the GUI.)
    #[arg(long)]
    scan: bool,

    /// Alias of --scan.
    #[arg(long)]
    connect: bool,

    /// Image file to decode for --scan/--connect.
    #[arg(long, value_name = "PATH")]
    image: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let adapter = default_adapter();
    match dispatch(&cli, adapter.as_ref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(cli: &Cli, adapter: &dyn WifiAdapter) -> Result<(), String> {
    if cli.list {
        return list_networks(adapter);
    }
    if cli.scan || cli.connect {
        return scan_connect(adapter, cli.image.as_deref());
    }
    if cli.custom {
        let ssid = cli
            .ssid
            .clone()
            .ok_or_else(|| "--custom requires --ssid".to_string())?;
        return build_custom(&ssid, &cli.security, cli.password.as_deref(), cli.hidden);
    }
    if let Some(ssid) = &cli.ssid {
        return share_ssid(adapter, ssid);
    }
    if cli.share {
        return share_current(adapter);
    }

    // No flags: open the interactive menu.
    qr_wifi_tui::run_menu(adapter);
    Ok(())
}

fn list_networks(adapter: &dyn WifiAdapter) -> Result<(), String> {
    let networks = adapter.list_networks().map_err(|e| e.to_string())?;
    if networks.is_empty() {
        println!("No Wi-Fi networks found.");
        return Ok(());
    }
    let ssid_width = column_width(&networks);
    for network in networks {
        let mark = if network.active { "*" } else { " " };
        let ssid = truncate_for_column(&network.ssid, ssid_width);
        let signal = match network.signal {
            Some(value) => format!("{value}%"),
            None => "—".to_string(),
        };
        println!(
            "{mark} {:<width$} {:<6} {:>4}",
            ssid,
            network.security.to_string(),
            signal,
            width = ssid_width
        );
    }
    Ok(())
}

/// Column width for the SSID column: the widest SSID, clamped to a sane range
/// so short lists still line up and very long SSIDs get truncated.
fn column_width(networks: &[qr_wifi_core::WifiNetwork]) -> usize {
    networks
        .iter()
        .map(|n| n.ssid.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, 32)
}

/// Truncate `s` to `width` characters for column display (appending `…` if it
/// was shortened). Shorter strings are left to the formatter to pad.
fn truncate_for_column(s: &str, width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out: String = chars[..width - 1].iter().collect();
    out.push('…');
    out
}

fn share_current(adapter: &dyn WifiAdapter) -> Result<(), String> {
    let ssid = adapter.current_ssid().map_err(|e| e.to_string())?;
    let creds = adapter.credentials(&ssid).map_err(|e| e.to_string())?;
    print_qr(&creds)
}

fn share_ssid(adapter: &dyn WifiAdapter, ssid: &str) -> Result<(), String> {
    let creds = adapter.credentials(ssid).map_err(|e| e.to_string())?;
    print_qr(&creds)
}

fn build_custom(
    ssid: &str,
    security: &str,
    password: Option<&str>,
    hidden: bool,
) -> Result<(), String> {
    let creds = WifiCredentials {
        ssid: ssid.to_string(),
        security: WifiSecurity::parse(security),
        password: password.map(str::to_string).filter(|p| !p.is_empty()),
        hidden,
    };
    print_qr(&creds)
}

fn scan_connect(adapter: &dyn WifiAdapter, image: Option<&str>) -> Result<(), String> {
    let path = image.ok_or_else(|| {
        "--scan/--connect requires --image <path>. Live camera scanning is available in the GUI."
            .to_string()
    })?;
    let payload = decode_image_path(Path::new(path)).map_err(|e| e.to_string())?;
    println!("Scanned: {payload}");
    let creds = parse_payload(&payload).map_err(|e| e.to_string())?;
    adapter.connect(&creds).map_err(|e| e.to_string())?;
    println!("Connected to {}.", creds.ssid);
    Ok(())
}
