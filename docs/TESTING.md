# QR Wi-Fi RS — Testing Guide

This document explains how to test the QR Wi-Fi RS codebase, including strategies for testing OS-specific code and the fake adapter pattern.

## Table of Contents

- [Overview](#overview)
- [Running Tests](#running-tests)
- [Test Organization](#test-organization)
- [The Fake Adapter Pattern](#the-fake-adapter-pattern)
- [Testing Platform Code](#testing-platform-code)
- [Testing Pure Logic](#testing-pure-logic)
- [Integration Tests](#integration-tests)
- [Testing Strategies by Module](#testing-strategies-by-module)

---

## Overview

QR Wi-Fi RS has two testing challenges:

1. **Platform-specific code:** Can't run macOS commands on Linux, etc.
2. **OS state is external:** Tests shouldn't modify the user's Wi-Fi connections

We solve these with:

1. **Parse-only unit tests** for platform code (test with mock output)
2. **Fake adapter** for integration testing (in-memory implementation)
3. **Pure logic tests** for payload, QR encoding, etc.

---

## Running Tests

### Run All Tests

```bash
cargo test
```

### Run Specific Crate

```bash
cargo test -p qr-wifi-core
cargo test -p qr-wifi-cli
```

### Run Specific Test

```bash
cargo test test_name
```

### Run Tests with Output

```bash
cargo test -- --nocapture  # Show print! output
cargo test -- --show-output  # Show test output
```

---

## Test Organization

```
crates/
├── core/
│   ├── src/
│   │   ├── types.rs         # Has #[cfg(test)] module with tests
│   │   ├── payload.rs       # Has #[cfg(test)] module with tests
│   │   ├── service.rs       # Has #[cfg(test)] module with FakeAdapter
│   │   └── platform/
│   │       ├── macos.rs     # Has #[cfg(test)] parsing tests
│   │       ├── linux.rs     # Has #[cfg(test)] parsing tests
│   │       └── windows.rs   # Has #[cfg(test)] parsing tests
│   └── tests/
│       └── payload_round_trip.rs  # Integration test file
└── tui/
    └── src/
        └── fuzzy.rs        # Has #[cfg(test)] module
```

---

## The Fake Adapter Pattern

Testing code that needs a `WifiAdapter` requires an implementation that doesn't touch the OS. We use a **FakeAdapter** for this.

### Implementation

Located in `crates/core/src/service.rs`:

```rust
struct FakeAdapter {
    networks: Vec<WifiNetwork>,
    current: String,
    calls: Mutex<Vec<String>>,
    connected: Mutex<Vec<WifiCredentials>>,
}

impl WifiAdapter for FakeAdapter {
    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        Ok(self.networks.clone())
    }

    fn current_ssid(&self) -> Result<String> {
        self.push_call("current_ssid");
        Ok(self.current.clone())
    }

    fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
        self.push_call(format!("credentials:{ssid}"));
        Ok(credentials(ssid))
    }

    fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
        self.push_call(format!("connect:{}", credentials.ssid));
        let mut guard = match self.connected.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.push(credentials.clone());
        Ok(())
    }
}
```

### Features

1. **Deterministic:** Returns predefined values
2. **Observable:** Tracks what methods were called
3. **Safe:** Runs entirely in-memory
4. **Thread-aware:** Uses `Mutex` for interior mutability

### Usage in Tests

```rust
#[test]
fn share_current_resolves_current_ssid_and_returns_qr() {
    let adapter = FakeAdapter::new();
    let expected = credentials("Home");

    let share = share_current(&adapter).unwrap();

    assert_share_matches(&share, &expected);
    assert_eq!(adapter.calls(), vec!["current_ssid", "credentials:Home"]);
}
```

### Benefits

- **Fast:** No subprocess calls
- **Deterministic:** Always returns the same data
- **Observable:** Can verify exact call sequence
- **Isolated:** Doesn't affect system state

---

## Testing Platform Code

Platform code (macOS, Linux, Windows) can't run the actual OS commands in tests. Instead, we test the **parsing logic**.

### Example: macOS SSID Parsing

In `crates/core/src/platform/macos.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_current_ssid_from_networksetup() {
        let output = "    Aggregated Wi-Fi";
        assert_eq!(parse_current_ssid(output), Ok("Aggregated Wi-Fi".to_string()));
    }

    #[test]
    fn parse_redacted_ssid() {
        let output = "    <redacted>";
        assert_eq!(parse_current_ssid(output), Err(CoreError::RedactedSsid));
    }

    #[test]
    fn parse_keychain_password() {
        let output = r#"
keychain:  "/Users/user/Library/Keychains/login.keychain-db"
class:   "internet password"
attributes:
    0x00000007 <blob> = "MyNetwork"{ ... length of value)
password: "mysecretpassword"

"#;
        assert_eq!(parse_password(output), Ok("mysecretpassword".to_string()));
    }
}
```

### Example: Linux Network Parsing

In `crates/core/src/platform/linux.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_network_list() {
        let output = "MyNetwork:WPA2:65\nGuest:WPA:40";
        let networks = parse_networks(output).unwrap();
        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].ssid, "MyNetwork");
        assert_eq!(networks[0].security, WifiSecurity::Wpa2);
    }

    #[test]
    fn parse_connection_show() {
        let output = r#"
802-11-wireless-security.psk:mysecretpassword
802-11-wireless.hidden:false
"#;
        assert_eq!(parse_password(output), Ok("mysecretpassword".to_string()));
    }
}
```

### Strategy

1. **Capture real output** from the OS commands on each platform
2. **Write tests** that parse the captured output
3. **Update tests** when OS output format changes

This tests the **parsing logic** without needing the actual OS.

---

## Testing Pure Logic

Modules that don't touch the OS (payload, QR, types) can be tested directly.

### Example: Payload Round-Trip

In `crates/core/src/payload.rs`:

```rust
#[test]
fn round_trip_with_password() {
    let creds = WifiCredentials::new("My Network", WifiSecurity::Wpa2)
        .with_password("test;pass");

    let payload = build_payload(&creds);
    let parsed = parse_payload(&payload).unwrap();

    assert_eq!(parsed, creds);
}

#[test]
fn escaped_semicolon_in_password() {
    let creds = WifiCredentials::new("Test", WifiSecurity::Wpa)
        .with_password("pass;word");

    let payload = build_payload(&creds);
    assert!(payload.contains(r"P:pass\;word"));
}
```

### Example: QR Encoding

In `crates/core/src/qr.rs`:

```rust
#[test]
fn encode_decode_round_trip() {
    let payload = "WIFI:T:WPA;S:Test;P:password;;";
    let png = to_png(payload).unwrap();
    let decoded = decode_image(&png).unwrap();
    assert_eq!(decoded, payload);
}
```

---

## Integration Tests

### File: `crates/core/tests/payload_round_trip.rs`

This is an integration test file that tests the full pipeline:

```rust
use qr_wifi_core::{build_payload, decode_image_base64, parse_payload, to_png_base64, WifiCredentials, WifiSecurity};

#[test]
fn full_round_trip() {
    // 1. Create credentials
    let creds = WifiCredentials::new("Test;Net", WifiSecurity::Wpa3)
        .with_password("p;w")
        .hidden(true);

    // 2. Build payload
    let payload = build_payload(&creds);

    // 3. Generate QR
    let png_base64 = to_png_base64(&payload).unwrap();

    // 4. Decode QR
    let decoded_payload = decode_image_base64(&png_base64).unwrap();

    // 5. Parse payload
    let parsed_creds = parse_payload(&decoded_payload).unwrap();

    // 6. Verify
    assert_eq!(parsed_creds, creds);
}
```

### Benefits

- Tests the **full pipeline** end-to-end
- Catches integration issues between modules
- Runs quickly (no external dependencies)

---

## Testing Strategies by Module

### `types.rs`

- Test enum variants and parsing
- Test sorting logic
- Test JSON serialization

### `payload.rs`

- Test encoding (credentials → payload)
- Test parsing (payload → credentials)
- Test edge cases (escaping, special characters)
- Test round-trips

### `qr.rs`

- Test QR generation (payload → PNG)
- Test QR decoding (PNG → payload)
- Test base64 encoding/decoding
- Test Unicode rendering

### `service.rs`

- Test all service functions with `FakeAdapter`
- Verify correct adapter methods are called
- Test error propagation

### `platform/*.rs`

- Test output parsing with captured command output
- Test error handling for malformed output
- (Don't test actual OS commands)

### `ipc.rs`

- Test request/response serialization
- Test message framing (length prefix)
- Test error responses

### `tui/fuzzy.rs`

- Test fuzzy scoring algorithm
- Test ranking behavior
- Test tiebreaking

---

## Best Practices

### 1. Use Descriptive Test Names

```rust
// Good
fn share_current_returns_error_when_no_network_connected() { ... }

// Bad
fn test_share() { ... }
```

### 2. Test One Thing Per Test

```rust
// Good
#[test]
fn parse_ssid_handles_redacted() { ... }

#[test]
fn parse_ssid_handles_empty() { ... }

// Bad
#[test]
fn parse_ssid_handles_everything() {
    // tests redacted, empty, whitespace, etc. all in one
}
```

### 3. Use Helpers for Common Setup

```rust
fn credentials(ssid: &str) -> WifiCredentials {
    WifiCredentials::new(ssid, WifiSecurity::Wpa2)
        .with_password(format!("{ssid}-password"))
}

fn network(ssid: &str, active: bool) -> WifiNetwork {
    WifiNetwork {
        ssid: ssid.into(),
        security: WifiSecurity::Wpa2,
        signal: None,
        active,
    }
}
```

### 4. Make Tests Independent

Each test should set up its own state and not depend on other tests.

```rust
#[test]
fn test_one() {
    let adapter = FakeAdapter::new();  // Fresh state
    // ...
}

#[test]
fn test_two() {
    let adapter = FakeAdapter::new();  // Fresh state, not affected by test_one
    // ...
}
```

### 5. Use assert_eq with Clear Messages

```rust
// Good
assert_eq!(actual, expected, "SSID mismatch");

// Okay (but less clear)
assert_eq!(actual, expected);
```

---

## Summary

Testing QR Wi-Fi RS:

1. **Unit tests** in each module (`#[cfg(test)]`)
2. **FakeAdapter** for testing service functions
3. **Parse-only tests** for platform code
4. **Integration tests** for full pipelines
5. **No OS calls** in tests (use mocks/captured output)

This strategy keeps tests fast, deterministic, and runnable on any platform.
