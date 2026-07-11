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

The current formula supports HEAD installs:

```sh
brew install --HEAD ./Formula/qr-wifi-rs.rb
```

Stable `brew install qr-wifi-rs` still needs a published source tarball URL and
real SHA256 in `Formula/qr-wifi-rs.rb`.

After a tag release, update the formula with:

```ruby
url "https://gitea.thetomyou.com/mistercorea/qr_wifi_rs/archive/v0.1.0.tar.gz"
sha256 "<release tarball sha256>"
```

Then publish the formula in a Homebrew tap.
