#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

[[ -x scripts/release-all.sh ]] || fail "scripts/release-all.sh must exist and be executable"

help_output=$(scripts/release-all.sh --help)
grep -Fq './scripts/release-all.sh <version>' <<<"$help_output" || fail "release help must document the single command"
grep -Fq 'Gitea' <<<"$help_output" || fail "release help must identify the canonical Gitea push"
grep -Fq 'GitHub Actions' <<<"$help_output" || fail "release help must explain GitHub packaging"

workflow=.github/workflows/release.yml
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
