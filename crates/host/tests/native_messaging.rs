//! Process-level contract test for the browser Native Messaging host.
//!
//! Unit tests cover framing helpers in `qr-wifi-core`; this test launches the
//! real binary and proves stdin -> Rust core -> stdout works as one system.

use std::io::{Read, Write};
use std::process::{Command, Stdio};

use qr_wifi_core::{Response, ResponseData};

#[test]
fn host_round_trips_a_framed_request() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_qr-wifi-host"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start native host");

    let request = serde_json::json!({
        "command": "share_custom",
        "credentials": {
            "ssid": "Native Host Test",
            "security": "WPA2",
            "password": "test-password",
            "hidden": false
        }
    });
    let request = serde_json::to_vec(&request).unwrap();

    let mut input = child.stdin.take().unwrap();
    input
        .write_all(&(request.len() as u32).to_le_bytes())
        .unwrap();
    input.write_all(&request).unwrap();
    drop(input);

    let mut output = child.stdout.take().unwrap();
    let mut length = [0u8; 4];
    output.read_exact(&mut length).unwrap();
    let mut body = vec![0u8; u32::from_le_bytes(length) as usize];
    output.read_exact(&mut body).unwrap();

    let response: Response = serde_json::from_slice(&body).unwrap();
    assert!(response.ok);
    assert!(matches!(
        response.data,
        Some(ResponseData::Qr { ref payload, ref png_base64 })
            if payload.starts_with("WIFI:T:WPA2;S:Native Host Test;")
                && !png_base64.is_empty()
    ));

    assert!(child.wait().unwrap().success());
}
