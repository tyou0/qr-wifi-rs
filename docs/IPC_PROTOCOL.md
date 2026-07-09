# QR Wi-Fi RS — Native Messaging IPC Protocol

This document explains the Native Messaging protocol used between the browser extension and the `qr-wifi-host` binary.

## Table of Contents

- [Overview](#overview)
- [Native Messaging Format](#native-messaging-format)
- [Protocol Messages](#protocol-messages)
- [Request Types](#request-types)
- [Response Types](#response-types)
- [Error Handling](#error-handling)
- [Implementation](#implementation)
- [Security Considerations](#security-considerations)

---

## Overview

Browser extensions run in a sandbox and cannot directly execute system commands or access OS APIs. To bridge this gap, browsers support **Native Messaging** — a standardized protocol for communicating with native binaries.

### How It Works

```
┌─────────────────┐         Native Messaging          ┌──────────────────┐
│  Browser        │  ────────────────────────────────> │  qr-wifi-host    │
│  Extension      │  length-prefixed JSON on stdio    │  (Rust binary)   │
└─────────────────┘                                     └────────┬─────────┘
                                                                │
                                                                ↓
                                                        ┌───────────────┐
                                                        │ qr-wifi-core  │
                                                        │   (logic)     │
                                                        └───────┬───────┘
                                                                │
                                                                ↓
                                                        ┌───────────────┐
                                                        │  OS (WiFi)    │
                                                        └───────────────┘
```

1. Extension sends a JSON request to `qr-wifi-host`
2. `qr-wifi-host` processes the request using `qr-wifi-core`
3. `qr-wifi-host` sends a JSON response back to the extension
4. Extension displays the result

---

## Native Messaging Format

The Native Messaging protocol is defined by Chrome/Chromium and Firefox:

### Message Structure

Each message consists of:

1. **4-byte length header** (little-endian 32-bit integer)
2. **JSON payload** (UTF-8 encoded, length = value from header)

```
[4 bytes: length][JSON body]
```

### Example

For the JSON `{"command":"share_current"}` (24 bytes):

```
Bytes 0-3:   18 00 00 00  (little-endian for 24)
Bytes 4-27:  {"command":"share_current"}
```

### Constraints

- Maximum message size: **1 MB** (Chrome/Firefox limit)
- Length header is **unsigned 32-bit little-endian**
- JSON must be valid UTF-8

---

## Protocol Messages

QR Wi-Fi RS uses a simple request/response pattern:

### Request Format

```json
{
  "command": "command_name",
  ...command-specific fields...
}
```

### Response Envelope

All responses are wrapped in an envelope:

```json
{
  "ok": true,
  "data": {
    "kind": "response_kind",
    ...response-specific fields...
  }
}
```

Or on error:

```json
{
  "ok": false,
  "error": "Error message describing what went wrong"
}
```

---

## Request Types

### 1. Current SSID

Get the currently connected network's SSID.

**Request:**
```json
{
  "command": "current_ssid"
}
```

**Response:**
```json
{
  "ok": true,
  "data": {
    "kind": "ssid",
    "ssid": "MyNetwork"
  }
}
```

### 2. List Networks

Get all saved/visible Wi-Fi networks.

**Request:**
```json
{
  "command": "list_networks"
}
```

**Response:**
```json
{
  "ok": true,
  "data": {
    "kind": "networks",
    "networks": [
      {
        "ssid": "MyNetwork",
        "security": "Wpa2",
        "signal": null,
        "active": true
      },
      {
        "ssid": "Guest",
        "security": "Wpa",
        "signal": null,
        "active": false
      }
    ]
  }
}
```

### 3. Get Credentials

Retrieve saved credentials for a specific SSID.

**Request:**
```json
{
  "command": "get_credentials",
  "ssid": "MyNetwork"
}
```

**Response:**
```json
{
  "ok": true,
  "data": {
    "kind": "credentials",
    "credentials": {
      "ssid": "MyNetwork",
      "security": "Wpa2",
      "password": "mysecretpassword",
      "hidden": false
    }
  }
}
```

### 4. Share Current

Generate a QR for the currently connected network.

**Request:**
```json
{
  "command": "share_current"
}
```

**Response:**
```json
{
  "ok": true,
  "data": {
    "kind": "qr",
    "payload": "WIFI:T:WPA2;S:MyNetwork;P:mysecretpassword;;",
    "png_base64": "iVBORw0KGgoAAAANSUhEUgAA..."
  }
}
```

### 5. Share Custom

Generate a QR from explicit credentials.

**Request:**
```json
{
  "command": "share_custom",
  "credentials": {
    "ssid": "Guest",
    "security": "Wpa",
    "password": "guestpass",
    "hidden": false
  }
}
```

**Response:** (same as Share Current)

### 6. Connect

Connect to a network using explicit credentials.

**Request:**
```json
{
  "command": "connect",
  "credentials": {
    "ssid": "Guest",
    "security": "Wpa",
    "password": "guestpass",
    "hidden": false
  }
}
```

**Response:**
```json
{
  "ok": true,
  "data": {
    "kind": "connected"
  }
}
```

### 7. Connect Payload

Parse a `WIFI:` payload and connect.

**Request:**
```json
{
  "command": "connect_payload",
  "payload": "WIFI:T:WPA;S:Guest;P:guestpass;;"
}
```

**Response:**
```json
{
  "ok": true,
  "data": {
    "kind": "connected"
  }
}
```

### 8. Decode QR

Decode a QR image (base64-encoded) into credentials.

**Request:**
```json
{
  "command": "decode_qr",
  "image_base64": "iVBORw0KGgoAAAANSUhEUgAA..."
}
```

**Response:**
```json
{
  "ok": true,
  "data": {
    "kind": "decoded",
    "credentials": {
      "ssid": "MyNetwork",
      "security": "Wpa2",
      "password": "mysecretpassword",
      "hidden": false
    }
  }
}
```

---

## Response Types

### Response Data Kinds

| Kind | Fields | Description |
|------|--------|-------------|
| `ssid` | `ssid` | Current network name |
| `networks` | `networks` | List of networks |
| `credentials` | `credentials` | Network credentials |
| `qr` | `payload`, `png_base64` | QR code image and payload |
| `decoded` | `credentials` | Decoded QR credentials |
| `connected` | (none) | Connection succeeded |

### Error Responses

All errors follow the same format:

```json
{
  "ok": false,
  "error": "Human-readable error message"
}
```

Example errors:
```json
{
  "ok": false,
  "error": "No credentials found for SSID: UnknownNetwork"
}
```

---

## Error Handling

### Extension Side

The extension checks `response.ok`:

```javascript
const response = await send({command: "share_current"});
if (!response.ok) {
  throw new Error(response.error ?? "native host error");
}
return response.data;
```

### Host Side

The host converts any `CoreError` to a response:

```rust
match handle_request(request, adapter) {
    Ok(data) => write_message(&Response::ok(data)),
    Err(error) => write_message(&Response::error(error.to_string())),
}
```

---

## Implementation

### Host Loop

The host runs a simple read-process-write loop:

```rust
pub fn run_loop<R: Read, W: Write>(
    input: &mut R,
    output: &mut W,
    adapter: &dyn WifiAdapter,
) -> Result<()> {
    loop {
        // Read length-prefixed JSON
        let request = read_message(input)?;

        // Process request
        let response = handle_request(request, adapter);

        // Write response
        write_message(output, response)?;
    }
}
```

### Reading Messages

```rust
pub fn read_message<R: Read>(input: &mut R) -> Result<Request> {
    // 1. Read 4-byte length header
    let mut len_bytes = [0u8; 4];
    input.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    // 2. Reject if too large
    if len > MAX_MESSAGE_BYTES {
        return Err(CoreError::MessageTooLarge(len));
    }

    // 3. Read JSON body
    let mut buffer = vec![0u8; len];
    input.read_exact(&mut buffer)?;

    // 4. Parse as Request
    Ok(serde_json::from_slice(&buffer)?)
}
```

### Writing Messages

```rust
pub fn write_message<W: Write>(output: &mut W, response: Response) -> Result<()> {
    // 1. Serialize to JSON
    let json = serde_json::to_vec(&response)?;

    // 2. Write length header
    let len = json.len() as u32;
    output.write_all(&len.to_le_bytes())?;

    // 3. Write JSON body
    output.write_all(&json)?;

    // 4. Flush
    output.flush()?;

    Ok(())
}
```

---

## Security Considerations

### Manifest Validation

The browser only connects to the host if:

1. The manifest file name matches the extension's `allowed_extensions` / `allowed_origins`
2. The binary path in the manifest exists and is executable
3. The extension ID matches what's in the manifest

### Message Size Limit

The host enforces a 1 MB message limit to prevent memory exhaustion attacks:

```rust
const MAX_MESSAGE_BYTES: usize = 1 << 20; // 1 MB
```

### Privilege Level

`qr-wifi-host` runs with the **same privileges** as the browser process. This means:
- On macOS/Linux: User-level only (no `sudo` required)
- On Windows: Standard user permissions

The host can only do what the user can do (e.g., it can't access admin-only networks).

### No Authentication

Native Messaging has **no built-in authentication** — any extension with the correct ID can talk to the host. This is by design (the browser checks the extension ID before connecting).

### Sensitive Data

The protocol transmits **Wi-Fi passwords** in plaintext (as part of `WIFI:` payloads and credential objects). This is acceptable because:

1. Communication is over localhost (stdin/stdout)
2. Only authorized extensions can connect
3. The OS already requires user permission for native messaging

---

## Summary

The Native Messaging protocol in QR Wi-Fi RS:

1. **Uses standard Chrome/Firefox format** (length-prefixed JSON)
2. **Defines request/response types** for all Wi-Fi operations
3. **Handles errors consistently** with `{ok: false, error: "..."}`
4. **Enforces size limits** for safety
5. **Relies on browser security** for extension authorization

This design keeps the protocol simple, debuggable, and compatible with all major browsers.
