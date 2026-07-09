#!/usr/bin/env sh
set -eu

host_name="com.thetomyou.qrwifi"
install_dir="${QR_WIFI_INSTALL_DIR:-$HOME/.local/bin}"
host_path="$install_dir/qr-wifi-host"
chrome_id="${QR_WIFI_CHROME_EXTENSION_ID:-}"

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
    -h|--help)
      cat <<'USAGE'
Usage: scripts/install-native-host.sh [--chrome-extension-id ID] [--install-dir DIR]

Builds qr-wifi-host, installs it under ~/.local/bin by default, and registers
user-level Native Messaging manifests for Chrome/Chromium and Firefox.

Chrome requires the unpacked extension ID. Load extension/ first, copy the ID,
then rerun with --chrome-extension-id ID.
USAGE
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      exit 2
      ;;
  esac
done

cargo build --release -p qr-wifi-host
mkdir -p "$install_dir"
cp target/release/qr-wifi-host "$host_path"
chmod 755 "$host_path"

case "$(uname -s)" in
  Darwin)
    chrome_dir="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
    chromium_dir="$HOME/Library/Application Support/Chromium/NativeMessagingHosts"
    firefox_dir="$HOME/Library/Application Support/Mozilla/NativeMessagingHosts"
    ;;
  Linux)
    chrome_dir="$HOME/.config/google-chrome/NativeMessagingHosts"
    chromium_dir="$HOME/.config/chromium/NativeMessagingHosts"
    firefox_dir="$HOME/.mozilla/native-messaging-hosts"
    ;;
  *)
    echo "Native host auto-registration supports macOS/Linux. Windows: use extension/com.thetomyou.qrwifi.json.template and registry path from README." >&2
    exit 1
    ;;
esac

write_manifest() {
  dir="$1"
  mkdir -p "$dir"
  cat > "$dir/$host_name.json" <<EOF
{
  "name": "$host_name",
  "description": "QR Wi-Fi RS native messaging host",
  "path": "$host_path",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://${chrome_id:-REPLACE_WITH_EXTENSION_ID}/"],
  "allowed_extensions": ["qr-wifi-rs@thetomyou.com"]
}
EOF
}

write_manifest "$chrome_dir"
write_manifest "$chromium_dir"
write_manifest "$firefox_dir"

echo "Installed $host_path"
echo "Registered Native Messaging host manifests."
if [ -z "$chrome_id" ]; then
  echo "Chrome/Chromium manifest contains REPLACE_WITH_EXTENSION_ID. Rerun with --chrome-extension-id after loading extension/."
fi
