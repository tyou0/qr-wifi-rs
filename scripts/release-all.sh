#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  ./scripts/release-all.sh <version>

Example:
  ./scripts/release-all.sh 0.2.2

Creates one coordinated release from canonical Gitea repository:
  1. validates clean main and both forge remotes
  2. updates Cargo and Tauri versions
  3. runs release/format/test/GUI checks
  4. commits and atomically pushes main plus annotated tag to Gitea
  5. Gitea push mirror syncs tag to GitHub
  6. GitHub Actions builds uniquely named CLI and desktop packages for Linux,
     Windows, macOS ARM64, and macOS Intel, then publishes GitHub Release
USAGE
}

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
  "")
    usage >&2
    exit 2
    ;;
esac

version=${1#v}
tag="v$version"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]]; then
  printf 'Invalid semantic version: %s\n' "$1" >&2
  exit 2
fi

for command in cargo git python3; do
  command -v "$command" >/dev/null || {
    printf 'Missing required command: %s\n' "$command" >&2
    exit 1
  }
done

cd "$(git rev-parse --show-toplevel)"

[[ "$(git branch --show-current)" == main ]] || {
  printf 'Release must run from main.\n' >&2
  exit 1
}

[[ -z "$(git status --porcelain)" ]] || {
  printf 'Release requires a clean worktree.\n' >&2
  git status --short >&2
  exit 1
}

for remote in gitea origin; do
  git remote get-url "$remote" >/dev/null 2>&1 || {
    printf 'Missing required git remote: %s\n' "$remote" >&2
    exit 1
  }
done

printf 'Fetching Gitea and GitHub refs...\n'
git fetch --quiet gitea main --tags
git fetch --quiet origin main --tags

head=$(git rev-parse HEAD)
for ref in gitea/main origin/main; do
  [[ "$(git rev-parse "$ref")" == "$head" ]] || {
    printf 'Local main is not synchronized with %s. Fetch/reconcile first.\n' "$ref" >&2
    exit 1
  }
done

if git rev-parse -q --verify "refs/tags/$tag" >/dev/null ||
   git ls-remote --exit-code --tags gitea "refs/tags/$tag" >/dev/null 2>&1 ||
   git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1; then
  printf 'Release tag already exists: %s\n' "$tag" >&2
  exit 1
fi

printf 'Release %s from %s\n' "$tag" "$head"
printf 'This will commit version files and publish main + %s to Gitea.\n' "$tag"
printf 'Continue? [y/N] '
read -r answer
[[ "$answer" =~ ^[Yy]$ ]] || {
  printf 'Cancelled.\n'
  exit 1
}

published=0
rollback() {
  rc=$?
  if (( rc != 0 && published == 0 )); then
    git tag -d "$tag" >/dev/null 2>&1 || true
    git reset --hard "$head" >/dev/null 2>&1 || true
    printf 'Release failed before publication; local worktree restored to %s.\n' "$head" >&2
  fi
  exit "$rc"
}
trap rollback EXIT

VERSION="$version" python3 - <<'PY'
import json
import os
from pathlib import Path
import re

version = os.environ["VERSION"]

cargo = Path("Cargo.toml")
text = cargo.read_text()
updated, count = re.subn(
    r'(?m)^(\[workspace\.package\]\nversion = ")[^"]+("$)',
    rf'\g<1>{version}\g<2>',
    text,
    count=1,
)
if count != 1:
    raise SystemExit("could not update [workspace.package] version in Cargo.toml")
cargo.write_text(updated)

tauri_path = Path("src-tauri/tauri.conf.json")
tauri = json.loads(tauri_path.read_text())
tauri["version"] = version
tauri_path.write_text(json.dumps(tauri, indent=2) + "\n")
PY

# Refresh workspace package versions in Cargo.lock, then enforce release gates.
cargo check -p qr-wifi-gui
./scripts/test-release-packaging.sh
cargo fmt --all -- --check
cargo test --locked
cargo check --locked -p qr-wifi-gui

git diff --check
git add Cargo.toml Cargo.lock src-tauri/tauri.conf.json
git commit -m "chore(release): $tag"
git tag -a "$tag" -m "QR Wi-Fi RS $tag"

git push --atomic gitea main "refs/tags/$tag"
published=1

printf '\nPublished %s to Gitea.\n' "$tag"
printf 'Gitea sync-on-commit mirror will send it to GitHub.\n'
printf 'GitHub Actions will publish: https://github.com/tyou0/qr-wifi-rs/releases/tag/%s\n' "$tag"
printf 'Workflow: https://github.com/tyou0/qr-wifi-rs/actions/workflows/release.yml\n'
