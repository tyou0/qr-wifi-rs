#!/usr/bin/env sh
set -eu

# Install and register the QR Wi-Fi RS Native Messaging host.
# Browser-specific manifests stay separate because Chrome uses
# `allowed_origins` while Firefox uses `allowed_extensions`.

host_name="com.thetomyou.qrwifi"
firefox_id="qr-wifi-rs@thetomyou.com"
chrome_id="${QR_WIFI_CHROME_EXTENSION_ID:-}"
uninstall=false
skip_build=false
host_path_set=false

reset='\033[0m'
bold='\033[1m'
blue='\033[94m'

info() { printf "${blue}%s${reset}\n" "$*"; }
header() { printf "\n${bold}%s${reset}\n" "$*"; }

detect_platform() {
  case "$(uname -s)" in
    Darwin) echo "macos" ;;
    Linux) echo "linux" ;;
    MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
    *) echo "unknown" ;;
  esac
}

platform="${QR_WIFI_PLATFORM:-$(detect_platform)}"

case "$platform" in
  windows)
    host_binary="qr-wifi-host.exe"
    if [ -n "${LOCALAPPDATA:-}" ] && command -v cygpath >/dev/null 2>&1; then
      default_install_dir="$(cygpath -u "$LOCALAPPDATA")/QR Wi-Fi RS"
    else
      default_install_dir="$HOME/.local/share/qr-wifi-rs"
    fi
    ;;
  macos|linux)
    host_binary="qr-wifi-host"
    default_install_dir="$HOME/.local/bin"
    ;;
  *)
    echo "Unsupported platform: $platform" >&2
    exit 1
    ;;
esac

install_dir="${QR_WIFI_INSTALL_DIR:-$default_install_dir}"
if [ -n "${QR_WIFI_HOST_PATH:-}" ]; then
  host_path="$QR_WIFI_HOST_PATH"
  host_path_set=true
else
  host_path="$install_dir/$host_binary"
fi

show_help() {
  cat <<'EOF'
Usage: scripts/install-native-host.sh [OPTIONS]

Build and install qr-wifi-host, then register browser Native Messaging manifests.

OPTIONS:
  --chrome-extension-id ID  Chrome/Chromium extension ID
  --install-dir DIR         Installation directory
  --host-path PATH          Existing qr-wifi-host path to register
  --skip-build              Register an existing executable
  --uninstall               Remove manifests and script-installed binary
  -h, --help                Show help

Firefox uses the fixed add-on ID qr-wifi-rs@thetomyou.com. Chrome registration
is skipped unless --chrome-extension-id is supplied.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --chrome-extension-id)
      chrome_id="${2:?missing Chrome extension ID}"
      shift 2
      ;;
    --install-dir)
      install_dir="${2:?missing installation directory}"
      if [ "$host_path_set" = false ]; then
        host_path="$install_dir/$host_binary"
      fi
      shift 2
      ;;
    --host-path)
      host_path="${2:?missing host path}"
      host_path_set=true
      shift 2
      ;;
    --skip-build)
      skip_build=true
      shift
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

manifest_locations() {
  case "$platform" in
    macos)
      chrome_manifest="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts/$host_name.json"
      chromium_manifest="$HOME/Library/Application Support/Chromium/NativeMessagingHosts/$host_name.json"
      firefox_manifest="$HOME/Library/Application Support/Mozilla/NativeMessagingHosts/$host_name.json"
      ;;
    linux)
      chrome_manifest="$HOME/.config/google-chrome/NativeMessagingHosts/$host_name.json"
      chromium_manifest="$HOME/.config/chromium/NativeMessagingHosts/$host_name.json"
      firefox_manifest="$HOME/.mozilla/native-messaging-hosts/$host_name.json"
      ;;
    windows)
      chrome_manifest="$install_dir/$host_name.chrome.json"
      chromium_manifest=""
      firefox_manifest="$install_dir/$host_name.firefox.json"
      ;;
  esac
}

manifest_locations

remove_file() {
  if [ -n "$1" ] && [ -f "$1" ]; then
    rm -f "$1"
    info "Removed: $1"
  fi
}

if [ "$uninstall" = true ]; then
  header "Uninstalling QR Wi-Fi RS native host"
  remove_file "$chrome_manifest"
  remove_file "$chromium_manifest"
  remove_file "$firefox_manifest"

  if [ "$platform" = "windows" ] && command -v reg >/dev/null 2>&1; then
    reg delete "HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\$host_name" /f >/dev/null 2>&1 || true
    reg delete "HKCU\\Software\\Mozilla\\NativeMessagingHosts\\$host_name" /f >/dev/null 2>&1 || true
  fi

  if [ "$host_path_set" = false ]; then
    remove_file "$host_path"
  fi
  info "Uninstall complete"
  exit 0
fi

header "Installing QR Wi-Fi RS native host"

if [ "$skip_build" = true ]; then
  if [ ! -x "$host_path" ]; then
    echo "Host binary is not executable: $host_path" >&2
    exit 1
  fi
else
  cargo build --release --locked -p qr-wifi-host
  mkdir -p "$install_dir"
  cp "target/release/$host_binary" "$host_path"
  chmod 755 "$host_path"
fi

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

to_windows_path() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -w "$1"
  else
    printf '%s\n' "$1"
  fi
}

write_chrome_manifest() {
  file="$1"
  binary_path="$host_path"
  if [ "$platform" = "windows" ]; then
    binary_path="$(to_windows_path "$host_path")"
  fi
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<EOF
{
  "name": "$host_name",
  "description": "QR Wi-Fi RS native messaging host",
  "path": "$(json_escape "$binary_path")",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://$chrome_id/"]
}
EOF
}

write_firefox_manifest() {
  file="$1"
  binary_path="$host_path"
  if [ "$platform" = "windows" ]; then
    binary_path="$(to_windows_path "$host_path")"
  fi
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<EOF
{
  "name": "$host_name",
  "description": "QR Wi-Fi RS native messaging host",
  "path": "$(json_escape "$binary_path")",
  "type": "stdio",
  "allowed_extensions": ["$firefox_id"]
}
EOF
}

write_firefox_manifest "$firefox_manifest"
info "Firefox manifest: $firefox_manifest"

if [ -n "$chrome_id" ]; then
  write_chrome_manifest "$chrome_manifest"
  info "Chrome manifest: $chrome_manifest"
  if [ -n "$chromium_manifest" ]; then
    write_chrome_manifest "$chromium_manifest"
    info "Chromium manifest: $chromium_manifest"
  fi
else
  info "Chrome registration skipped: provide --chrome-extension-id ID"
fi

if [ "$platform" = "windows" ]; then
  if ! command -v reg >/dev/null 2>&1; then
    echo "Windows registry tool not found: reg" >&2
    exit 1
  fi

  firefox_registry_manifest="$(to_windows_path "$firefox_manifest")"
  reg add "HKCU\\Software\\Mozilla\\NativeMessagingHosts\\$host_name" /ve /t REG_SZ /d "$firefox_registry_manifest" /f >/dev/null

  if [ -n "$chrome_id" ]; then
    chrome_registry_manifest="$(to_windows_path "$chrome_manifest")"
    reg add "HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\$host_name" /ve /t REG_SZ /d "$chrome_registry_manifest" /f >/dev/null
  fi
fi

header "Installation complete"
info "Host: $host_path"
info "Restart browser after changing Native Messaging registration."
