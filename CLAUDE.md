# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

QR Wi-Fi RS is a cross-platform Rust toolkit for sharing Wi-Fi as QR codes and connecting from scanned/copied QR codes. The workspace contains four crates sharing a common core:

- **qr-wifi-core** — Shared library with domain types, `WIFI:` payload encoding/decoding, QR code generation, OS Wi-Fi adapters, and the Native Messaging IPC protocol.
- **qr-wifi-cli** — Command-line interface (`qr-wifi`). With no flags, drops into the interactive menu.
- **qr-wifi-tui** — Terminal UI (same interactive menu, can be run directly via `qr-wifi-tui`).
- **qr-wifi-host** — Native Messaging host for browser extension communication.
- **qr-wifi-gui** — Tauri desktop GUI in `src-tauri/` (separate workspace member, pulls Tauri dependencies).

## Build & Test Commands

```sh
# Build all workspace default members (core + cli + tui + host)
cargo build
cargo build --release

# Build a specific crate
cargo build -p qr-wifi-cli
cargo build -p qr-wifi-tui
cargo build -p qr-wifi-host
cargo build -p qr-wifi-core

# Tauri GUI (compile-check only; full bundle via Tauri CLI)
cargo check -p qr-wifi-gui

# Test & lint
cargo test
cargo fmt --all -- --check
cargo clippy --workspace --all-targets

# Run
cargo run -p qr-wifi-cli -- --list
cargo run -p qr-wifi-cli -- --share
cargo run -p qr-wifi-tui
cargo run -p qr-wifi-host
```

Tauri GUI requires the Tauri CLI:
```sh
cargo install tauri-cli --version "^2"
cargo tauri dev      # Dev mode with hot reload
cargo tauri build    # Build installer/bundle
```

## Architecture

### Core Library Structure (`crates/core/`)

The core crate is organized by responsibility:

- **types** — Domain models: `WifiCredentials`, `WifiNetwork`, `WifiSecurity` (WPA/WEP/nopass)
- **payload** — Build and parse `WIFI:` QR code payloads (`WIFI:T:WPA;S:...;P:...;;`)
- **qr** — QR matrix generation → PNG/terminal art, plus image decoding (via `rqrr`)
- **platform** — OS-specific Wi-Fi adapters (trait `WifiAdapter` with `macos`, `linux`, `windows` modules)
- **service** — High-level feature functions used by all frontends (share, connect, list, decode)
- **ipc** — Native Messaging protocol (`Request`/`Response` types, framing)

All four binaries and the Tauri GUI call into `qr-wifi-core`; no Wi-Fi logic lives in the UI layers.

### Platform Adapters

Each OS implements the `WifiAdapter` trait:

| Method      | macOS (`networksetup`/`security`) | Linux (`nmcli`) | Windows (`netsh`) |
| ------------| ----------------------------------- | --------------- | ------------------ |
| `current_ssid` | Multiple fallback methods including syslog parsing | `nmcli -g` | `netsh wlan show interfaces` |
| `list_networks` | `networksetup -listpreferredwirelessnetworks` | `nmcli device wifi list` | `netsh wlan show networks` |
| `get_credentials` | Keychain via `security find-internet-password` | `nmcli -s connection show` | `netsh wlan show profiles ... key=clear` |
| `connect` | `networksetup -setairportnetwork` | `nmcli device wifi connect` | `netsh wlan connect` |

macOS SSID detection has special handling for when the OS returns `<redacted>` — it falls back through several methods including parsing `log show` for `airportd` entries.

### Native Messaging Protocol

The host communicates with the browser extension via length-prefixed JSON (standard Native Messaging format). Requests use a `command` field:

| command | input | response data |
| ------- | ----- | ------------- |
| `current_ssid` | — | `{ ssid }` |
| `list_networks` | — | `{ networks: [...] }` |
| `get_credentials` | `{ ssid }` | `{ credentials }` |
| `share_current` | — | `{ payload, png_base64 }` |
| `share_custom` | `{ credentials }` | `{ payload, png_base64 }` |
| `connect` | `{ credentials }` | `{ kind: "connected" }` |
| `connect_payload` | `{ payload }` | `{ kind: "connected" }` |
| `decode_qr` | `{ image_base64 }` | `{ credentials }` |

Response envelope: `{ ok: true, data: ... }` or `{ ok: false, error: ... }`.

## Key Design Patterns

- **Shared menu between CLI and TUI** — `qr-wifi` with no flags calls `qr_wifi_tui::run_menu()`, avoiding duplication.
- **Trait-based platform abstraction** — `WifiAdapter` trait lets the core logic work with any OS implementation.
- **One-shot vs interactive** — The CLI supports both flag-based one-shot actions (`--share`, `--ssid ...`) and an interactive menu.
- **No bundler for Tauri frontend** — The GUI webview (`frontend/`) is vanilla HTML/CSS/JS, no TypeScript, no build step.

## Browser Extension Setup

The Native Messaging host is registered via `scripts/install-native-host.sh`:

```sh
scripts/install-native-host.sh --chrome-extension-id <extension-id>
```

This places the manifest in the OS-specific location (e.g., `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/` on macOS) pointing at the installed `qr-wifi-host` binary.
