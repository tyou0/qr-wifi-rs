#!/usr/bin/env sh
set -eu

# Runs the Windows installer branch without touching a real registry.

repo_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

mkdir -p "$tmp/bin" "$tmp/install"
host="$tmp/qr-wifi-host.exe"
reg_log="$tmp/reg.log"
: > "$host"
chmod 755 "$host"

cat > "$tmp/bin/reg" <<'EOF'
#!/usr/bin/env sh
printf '%s\n' "$*" >> "$QR_WIFI_REG_LOG"
EOF
chmod 755 "$tmp/bin/reg"

export QR_WIFI_REG_LOG="$reg_log"
PATH="$tmp/bin:$PATH" \
QR_WIFI_PLATFORM=windows \
QR_WIFI_INSTALL_DIR="$tmp/install" \
QR_WIFI_HOST_PATH="$host" \
  "$repo_root/scripts/install-native-host.sh" \
    --skip-build \
    --chrome-extension-id abcdefghijklmnopqrstuvwxyzabcdef

chrome_manifest="$tmp/install/com.thetomyou.qrwifi.chrome.json"
firefox_manifest="$tmp/install/com.thetomyou.qrwifi.firefox.json"

test -f "$chrome_manifest"
test -f "$firefox_manifest"
grep -F '"allowed_origins"' "$chrome_manifest" >/dev/null
grep -F '"allowed_extensions"' "$firefox_manifest" >/dev/null
grep -F 'Software\Google\Chrome\NativeMessagingHosts\com.thetomyou.qrwifi' "$reg_log" >/dev/null
grep -F 'Software\Mozilla\NativeMessagingHosts\com.thetomyou.qrwifi' "$reg_log" >/dev/null
grep -F '.chrome.json' "$reg_log" >/dev/null
grep -F '.firefox.json' "$reg_log" >/dev/null

printf '%s\n' "native host installer test passed"
