# QR Wi-Fi RS — Platform Adapters Explained

This document explains how QR Wi-Fi RS abstracts operating system differences and implements platform-specific Wi-Fi operations.

## Table of Contents

- [Overview](#overview)
- [The WifiAdapter Trait](#the-wifiadapter-trait)
- [Platform Implementations](#platform-implementations)
- [Conditional Compilation](#conditional-compilation)
- [macOS Details](#macos-details)
- [Linux Details](#linux-details)
- [Windows Details](#windows-details)
- [Testing Platform Code](#testing-platform-code)

---

## Overview

Each operating system has its own tools and APIs for managing Wi-Fi:

| OS | Primary Tool | List Networks | Get Password | Connect |
|----|--------------|---------------|--------------|---------|
| macOS | `networksetup`, `security` | `-listpreferredwirelessnetworks` | Keychain via `security` | `-setairportnetwork` |
| Linux | `nmcli` (NetworkManager) | `device wifi list` | `connection show` | `device wifi connect` |
| Windows | `netsh` | `wlan show networks` | `wlan show profiles ... key=clear` | `wlan connect` |

QR Wi-Fi RS abstracts these differences behind the **`WifiAdapter` trait**, so the core logic doesn't need to know which OS it's running on.

---

## The WifiAdapter Trait

```rust
/// Abstraction over OS-specific Wi-Fi operations.
///
/// Each platform implements this trait using its native tools.
pub trait WifiAdapter: Send + Sync {
    /// List all saved/visible Wi-Fi networks.
    fn list_networks(&self) -> Result<Vec<WifiNetwork>>;

    /// Get the SSID of the currently connected network (if any).
    fn current_ssid(&self) -> Result<String>;

    /// Retrieve saved credentials for a specific SSID.
    fn credentials(&self, ssid: &str) -> Result<WifiCredentials>;

    /// Connect to a network using the provided credentials.
    fn connect(&self, credentials: &WifiCredentials) -> Result<()>;
}
```

### Why `Send + Sync`?

These bounds allow the adapter to be shared across threads (useful for caching or async operations).

### Why `&self` not `&mut self`?

All operations are stateless — they query or modify OS state, not adapter state. This enables:
- Easy sharing via `&dyn WifiAdapter`
- No need for `Arc<Mutex<>>` wrappers
- Simpler call sites

---

## Platform Implementations

### Factory Function

```rust
pub fn default_adapter() -> Box<dyn WifiAdapter> {
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacosAdapter::new());

    #[cfg(target_os = "linux")]
    return Box::new(linux::LinuxAdapter::new());

    #[cfg(target_os = "windows")]
    return Box::new(windows::WindowsAdapter::new());

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    compile_error!("QR Wi-Fi RS supports macOS, Linux, and Windows only");
}
```

This is evaluated at **compile time** — only the current OS's code is linked into the binary.

### Platform Module Structure

Each platform lives in `crates/core/src/platform/{os}.mod`:

```
platform/
├── mod.rs          # Trait definition, factory
├── command.rs      # Shell execution helpers (shared)
├── macos.rs        # macOS implementation
├── linux.rs        # Linux implementation
└── windows.rs      # Windows implementation
```

---

## Conditional Compilation

Rust provides `#[cfg(...)]` attributes to include code only on specific platforms:

```rust
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "windows")]
mod windows;
```

### Available Target OS Values

- `target_os = "macos"` — macOS
- `target_os = "linux"` — Linux (any distro)
- `target_os = "windows"` — Windows

### Other Useful Conditions

```rust
#[cfg(unix)]           // Any Unix-like (macOS, Linux, *BSD)
#[cfg(windows)]        // Windows
#[cfg(debug_assertions)]  // Debug builds only
#[cfg(test)]           // When running `cargo test`
```

---

## macOS Details

### SSID Detection (Multiple Fallbacks)

macOS is tricky because the OS may redact the SSID as `<redacted>` in recent versions. We try multiple methods in order:

```rust
impl MacosAdapter {
    fn current_ssid(&self) -> Result<String> {
        // 1. networksetup -getairportnetwork
        if let Ok(ssid) = self.ssid_via_networksetup() {
            return Ok(ssid);
        }

        // 2. ipconfig getsummary
        if let Ok(ssid) = self.ssid_via_ipconfig() {
            return Ok(ssid);
        }

        // 3. Swift + CoreWLAN (live SSID)
        if let Ok(ssid) = self.ssid_via_corewlan() {
            return Ok(ssid);
        }

        // 4. system_profiler SPAirPortDataType
        if let Ok(ssid) = self.ssid_via_system_profiler() {
            return Ok(ssid);
        }

        // 5. Log search for airportd NetworkName
        self.ssid_via_log()
    }
}
```

### Wi-Fi Interface Detection

macOS may have multiple AirPort interfaces (`en0`, `en1`, etc.). We:

1. Try `networksetup -listallhardwareports` to find Wi-Fi devices
2. Cache the result (interfaces don't change at runtime)
3. Fall back to `en0` if detection fails

### Keychain Password Retrieval

```bash
security find-internet-password -wa "$SSID" 2>/dev/null
```

This prompts the user for Keychain access on first run. The password is extracted from the command output.

### Connection

```bash
networksetup -setairportnetwork "$DEVICE" "$SSID" "$PASSWORD"
```

---

## Linux Details

### nmcli (NetworkManager)

QR Wi-Fi RS uses `nmcli`, which is the standard CLI for NetworkManager on most Linux distributions.

### List Networks

```bash
nmcli -t -f SSID,SECURITY,SIGNAL device wifi list --rescan yes
```

Output format (tab-separated):
```
MyNetwork:WPA2:65
Guest:WPA:40
```

### Get Current SSID

```bash
nmcli -t -f active,ssid dev wifi | grep '^yes' | cut -d: -f2
```

### Get Credentials

```bash
nmcli -s connection show "$SSID"
```

The `-s` flag shows secrets (passwords). Output format:
```
802-11-wireless-security.psk:mysecretpassword
...
```

### Connect

```bash
nmcli device wifi connect "$SSID" password "$PASSWORD"
```

For hidden networks:
```bash
nmcli connection add con-name "$SSID" type wifi ssid "$SSID"
nmcli connection modify "$SSID" 802-11-wireless.hidden yes
nmcli connection up "$SSID"
```

---

## Windows Details

### netsh Commands

Windows uses `netsh wlan` for all Wi-Fi operations.

### List Networks

```cmd
netsh wlan show networks mode=bssid
```

Parsing is complex due to Windows localization. We match on patterns like:
```
SSID 1 : MyNetwork
    Network type             : Infrastructure
    Authentication          : WPA2-Personal
    Encryption              : CCMP
```

### Get Current SSID

```cmd
netsh wlan show interfaces
```

Parse the `SSID` line:
```
SSID                   : MyNetwork
```

### Get Credentials

```cmd
netsh wlan show profiles name="MyNetwork" key=clear
```

Parse the `Key Content` line:
```
Key Content            : mysecretpassword
```

### Connect

```cmd
netsh wlan connect name="MyNetwork"
```

For networks with passwords, the profile must already exist (it's created when you first connect).

---

## Testing Platform Code

### Challenge: OS Commands Require Actual OS

You can't run `networksetup` on Linux or `nmcli` on macOS. How do we test platform code?

### Strategy 1: Parse-Only Tests

Test the **output parsing logic** with mock command output:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_current_ssid() {
        let output = "    Aggregated Wi-Fi";
        assert_eq!(parse_current_ssid(output), Ok("Aggregated Wi-Fi".to_string()));
    }

    #[test]
    fn parse_networks() {
        let output = "MyNetwork:WPA2:65\nGuest:WPA:40";
        let networks = parse_networks(output).unwrap();
        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].ssid, "MyNetwork");
    }
}
```

### Strategy 2: Fake Adapter for Integration Tests

For testing higher-level code (service layer), use a **fake adapter**:

```rust
struct FakeAdapter {
    networks: Vec<WifiNetwork>,
    current: String,
    // ...
}

impl WifiAdapter for FakeAdapter {
    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        Ok(self.networks.clone())
    }

    fn current_ssid(&self) -> Result<String> {
        Ok(self.current.clone())
    }

    // ...
}
```

This allows testing service functions without touching the OS.

### Strategy 3: Platform-Specific Tests

Run tests only on the relevant OS:

```rust
#[cfg(test)]
#[cfg(target_os = "macos")]
mod macos_tests {
    use super::*;

    #[test]
    fn test_networksetup_path() {
        // This test only runs on macOS
        assert!(command_exists("networksetup"));
    }
}
```

---

## Summary

Platform adapters in QR Wi-Fi RS:

1. **Hide OS differences** behind the `WifiAdapter` trait
2. **Compile only relevant code** using `#[cfg(target_os = ...)]`
3. **Parse CLI output** since most OSs expose Wi-Fi via commands
4. **Handle edge cases** (macOS redaction, Windows localization)
5. **Test parsing logic** with mock output, not real OS calls

This design keeps the core logic OS-agnostic while still supporting all major platforms.
