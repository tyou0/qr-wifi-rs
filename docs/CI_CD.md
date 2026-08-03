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

It runs when a tag starting with `v` is pushed:

```sh
git tag v0.2.1
git push origin v0.2.1
```

It builds and uploads:

- Linux x86_64 CLI/TUI/native-host tarball
- macOS ARM64 and Intel CLI/TUI/native-host tarballs
- Windows CLI/TUI/native-host tarball
- Linux desktop `.deb` and AppImage
- Windows desktop NSIS installer
- macOS ARM64 and Intel desktop `.app` zips and `.dmg` files
- unsigned Chrome/Firefox extension package for local loading

On tag pushes, it also creates a GitHub Release using the built-in
`GITHUB_TOKEN`. The workflow has `contents: write` permission for that.

## Homebrew release note

The personal tap installs the pinned `v0.2.1` source tarball:

```sh
brew trust tyou0/qr-wifi-rs
brew tap tyou0/qr-wifi-rs
brew install qr-wifi-rs
```

The tap is hosted at `https://github.com/tyou0/homebrew-qr-wifi-rs`; nothing is
submitted to Homebrew core.
