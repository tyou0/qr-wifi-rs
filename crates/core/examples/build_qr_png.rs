//! Build a Wi-Fi QR code from scratch and write it to a PNG file.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p qr-wifi-core --example build_qr_png -- "Guest" "s3cret" out.png
//! ```
//!
//! Examples live in `crates/core/examples/` and are a standard way for a Rust
//! library to show real, runnable usage (like a `README` code block you can
//! execute).

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use base64::{engine::general_purpose::STANDARD, Engine};
use qr_wifi_core::{to_png_base64, WifiCredentials, WifiSecurity};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let (ssid, password, out_path) = match args.as_slice() {
        [_, ssid, password, out_path] => (ssid.clone(), password.clone(), out_path.clone()),
        _ => {
            eprintln!("usage: build_qr_png <SSID> <PASSWORD> <OUTPUT.png>");
            return ExitCode::FAILURE;
        }
    };

    let credentials = WifiCredentials::new(ssid, WifiSecurity::Wpa).with_password(password);

    let payload = qr_wifi_core::build_payload(&credentials);
    println!("Payload: {payload}");

    let png_base64 = match to_png_base64(&payload) {
        Ok(png) => png,
        Err(error) => {
            eprintln!("QR generation failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    let bytes = match STANDARD.decode(png_base64) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("base64 decode failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = fs::write(Path::new(&out_path), bytes) {
        eprintln!("could not write {out_path}: {error}");
        return ExitCode::FAILURE;
    }

    println!("wrote {out_path}");
    ExitCode::SUCCESS
}
