#!/usr/bin/env bash
# Install the desktop app and terminal tools for the current user.
set -euo pipefail

script_dir=$(cd "$(dirname "$0")" && pwd)
prefix="${HOME:?}/.local"
applications_dir="$HOME/Applications"
prebuilt=""

die() { printf '%s\n' "$*" >&2; exit 1; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix|--applications-dir|--prebuilt)
      [[ $# -ge 2 && -n "$2" ]] || die "Missing value for $1"
      case "$1" in
        --prefix) prefix=$2 ;;
        --applications-dir) applications_dir=$2 ;;
        --prebuilt) prebuilt=$2 ;;
      esac
      shift 2 ;;
    -h|--help)
      printf '%s\n' 'Usage: bash scripts/install.sh [--prefix DIR] [--applications-dir DIR] [--prebuilt DIR]' \
        'Build and install desktop + CLI + TUI + native host on macOS or Linux.' \
        'From a combined release archive, run: bash install.sh (no Rust needed).' \
        'Default: ~/.local/bin, ~/.local/share/applications (Linux), ~/Applications (macOS).'
      exit 0 ;;
    *) die "Unknown option: $1" ;;
  esac
done

case "$(uname -s)" in
  Darwin) os=macos ;;
  Linux) os=linux ;;
  *) die 'Use scripts/install.ps1 on Windows.' ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  arm64|aarch64) arch=arm64 ;;
  *) die 'Unsupported CPU architecture.' ;;
esac
[[ "$prefix" == /* && "$applications_dir" == /* ]] || die 'Install paths must be absolute.'

if [[ -z "$prebuilt" && -f "$script_dir/platform.txt" ]]; then
  prebuilt=$script_dir
fi
if [[ -n "$prebuilt" ]]; then
  [[ -f "$prebuilt/platform.txt" ]] || die 'Not a combined release archive: platform.txt is missing.'
  [[ "$(<"$prebuilt/platform.txt")" == "$os-$arch" ]] || die "Archive does not match $os-$arch. Download the matching release."
  binaries="$prebuilt/bin"
  app="$prebuilt/QR Wi-Fi RS.app"
  icon="$prebuilt/icon.png"
else
  repo=$(cd "$script_dir/.." && pwd)
  [[ -f "$repo/Cargo.toml" ]] || die 'Run from a source checkout or an extracted combined release.'
  command -v cargo >/dev/null || die 'Install stable Rust from https://rustup.rs first.'
  if [[ "$os" == linux ]]; then
    command -v pkg-config >/dev/null && pkg-config --exists webkit2gtk-4.1 gtk+-3.0 openssl ||
      die 'Missing Linux build dependencies. See README: Ubuntu / Arch prerequisites.'
  fi
  cd "$repo"
  host=$(rustc -vV | sed -n 's/^host: //p')
  [[ -n "$host" ]] || die 'Cannot determine the native Rust target.'
  export CARGO_TARGET_DIR="$repo/target"
  cargo tauri --version >/dev/null 2>&1 || cargo install tauri-cli --locked --version 2.11.4
  cargo build --release --locked --target "$host" -p qr-wifi-cli -p qr-wifi-tui -p qr-wifi-host
  if [[ "$os" == macos ]]; then
    cargo tauri build --ci --target "$host" --bundles app -- --locked
  else
    cargo tauri build --ci --target "$host" --no-bundle -- --locked
  fi
  binaries="$CARGO_TARGET_DIR/$host/release"
  app="$binaries/bundle/macos/QR Wi-Fi RS.app"
  icon="$repo/src-tauri/icons/128x128.png"
fi

# Validate the complete payload before changing the installation.
for name in qr-wifi qr-wifi-tui qr-wifi-host qr-wifi-gui; do
  [[ -x "$binaries/$name" ]] || die "Missing executable: $binaries/$name"
done
if [[ "$os" == macos ]]; then
  [[ -f "$app/Contents/Info.plist" && -x "$app/Contents/MacOS/qr-wifi-gui" ]] || die "Missing macOS app bundle: $app"
else
  [[ -f "$icon" ]] || die "Missing desktop icon: $icon"
fi

mkdir -p "$prefix/bin"
for name in qr-wifi qr-wifi-tui qr-wifi-host qr-wifi-gui; do
  install -m 755 "$binaries/$name" "$prefix/bin/$name"
done
if [[ "$os" == macos ]]; then
  mkdir -p "$applications_dir"
  ditto "$app" "$applications_dir/QR Wi-Fi RS.app"
  printf 'Desktop app: %s/QR Wi-Fi RS.app\n' "$applications_dir"
else
  data_dir="$prefix/share"
  mkdir -p "$data_dir/applications" "$data_dir/icons/hicolor/128x128/apps"
  install -m 644 "$icon" "$data_dir/icons/hicolor/128x128/apps/qr-wifi-rs.png"
  # Desktop Entry Exec quoting has two escaping layers; %% is a literal percent.
  exec_path=$(printf '%s' "$prefix/bin/qr-wifi-gui" | sed \
    -e 's/\\/\\\\\\\\/g' -e 's/"/\\\\"/g' -e 's/\$/\\\\$/g' -e 's/`/\\\\`/g' -e 's/%/%%/g')
  printf '%s\n' '[Desktop Entry]' 'Type=Application' 'Name=QR Wi-Fi RS' \
    "Exec=\"$exec_path\"" 'Icon=qr-wifi-rs' 'Terminal=false' 'Categories=Network;Utility;' \
    > "$data_dir/applications/com.thetomyou.qrwifirs.desktop"
  if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$data_dir/applications"
  fi
  printf 'Desktop launcher: %s/applications/com.thetomyou.qrwifirs.desktop\n' "$data_dir"
  command -v nmcli >/dev/null || printf '%s\n' 'Wi-Fi access requires NetworkManager (nmcli). See README prerequisites.'
fi
printf 'Installed desktop, CLI, TUI, and native host. Commands: %s/bin\n' "$prefix"
case ":$PATH:" in
  *":$prefix/bin:"*) ;;
  *) printf 'Add to your shell profile: export PATH=%q:"$PATH"\n' "$prefix/bin" ;;
esac
