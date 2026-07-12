//! QR code generation (PNG + compact terminal art) and decoding.
//!
//! The matrix is produced by the [`qrcode`](https://crates.io/crates/qrcode)
//! crate and turned into a PNG or terminal art through its `render` API.
//! Decoding (for camera-scan flows) uses [`rqrr`](https://crates.io/crates/rqrr).

use std::io::Cursor;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{DynamicImage, ImageFormat, ImageReader, Limits, Luma};
use qrcode::render::unicode::Dense1x2;
use qrcode::QrCode;

use crate::error::{CoreError, Result};
use crate::payload::build_payload;
use crate::types::WifiCredentials;

/// Pixels per QR module in the generated PNG.
const MODULE_SCALE: u32 = 8;

/// Limits applied before decoding untrusted camera or extension images.
const MAX_IMAGE_BYTES: u64 = 12 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 8192;
const MAX_IMAGE_ALLOC: u64 = 128 * 1024 * 1024;

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
    if img.width() > MAX_IMAGE_DIMENSION || img.height() > MAX_IMAGE_DIMENSION {
        return Err(CoreError::QrDecode(format!(
            "image dimensions exceed {MAX_IMAGE_DIMENSION}x{MAX_IMAGE_DIMENSION}"
        )));
    }
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
    let estimated_bytes = (image_base64.len() / 4).saturating_mul(3) as u64;
    ensure_image_size(estimated_bytes)?;
    let bytes = STANDARD.decode(image_base64)?;
    decode_image_bytes(&bytes)
}

/// Decode a QR code from an image file on disk.
pub fn decode_image_path(path: &std::path::Path) -> Result<String> {
    ensure_image_size(std::fs::metadata(path)?.len())?;
    let bytes = std::fs::read(path)?;
    decode_image_bytes(&bytes)
}

fn decode_image_bytes(bytes: &[u8]) -> Result<String> {
    ensure_image_size(bytes.len() as u64)?;

    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| CoreError::QrDecode(e.to_string()))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_IMAGE_ALLOC);
    reader.limits(limits);

    let image = reader
        .decode()
        .map_err(|e| CoreError::QrDecode(e.to_string()))?;
    decode_image(image)
}

fn ensure_image_size(size: u64) -> Result<()> {
    if size > MAX_IMAGE_BYTES {
        return Err(CoreError::QrDecode(format!(
            "encoded image is {size} bytes; maximum is {MAX_IMAGE_BYTES}"
        )));
    }
    Ok(())
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

    #[test]
    fn rejects_images_above_encoded_size_limit() {
        let error = ensure_image_size(MAX_IMAGE_BYTES + 1).unwrap_err();
        assert!(error.to_string().contains("maximum"));
    }

    #[test]
    fn rejects_images_above_dimension_limit() {
        let image = DynamicImage::new_luma8(MAX_IMAGE_DIMENSION + 1, 1);
        let error = decode_image(image).unwrap_err();
        assert!(error.to_string().contains("dimensions"));
    }
}
