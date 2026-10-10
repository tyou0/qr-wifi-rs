#!/usr/bin/env bash
set -euo pipefail

# Create inactive profiles only in a disposable Linux container. Refuse to
# configure NetworkManager on a developer's desktop or a bare CI runner.
if [[ $(uname -s) != Linux || $EUID -ne 0 ]] ||
   [[ ! -f /.dockerenv && ! -f /run/.containerenv ]]; then
  printf 'Run this test as root inside a disposable Linux container with NetworkManager, D-Bus, and Rust installed.\n' >&2
  exit 1
fi
cd "$(dirname "$0")/.."
export LC_ALL=C.UTF-8
mkdir -p /run/dbus /etc/NetworkManager/conf.d
printf '[keyfile]\nunmanaged-devices=interface-name:*\n' > /etc/NetworkManager/conf.d/qr-wifi-tests.conf
if [[ ! -S /run/dbus/system_bus_socket ]]; then
  dbus-daemon --system --fork
fi
qr_wifi_nm_pid=''
cleanup() {
  if [[ -n $qr_wifi_nm_pid ]]; then kill "$qr_wifi_nm_pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
if ! nmcli general status >/dev/null 2>&1; then
  NetworkManager --no-daemon --log-level=WARN > /tmp/qr-wifi-networkmanager.log 2>&1 &
  qr_wifi_nm_pid=$!
fi
for attempt in {1..30}; do
  if nmcli general status >/dev/null 2>&1; then break; fi
  sleep 0.5
done

nmcli connection add type wifi con-name 'qr-wifi-test-renamed-wpa2' ifname wlan0 \
  ssid '  카페:Guest\WiFi  ' wifi-sec.key-mgmt wpa-psk wifi-sec.proto rsn \
  wifi-sec.psk '  fixture:p\ass  ' wifi.hidden yes connection.autoconnect no
nmcli connection add type wifi con-name 'qr-wifi-test-renamed-open' ifname wlan0 \
  ssid 'Guest:Open' connection.autoconnect no
nmcli connection add type wifi con-name 'qr-wifi-test-renamed-wep' ifname wlan0 \
  ssid 'Legacy' wifi-sec.key-mgmt none wifi-sec.wep-tx-keyidx 2 \
  wifi-sec.wep-key2 abcde connection.autoconnect no
nmcli connection add type wifi con-name 'qr-wifi-test-renamed-wpa3' ifname wlan0 \
  ssid 'WPA3:Fixture' wifi-sec.key-mgmt sae wifi-sec.psk fixture-password connection.autoconnect no

QR_WIFI_NMCLI_FIXTURES=1 cargo test --locked -p qr-wifi-core \
  --test linux_nmcli_credentials -- --ignored
