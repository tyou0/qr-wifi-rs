use std::path::Path;

use qr_wifi_core::{decode_qr_path, WifiCredentials, WifiSecurity};

#[test]
fn decodes_styled_phone_wifi_qr() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/styled-wifi-qr.jpg");
    let decoded = decode_qr_path(&fixture).expect("styled phone Wi-Fi QR should decode");
    let expected = WifiCredentials::new("ScannerFixture", WifiSecurity::Wpa)
        .with_password("not-a-real-password");
    assert_eq!(decoded, expected);
}

#[test]
fn decodes_inverted_styled_phone_wifi_qr() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/styled-wifi-qr.jpg");
    let image = image::open(fixture).expect("styled fixture should open");
    let inverted = image::DynamicImage::ImageRgba8({
        let mut pixels = image.to_rgba8();
        image::imageops::invert(&mut pixels);
        pixels
    });
    let payload = qr_wifi_core::decode_image(inverted).expect("inverted styled QR should decode");
    assert_eq!(
        payload,
        "WIFI:S:ScannerFixture;T:WPA;P:not-a-real-password;H:false;;"
    );
}
