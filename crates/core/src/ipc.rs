//! IPC protocol shared by the Native Messaging host and any frontend.
//!
//! Browser extensions cannot call OS Wi-Fi tooling directly, so a small native
//! host executable speaks this JSON protocol with them (and with any other
//! future client). The same [`handle_request`] dispatcher is what every binary
//! uses, keeping the behavior identical across CLI/TUI/GUI/extension.
//!
//! Wire framing follows the
//! [Chrome/Firefox Native Messaging](https://developer.chrome.com/docs/apps/nativeMessaging)
//! convention: a 4-byte little-endian length header followed by UTF-8 JSON.

use std::io::{ErrorKind, Read, Write};

use serde::{Deserialize, Serialize};

use crate::platform::WifiAdapter;
use crate::types::{WifiCredentials, WifiNetwork};
use crate::{service, Result};

/// Chrome permits up to 64 MiB from an extension to a host and 1 MiB in the
/// other direction. Use a smaller request ceiling that still accepts normal
/// camera photos while bounding memory use.
const MAX_REQUEST_BYTES: usize = 16 << 20;
const MAX_RESPONSE_BYTES: usize = 1 << 20;

/// A single request from a client (extension, CLI, GUI, ...).
///
/// Serialized as internally-tagged JSON: `{"command":"current_ssid"}`,
/// `{"command":"get_credentials","ssid":"Home"}`, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Request {
    /// Ask for the currently connected SSID.
    CurrentSsid,
    /// Ask for the OS network list.
    ListNetworks,
    /// Ask for saved credentials of one SSID.
    GetCredentials {
        /// The SSID to look up.
        ssid: String,
    },
    /// Build a QR for the active network (payload + PNG).
    ShareCurrent,
    /// Build a QR from explicit credentials.
    ShareCustom {
        /// The credentials to encode.
        credentials: WifiCredentials,
    },
    /// Connect to a network from explicit credentials.
    Connect {
        /// The credentials to connect with.
        credentials: WifiCredentials,
    },
    /// Parse a raw `WIFI:` payload and connect.
    ConnectPayload {
        /// The `WIFI:T:...;S:...;...` string.
        payload: String,
    },
    /// Decode a QR from a base64 image and return the parsed credentials.
    DecodeQr {
        /// Base64-encoded image bytes (e.g. a captured PNG).
        image_base64: String,
    },
}

/// Typed response payload, tagged by `kind` on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseData {
    /// The current SSID.
    Ssid {
        /// The active network name.
        ssid: String,
    },
    /// The OS network list.
    Networks {
        /// Networks, active one first.
        networks: Vec<WifiNetwork>,
    },
    /// Saved credentials for a network.
    Credentials {
        /// The resolved credentials (password may be `None`).
        credentials: WifiCredentials,
    },
    /// A rendered QR (matrix image + raw payload).
    Qr {
        /// The `WIFI:` payload string.
        payload: String,
        /// The QR image as base64 PNG.
        png_base64: String,
    },
    /// A QR decoded back into credentials.
    Decoded {
        /// The credentials parsed from the QR.
        credentials: WifiCredentials,
    },
    /// A connection succeeded.
    Connected,
}

/// Envelope wrapping either success data or an error message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// `true` on success, `false` when `error` is set.
    pub ok: bool,
    /// Success payload, present only when `ok` is `true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ResponseData>,
    /// Human-readable error, present only when `ok` is `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    /// Build a success response carrying `data`.
    pub fn ok(data: ResponseData) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    /// Build an error response carrying `message`.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// Dispatch a request against a Wi-Fi adapter, returning a typed response.
pub fn handle_request(request: &Request, adapter: &dyn WifiAdapter) -> Response {
    match request {
        Request::CurrentSsid => match adapter.current_ssid() {
            Ok(ssid) => Response::ok(ResponseData::Ssid { ssid }),
            Err(e) => Response::error(e.to_string()),
        },
        Request::ListNetworks => match service::networks(adapter) {
            Ok(networks) => Response::ok(ResponseData::Networks { networks }),
            Err(e) => Response::error(e.to_string()),
        },
        Request::GetCredentials { ssid } => match adapter.credentials(ssid) {
            Ok(credentials) => Response::ok(ResponseData::Credentials { credentials }),
            Err(e) => Response::error(e.to_string()),
        },
        Request::ShareCurrent => match service::share_current(adapter) {
            Ok(share) => qr_response(share),
            Err(e) => Response::error(e.to_string()),
        },
        Request::ShareCustom { credentials } => match service::share_custom(credentials) {
            Ok(share) => qr_response(share),
            Err(e) => Response::error(e.to_string()),
        },
        Request::Connect { credentials } => {
            match service::connect_credentials(adapter, credentials) {
                Ok(()) => Response::ok(ResponseData::Connected),
                Err(e) => Response::error(e.to_string()),
            }
        }
        Request::ConnectPayload { payload } => match service::connect_payload(adapter, payload) {
            Ok(_) => Response::ok(ResponseData::Connected),
            Err(e) => Response::error(e.to_string()),
        },
        Request::DecodeQr { image_base64 } => match service::decode_qr_base64(image_base64) {
            Ok(credentials) => Response::ok(ResponseData::Decoded { credentials }),
            Err(e) => Response::error(e.to_string()),
        },
    }
}

fn qr_response(share: service::QrShare) -> Response {
    Response::ok(ResponseData::Qr {
        payload: share.payload,
        png_base64: share.png_base64,
    })
}

/// Read one length-prefixed Native Messaging message.
///
/// Returns `Ok(None)` on a clean EOF (the client closed the stream).
pub fn read_message<R: Read>(reader: &mut R) -> Result<Option<Request>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let length = u32::from_le_bytes(len_buf) as usize;
    if length == 0 {
        return Ok(None);
    }
    if length > MAX_REQUEST_BYTES {
        return Err(crate::error::CoreError::Payload(format!(
            "native request too large: {length} bytes (max {MAX_REQUEST_BYTES})"
        )));
    }
    let mut buffer = vec![0u8; length];
    reader.read_exact(&mut buffer)?;
    let request = serde_json::from_slice(&buffer)?;
    Ok(Some(request))
}

/// Write one length-prefixed Native Messaging message.
pub fn write_message<W: Write>(writer: &mut W, response: &Response) -> Result<()> {
    let json = serde_json::to_vec(response)?;
    if json.len() > MAX_RESPONSE_BYTES {
        return Err(crate::error::CoreError::Payload(format!(
            "native response too large: {} bytes (max {MAX_RESPONSE_BYTES})",
            json.len()
        )));
    }
    let length = (json.len() as u32).to_le_bytes();
    writer.write_all(&length)?;
    writer.write_all(&json)?;
    writer.flush()?;
    Ok(())
}

/// Convenience helper for host binaries: keep reading requests and writing
/// responses until the client closes the stream.
pub fn run_loop<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    adapter: &dyn WifiAdapter,
) -> Result<()> {
    while let Some(request) = read_message(reader)? {
        let response = handle_request(&request, adapter);
        write_message(writer, &response)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;
    use crate::types::{WifiCredentials, WifiSecurity};
    use std::sync::Mutex;

    /// A deterministic adapter used to exercise `handle_request` without the OS.
    struct MockAdapter {
        current: String,
        password: Option<String>,
        calls: Mutex<Vec<String>>,
    }

    impl WifiAdapter for MockAdapter {
        fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
            Ok(vec![WifiNetwork {
                ssid: self.current.clone(),
                security: WifiSecurity::Wpa,
                signal: Some(70),
                active: true,
            }])
        }
        fn current_ssid(&self) -> Result<String> {
            Ok(self.current.clone())
        }
        fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
            self.calls.lock().unwrap().push(format!("creds:{ssid}"));
            Ok(WifiCredentials {
                ssid: ssid.to_string(),
                security: WifiSecurity::Wpa,
                password: self.password.clone(),
                hidden: false,
            })
        }
        fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("connect:{}", credentials.ssid));
            Ok(())
        }
    }

    fn make_adapter() -> MockAdapter {
        MockAdapter {
            current: "Home".into(),
            password: Some("secret".into()),
            calls: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn handles_current_ssid() {
        let adapter = make_adapter();
        let resp = handle_request(&Request::CurrentSsid, &adapter);
        assert!(resp.ok);
        assert!(matches!(resp.data, Some(ResponseData::Ssid { ref ssid }) if ssid == "Home"));
    }

    #[test]
    fn handles_list_networks() {
        let adapter = make_adapter();
        let resp = handle_request(&Request::ListNetworks, &adapter);
        assert!(resp.ok);
        assert!(matches!(resp.data, Some(ResponseData::Networks { .. })));
    }

    #[test]
    fn share_current_returns_payload_and_png() {
        let adapter = make_adapter();
        let resp = handle_request(&Request::ShareCurrent, &adapter);
        assert!(resp.ok);
        match resp.data {
            Some(ResponseData::Qr {
                payload,
                png_base64,
            }) => {
                assert!(payload.starts_with("WIFI:T:WPA;S:Home;P:secret;"));
                assert!(!png_base64.is_empty());
            }
            other => panic!("unexpected data {other:?}"),
        }
    }

    #[test]
    fn share_current_reports_error_when_no_network() {
        struct NoneAdapter;
        impl WifiAdapter for NoneAdapter {
            fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
                Ok(Vec::new())
            }
            fn current_ssid(&self) -> Result<String> {
                Err(CoreError::NoActiveNetwork)
            }
            fn credentials(&self, _: &str) -> Result<WifiCredentials> {
                Err(CoreError::NoActiveNetwork)
            }
            fn connect(&self, _: &WifiCredentials) -> Result<()> {
                Ok(())
            }
        }
        let resp = handle_request(&Request::ShareCurrent, &NoneAdapter);
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("Wi-Fi"));
    }

    #[test]
    fn request_round_trips_through_json() {
        let req = Request::GetCredentials {
            ssid: "Cafe".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"command\":\"get_credentials\""));
        let back: Request = serde_json::from_str(&json).unwrap();
        match back {
            Request::GetCredentials { ssid } => assert_eq!(ssid, "Cafe"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn native_messaging_framing_round_trips() {
        let response = Response::ok(ResponseData::Ssid {
            ssid: "Home".into(),
        });

        let mut buffer = Vec::new();
        write_message(&mut buffer, &response).unwrap();

        let len = u32::from_le_bytes(buffer[..4].try_into().unwrap()) as usize;
        assert_eq!(len, buffer.len() - 4);
        let parsed: Response = serde_json::from_slice(&buffer[4..]).unwrap();
        assert!(parsed.ok);
    }

    #[test]
    fn read_message_returns_none_at_eof() {
        let mut empty = std::io::Cursor::new(Vec::<u8>::new());
        assert!(read_message(&mut empty).unwrap().is_none());
    }

    #[test]
    fn read_message_rejects_oversized_frame() {
        // Regression for the host DoS fix: an attacker-controlled length header
        // above the Native Messaging ceiling must be rejected *before* we ever
        // allocate a buffer for it.
        let oversize: u32 = (MAX_REQUEST_BYTES + 1) as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&oversize.to_le_bytes());
        let mut cursor = std::io::Cursor::new(bytes);
        assert!(read_message(&mut cursor).is_err());
    }

    #[test]
    fn write_message_rejects_oversized_response() {
        let response = Response::error("x".repeat(MAX_RESPONSE_BYTES));
        let mut bytes = Vec::new();
        assert!(write_message(&mut bytes, &response).is_err());
        assert!(bytes.is_empty());
    }
}
