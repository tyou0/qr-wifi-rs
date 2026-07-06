//! Build a `WIFI:` payload, then parse it back — a no-I/O round trip that shows
//! the parser/handling of the escaped format.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p qr-wifi-core --example parse_payload -- 'WIFI:T:WPA2;S:Cafe\\;Lounge;P:latte;;'
//! ```

use std::env;
use std::process::ExitCode;

use qr_wifi_core::{build_payload, parse_payload, WifiCredentials, WifiSecurity};

fn main() -> ExitCode {
    let Some(payload) = env::args().nth(1) else {
        // No argument: demonstrate a round trip with tricky characters.
        let creds =
            WifiCredentials::new("Cafe;Lounge", WifiSecurity::Wpa2).with_password(r#"p\;,:"x"#);
        let built = build_payload(&creds);
        println!("built   : {built}");
        match parse_payload(&built) {
            Ok(parsed) => println!("parsed  : {parsed:?}"),
            Err(error) => eprintln!("parse failed: {error}"),
        }
        return ExitCode::SUCCESS;
    };

    println!("input   : {payload}");
    match parse_payload(&payload) {
        Ok(parsed) => {
            println!("ssid    : {}", parsed.ssid);
            println!("security: {}", parsed.security);
            println!("password: {}", parsed.password.as_deref().unwrap_or(""));
            println!("hidden  : {}", parsed.hidden);
        }
        Err(error) => {
            eprintln!("not a valid WIFI: payload: {error}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
