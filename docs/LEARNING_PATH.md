# QR Wi-Fi RS Learning Path

This repo can be used as a practical Rust textbook. Each chapter has a real
feature, Rust concepts to learn, files to read, and a small exercise.

The rule: learn Rust through app behavior, not toy snippets.

## How to use this path

Work in short loops:

1. Read the listed file.
2. Run the listed example or test.
3. Make the exercise change.
4. Keep the frontend thin. Shared behavior belongs in `qr-wifi-core`.

Suggested pace: one chapter per session.

## Chapter 1: Cargo workspace

Goal: understand how one repo can contain multiple Rust crates.

Read:

- `Cargo.toml`
- `crates/core/Cargo.toml`
- `crates/cli/Cargo.toml`

Learn:

- Workspaces
- Package vs crate
- Shared dependency versions
- `default-members`
- Local path dependencies

Run:

```sh
cargo build -p qr-wifi-core
cargo build -p qr-wifi-cli
```

Exercise:

Add a new example under `crates/core/examples/` that prints a `WifiCredentials`
value with `Debug`.

Checkpoint:

You can explain why `qr-wifi-cli` depends on `qr-wifi-core`, but `qr-wifi-core`
does not depend on the CLI.

## Chapter 2: Domain types

Goal: learn structs, enums, methods, derives, and basic tests.

Read:

- `crates/core/src/types.rs`

Learn:

- `struct`
- `enum`
- `impl`
- `#[derive(Debug, Clone, PartialEq, Eq)]`
- `serde` renaming
- Builder-style methods
- Slice mutation with `&mut [T]`

Run:

```sh
cargo test -p qr-wifi-core types
```

Exercise:

Add a test for sorting two inactive networks whose names differ only by case.

Checkpoint:

You can explain why `WifiSecurity::Nopass` does not require a password.

## Chapter 3: WIFI payload parser

Goal: learn string parsing, escaping, `Result`, and edge-case tests.

Read:

- `crates/core/src/payload.rs`
- `crates/core/examples/parse_payload.rs`

Learn:

- Borrowed `&str`
- Owned `String`
- `Option`
- `Result`
- `?`
- Private helper functions
- Unit tests inside `#[cfg(test)]`

Run:

```sh
cargo test -p qr-wifi-core payload
cargo run -p qr-wifi-core --example parse_payload
```

Exercise:

Add one failing-payload test before changing parser behavior. Then make the
smallest parser change that passes it.

Checkpoint:

You can explain why `WIFI:T:nopass;S:Open;P:ignored;;` returns no password.

## Chapter 4: QR encode/decode

Goal: learn crate APIs, binary data, base64, and end-to-end tests.

Read:

- `crates/core/src/qr.rs`
- `crates/core/examples/build_qr_png.rs`
- `crates/core/tests/payload_round_trip.rs`

Learn:

- Third-party crates
- `Vec<u8>`
- PNG bytes
- Base64 encoding
- Public integration tests

Run:

```sh
cargo test -p qr-wifi-core qr
cargo run -p qr-wifi-core --example build_qr_png -- "Guest" "s3cret" out.png
```

Exercise:

Add an integration test that builds credentials with escaped characters,
renders a QR, decodes it, and parses it back.

Checkpoint:

You can explain the difference between `to_png`, `to_png_base64`, and
`credentials_to_qr`.

## Chapter 5: Error handling

Goal: learn typed errors without exceptions.

Read:

- `crates/core/src/error.rs`

Learn:

- Error enums
- `thiserror`
- `#[from]`
- `Display`
- Library `Result<T>` aliases

Run:

```sh
cargo test -p qr-wifi-core error
```

Exercise:

Add a new test that converts an invalid base64 string into `CoreError`.

Checkpoint:

You can explain why core functions return `Result<T>` instead of panicking.

## Chapter 6: Platform abstraction

Goal: learn traits and OS-specific implementations.

Read:

- `crates/core/src/platform/mod.rs`
- Your OS file in `crates/core/src/platform/`

Learn:

- Traits
- Trait objects
- `Box<dyn Trait>`
- `Send + Sync`
- `#[cfg(target_os = "...")]`
- Parsing command output without running OS commands in tests

Run:

```sh
cargo test -p qr-wifi-core platform
```

Exercise:

Add one parser test using captured output from your OS command.

Checkpoint:

You can explain why every frontend receives `&dyn WifiAdapter`.

## Chapter 7: Shared service layer

Goal: learn how app features stay shared across CLI, TUI, GUI, and extension.

Read:

- `crates/core/src/service.rs`
- `crates/core/examples/service_contract.rs`
- `crates/core/tests/public_api_contract.rs`

Learn:

- Application service functions
- Fakes for testing
- Interior mutability with `Mutex`
- Public API contract tests
- Keeping UI code thin

Run:

```sh
cargo test -p qr-wifi-core public_api_contract
cargo run -p qr-wifi-core --example service_contract
```

Exercise:

Add a new service test before adding any new frontend behavior.

Checkpoint:

You can trace "share current Wi-Fi" from a UI action to `share_current()`.

## Chapter 8: Native Messaging IPC

Goal: learn serde-tagged enums and browser-to-native protocol design.

Read:

- `crates/core/src/ipc.rs`
- `crates/host/src/main.rs`
- `docs/IPC_PROTOCOL.md`

Learn:

- `#[serde(tag = "command")]`
- Request/response enums
- Length-prefixed binary framing
- `Read` and `Write` traits
- Protocol tests

Run:

```sh
cargo test -p qr-wifi-core ipc
```

Exercise:

Add a request/response serialization test for one command you care about.

Checkpoint:

You can explain why the browser extension never runs `nmcli`, `netsh`, or
`networksetup` directly.

## Chapter 9: CLI frontend

Goal: learn binary crates, argument parsing, and error presentation.

Read:

- `crates/cli/src/main.rs`

Learn:

- `main`
- `ExitCode`
- `clap`
- Converting typed errors to user messages
- Calling shared core functions

Run:

```sh
cargo run -p qr-wifi-cli -- --help
cargo run -p qr-wifi-cli -- --custom --ssid Guest --password s3cret
```

Exercise:

Add one CLI flag only after the equivalent core service behavior exists.

Checkpoint:

You can explain why the CLI should not parse `WIFI:` payloads itself.

## Chapter 10: TUI frontend

Goal: learn terminal UI as a thin caller.

Read:

- `crates/tui/src/lib.rs`
- `crates/tui/src/fuzzy.rs`

Learn:

- Library plus binary crate pattern
- Terminal input/output
- Small algorithms with unit tests
- Reusing the same menu from CLI

Run:

```sh
cargo test -p qr-wifi-tui
cargo run -p qr-wifi-tui
```

Exercise:

Add a fuzzy-search test before changing ranking behavior.

Checkpoint:

You can explain why `qr-wifi` with no flags can reuse `qr-wifi-tui`.

## Chapter 11: GUI and browser extension

Goal: learn integration boundaries.

Read:

- `src-tauri/src/main.rs`
- `frontend/main.js`
- `extension/popup.js`
- `extension/popup.html`

Learn:

- Tauri command wrappers
- Browser Native Messaging
- JSON protocol boundaries
- Keeping JS as presentation, not business logic

Run:

```sh
cargo check -p qr-wifi-gui
cargo run -p qr-wifi-host
```

Exercise:

Pick one frontend action and trace the core function it reaches. If the frontend
duplicates core logic, move that logic into `qr-wifi-core`.

Checkpoint:

You can explain how CLI, TUI, GUI, Chrome, and Firefox share the same features.

## Chapter 12: Packaging and install paths

Goal: learn how Rust apps become installable tools.

Read:

- `README.md` install section
- `scripts/install-native-host.sh`
- `scripts/homebrew-formula.rb`
- `src-tauri/tauri.conf.json`

Learn:

- `cargo install`
- Release binaries
- Native Messaging manifest locations
- Homebrew formula shape
- Tauri installers

Run:

```sh
cargo build --release
cargo install --path crates/cli
```

Exercise:

Document one install path you personally use, then make the script/docs match
that path exactly.

Checkpoint:

You can explain the difference between installing the CLI, the GUI app, and the
browser native host.

## Feature map

| Feature | Core function | Rust lesson | Frontends |
| --- | --- | --- | --- |
| List networks | `service::networks` | Sorting, traits, adapter boundary | CLI, TUI, GUI, extension |
| Share current Wi-Fi | `service::share_current` | Composition, `Result`, fake tests | CLI, TUI, GUI, extension |
| Share by SSID | `service::share_ssid` | Borrowed input, adapter lookup | CLI, TUI, extension |
| Custom QR | `service::share_custom` | Structs, enums, QR rendering | CLI, TUI, GUI, extension |
| Connect from payload | `service::connect_payload` | Parsing then side effects | CLI, TUI, GUI, extension |
| Decode QR image | `service::decode_qr_base64` / `decode_qr_path` | Binary data, base64, images | CLI, GUI, extension |
| Browser IPC | `ipc::handle_request` | Tagged enums, JSON, framing | Chrome, Firefox |

## Contribution rule for learners

When adding a feature:

1. Add or update the core type.
2. Add parser/QR/service behavior in `qr-wifi-core`.
3. Add unit or integration tests.
4. Add one runnable example if it teaches a new Rust concept.
5. Expose the feature through frontends with minimal wrapper code.
6. Update this learning path if the feature creates a new lesson.

This keeps the repo maintainable and keeps the learning material honest.
