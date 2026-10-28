# Security Model

QR Wi-Fi RS handles Wi-Fi passwords. This chapter documents where secrets
travel, which boundaries are trusted, and which risks remain.

## Trust boundaries

The application is local-only. It has no server and sends no telemetry.

1. A frontend asks for a feature.
2. The desktop app calls a Tauri command; CLI and TUI call the core directly.
3. `qr-wifi-core` invokes the operating system's Wi-Fi tooling.
4. Credentials return only to that local frontend and may be encoded into a QR.

The generated QR and raw `WIFI:` payload contain the Wi-Fi password by design.
Anyone who can see or copy either value can join that network.

## Protections in this repository

- OS commands use `std::process::Command`, never a shell, so SSIDs cannot inject
  shell syntax.
- Sensitive command arguments are redacted from returned errors.
- `WifiCredentials` redacts passwords from Rust `Debug` output.
- Encoded image size, decoded dimensions, and decoder allocations are bounded.
- Tauri loads local assets under a restrictive Content Security Policy.
- Windows Wi-Fi profiles are passed directly to the Native Wi-Fi API; plaintext
  passwords are not written to temporary profile files.
- Linux uses NetworkManager's typed D-Bus API. Passwords remain in memory and
  the OS credential store; no Wi-Fi subprocess receives password arguments.
  D-Bus error descriptions are omitted because a daemon could quote a secret.

## Residual risks

- macOS uses CoreWLAN with in-memory passwords, avoiding password-bearing
  process arguments. Wi-Fi identity access requires Location Services permission.
- Ad-hoc macOS signing protects bundle integrity but does not establish a trusted
  developer identity. Downloaded builds still require manual approval in macOS.
- Tauri's current Linux WebKit stack depends on unmaintained GTK3 Rust bindings
  and `glib 0.18`, which carries RUSTSEC-2024-0429. The affected iterator is
  upstream code; keep Tauri/WebKit updated and treat Linux desktop as an open
  dependency risk until Tauri moves to maintained bindings.
- Virtual machines can verify compilation, tests, installers, and protocol
  behavior. Real Wi-Fi radios, camera permissions, and OS credential stores need
  physical-device tests.

## Verification commands

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo audit
```

## Study exercise

Trace a password from `WifiCredentials` through one frontend. Mark every point
where it is serialized, displayed, passed to an OS process, or intentionally
discarded. Then add a test that would fail if a new log exposed it.
