//! Integration test exercising the public library API end-to-end (no OS calls).

use qr_wifi_core::{
    build_payload, decode_image_base64, parse_payload, to_png_base64, WifiCredentials, WifiSecurity,
};

#[test]
fn payload_round_trips() {
    let creds = WifiCredentials::new("Home", WifiSecurity::Wpa)
        .with_password("s3cret")
        .hidden(true);
    let payload = build_payload(&creds);
    assert_eq!(parse_payload(&payload).unwrap(), creds);
}

#[test]
fn qr_encode_then_decode_returns_same_payload() {
    let creds = WifiCredentials::new("Cafe", WifiSecurity::Wpa).with_password("coffee1");
    let payload = build_payload(&creds);
    let png = to_png_base64(&payload).unwrap();
    let decoded_payload = decode_image_base64(&png).unwrap();
    assert_eq!(decoded_payload, payload);
    assert_eq!(parse_payload(&decoded_payload).unwrap(), creds);
}
