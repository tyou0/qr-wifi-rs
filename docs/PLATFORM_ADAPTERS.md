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
| macOS | CoreWLAN | Native scan | CoreWLAN Keychain API | Native association + verification |
| Linux | NetworkManager D-Bus | Access-point objects | Settings/secrets | Activation API |
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

The adapter calls CoreWLAN directly through Rust Objective-C bindings. It needs
neither Swift nor CLI tools at runtime. `CWWiFiClient` supplies the current
wireless interface without assuming an `en0` name. `CWInterface.ssid` identifies
the current network; saved profiles and log history cannot establish this.

`scanForNetworksWithSSID:includeHidden:error:` provides real visible networks
and directed scans for a requested SSID, including hidden QR networks. Exact
SSID bytes and meaningful whitespace are preserved. Security capabilities are
checked before association; enterprise networks cannot be represented by the
personal Wi-Fi credential model. `CWKeychainFindWiFiPassword` reads saved keys
without a password-bearing subprocess.

`associateToNetwork:password:error:` synchronously associates using an in-memory
password. The adapter then verifies the live SSID, waiting up to 15 seconds for
confirmation. Native errors expose numeric codes, omitting descriptions that
might contain credentials. A failed join cannot become success merely because
an external command exited with status zero.

macOS protects Wi-Fi identities with Location Services authorization. The GUI
requests permission on its main run loop and includes usage descriptions in the
bundle. Enable QR Wi-Fi RS under System Settings > Privacy & Security > Location
Services when denied; terminal builds need their terminal app authorized.
No location coordinates are requested. Camera permission is separate.

---

## Linux Details

### Native NetworkManager D-Bus

Linux talks directly to `org.freedesktop.NetworkManager` on the system bus.
The adapter has no shell commands, distro branches, localized labels, or
password arguments. NetworkManager must manage the wireless interface;
standalone iwd/wpa_supplicant configurations are not supported. The manager
continues to own authentication, DHCP, DNS and routes. A raw kernel scan alone
does not supply those services.

`GetDevices` selects managed wireless radios. `GetAllAccessPoints` supplies
SSID bytes, security capabilities, strength and the active AP. SSID punctuation,
Unicode and significant whitespace are preserved. A missing compatible SSID
triggers a directed `RequestScan`; the adapter waits for `LastScan` to advance
before looking up the requested network again. It never treats a broadcast
network as hidden merely to force a connection.

Saved credentials come from `Settings.ListConnections`, `GetSettings`, and
`GetSecrets`. Profile names may differ from SSIDs. Enterprise authentication
is rejected because a personal Wi-Fi QR cannot represent it.

`AddAndActivateConnection2` sends typed settings and secrets directly to the
manager. A matching saved profile can be activated without re-entering its key.
The connection is successful only after its active state is activated and the
radio's actual SSID matches the request. Failed or timed-out new attempts are
deactivated and their newly created profile is removed. The app does not restart
network services or take the interface away from NetworkManager.

Privileged requests allow the normal desktop authorization prompt. Launch the
GUI through the desktop launcher: a process started through SSH can have different
PolicyKit permissions even when it displays on the same Wayland session.


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
association asynchronously. The adapter registers for ACM notifications before
requesting the join, filters by interface and SSID, and waits up to 45 seconds
for a successful connection-complete result. A failed request or timeout is
returned to the frontend as an error. A timed-out attempt is cancelled.

Recent Windows versions can require Location services and desktop-app location
permission to read Wi-Fi networks. The adapter explains this setting when
Windows denies access. WLAN AutoConfig, a usable Wi-Fi adapter, and permission
to read or update profiles are also required.

---

## Testing Platform Code

### Challenge: Native Services Require Their OS

macOS CoreWLAN, Linux system D-Bus services, and Windows WLAN APIs require
the corresponding operating system. Tests isolate native service calls from
profile encoding and connection-completion logic.

### Strategy 1: Parse-Only Tests

Test the **output parsing logic** with mock command output. The platform modules
use `cfg(any(test, target_os = "..."))`, so `cargo test` runs all parser and
profile regressions on any supported host. Windows native SSID/security tests
additionally run on Windows. No unit test reads or changes real Wi-Fi settings.

`scripts/test-linux-nmcli.sh` uses nmcli only to seed an isolated daemon, then
checks native D-Bus profile generation and retrieval. A private fake D-Bus
service checks scan completion, authorization flags, activation failure and
request acceptance versus real completion. It also exercises the adapter against inactive
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

The macOS native security/error tests run only on macOS. For the opt-in real
radio regression (which rejects an absent network rather than returning false
success), run:

```bash
cargo test --locked -p qr-wifi-core --test macos_hardware -- --ignored
```

---

## Summary

Platform adapters in QR Wi-Fi RS:

1. **Hide OS differences** behind the `WifiAdapter` trait
2. **Compile only relevant code** using `#[cfg(target_os = ...)]`
3. **Use native APIs** for typed Wi-Fi state and credential handling
4. **Handle permissions and asynchronous activation** explicitly
5. **Test domain rules and service boundaries**, then verify on real hardware

This design keeps the core logic OS-agnostic while still supporting all major platforms.
