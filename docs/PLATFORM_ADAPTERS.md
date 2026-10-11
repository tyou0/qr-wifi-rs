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
| Windows | Native Wi-Fi API | `WlanGetAvailableNetworkList` | `WlanGetProfile` XML | `WlanSetProfile` / `WlanConnect` |

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
    return Box::new(linux::LinuxAdapter);

    #[cfg(target_os = "windows")]
    return Box::new(windows::WindowsAdapter);

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    compile_error!("QR Wi-Fi RS supports macOS, Linux, and Windows only");
}
```

This is evaluated at **compile time** — only the current OS's code is linked into the binary.

### Platform Module Structure

Each platform lives in `crates/core/src/platform/{os}.rs`:

```
platform/
├── mod.rs          # Trait definition, factory
├── command.rs      # Shell execution helpers (shared)
├── macos.rs        # macOS implementation
├── linux.rs        # Linux implementation
├── windows.rs      # Windows profile XML and adapter
└── windows/
    └── native.rs   # Windows-only owned FFI wrappers
```

---

## Conditional Compilation

Rust provides `#[cfg(...)]` attributes to include code only on specific platforms:

```rust
#[cfg(any(test, target_os = "macos"))]
mod macos;

#[cfg(any(test, target_os = "linux"))]
mod linux;

#[cfg(any(test, target_os = "windows"))]
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
LC_ALL=C.UTF-8 nmcli --colors no -t -f ACTIVE,SSID,SIGNAL,SECURITY device wifi --rescan no
```

The adapter actually requests `ACTIVE,SSID,SIGNAL,SECURITY` with `--rescan no`
and `--colors no`, running the process with `LC_ALL=C.UTF-8`. Plain `C` would
replace Unicode SSIDs with question marks. Terse fields are colon
separated, with literal colons and backslashes escaped:
```
yes:MyNetwork:65:WPA2
no:Cafe\:Guest:40:
```

### Get Current SSID

```bash
LC_ALL=C.UTF-8 nmcli --colors no -t -f ACTIVE,SSID device wifi --rescan no
```

### Get Credentials

```bash
nmcli -t -f UUID,TYPE connection show
nmcli --escape no -g 802-11-wireless.ssid connection show uuid "$UUID"
nmcli --show-secrets --escape no -t -m multiline \
  -f 802-11-wireless,802-11-wireless-security connection show uuid "$UUID"
```

Saved profile names can differ from SSIDs, so the adapter first resolves a Wi-Fi
profile by its actual SSID, then queries it by UUID. Multiline property names
are stable and values retain literal punctuation and spaces. The result keeps
key management, the selected WEP key or WPA key, and the hidden setting:
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
nmcli device wifi connect "$SSID" password "$PASSWORD" hidden yes
```

---

## Windows Details

### Native Wi-Fi API

`windows/native.rs` owns a `WlanOpenHandle` session and every buffer returned by
`wlanapi.dll`. `Drop` calls `WlanCloseHandle` and `WlanFreeMemory` on error paths
as well as successful paths. Only this module compiles for Windows; profile
encoding and decoding remain testable on every host.

`WlanEnumInterfaces` finds wireless adapters. `WlanQueryInterface` reads the
connected SSID, while `WlanGetAvailableNetworkList` supplies SSID bytes,
authentication, signal quality, and active flags. Network names use UTF-8 bytes
directly, avoiding console encoding and translated `netsh` labels.

### Saved Credentials

`WlanGetProfileList` and `WlanGetProfile` resolve a profile by its SSID even if
the profile was renamed. Profile XML supplies security, the hidden flag, and
the key. Plaintext access is requested only for the selected profile. If
Windows returns an encrypted key (`protected=true`), the password stays absent.
Enterprise profiles are rejected because the personal Wi-Fi QR model cannot
represent 802.1X credentials.

### Connect

`WlanSetProfile` receives XML directly in memory, with hexadecimal SSID bytes
and escaped names/passwords. A 64-digit hexadecimal WPA key uses `networkKey`;
ordinary passwords use `passPhrase`. Existing profiles retain their name and
per-user/all-user scope, while new profiles are per-user. No temporary file
contains the password.

`WlanConnect` requests a connection using the saved profile. Windows completes
association asynchronously; a successful return means the request was accepted.

Recent Windows versions can require Location services and desktop-app location
permission to read Wi-Fi networks. The adapter explains this setting when
Windows denies access. WLAN AutoConfig, a usable Wi-Fi adapter, and permission
to read or update profiles are also required.

---

## Testing Platform Code

### Challenge: OS Commands Require Actual OS

You can't run `networksetup` on Linux or `nmcli` on macOS. How do we test platform code?

### Strategy 1: Parse-Only Tests

Test the **output parsing logic** with mock command output. The platform modules
use `cfg(any(test, target_os = "..."))`, so `cargo test` runs all parser and
profile regressions on any supported host. Windows native SSID/security tests
additionally run on Windows. No unit test reads or changes real Wi-Fi settings.

`scripts/test-linux-nmcli.sh` also exercises the real adapter against inactive
renamed WPA2/WPA3/WEP/open profiles, including a Unicode SSID and spaces in a
password. It requires a disposable Linux container running as root, and marks
every interface unmanaged before starting NetworkManager. CI runs this check
with Rust 1.88. The Rust integration test is ignored during ordinary tests
because it requires these explicit fixtures.

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
