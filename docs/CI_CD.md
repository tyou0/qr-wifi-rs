# CI/CD

This repo has GitHub Actions workflows and a small Gitea Actions CI workflow.

## CI

GitHub CI lives at:

- `.github/workflows/ci.yml`

It runs on pushes, pull requests, and manual dispatch:

- `cargo fmt --all -- --check`
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo test --locked`
- `cargo check --locked -p qr-wifi-gui`
- process-level Native Messaging host test
- `cargo audit`
- Chrome/Firefox extension lint
- Homebrew formula Ruby syntax check
- native-host installer syntax and behavior test

The Rust job runs on:

- Ubuntu x86_64
- Windows x86_64
- macOS ARM64
- macOS Intel

Gitea CI lives at:

- `.gitea/workflows/ci.yml`

It runs the same checks on Linux. Add macOS/Windows Gitea runners later if your
Gitea instance has them.

## Release/CD

GitHub release automation lives at:

- `.github/workflows/release.yml`

### Single release command

Run from a clean, synchronized `main` checkout on Podman:

```sh
./scripts/release-all.sh 0.2.2
```

The script validates Gitea/GitHub refs, updates `Cargo.toml`, `Cargo.lock`, and
`src-tauri/tauri.conf.json`, runs the packaging contract plus Rust release gates,
creates an annotated `v0.2.2` tag, and atomically pushes `main` and the tag to the
canonical `gitea` remote. Gitea's sync-on-commit push mirror then sends both refs
to GitHub. The mirrored `v*` tag triggers GitHub Actions and creates the GitHub
Release.

Use a new semantic version for every run. The script refuses a dirty worktree,
non-`main` branch, forge-ref drift, or an existing local/Gitea/GitHub tag.

### Package matrix and unique names

Every filename includes component, version, OS, and architecture. This prevents
artifact collisions when Gitea synchronizes the release tag to GitHub.

| Target | Terminal package | Desktop packages |
| --- | --- | --- |
| Linux x86_64 | `qr-wifi-rs-cli-0.2.2-linux-x86_64.tar.gz` | `.AppImage`, `.deb`, `.rpm` |
| Windows x86_64 | `qr-wifi-rs-cli-0.2.2-windows-x86_64.tar.gz` | `-setup.exe` (NSIS), `.msi` |
| macOS ARM64 | `qr-wifi-rs-cli-0.2.2-macos-arm64.tar.gz` | `.app.zip`, `.dmg` |
| macOS Intel | `qr-wifi-rs-cli-0.2.2-macos-x86_64.tar.gz` | `.app.zip`, `.dmg` |

The release also contains:

- `qr-wifi-rs-browser-extension-0.2.2-unsigned.zip`
- `qr-wifi-rs-0.2.2-SHA256SUMS.txt`

A manual tag push still works, but the release script is the supported path
because it keeps Cargo/Tauri versions synchronized and runs the release gates.

## Homebrew release note

The personal tap installs the pinned `v0.2.4` public GitHub source tarball:

```sh
brew trust tyou0/qr-wifi-rs
brew tap tyou0/qr-wifi-rs
brew install qr-wifi-rs
```

The tap is hosted at `https://github.com/tyou0/homebrew-qr-wifi-rs`; nothing is
submitted to Homebrew core.
