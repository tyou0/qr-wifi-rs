# QR Wi-Fi RS

Pure-Rust, cross-platform (macOS / Linux / Windows) toolkit for **sharing Wi-Fi
as a QR code** and **connecting from a scanned/copied QR code**.

This is the Rust counterpart to [`qr_wifi_bun`](../qr_wifi_bun). The two
projects are fully separated:

- `qr_wifi_bun` — Bun/TypeScript CLI, TUI, and Electron desktop app.
- `qr_wifi_rs` — Rust workspace (this project): CLI, TUI, and Tauri desktop GUI.

Everything that touches the OS (CoreWLAN, NetworkManager D-Bus, Windows Native Wi-Fi) is written in
Rust. The Tauri webview frontend uses vanilla HTML/CSS/JS with no TypeScript or bundler.

## Layout

```
qr_wifi_rs/
├── crates/
│   ├── core/      qr-wifi-core  shared lib: domain types, WIFI: payload, QR
│   │                            encode/decode, OS Wi-Fi adapters
│   ├── cli/       qr-wifi       command-line interface
│   └── tui/       qr-wifi-tui   terminal UI (ratatui)
├── src-tauri/     qr-wifi-gui   Tauri desktop GUI (thin commands over core)
└── frontend/      vanilla HTML/CSS/JS for the Tauri webview
```

All three frontends (CLI, TUI, and Tauri GUI) share one implementation
through `qr-wifi-core`.

## Learn Rust with this repo

This codebase is also structured as a practical Rust study project. Start here:

- [`docs/RUST_STUDY_GUIDE.md`](docs/RUST_STUDY_GUIDE.md) - Rust concepts mapped to this repo.
- [`docs/LEARNING_PATH.md`](docs/LEARNING_PATH.md) - chapter-by-chapter curriculum, exercises, and checkpoints.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) - why the shared-core design works.
- [`docs/TESTING.md`](docs/TESTING.md) - fake adapters, pure unit tests, and platform parsing tests.

Runnable learning examples live in `crates/core/examples/`:

```sh
cargo run -p qr-wifi-core --example parse_payload
cargo run -p qr-wifi-core --example build_qr_png -- "Guest" "s3cret" out.png
cargo run -p qr-wifi-core --example service_contract
```

The learning rule for this repo: add behavior to `qr-wifi-core` first, test it
there, then expose it through CLI, TUI, and GUI as thin
frontends.

## Prerequisites

- **Rust toolchain** (stable, via <https://rustup.rs>, source builds only): `rustc` + `cargo`.
  Confirm with `cargo --version`.
- **Tauri CLI** (only for the desktop GUI): `cargo install tauri-cli --version "^2"`.
- OS Wi-Fi tooling:
  CoreWLAN (macOS), NetworkManager D-Bus (Linux),
  Windows WLAN AutoConfig / Native Wi-Fi (built into Windows).

## Install

### Desktop + CLI + TUI together

The combined installer installs the desktop app, `qr-wifi` (CLI), and
`qr-wifi-tui`. It creates a desktop launcher and installs per user, without
`sudo`. Close the app before upgrading.

From a source checkout, install the prerequisites below, then run **one** command
from the repository root (the script also installs Tauri CLI if missing):

```sh
# macOS, Ubuntu, or Arch Linux (Bash)
bash scripts/install.sh
```

```powershell
# Windows PowerShell 5.1 or PowerShell 7
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\install.ps1
```

These commands actually install the desktop app; no separate DMG/MSI/DEB step
is needed. Builds target the OS and CPU running the script. A Linux build does
not create a Windows or macOS application.

| System | Desktop installation | Terminal commands |
| --- | --- | --- |
| macOS | `~/Applications/QR Wi-Fi RS.app` | `~/.local/bin` |
| Ubuntu / Arch | Applications menu, launcher in `~/.local/share/applications` | `~/.local/bin` |
| Windows | Start menu, files in `%LOCALAPPDATA%\Programs\QR Wi-Fi RS` | Installer adds its `bin` directory to user PATH; reopen terminal |

On macOS/Linux, add this to your shell profile if `~/.local/bin` is not on PATH:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

### Install a combined release (no Rust required)

Download `qr-wifi-rs-VERSION-PLATFORM.tar.gz` from
[Releases](https://github.com/tyou0/qr-wifi-rs/releases/latest), verify it against
the release's SHA256SUMS file, and extract it. Platforms: `linux-x86_64`,
`windows-x86_64`, `macos-arm64`, and `macos-x86_64`. Inside the extracted folder:

```sh
# macOS / Linux
bash install.sh
```

```powershell
# Windows; tar -xf also extracts .tar.gz on Windows 10/11
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

The Linux archive targets Ubuntu 24.04+ and current Arch with the runtime
dependencies below. For older Ubuntu releases, build from source on that
release. Downloaded macOS apps are ad-hoc signed, not notarized; Windows
binaries are unsigned. The separate `desktop-*` packages still contain only
the desktop app; choose the combined archive for CLI/TUI too.

### OS prerequisites

See [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/) for details.

**Ubuntu 22.04+ (source build):**

```sh
sudo apt update
sudo apt install build-essential pkg-config curl libwebkit2gtk-4.1-dev \
  libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
  network-manager gstreamer1.0-plugins-good
```

For the prebuilt Linux archive on Ubuntu 24.04+, only runtime packages are needed:

```sh
sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0t64 libayatana-appindicator3-1 \
  librsvg2-2 network-manager gstreamer1.0-plugins-good
```

**Arch Linux (source build):**

```sh
sudo pacman -Syu --needed base-devel pkgconf curl webkit2gtk-4.1 openssl \
  gtk3 libappindicator-gtk3 librsvg networkmanager gst-plugins-good gst-plugin-pipewire
```

For the prebuilt archive, omit `base-devel`, `pkgconf`, and `curl`.
Linux Wi-Fi operations require **NetworkManager managing the Wi-Fi interface**;
an iwd-only setup is not supported. Installing NetworkManager does not switch
your existing network configuration automatically.

**macOS (source build):** install Rust and Xcode Command Line Tools (`xcode-select --install`).

**Windows 10/11:** the desktop runtime requires
[Microsoft Edge WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).
Source builds additionally need Rust's **MSVC** toolchain and Visual Studio
Build Tools with **Desktop development with C++** (including the Windows SDK).
Wi-Fi detection/listing on recent Windows versions may require **Location
services** and **desktop-app location access** in Settings > Privacy & security
> Location. Saved passwords remain subject to Windows policy and user permissions.

### Homebrew

The formula is published in the free `tyou0/qr-wifi-rs` personal tap:

```sh
brew trust tyou0/qr-wifi-rs
brew tap tyou0/qr-wifi-rs
brew install qr-wifi-rs
```

The trust step is required before Homebrew will use this personal tap. A normal
`brew install qr-wifi-rs` builds and installs the macOS desktop application as
well as the terminal tools.

For latest `main` instead of the pinned release:

```sh
brew install --HEAD tyou0/qr-wifi-rs/qr-wifi-rs
```

This installs:

- `qr-wifi`
- `qr-wifi-tui`
- `qr-wifi-gui`
- `QR Wi-Fi RS.app` under the formula prefix; launch it with:

  ```sh
  open "$(brew --prefix qr-wifi-rs)/QR Wi-Fi RS.app"
  ```

The tap formula builds the pinned `v0.2.4` public GitHub source archive. Nothing is submitted
to Homebrew core or an app store.

## Build

`crates/core`, `crates/cli`, and `crates/tui` are the workspace
default members, so plain `cargo` commands build/test them. The Tauri GUI
(`src-tauri`) is a separate member that pulls the larger Tauri dependency tree.

```sh
# Debug build of CLI + TUI (and the shared core)
cargo build

# Optimized release build
cargo build --release

# Build just one crate
cargo build -p qr-wifi-cli
cargo build -p qr-wifi-tui
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
| `qr-wifi-gui`     | `qr-wifi-gui`  | Tauri desktop app (run via Tauri CLI) |

Same commands, repeated for copy/paste:

```sh
cargo install --path crates/cli
cargo install --path crates/tui
```

## Test & lint

```sh
cargo test                          # core unit tests + integration tests
cargo fmt                           # format
cargo fmt --all -- --check          # verify formatting
cargo clippy --workspace --all-targets
```

## CI/CD

CI/CD is configured in:

- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `.gitea/workflows/ci.yml`

### One-command release for every platform

From a clean `main` checkout on Podman, this single command bumps the coordinated
Cargo and Tauri versions, runs release gates, commits, tags,
and pushes to canonical Gitea:

```sh
./scripts/release-all.sh 0.2.2
```

Gitea's sync-on-commit push mirror sends `main` and the new tag to GitHub. The
tag starts GitHub Actions, which publishes one GitHub Release with uniquely
named packages (version + OS + architecture, so synced artifacts never collide):

- Linux x86_64: CLI/TUI `.tar.gz`; desktop `.AppImage`, `.deb`, `.rpm`
- Windows x86_64: CLI/TUI `.tar.gz`; desktop NSIS `-setup.exe`, `.msi`
- macOS ARM64: CLI/TUI `.tar.gz`; desktop `.app.zip`, `.dmg`
- macOS Intel x86_64: CLI/TUI `.tar.gz`; desktop `.app.zip`, `.dmg`
- Release integrity: versioned `SHA256SUMS.txt`

Example desktop filename:

```text
qr-wifi-rs-desktop-0.2.2-windows-x86_64-setup.exe
```

The command refuses dirty/non-`main`/unsynchronized worktrees and existing tags.
It asks before publishing. Use a new semantic version for each release.

See [`docs/CI_CD.md`](docs/CI_CD.md) for the concise operator runbook. See
[`docs/DEPLOYMENT_GUIDE.md`](docs/DEPLOYMENT_GUIDE.md) for the teaching-oriented
architecture, current automation status, trust boundaries, store publication
blueprints (Homebrew, WinGet, Microsoft Store, Apple, and Linux),
verification gates, and rollback procedures.

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

# Build desktop-only distribution packages for the current OS.
# For desktop + CLI + TUI installation, use the combined installer above.
cargo tauri build

# The resulting app binary is under target/release/
```

The GUI has Share / Custom / Connect tabs. Camera scanning is decoded in Rust
(`decode_qr` command) so no JS QR library is needed. Generated QR codes show
the image and the raw `WIFI:` payload string at the bottom.

### Linux camera prerequisites (including Omarchy)

The Linux desktop bundle enables WebKitGTK media streams and grants camera
capture only to its own per-process loopback UI origin. Scanning requests video
only, with no microphone. On Arch/Omarchy, install the WebKit/GStreamer camera
stack before launching the app:

```sh
sudo pacman -Syu --needed webkit2gtk-4.1 gst-plugins-good gst-plugin-pipewire
```

Omarchy's [base package list](https://github.com/basecamp/omarchy/blob/master/install/omarchy-base.packages)
includes WirePlumber and the GTK/Hyprland portals, but does not explicitly list
these GStreamer camera plugins. A working Chromium camera does not establish
that the Tauri app's WebKitGTK/GStreamer capture stack is installed.

For a camera that does not appear or start, check from a terminal inside the
logged-in Omarchy desktop session:

```sh
systemctl --user is-active pipewire wireplumber xdg-desktop-portal
gst-inspect-1.0 video4linux2
gst-inspect-1.0 pipewire
gst-device-monitor-1.0 Video/Source
```

`gst-inspect-1.0` comes from `gstreamer`; `gst-device-monitor-1.0` comes from
`gst-plugins-base-libs` on Arch. The
[gst-plugins-good package](https://archlinux.org/packages/extra/x86_64/gst-plugins-good/files/)
provides the Video4Linux camera plugin, and
[gst-plugin-pipewire](https://archlinux.org/packages/extra/x86_64/gst-plugin-pipewire/)
provides the PipeWire integration. Cameras needing libcamera may additionally
require `pipewire-libcamera` and a supported kernel/driver; installing webview
plugins alone cannot add hardware support.

Close other camera apps before retrying. A `NotReadableError` can indicate a
busy device, a driver problem, or a capture-backend failure. Restart QR Wi-Fi RS
after installing plugins. Camera access and preview startup each time out after
20 seconds rather than leaving the app silently waiting. A late camera stream
is immediately stopped after an access timeout. The console distinguishes
**Camera access timed out** from **Camera preview timed out**.

For a built-in laptop camera, first check whether another camera app can use it
and note the laptop model. Some built-in sensors need a libcamera-compatible
driver stack; USB webcam results alone do not establish built-in camera support.

Use **Connect > Start Scanning**, scan a Wi-Fi QR, then
press **Stop Scanning** and verify that the camera indicator turns off. On systems with
multiple cameras, the app currently uses the webview's chosen camera; it has no
camera selector.

The Arch release check starts the GUI under Xvfb; it does not exercise a physical
camera, a Hyprland session, or Omarchy's portal permissions. Camera compatibility
still needs this on-device check. Windows honors the global Settings > Privacy
and security > Camera desktop-app switch.

## macOS Wi-Fi permissions

The macOS desktop app requires macOS 11 or later. Its adapter uses Apple's CoreWLAN framework directly. The desktop app
requests Location Services permission because macOS protects Wi-Fi network names.
Enable it under System Settings > Privacy & Security > Location Services if
access was denied. Terminal builds need authorization for their terminal app.
No location coordinates are collected. Camera scanning requires camera permission.

Current-network detection reads the live interface, never a saved profile or
historical log. Connections use a directed scan, check the requested security,
associate through CoreWLAN, then verify the live SSID before reporting success.
Passwords stay in memory and are never passed through command arguments.

## Platform support

| OS      | Detect SSID              | List networks                        | Read password                    | Connect                          |
| ------- | ------------------------ | ------------------------------------ | -------------------------------- | -------------------------------- |
| macOS | CoreWLAN live interface | CoreWLAN scan | CoreWLAN Keychain API | CoreWLAN association + verification |
| Linux   | NetworkManager D-Bus    | NetworkManager access-point objects | NetworkManager settings/secrets | NetworkManager activation API    |
| Windows | Native Wi-Fi query       | Native Wi-Fi available-network list | Native Wi-Fi profile XML        | Native Wi-Fi profile + connect   |

Linux requires NetworkManager managing the wireless interface. Saved profiles
are resolved by their SSID bytes, including renamed profiles; punctuation,
Unicode names, and hidden networks are supported. No `nmcli` executable or
localized command output is used. A missing SSID triggers a directed scan,
and the adapter waits for scan completion before reporting it unavailable. Windows uses
structured Native Wi-Fi data instead of localized `netsh` output. Personal
WPA/WPA2/WPA3, WEP, and open profiles retain their security and hidden settings;
enterprise authentication cannot be represented by this toolkit's personal Wi-Fi QR model.

`cargo test` runs all platforms' parser/profile regressions on every host, without
changing Wi-Fi settings. CI builds, tests, and lints the complete workspace on
Linux, Windows, and both Mac architectures, and tests the declared Rust 1.88
minimum separately. Real Wi-Fi association and camera capture still require
hardware checks on each OS. Both Linux and Windows wait for actual OS connection
completion before reporting success. Windows uses Native Wi-Fi completion
notifications; Linux checks the activated connection and its actual SSID.
Public Wi-Fi may still require its browser sign-in page after joining.

## License

MIT
