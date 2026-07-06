//! QR code generation (PNG + compact terminal art) and decoding.
//!
//! The matrix is produced by the [`qrcode`](https://crates.io/crates/qrcode)
//! crate and turned into a PNG or terminal art through its `render` API.
//! Decoding (for camera-scan flows) uses [`rqrr`](https://crates.io/crates/rqrr).

use std::io::Cursor;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{DynamicImage, ImageFormat, Luma};
use qrcode::render::unicode::Dense1x2;
use qrcode::QrCode;

use crate::error::{CoreError, Result};
use crate::payload::build_payload;
use crate::types::WifiCredentials;

/// Pixels per QR module in the generated PNG.
const MODULE_SCALE: u32 = 8;

/// Build a QR matrix for the given payload.
fn build_matrix(payload: &str) -> Result<QrCode> {
    QrCode::new(payload.as_bytes()).map_err(|e| CoreError::QrGen(e.to_string()))
}

/// Build the `WIFI:` payload and render it to a base64 PNG in one step.
///
/// This is the single shared "share" entry point used by the IPC host, the
/// Tauri GUI, and any other frontend, so the QR/payload pair is built in one
/// place.
pub fn credentials_to_qr(credentials: &WifiCredentials) -> Result<(String, String)> {
    let payload = build_payload(credentials);
    let png_base64 = to_png_base64(&payload)?;
    Ok((payload, png_base64))
}

/// Render a payload to PNG bytes (including a quiet zone).
pub fn to_png(payload: &str) -> Result<Vec<u8>> {
    let code = build_matrix(payload)?;
    let image = code
        .render::<Luma<u8>>()
        .module_dimensions(MODULE_SCALE, MODULE_SCALE)
        .quiet_zone(true)
        .build();

    let mut buffer = Vec::with_capacity(16 * 1024);
    DynamicImage::ImageLuma8(image)
        .write_to(&mut Cursor::new(&mut buffer), ImageFormat::Png)
        .map_err(|e| CoreError::QrGen(e.to_string()))?;
    Ok(buffer)
}

/// Render a payload to PNG and return it as base64 (handy for IPC/web).
pub fn to_png_base64(payload: &str) -> Result<String> {
    let bytes = to_png(payload)?;
    Ok(STANDARD.encode(&bytes))
}

/// Render a payload to compact terminal art using Unicode half-blocks.
///
/// Each printed character encodes two matrix rows, which keeps the output
/// small and close to square on most terminals.
pub fn to_unicode(payload: &str) -> Result<String> {
    let code = build_matrix(payload)?;
    Ok(code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Dark)
        .light_color(Dense1x2::Light)
        .module_dimensions(2, 1)
        .build())
}

/// Decode the first QR code found in an image.
pub fn decode_image(img: DynamicImage) -> Result<String> {
    let mut prepared = rqrr::PreparedImage::prepare(img.to_luma8());
    let grids = prepared.detect_grids();
    let (_meta, content) = grids
        .into_iter()
        .next()
        .ok_or_else(|| CoreError::QrDecode("no QR code found in image".into()))?
        .decode()
        .map_err(|e| CoreError::QrDecode(e.to_string()))?;
    Ok(content)
}

/// Decode a QR code from base64-encoded image bytes (e.g. a PNG captured by a
/// webcam and sent over IPC).
pub fn decode_image_base64(image_base64: &str) -> Result<String> {
    let bytes = STANDARD.decode(image_base64)?;
    let img = image::load_from_memory(&bytes).map_err(|e| CoreError::QrDecode(e.to_string()))?;
    decode_image(img)
}

/// Decode a QR code from an image file on disk.
pub fn decode_image_path(path: &std::path::Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let img = image::load_from_memory(&bytes).map_err(|e| CoreError::QrDecode(e.to_string()))?;
    decode_image(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &str = "WIFI:T:WPA;S:Home;P:secret;;";

    #[test]
    fn builds_png_for_simple_payload() {
        let bytes = to_png(PAYLOAD).unwrap();
        // PNG magic bytes.
        assert_eq!(
            &bytes[..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        );
    }

    #[test]
    fn base64_round_trips_png() {
        let encoded = to_png_base64(PAYLOAD).unwrap();
        let decoded = STANDARD.decode(encoded).unwrap();
        let direct = to_png(PAYLOAD).unwrap();
        assert_eq!(decoded, direct);
    }

    #[test]
    fn unicode_render_is_non_empty_and_rectangular() {
        let art = to_unicode(PAYLOAD).unwrap();
        assert!(!art.is_empty());
        let widths: Vec<usize> = art.lines().map(str::chars).map(Iterator::count).collect();
        assert!(widths.iter().all(|&w| w == widths[0]));
    }

    #[test]
    fn rejects_oversized_payload() {
        // A payload far beyond QR capacity should error, not panic.
        let huge = "WIFI:T:WPA;S:".to_string() + &"a".repeat(50_000) + ";P:x;;";
        assert!(build_matrix(&huge).is_err());
    }

    #[test]
    fn round_trips_encode_then_decode() {
        // Encode a payload to PNG, then decode it back through rqrr. This is an
        // end-to-end sanity check of the encode/decode pair.
        let png = to_png(PAYLOAD).unwrap();
        let img = image::load_from_memory(&png).unwrap();
        let decoded = decode_image(img).unwrap();
        assert_eq!(decoded, PAYLOAD);
    }
}
