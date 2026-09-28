#!/usr/bin/env bash
# Package existing native build outputs with the shared all-components installer.
set -euo pipefail
cd "$(dirname "$0")/.."
version=${1:?Usage: package-suite.sh VERSION PLATFORM [BUILD_DIR]}
platform=${2:?Missing platform}
build_dir=${3:-target/release}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { printf 'Invalid version\n' >&2; exit 1; }
case "$platform" in
  linux-x86_64|macos-arm64|macos-x86_64) suffix="" ;;
  windows-x86_64) suffix=.exe ;;
  *) printf 'Unsupported platform: %s\n' "$platform" >&2; exit 1 ;;
esac
package="qr-wifi-rs-${version}-${platform}"
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
mkdir -p "$staging/$package/bin" dist
for name in qr-wifi qr-wifi-tui qr-wifi-host qr-wifi-gui; do
  cp "$build_dir/$name$suffix" "$staging/$package/bin/"
done
if [[ "$platform" == macos-* ]]; then
  ditto "$build_dir/bundle/macos/QR Wi-Fi RS.app" "$staging/$package/QR Wi-Fi RS.app"
fi
cp src-tauri/icons/128x128.png "$staging/$package/icon.png"
cp scripts/install.sh scripts/install.ps1 README.md LICENSE "$staging/$package/"
printf '%s\n' "$platform" > "$staging/$package/platform.txt"
tar -czf "dist/$package.tar.gz" -C "$staging" "$package"
printf 'Combined installer archive: dist/%s.tar.gz\n' "$package"
