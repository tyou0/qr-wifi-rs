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
- Homebrew formula Ruby syntax check
- native-host installer shell syntax check

The Rust job runs on:

- Ubuntu
- macOS
- Windows

Gitea CI lives at:

- `.gitea/workflows/ci.yml`

It runs the same checks on Linux. Add macOS/Windows Gitea runners later if your
Gitea instance has them.

## Release/CD

GitHub release automation lives at:

- `.github/workflows/release.yml`

It runs when a tag starting with `v` is pushed:

```sh
git tag v0.1.0
git push origin v0.1.0
```

It builds and uploads:

- Linux CLI/TUI/native-host tarball
- macOS CLI/TUI/native-host tarball
- Windows CLI/TUI/native-host tarball
- macOS desktop `.app` zip
- macOS desktop `.dmg`

On tag pushes, it also creates a GitHub Release using the built-in
`GITHUB_TOKEN`. The workflow has `contents: write` permission for that.

## Homebrew release note

The current formula installs the pinned `v0.1.0` source tarball:

```sh
brew install ./Formula/qr-wifi-rs.rb
```

It can also be installed directly from Gitea without cloning this repo:

```sh
brew install https://gitea.thetomyou.com/mistercorea/qr_wifi_rs/raw/branch/main/Formula/qr-wifi-rs.rb
```

Plain `brew install qr-wifi-rs` still requires publishing this formula in a
Homebrew tap.
