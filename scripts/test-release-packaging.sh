#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

[[ -x scripts/release-all.sh ]] || fail "scripts/release-all.sh must exist and be executable"

formula=Formula/qr-wifi-rs.rb
cmp -s "$formula" scripts/homebrew-formula.rb ||
  fail "checked-in tap formula copy must match Formula/qr-wifi-rs.rb"

python3 - "$formula" <<'PY' || fail "Homebrew stable formula source metadata is invalid"
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text()
urls = re.findall(r'^\s*url\s+"([^"]+)"\s*$', text, re.MULTILINE)
expected_url = re.compile(
    r"https://github\.com/tyou0/qr-wifi-rs/archive/refs/tags/"
    r"v[0-9]+\.[0-9]+\.[0-9]+(?:[+-][0-9A-Za-z.-]+)?\.tar\.gz"
)
if len(urls) != 1 or expected_url.fullmatch(urls[0]) is None:
    raise SystemExit("formula must contain exactly one complete public GitHub tag archive URL")

checksums = re.findall(r'^\s*sha256\s+"([^"]+)"\s*$', text, re.MULTILINE)
if len(checksums) != 1 or re.fullmatch(r"[0-9a-fA-F]{64}", checksums[0]) is None:
    raise SystemExit("formula must contain exactly one 64-hex SHA-256")
PY

help_output=$(scripts/release-all.sh --help)
grep -Fq './scripts/release-all.sh <version>' <<<"$help_output" || fail "release help must document the single command"
grep -Fq 'Gitea' <<<"$help_output" || fail "release help must identify the canonical Gitea push"
grep -Fq 'GitHub Actions' <<<"$help_output" || fail "release help must explain GitHub packaging"
for invalid_version in \
  0.0.0 \
  1.2.3-beta \
  1.2.3+build \
  01.2.3 \
  1.02.3 \
  1.2.03 \
  65536.1.1 \
  1.65536.1 \
  1.1.65536 \
  1.2 \
  1.2.3.4; do
  if python3 scripts/set-release-version.py "$invalid_version" --validate-only >/dev/null 2>&1; then
    fail "store-ready release accepted invalid version: $invalid_version"
  fi
done
for valid_version in 0.0.1 1.2.3 65535.65535.65535; do
  python3 scripts/set-release-version.py "$valid_version" --validate-only ||
    fail "store-ready release rejected valid version: $valid_version"
done

# Exercise the real version-rewrite helper in isolation. This proves all three
# source files change together without mutating the checkout under test.
version_fixture=$(mktemp -d)
trap 'rm -rf "$version_fixture"' EXIT
mkdir -p "$version_fixture/src-tauri" "$version_fixture/extension"
printf '[workspace.package]\nversion = "0.0.1"\n' >"$version_fixture/Cargo.toml"
printf '{"version":"0.0.1"}\n' >"$version_fixture/src-tauri/tauri.conf.json"
printf '{"manifest_version":3,"version":"0.0.1"}\n' >"$version_fixture/extension/manifest.json"
python3 scripts/set-release-version.py 12.34.56 --root "$version_fixture"
python3 - "$version_fixture" <<'PY' || fail "coordinated version rewrite failed"
import json
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
cargo = root.joinpath("Cargo.toml").read_text()
versions = {
    re.search(r'version = "([^"]+)"', cargo).group(1),
    json.loads(root.joinpath("src-tauri/tauri.conf.json").read_text())["version"],
    json.loads(root.joinpath("extension/manifest.json").read_text())["version"],
}
if versions != {"12.34.56"}:
    raise SystemExit(f"unexpected rewritten versions: {versions}")
PY

python3 - <<'PY' || fail "workspace, Tauri, and extension versions must match"
import json
import re
from pathlib import Path

cargo = Path("Cargo.toml").read_text()
match = re.search(r'(?m)^\[workspace\.package\]\nversion = "([^"]+)"$', cargo)
if match is None:
    raise SystemExit("workspace package version not found")
workspace_version = match.group(1)
tauri_version = json.loads(Path("src-tauri/tauri.conf.json").read_text())["version"]
extension_version = json.loads(Path("extension/manifest.json").read_text())["version"]
if len({workspace_version, tauri_version, extension_version}) != 1:
    raise SystemExit(
        f"version drift: workspace={workspace_version}, tauri={tauri_version}, "
        f"extension={extension_version}"
    )
PY

grep -Fq 'python3 scripts/set-release-version.py "$version"' scripts/release-all.sh ||
  fail "release script must run the coordinated version helper"
grep -Fq 'git add Cargo.toml Cargo.lock src-tauri/tauri.conf.json extension/manifest.json' scripts/release-all.sh ||
  fail "release commit must stage every coordinated version file"

for topic in \
  'Chrome Web Store blueprint' \
  'Firefox Add-ons (AMO) blueprint' \
  'WinGet' \
  'Microsoft Store' \
  'macOS signing, notarization, and Mac App Store' \
  'Homebrew personal tap' \
  'Rollback and incident response'; do
  grep -Fq "$topic" docs/DEPLOYMENT_GUIDE.md ||
    fail "deployment guide is missing topic: $topic"
done
grep -Fq 'not configured' docs/DEPLOYMENT_GUIDE.md ||
  fail "deployment guide must distinguish future store publication from current automation"

workflow=.github/workflows/release.yml
if grep -Fq 'mapfile' "$workflow"; then
  fail "release collection must support macOS Bash 3.2 (mapfile is unavailable)"
fi
for bundle in 'appimage,deb,rpm' 'nsis,msi' 'app,dmg'; do
  grep -Fq "bundles: $bundle" "$workflow" || fail "missing desktop bundle matrix: $bundle"
done

grep -Fq 'package="qr-wifi-rs-cli-${version}-${{ matrix.target }}"' "$workflow" || fail "missing unique CLI package prefix"
grep -Fq 'tar -czf "dist/$package.tar.gz"' "$workflow" || fail "missing versioned CLI tarball"

for package_pattern in \
  'qr-wifi-rs-desktop-${version}-${{ matrix.target }}.deb' \
  'qr-wifi-rs-desktop-${version}-${{ matrix.target }}.AppImage' \
  'qr-wifi-rs-desktop-${version}-${{ matrix.target }}.rpm' \
  'qr-wifi-rs-desktop-${version}-${{ matrix.target }}-setup.exe' \
  'qr-wifi-rs-desktop-${version}-${{ matrix.target }}.msi' \
  'qr-wifi-rs-desktop-${version}-${{ matrix.target }}.app.zip' \
  'qr-wifi-rs-desktop-${version}-${{ matrix.target }}.dmg' \
  'qr-wifi-rs-browser-extension-${version}-unsigned.zip'; do
  grep -Fq "$package_pattern" "$workflow" || fail "missing unique release filename: $package_pattern"
done

printf 'release packaging contract: PASS\n'
