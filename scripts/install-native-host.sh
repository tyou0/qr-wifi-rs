#!/usr/bin/env sh
set -eu

# QR Wi-Fi RS Native Messaging Host Installer
#
# Builds qr-wifi-host, installs it, and registers Native Messaging manifests
# for Chrome/Chromium/Firefox. Supports macOS, Linux, and Windows.

host_name="com.thetomyou.qrwifi"
install_dir="${QR_WIFI_INSTALL_DIR:-$HOME/.local/bin}"
host_path="$install_dir/qr-wifi-host"
chrome_id="${QR_WIFI_CHROME_EXTENSION_ID:-}"
firefox_id="qr-wifi-rs@thetomyou.com"
uninstall=false

# ANSI colors
reset='\033[0m'
bold='\033[1m'
green='\033[92m'
yellow='\033[93m'
blue='\033[94m'

info() { printf "${blue}%s${reset}\n" "$*"; }
success() { printf "${green}%s${reset}\n" "$*"; }
warn() { printf "${yellow}%s${reset}\n" "$*"; }
header() { printf "\n${bold}%s${reset}\n" "$*"; }

show_help() {
  cat <<'EOF'
Usage: scripts/install-native-host.sh [OPTIONS]

Builds qr-wifi-host, installs it locally, and registers Native Messaging manifests
for Chrome/Chromium/Firefox browsers.

OPTIONS:
  --chrome-extension-id ID    Chrome/Chromium extension ID (required for those browsers)
  --install-dir DIR           Installation directory (default: ~/.local/bin)
  --uninstall                 Remove installed binary and manifests
  -h, --help                  Show this help message

EXAMPLES:
  # Install (macOS/Linux)
  scripts/install-native-host.sh --chrome-extension-idabcdefghijklmnopqrstuvwxyz

  # Install (Windows)
  scripts/install-native-host.sh --chrome-extension-id ABC...XYZ --install-dir "%APPDATA%\qr-wifi-rs"

  # Uninstall
  scripts/install-native-host.sh --uninstall

CHROME EXTENSION ID:
  Load 'extension/' as an unpacked extension first, then copy the extension ID
  from chrome://extensions/ (the "ID" field near the extension name).

  On Firefox, the ID is fixed (qr-wifi-rs@thetomyou.com) and doesn't need to be specified.
EOF
}

detect_platform() {
  case "$(uname -s)" in
    Darwin)  echo "macos" ;;
    Linux)   echo "linux" ;;
    MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
    *) echo "unknown" ;;
  esac
}

platform="$(detect_platform)"

# Parse arguments
while [ "$#" -gt 0 ]; do
  case "$1" in
    --chrome-extension-id)
      chrome_id="${2:?missing Chrome extension ID}"
      shift 2
      ;;
    --install-dir)
      install_dir="${2:?missing install directory}"
      host_path="$install_dir/qr-wifi-host"
      shift 2
      ;;
    --uninstall)
      uninstall=true
      shift
      ;;
    -h|--help)
      show_help
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      show_help
      exit 2
      ;;
  esac
done

# Remove manifests and binary
uninstall_host() {
  header "Uninstalling QR Wi-Fi RS Native Messaging host..."

  removed=0

  case "$platform" in
    macos|linux)
      chrome_dir="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
      chromium_dir="$HOME/Library/Application Support/Chromium/NativeMessagingHosts"
      firefox_dir="$HOME/Library/Application Support/Mozilla/NativeMessagingHosts"

      if [ "$platform" = "linux" ]; then
        chrome_dir="$HOME/.config/google-chrome/NativeMessagingHosts"
        chromium_dir="$HOME/.config/chromium/NativeMessagingHosts"
        firefox_dir="$HOME/.mozilla/native-messaging-hosts"
      fi

      for dir in "$chrome_dir" "$chromium_dir" "$firefox_dir"; do
        manifest="$dir/$host_name.json"
        if [ -f "$manifest" ]; then
          rm -f "$manifest"
          info "Removed: $manifest"
          removed=$((removed + 1))
        fi
      done

      if [ -f "$host_path" ]; then
        rm -f "$host_path"
        info "Removed: $host_path"
        removed=$((removed + 1))
      fi
      ;;
    windows)
      # Windows requires registry operations
      if command -v reg >/dev/null 2>&1; then
        reg_path="HKCU\Software\Google\Chrome\NativeMessagingHosts\$host_name"
        if reg query "$reg_path" >/dev/null 2>&1; then
          reg delete "$reg_path" /f >/dev/null 2>&1
          info "Removed Chrome registry entry"
          removed=$((removed + 1))
        fi
      fi

      if [ -f "$host_path" ]; then
        rm -f "$host_path"
        info "Removed: $host_path"
        removed=$((removed + 1))
      fi
      ;;
  esac

  if [ "$removed" -eq 0 ]; then
    warn "Nothing to remove (host not installed)"
  else
    success "Uninstalled $removed item(s)"
  fi

  exit 0
}

# Perform uninstall if requested
if [ "$uninstall" = true ]; then
  uninstall_host
fi

# Install
header "Installing QR Wi-Fi RS Native Messaging host..."

# Build the host binary
info "Building qr-wifi-host..."
cargo build --release -p qr-wifi-host

# Create install directory
mkdir -p "$install_dir"

# Copy binary
info "Installing to: $host_path"
cp target/release/qr-wifi-host "$host_path"
chmod 755 "$host_path"

# Write manifests based on platform
case "$platform" in
  macos)
    chrome_dir="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
    chromium_dir="$HOME/Library/Application Support/Chromium/NativeMessagingHosts"
    firefox_dir="$HOME/Library/Application Support/Mozilla/NativeMessagingHosts"
    ;;
  linux)
    chrome_dir="$HOME/.config/google-chrome/NativeMessagingHosts"
    chromium_dir="$HOME/.config/chromium/NativeMessagingHosts"
    firefox_dir="$HOME/.mozilla/native-messaging-hosts"
    ;;
  windows)
    warn "Windows: Manual registry setup required"
    warn "See below for instructions"
    ;;
esac

write_manifest_file() {
  dir="$1"
  mkdir -p "$dir"
  cat > "$dir/$host_name.json" <<EOF
{
  "name": "$host_name",
  "description": "QR Wi-Fi RS native messaging host",
  "path": "$host_path",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://${chrome_id:-REPLACE_WITH_EXTENSION_ID}/"],
  "allowed_extensions": ["$firefox_id"]
}
EOF
}

# Write registry key on Windows
write_registry() {
  if command -v reg >/dev/null 2>&1; then
    reg_path="HKCU\Software\Google\Chrome\NativeMessagingHosts\$host_name"
    reg add "$reg_path" /ve /t REG_SZ /d "$host_path" /f >/dev/null 2>&1
    reg add "$reg_path" /v "AllowedOrigins" /t REG_SZ /d "chrome-extension://${chrome_id}/" /f >/dev/null 2>&1
    success "Registered Chrome Native Messaging host"
  fi
}

case "$platform" in
  macos|linux)
    write_manifest_file "$chrome_dir"
    write_manifest_file "$chromium_dir"
    write_manifest_file "$firefox_dir"
    success "Registered Native Messaging hosts"
    ;;
  windows)
    if [ -n "$chrome_id" ]; then
      write_registry
    else
      warn "Skipping Chrome registration (no extension ID provided)"
    fi
    ;;
esac

# Summary
header "Installation complete!"

success "Binary: $host_path"

case "$platform" in
  macos|linux)
    success "Manifests:"
    info "  Chrome:    $chrome_dir/$host_name.json"
    info "  Chromium: $chromium_dir/$host_name.json"
    info "  Firefox:  $firefox_dir/$host_name.json"

    if [ -z "$chrome_id" ]; then
      warn ""
      warn "⚠️  Chrome/Chromium manifests contain a placeholder extension ID."
      warn ""
      warn "To fix:"
      warn "  1. Load 'extension/' as an unpacked extension"
      warn "  2. Copy the extension ID from chrome://extensions/"
      warn "  3. Re-run this script with --chrome-extension-id ID"
      warn ""
      warn "Or edit the manifests directly and replace 'REPLACE_WITH_EXTENSION_ID'"
      warn "with your actual extension ID."
    fi
    ;;
  windows)
    info ""
    info "For Firefox on Windows, create this file:"
    info "  %APPDATA%\\Mozilla\\NativeMessagingHosts\\$host_name.json"
    info ""
    cat <<'EOF'
{
  "name": "com.thetomyou.qrwifi",
  "description": "QR Wi-Fi RS native messaging host",
  "path": "PATH_TO_QR_WIFI_HOST",
  "type": "stdio",
  "allowed_extensions": ["qr-wifi-rs@thetomyou.com"]
}
EOF
    info ""
    info "Replace PATH_TO_QR_WIFI_HOST with: $host_path"
    ;;
esac

info ""
info "Next steps:"
info "  1. Load 'extension/' as an unpacked extension in your browser"
info "  2. Click the extension icon to open the popup"
info "  3. Grant native messaging permission when prompted"
info ""
success "Ready!"
