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
