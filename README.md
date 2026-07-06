# QR Wi-Fi RS

Pure-Rust, cross-platform (macOS / Linux / Windows) toolkit for **sharing Wi-Fi
as a QR code** and **connecting from a scanned/copied QR code**.

This is the Rust counterpart to [`qr_wifi_bun`](../qr_wifi_bun). The two
projects are fully separated:

- `qr_wifi_bun` — Bun/TypeScript CLI, TUI, and Electron desktop app.
- `qr_wifi_rs` — Rust workspace (this project): CLI, TUI, Tauri desktop GUI,
  and a browser-extension Native Messaging host.

Everything that touches the OS (`networksetup`, `nmcli`, `netsh`) is written in
Rust. The only non-Rust pieces are the Tauri webview frontend (vanilla
HTML/CSS/JS, no TypeScript, no bundler) and the browser extension.

## Layout

```
qr_wifi_rs/
├── crates/
│   ├── core/      qr-wifi-core  shared lib: domain types, WIFI: payload, QR
│   │                            encode/decode, OS Wi-Fi adapters, IPC protocol
│   ├── cli/       qr-wifi       command-line interface
│   ├── tui/       qr-wifi-tui   terminal UI (ratatui)
│   └── host/      qr-wifi-host  Native Messaging host (Chrome/Firefox IPC)
├── src-tauri/     qr-wifi-gui   Tauri desktop GUI (thin commands over core)
├── frontend/      vanilla HTML/CSS/JS for the Tauri webview
└── extension/     Chrome/Firefox extension that talks to the host
```

All three binaries (CLI, TUI, host) and the Tauri GUI share one implementation
through `qr-wifi-core`.

## Prerequisites

- **Rust toolchain** (stable, via <https://rustup.rs>): `rustc` + `cargo`.
  Confirm with `cargo --version`.
- **Tauri CLI** (only for the desktop GUI): `cargo install tauri-cli --version "^2"`.
- OS Wi-Fi tooling (already present on each platform):
  `networksetup`/`ipconfig`/`security` (macOS), `nmcli` (Linux, NetworkManager),
  `netsh` (Windows).

## Build

`crates/core`, `crates/cli`, `crates/tui`, and `crates/host` are the workspace
default members, so plain `cargo` commands build/test them. The Tauri GUI
(`src-tauri`) is a separate member that pulls the larger Tauri dependency tree.

```sh
# Debug build of CLI + TUI + host (and the shared core)
cargo build

# Optimized release build
cargo build --release

# Build just one crate
cargo build -p qr-wifi-cli
cargo build -p qr-wifi-tui
cargo build -p qr-wifi-host
cargo build -p qr-wifi-core      # library only

# Compile-check the Tauri GUI (full bundle is done via the Tauri CLI below)
cargo check -p qr-wifi-gui
cargo build -p qr-wifi-gui
```

Build artifacts land in `target/debug/` (or `target/release/`):

| Binary            | Crate        | Purpose                          |
| ----------------- | ------------ | -------------------------------- |
| `qr-wifi`         | `qr-wifi-cli`  | Command-line interface          |
| `qr-wifi-tui`     | `qr-wifi-tui`  | Terminal UI                     |
| `qr-wifi-host`    | `qr-wifi-host` | Browser Native Messaging host   |
| `qr-wifi-gui`     | `qr-wifi-gui`  | Tauri desktop app (run via Tauri CLI) |

Install the CLI/TUI/host onto your `PATH` (`~/.cargo/bin`):

```sh
cargo install --path crates/cli
cargo install --path crates/tui
cargo install --path crates/host
```

## Test & lint

```sh
cargo test                          # core unit tests + integration tests
cargo fmt                           # format
cargo fmt --all -- --check          # verify formatting
cargo clippy --workspace --all-targets
```

## Run

There are several ways to run a binary. Use any of:

```sh
# 1. Direct debug artifact
./target/debug/qr-wifi --list
./target/debug/qr-wifi-tui

# 2. Release artifact (after `cargo build --release`)
./target/release/qr-wifi --share

# 3. Through cargo (no manual build step)
cargo run -p qr-wifi-cli -- --list
cargo run -p qr-wifi-cli -- --share
cargo run -p qr-wifi-cli -- --ssid "Home"
cargo run -p qr-wifi-cli                    # no flags → interactive menu
cargo run -p qr-wifi-tui                    # interactive menu directly
cargo run -p qr-wifi-host                   # reads Native Messaging frames on stdin

# 4. Installed on PATH (after `cargo install`)
qr-wifi --list
qr-wifi         # interactive menu
qr-wifi-tui
```

## CLI

Binary: `qr-wifi` (crate `qr-wifi-cli`). It is a **single command**: run with no
flags to drop into the interactive menu (the same one `qr-wifi-tui` opens), or
pass flags for a one-shot action. `qr-wifi --help` lists everything.

| Flag(s)                                   | Action                                                            |
| ----------------------------------------- | ---------------------------------------------------------------- |
| _(none)_                                  | Open the interactive menu (share / pick / custom / connect).      |
| `--list`                                  | List OS Wi-Fi networks (active first, then alphabetical).         |
| `--share`                                 | Generate a QR for the currently connected Wi-Fi.                  |
| `--ssid <name>`                           | Generate a QR for a saved network by SSID.                        |
| `--custom --ssid <s> [--password <p>] [--security …] [--hidden]` | Build a custom QR from manual details.            |
| `--scan` / `--connect`  `--image <path>`  | Decode a QR from an image file and connect.                       |

`--security` is `WPA` (default), `WEP`, or `nopass`.

```sh
qr-wifi                                   # interactive menu
qr-wifi --list
qr-wifi --share
qr-wifi --ssid "Home"
qr-wifi --custom --ssid Guest --password "s3cret"
qr-wifi --custom --ssid Open --security nopass
qr-wifi --scan --image ./wifi.png         # also: --connect
```

Every share/custom action prints the QR as terminal art followed by the raw
`WIFI:` payload string at the bottom, e.g.:

```
Payload: WIFI:T:WPA;S:Guest;P:s3cret;;
```

> Live camera scanning is in the Tauri GUI. The CLI `--scan`/`--connect` decodes
> a QR from an image file (`--image`).
>
> Exit code is non-zero on failure, with the error written to stderr.

## TUI (interactive menu)

Binaries: `qr-wifi` (no flags) and `qr-wifi-tui`. The menu is **not** a
full-screen dashboard — each screen is a short text menu that performs one
action and returns to the main menu.

```sh
qr-wifi                  # no flags → menu
qr-wifi-tui              # menu directly
./target/debug/qr-wifi-tui
cargo run -p qr-wifi-tui
```

Main menu:

```
1) Share current Wi-Fi
2) Share by SSID  (fzf finder)
3) Custom QR code
4) Connect / scan QR
5) Quit
```

- **Share by SSID** launches `fzf` over the network list (active network on top,
  rest alphabetical) for fuzzy selection. If `fzf` is not installed it falls
  back to a numbered list.
- **Custom QR code** prompts for SSID / security / password / hidden.
- **Connect / scan QR** decodes a QR from an image file path or a pasted
  `WIFI:` payload and connects.
- The generated QR (Unicode art) and the raw `WIFI:` payload are printed below
  each action.

## Tauri desktop GUI

Crate: `qr-wifi-gui` in `src-tauri/`. The webview is the vanilla
`frontend/` (HTML/CSS/JS). Build/run through the Tauri CLI (it bundles the
frontend and compiles the Rust binary together):

```sh
# One-time: install the Tauri CLI (v2)
cargo install tauri-cli --version "^2"

# Run in dev mode (hot frontend + Rust)
cargo tauri dev

# Produce an installer/bundle for the current OS (macOS dmg, Linux AppImage/deb, Windows msi/exe)
cargo tauri build

# The resulting app binary is under src-tauri/target/release/
```

The GUI has Share / Custom / Connect tabs. Camera scanning is decoded in Rust
(`decode_qr` command) so no JS QR library is needed. Generated QR codes show
the image and the raw `WIFI:` payload string at the bottom.

## Browser extension (Native Messaging)

The extension shares/connects to Wi-Fi by talking to the `qr-wifi-host` binary
over [Native Messaging](https://developer.chrome.com/docs/apps/nativeMessaging).

1. Build the host and put it somewhere stable:

   ```sh
   cargo build --release -p qr-wifi-host
   cp target/release/qr-wifi-host /usr/local/bin/qr-wifi-host
   ```

2. Register it with your browser by copying
   `extension/com.thetomyou.qrwifi.json.template` to the right directory (edit
   `path` and the Chrome extension ID first):

   - **Chrome (macOS):** `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.thetomyou.qrwifi.json`
   - **Firefox (macOS):** `~/Library/Application Support/Mozilla/NativeMessagingHosts/com.thetomyou.qrwifi.json`
   - **Linux:** `~/.config/google-chrome/NativeMessagingHosts/` (Chrome) /
     `~/.mozilla/native-messaging-hosts/` (Firefox)
   - **Windows:** registry key
     `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.thetomyou.qrwifi`

3. Load `extension/` as an unpacked extension (Chrome) or temporary add-on
   (Firefox). The popup's "Share current Wi-Fi" returns the QR; "Connect"
   connects the machine from a pasted `WIFI:` payload.

### IPC protocol

The host speaks a tiny JSON protocol (length-prefixed per the Native Messaging
spec). Requests are tagged by `command`:

| command           | input                                  | response data                |
| ----------------- | -------------------------------------- | ---------------------------- |
| `current_ssid`    | —                                      | `{ ssid }`                   |
| `list_networks`   | —                                      | `{ networks: [...] }`        |
| `get_credentials` | `{ ssid }`                             | `{ credentials }`            |
| `share_current`   | —                                      | `{ payload, png_base64 }`    |
| `share_custom`    | `{ credentials }`                      | `{ payload, png_base64 }`    |
| `connect`         | `{ credentials }`                      | `{ kind: "connected" }`      |
| `connect_payload` | `{ payload }`                          | `{ kind: "connected" }`      |
| `decode_qr`       | `{ image_base64 }`                     | `{ credentials }`            |

Responses use the envelope `{ ok: true, data }` or `{ ok: false, error }`.

## Current SSID detection (macOS)

Active SSID detection mirrors the Bun adapter, trying several methods in order
and falling back to the syslog when the OS redacts the name:

1. `networksetup -getairportnetwork <device>` (primary)
2. `ipconfig getsummary <device>`
3. `swift` CoreWLAN snippet (live SSID, then first saved profile)
4. `system_profiler SPAirPortDataType -json`
5. `log show` for the latest `airportd` `NetworkName` when a step returns
   `<redacted>`

The Wi-Fi interface is resolved from `networksetup -listallhardwareports`,
defaulting to `en0`.

## Platform support

| OS      | Detect SSID              | List networks                        | Read password                    | Connect                          |
| ------- | ------------------------ | ------------------------------------ | -------------------------------- | -------------------------------- |
| macOS   | networksetup/ipconfig/…  | `networksetup -listpreferred…`       | Keychain `security`              | `networksetup -setairportnetwork`|
| Linux   | `nmcli`                  | `nmcli device wifi`                  | `nmcli -s connection show`       | `nmcli device wifi connect`      |
| Windows | `netsh wlan show interf.`| `netsh wlan show networks`           | `netsh … key=clear`              | `netsh wlan connect`             |

## License

MIT
