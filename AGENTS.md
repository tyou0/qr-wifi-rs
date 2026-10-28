# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Project Overview

QR Wi-Fi RS is a cross-platform Rust toolkit for sharing Wi-Fi as QR codes and connecting from scanned/copied QR codes. The workspace contains four crates, including a shared core:

- **qr-wifi-core** — Shared library with domain types, `WIFI:` payload encoding/decoding, QR code generation, OS Wi-Fi adapters.
- **qr-wifi-cli** — Command-line interface (`qr-wifi`). With no flags, drops into the interactive menu.
- **qr-wifi-tui** — Terminal UI (same interactive menu, can be run directly via `qr-wifi-tui`).
- **qr-wifi-gui** — Tauri desktop GUI in `src-tauri/` (separate workspace member, pulls Tauri dependencies).

## Build & Test Commands

```sh
# Build all workspace default members (core + cli + tui)
cargo build
cargo build --release

# Build a specific crate
cargo build -p qr-wifi-cli
cargo build -p qr-wifi-tui
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

All three frontends call into `qr-wifi-core`; no Wi-Fi logic lives in the UI layers.

### Platform Adapters

Each OS implements the `WifiAdapter` trait:

| Method      | macOS (CoreWLAN) | Linux (NetworkManager D-Bus) | Windows (Native Wi-Fi API) |
| ------------| ----------------------------------- | --------------- | ------------------ |
| `current_ssid` | Live `CWInterface.ssid` | `ActiveAccessPoint` SSID bytes | `WlanQueryInterface` |
| `list_networks` | CoreWLAN scan | `GetAllAccessPoints` | `WlanGetAvailableNetworkList` |
| `get_credentials` | `CWKeychainFindWiFiPassword` | `GetSettings` / `GetSecrets` | `WlanGetProfile` XML |
| `connect` | Native association + live SSID verification | `AddAndActivateConnection2` | `WlanSetProfile` / `WlanConnect` |

Linux uses the same NetworkManager system D-Bus API across distributions; NetworkManager must manage the interface. A missing compatible SSID triggers a directed scan, and joins wait for actual activation. Windows waits for native connection-complete notifications.

Pure adapter parsing/profile tests compile on every test host. Windows FFI lives
in `crates/core/src/platform/windows/native.rs` and only compiles for Windows.

macOS uses CoreWLAN directly and requires Location Services authorization to identify the connected network. The GUI requests this permission; blocked terminal builds must authorize their terminal app. Saved profiles and historical logs are never used as proof of the current connection.

## Key Design Patterns

- **Shared menu between CLI and TUI** — `qr-wifi` with no flags calls `qr_wifi_tui::run_menu()`, avoiding duplication.
- **Trait-based platform abstraction** — `WifiAdapter` trait lets the core logic work with any OS implementation.
- **One-shot vs interactive** — The CLI supports both flag-based one-shot actions (`--share`, `--ssid ...`) and an interactive menu.
- **No bundler for Tauri frontend** — The GUI webview (`frontend/`) is vanilla HTML/CSS/JS, no TypeScript, no build step.
