# QR Wi-Fi RS — Rust Study Guide

A beginner-friendly walkthrough of this codebase for someone who knows
**Node.js / Python / Go** but is new to Rust. It covers three things:

1. **Cargo & packages** — how to find, add, and manage libraries (the
   npm/pip/go analogy).
2. **The architecture** — how `qr_wifi_rs` is organized and *why*.
3. **Rust concepts in the code** — ownership, traits, `Result`, enums, etc.,
   each mapped to what you already know.

---

## 1. Cargo & packages (finding and using libraries)

### The big mental model

Rust's package manager is **Cargo**. A Rust library is called a **crate**. The
public registry is **[crates.io](https://crates.io)** (think npmjs.com /
PyPI / the Go module proxy). There is **no `node_modules` per project** and **no
virtualenv**: dependencies are downloaded once into a **shared global cache**
(`~/.cargo/registry`, like Go's module cache) and reused across all projects.

### Cheat-sheet (you know the left columns)

| Concept            | Node / npm            | Python / pip               | Go              | Rust / Cargo                          |
| ------------------ | --------------------- | -------------------------- | --------------- | ------------------------------------- |
| Registry           | npmjs.com             | pypi.org                   | pkg.go.dev      | **crates.io**                         |
| Manifest           | `package.json`        | `requirements.txt` / pyproject | `go.mod`     | **`Cargo.toml`**                      |
| Lockfile           | `package-lock.json`   | `poetry.lock` / `uv.lock`  | `go.sum`        | **`Cargo.lock`**                      |
| Add a dependency   | `npm install X`       | `pip install X`            | `go get X`      | **`cargo add X`**                     |
| Add dev dependency | `npm i -D X`          | `pip install X` (dev group)| (same)          | **`cargo add X --dev`**               |
| Search             | `npm search X`        | (web)                      | (web)           | **`cargo search X`**                  |
| Install registry docs offline | `npm i`      | —                          | —               | **`cargo doc --open`** (builds from comments) |
| Build              | `npm run build`       | —                          | `go build`      | **`cargo build`**                     |
| Run                | `node file.js`        | `python file.py`           | `go run .`      | **`cargo run`**                       |
| Test               | `npm test`            | `pytest`                   | `go test ./...` | **`cargo test`**                      |
| Lint               | eslint                | ruff / flake8              | `go vet` / golangci-lint | **`cargo clippy`**          |
| Format             | prettier              | black                      | `gofmt`         | **`cargo fmt`**                       |
| Update deps        | `npm update`          | `pip install -U`           | `go get -u`     | **`cargo update`**                    |
| Monorepo           | npm/pnpm workspaces   | (poetry/uv groups)         | one module, many packages | **Cargo workspace**       |

### How to find a good crate

1. **Search the registry**: `cargo search qrcode` or browse crates.io.
2. **Look at the signals** (just like judging an npm package):
   - Recent version + last-publish date (is it maintained?).
   - Downloads / "recent downloads" (popularity).
   - A GitHub link and open issues.
   - Whether the README has examples.
3. **Read the API docs**: every published crate auto-generates docs at
   **[docs.rs/CRATE_NAME](https://docs.rs)** — this is Rust's equivalent of
   pkg.go.dev, and it's built from the `///` doc comments in the source.
4. **Bench/community check**: <https://blessed.rs> lists "standard" crates per
   task (HTTP, CLI, JSON, etc.) — handy when you don't know the ecosystem.

### `cargo add` in practice (what we did here)

From the project root:

```sh
cargo add serde serde_json                 # serialization
cargo add thiserror                        # ergonomic error enums
cargo add qrcode                           # generate QR matrices
cargo add image                            # PNG encode/decode
cargo add rqrr                             # decode QR from images
cargo add base64                           # base64 helpers
cargo add clap --features derive           # CLI parser (with derive macros)
cargo add crossterm                        # terminal raw-mode + key events
cargo add tauri                            # desktop GUI
```

Each `cargo add` writes a line into `Cargo.toml` under `[dependencies]` and
updates `Cargo.lock`. You can also edit `Cargo.toml` by hand (common in Go-like
fashion) and run `cargo build`.

### Version strings (`^1`, `=1.2.3`, `*`)

Cargo's default is **caret** requirements: `serde = "1"` means `>=1.0.0, <2.0.0`
— the same idea as npm's `^1.0.0` and Go's `v1` module path guaranteeing
compatibility. `=1.2.3` pins exactly (like npm's `1.2.3`). You rarely pin
exactly; the lockfile makes builds reproducible.

### Workspaces (this project uses one)

A **Cargo workspace** = npm/pnpm workspaces or a Go repo with several packages.
One top-level `Cargo.toml` (here, `qr_wifi_rs/Cargo.toml`) declares `members`,
and shared dependency versions are pinned **once** in
`[workspace.dependencies]`:

```toml
[workspace]
members = ["crates/core", "crates/cli", "crates/tui", "crates/host", "src-tauri"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
qrcode = "0.14"
qr-wifi-core = { path = "crates/core" }   # local crate, not from the registry
```

Each member crate opts in with `serde = { workspace = true }`. Benefits: one
`Cargo.lock`, one `target/`, consistent versions, and local crates reference
each other by `path`.

### Features (optional functionality)

`features = ["derive"]` turns part of a crate on, like npm
`optionalDependencies` or Go build tags. Example: `clap`'s `derive` feature
enables the `#[derive(Parser)]` macro.

### Pro tips

- `cargo doc --open` generates HTML docs for **your** crate + deps from doc
  comments (Rust's built-in typedoc/godoc).
- `cargo expand` (needs `cargo install cargo-expand`) shows what macros expand
  to — great for learning what `#[derive(...)]` actually does.
- `cargo tree` prints the dependency graph (like `npm ls`).
- `rust-toolchain.toml` pins a toolchain per project (like an `.nvmrc`).

---

## 2. Architecture — how & why

### Goal

One Wi-Fi QR toolkit, four frontends (CLI, TUI, Tauri GUI, browser-extension
host) that must behave identically, plus pure logic that's easy to unit test.

### Layout

```
qr_wifi_rs/
├── Cargo.toml              workspace root (virtual manifest)
├── crates/
│   ├── core/   qr-wifi-core   shared LIBRARY: types, WIFI: payload,
│   │                          QR encode/decode, OS Wi-Fi adapters, IPC protocol
│   ├── cli/    qr-wifi        CLI (flags; no flags → menu)
│   ├── tui/    qr-wifi-tui    menu + built-in fuzzy finder (lib + bin)
│   └── host/   qr-wifi-host   Chrome/Firefox Native Messaging host
├── src-tauri/  qr-wifi-gui    Tauri desktop GUI (thin commands over core)
├── frontend/                 vanilla HTML/CSS/JS for the Tauri webview
└── extension/                browser extension that talks to the host
```

### Why a workspace with a separate `core`?

The classic mistake is to couple logic to one UI. Here **all OS/QR/protocol
logic lives in `qr-wifi-core`** and every binary is a thin frontend that calls
into it. Consequences:

- The CLI, TUI, GUI, and host can never drift — there's literally one
  `current_ssid()`, one `build_payload()`, one IPC `handle_request()`.
- `core` has no I/O UI, so it's trivially unit-testable (38+ tests, no terminal,
  no network).
- Adding a 5th frontend (e.g. a web API) costs ~one small binary.

**Analogy**: this is the "library + multiple executables" pattern you'd write in
Go (a `pkg` + several `cmd/` mains) or a TS monorepo with a shared `core`
package. In Rust it's a workspace.

### Why a `WifiAdapter` trait?

macOS, Linux, and Windows use different CLI tools (`networksetup`, `nmcli`,
`netsh`). Instead of `if/else` scattered everywhere, the core defines one
interface and each OS implements it:

```rust
// crates/core/src/platform/mod.rs
pub trait WifiAdapter: Send + Sync {
    fn list_networks(&self) -> Result<Vec<WifiNetwork>>;
    fn current_ssid(&self) -> Result<String>;
    fn credentials(&self, ssid: &str) -> Result<WifiCredentials>;
    fn connect(&self, credentials: &WifiCredentials) -> Result<()>;
}
```

A `default_adapter()` factory picks the right implementation per OS using
conditional compilation, and returns a **trait object** `Box<dyn WifiAdapter>`
so the frontends don't care which OS they're on:

```rust
let adapter: Box<dyn WifiAdapter> = default_adapter(); // polymorphic
adapter.current_ssid()?; // same call site on every platform
```

This is Go's `interface{}`/interfaces or TypeScript's `interface` — but resolved
differently (see ownership below).

### Why `cfg(target_os = ...)`?

Platform-specific code is selected at **compile time**, not runtime, with
attributes. Only the macOS module is compiled on macOS, etc.:

```rust
#[cfg(target_os = "macos")]
mod macos;   // only compiled on macOS
```

So a Windows-only bug can't affect a macOS build, and there are zero runtime
branch costs. (Go's `//go:build` tags do the same thing.)

### Why the IPC / Native Messaging host?

Browsers sandbox extensions — they can't run `nmcli`/`netsh`. So a tiny native
binary (`qr-wifi-host`) reads length-prefixed JSON on stdin, dispatches through
the **same** `handle_request()` the GUI uses, and writes JSON on stdout. The
extension just talks JSON. One protocol, reused; no logic duplicated.

### Why Tauri + a vanilla frontend?

Tauri gives a native desktop window with a webview. To honor "everything in
Rust," all logic stays in Rust commands; the frontend is plain HTML/CSS/JS (no
TypeScript, no bundler) that just calls `invoke('command_name', { ... })`.

---

## 3. Rust concepts used here (with analogies)

### Enums + `match`  (file: `crates/core/src/types.rs`)

```rust
pub enum WifiSecurity { Wpa, Wpa2, Wpa3, Wep, Nopass }
```

Like a TypeScript string-literal union (`"WPA" | "WEP" | ...`) or Python's
`enum.Enum`, but the compiler forces `match` arms to be **exhaustive** — you
can't forget a case.

```rust
match self {
    WifiSecurity::Wpa  => "WPA",
    WifiSecurity::Wpa2 => "WPA2",
    // ... the compiler errors if any variant is missing
}
```

### Structs + `impl`  (no classes)

```rust
pub struct WifiCredentials { pub ssid: String, pub security: WifiSecurity, ... }

impl WifiCredentials {
    pub fn new(ssid: impl Into<String>, security: WifiSecurity) -> Self { ... }
}
```

There are no classes; data (`struct`) and behavior (`impl`) are separate. Think
Go struct + methods, or a JS class split into data and a prototype.

### Traits  (≈ interfaces)

A `trait` is a set of method signatures a type can implement — TypeScript
`interface`, Go interface, Python `abc`. The big difference from Go is that you
can also provide **default methods** and require other traits as supertraits.

### Ownership, borrowing, `&`, `Box<dyn Trait>`  (the Rust-iest part)

- Every value has one **owner**; when the owner goes out of scope, the value is
  freed (deterministic — no GC, no `defer`/`finally` needed).
- **Borrowing**: pass `&x` (a reference) instead of moving the value. `&mut x`
  for exclusive/mutable access.
- `Box<T>` = heap allocation (like `new()` in Go/C++). `Box<dyn Trait>` = a
  trait object (dynamic dispatch), equivalent to returning an interface in Go or
  an object conforming to a TS interface.
- **Why this matters here**: `default_adapter() -> Box<dyn WifiAdapter>` returns
  a heap-allocated, type-erased adapter; the caller uses it through the trait.

```rust
fn share_ssid(adapter: &dyn WifiAdapter, ssid: &str) { ... }  // borrow, don't own
```

You'll see `?` constantly — see below.

### `Result<T, E>` and the `?` operator  (instead of exceptions)

Rust has no `throw`/`try`/`except`. Fallible functions return `Result<T, E>`:

```rust
let ssid = adapter.current_ssid()?;  // ok -> unwrap; err -> return early
```

`?` = "if this is an error, return it from my function immediately." It's the
Rust equivalent of Go's `if err != nil { return err }`, but one character. Our
error type is `CoreError` (`crates/core/src/error.rs`), built with `thiserror`.

The dual type for "maybe absent" is `Option<T>` (`Some(x)` / `None`) — no `null`.

### Derive macros  (`#[derive(...)]`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiNetwork { ... }
```

`#[derive(...)]` auto-generates trait implementations (like Lombok or TS
decorators). `serde`'s `Serialize`/`Deserialize` make a type JSON-able (≈
`JSON.stringify` + a schema) — used everywhere for the IPC protocol and config.

### Modules, `pub`, visibility

`mod foo;` declares a module (file `foo.rs` or `foo/mod.rs`). Items are private
by default; `pub` exports them (like Go: capitalized = exported). We use
`pub(crate)` to share inside the crate without leaking it to other crates
(`crates/core/src/platform/command.rs`).

### Iterators & closures  (familiar from JS/Python)

```rust
let names: Vec<&str> = networks.iter().map(|n| n.ssid.as_str()).collect();
```

`.map(|x| ...)` is `.map(x => ...)` in JS or `(lambda x: ...)` in Python.
`collect()` materializes the lazy iterator into a `Vec`.

### Conditional compilation & tests

- `#[cfg(target_os = "macos")]` — compile this only on macOS.
- `#[cfg(test)] mod tests { ... }` — unit tests live in the same file, compiled
  only under `cargo test`. Run them with `cargo test`.

### Doc comments = documentation

Lines starting with `///` become rendered docs (docs.rs / `cargo doc`). Every
module here opens with a `//!` (module-level) doc explaining intent — read those
first when exploring a file.

---

## 4. Suggested reading order through the code

1. `crates/core/src/types.rs` — the domain (`WifiSecurity`, `WifiNetwork`,
   `WifiCredentials`, `sort_networks`). Smallest, clearest file.
2. `crates/core/src/payload.rs` — building/parsing the `WIFI:` string; great
   example of string handling, escaping, and exhaustive tests.
3. `crates/core/src/qr.rs` — QR encode → PNG / terminal art, and decode.
4. `crates/core/src/platform/mod.rs` — the `WifiAdapter` trait + factory.
5. `crates/core/src/platform/macos.rs` — multi-method SSID detection (the
   "why so many fallbacks" file).
6. `crates/core/src/ipc.rs` — the request/response protocol shared by the host.
7. `crates/cli/src/main.rs` — how a frontend turns flags into core calls.
8. `crates/tui/src/lib.rs` + `crates/tui/src/fuzzy.rs` — the menu and the
   built-in fuzzy finder (terminal raw-mode + RAII cleanup).
9. `src-tauri/src/main.rs` — Tauri commands as another thin frontend.

---

## 5. Day-to-day commands (cheat-sheet)

```sh
cargo build                      # build CLI/TUI/host
cargo build --release            # optimized
cargo run -p qr-wifi-cli -- --list
cargo test                       # all tests
cargo test -p qr-wifi-core       # one crate
cargo fmt                        # format
cargo fmt -- --check             # verify formatting (CI)
cargo clippy --workspace --all-targets   # lint
cargo doc --open                 # generate & view docs for this workspace
cargo add <crate>                # add a dependency
cargo tree                       # show dependency graph
```

Happy hacking — and when in doubt about a type or method, `cargo doc --open` or
docs.rs usually answers it immediately.
