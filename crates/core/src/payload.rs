//! Build and parse `WIFI:` QR payloads.
//!
//! Format: `WIFI:T:<security>;S:<ssid>;P:<password>;H:<true|false>;;`
//! Fields are escaped with a backslash for the characters `\ ; , : "`.
//! This module mirrors the reference implementation in
//! `qr_wifi_bun/src/core/wifiPayload.ts`.

use crate::error::{CoreError, Result};
use crate::types::{WifiCredentials, WifiSecurity};

const PREFIX: &str = "WIFI:";
const CHARS_TO_ESCAPE: &[char] = &['\\', ';', ',', ':', '"'];

/// Escape a single field value for embedding in a `WIFI:` payload.
fn escape_field(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if CHARS_TO_ESCAPE.contains(&ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Unescape a single field value read from a `WIFI:` payload.
fn unescape_field(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        out.push(ch);
    }
    out
}

/// Split the body of a payload on unescaped `;` separators, keeping any
/// backslash-escaped characters intact within a field.
fn split_fields(body: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in body.chars() {
        if escaped {
            current.push('\\');
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == ';' {
            fields.push(std::mem::take(&mut current));
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        fields.push(current);
    }
    fields
}

/// Build a `WIFI:` QR payload string from credentials.
pub fn build_payload(credentials: &WifiCredentials) -> String {
    let mut out = String::from(PREFIX);
    out.push_str("T:");
    out.push_str(credentials.security.as_token());
    out.push_str(";S:");
    out.push_str(&escape_field(&credentials.ssid));
    out.push(';');

    if credentials.security.requires_password() {
        if let Some(password) = &credentials.password {
            if !password.is_empty() {
                out.push_str("P:");
                out.push_str(&escape_field(password));
                out.push(';');
            }
        }
    }

    if credentials.hidden {
        out.push_str("H:true;");
    }
    out.push(';');
    out
}

/// Parse a `WIFI:` QR payload into credentials.
pub fn parse_payload(payload: &str) -> Result<WifiCredentials> {
    let body = payload
        .strip_prefix(PREFIX)
        .ok_or_else(|| CoreError::Payload("not a WIFI: payload".into()))?;

    let mut ssid: Option<String> = None;
    let mut security = WifiSecurity::Wpa;
    let mut password: Option<String> = None;
    let mut hidden = false;

    for field in split_fields(body) {
        if field.len() < 2 || field.as_bytes()[1] != b':' {
            continue;
        }
        let key = field.as_bytes()[0];
        // Safe to slice from byte 2 because the value starts after an ASCII ':'.
        let raw = unescape_field(&field[2..]);
        match key {
            b'T' => security = WifiSecurity::parse(&raw),
            b'S' => ssid = Some(raw),
            b'P' => password = Some(raw),
            b'H' => hidden = raw.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    let ssid = ssid
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CoreError::Payload("payload is missing SSID".into()))?;

    let password = if security == WifiSecurity::Nopass {
        None
    } else {
        password.filter(|p| !p.is_empty())
    };

    Ok(WifiCredentials {
        ssid,
        security,
        password,
        hidden,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(ssid: &str, security: WifiSecurity) -> WifiCredentials {
        WifiCredentials::new(ssid, security)
    }

    #[test]
    fn builds_minimal_payload() {
        let payload = build_payload(&creds("Home", WifiSecurity::Wpa).with_password("secret"));
        assert_eq!(payload, "WIFI:T:WPA;S:Home;P:secret;;");
    }

    #[test]
    fn builds_open_network_without_password() {
        let payload = build_payload(&creds("Open", WifiSecurity::Nopass));
        assert_eq!(payload, "WIFI:T:nopass;S:Open;;");
    }

    #[test]
    fn builds_hidden_network() {
        let payload = build_payload(
            &creds("Hidden", WifiSecurity::Wpa)
                .with_password("pw")
                .hidden(true),
        );
        assert_eq!(payload, "WIFI:T:WPA;S:Hidden;P:pw;H:true;;");
    }

    #[test]
    fn omits_empty_password() {
        let payload = build_payload(&creds("X", WifiSecurity::Wpa).with_password(""));
        assert_eq!(payload, "WIFI:T:WPA;S:X;;");
    }

    #[test]
    fn escapes_special_characters() {
        let payload = build_payload(
            &creds(r#"a\b;c,d:e"f"#, WifiSecurity::Wpa).with_password(r#"p\a;s,s:c"p"#),
        );
        assert_eq!(
            payload,
            r#"WIFI:T:WPA;S:a\\b\;c\,d\:e\"f;P:p\\a\;s\,s\:c\"p;;"#
        );
    }

    #[test]
    fn round_trips_typical_payload() {
        let original = creds("My Wi-Fi", WifiSecurity::Wpa)
            .with_password("p@ssw0rd!")
            .hidden(true);
        let payload = build_payload(&original);
        let parsed = parse_payload(&payload).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn parses_wep_and_open() {
        let wep = parse_payload("WIFI:T:WEP;S:Net;P:abc;;").unwrap();
        assert_eq!(wep.security, WifiSecurity::Wep);
        assert_eq!(wep.password.as_deref(), Some("abc"));

        let open = parse_payload("WIFI:T:nopass;S:Open;;").unwrap();
        assert_eq!(open.security, WifiSecurity::Nopass);
        assert!(open.password.is_none());
    }

    #[test]
    fn parses_unknown_security_as_wpa() {
        // WPA2/WPA3 are recognized; a truly unknown token falls back to WPA.
        assert_eq!(
            parse_payload("WIFI:T:WPA2;S:Net;P:x;;").unwrap().security,
            WifiSecurity::Wpa2
        );
        assert_eq!(
            parse_payload("WIFI:T:WPA3;S:Net;P:x;;").unwrap().security,
            WifiSecurity::Wpa3
        );
        assert_eq!(
            parse_payload("WIFI:T:WPA99;S:Net;P:x;;").unwrap().security,
            WifiSecurity::Wpa
        );
    }

    #[test]
    fn round_trips_special_characters() {
        let original = creds(r#"a\b;c,d:e"f"#, WifiSecurity::Wpa)
            .with_password(r#"p\;,:"w"#)
            .hidden(false);
        let payload = build_payload(&original);
        let parsed = parse_payload(&payload).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn rejects_non_wifi_payload() {
        assert!(parse_payload("https://example.com").is_err());
    }

    #[test]
    fn rejects_missing_ssid() {
        assert!(parse_payload("WIFI:T:WPA;;").is_err());
        assert!(parse_payload("WIFI:T:WPA;S:   ;P:x;;").is_err());
    }

    #[test]
    fn parse_drops_password_for_open_network() {
        let parsed = parse_payload("WIFI:T:nopass;S:Open;P:ignored;;").unwrap();
        assert!(parsed.password.is_none());
    }
}
