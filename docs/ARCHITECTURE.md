# QR Wi-Fi RS — Architecture Deep Dive

This document explains the architectural decisions behind QR Wi-Fi RS and how the codebase is organized.

## Table of Contents

- [Design Philosophy](#design-philosophy)
- [Workspace Structure](#workspace-structure)
- [Core Library Organization](#core-library-organization)
- [Platform Abstraction](#platform-abstraction)
- [Frontend Patterns](#frontend-patterns)
- [Data Flow](#data-flow)
- [Error Handling Strategy](#error-handling-strategy)
- [Testing Strategy](#testing-strategy)

---

## Design Philosophy

### Single Implementation, Multiple Frontends

The core design principle is: **all Wi-Fi logic lives in one place, frontends are thin callers**.

This avoids:
- Feature drift between CLI, TUI, GUI, and extension
- Duplicated bug fixes across multiple codebases
- Inconsistent behavior for the same operation

Every meaningful operation (share, connect, list networks) is implemented once in `qr-wifi-core`, and each UI is just a thin wrapper that:
1. Collects user input (flags, clicks, form data)
2. Calls a `qr-wifi-core` function
3. Presents the result (QR, success message, error)

### Why a Workspace?

A **Cargo workspace** allows multiple crates to share dependencies and build artifacts while keeping code organization clean. We use it because:

1. **Clear boundaries:** Each crate has a single responsibility (core logic, CLI UI, TUI UI, host bridge)
2. **Shared development:** One `cargo build` compiles all crates; one `Cargo.lock` ensures consistency
3. **Selective building:** `cargo build -p qr-wifi-cli` builds only what's needed
4. **Shared dependencies:** All crates use the same versions of `serde`, `thiserror`, etc.

### Why a Core Library?

**Alternative considered:** Put Wi-Fi logic directly in each binary.

**Problems with that approach:**
- Adding a feature requires editing 4+ places
- Bug fixes need to be replicated
- Testing OS code requires spinning up each frontend
- No clear "API surface" for the project

**Core library benefits:**
- One place to add, test, and document features
- Frontends compete on UX, not functionality
- Core can be unit-tested without any UI
- Easy to add a new frontend (e.g., a web API)

---

## Workspace Structure

```
qr_wifi_rs/
├── Cargo.toml              # Workspace manifest (virtual, no [package])
├── crates/
│   ├── core/                # qr-wifi-core (shared library)
│   ├── cli/                 # qr-wifi (CLI binary, uses TUI for menu)
│   ├── tui/                 # qr-wifi-tui (TUI library + binary)
│   └── host/                # qr-wifi-host (Native Messaging bridge)
├── src-tauri/               # qr-wifi-gui (Tauri desktop app)
├── frontend/               # Vanilla HTML/CSS/JS for Tauri webview
├── extension/              # Chrome/Firefox browser extension
└── docs/                   # Study materials
```

### Crate Responsibilities

| Crate | Type | Responsibility |
|-------|------|-----------------|
| `qr-wifi-core` | Library (rlib) | All Wi-Fi, QR, payload, and IPC logic |
| `qr-wifi-cli` | Binary | CLI argument parsing → core calls or TUI menu |
| `qr-wifi-tui` | Library + Binary | Interactive menu; also used by CLI |
| `qr-wifi-host` | Binary | Native Messaging stdio bridge |
| `qr-wifi-gui` | Binary | Tauri desktop app (thin command layer) |

### Dependency Graph

```
┌───────────────────────────────────────────────────────────────┐
│                    qr-wifi-core (library)                     │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────────┐    │
│  │ platform/  │  │  qr/, types/ │  │  payload, ipc/  │    │
│  │  adapters   │→ │  QR codecs   │← │   protocol       │    │
│  └─────────────┘  └──────────────┘  └──────────────────┘    │
│                          ↓                                     │
│                    service/ (high-level API)                  │
└───────────────────────────┬───────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        ↓                   ↓                   ↓
   ┌─────────┐        ┌─────────┐        ┌─────────┐
   │   CLI   │        │   TUI   │        │  Host   │
   └─────────┘        └─────────┘        └─────────┘
                            │
                            ↓
                       ┌─────────┐
                       │   GUI   │
                       └─────────┘
```

---

## Core Library Organization

The `qr-wifi-core` crate is organized by **responsibility**:

```
crates/core/src/
├── lib.rs           # Public API surface, module declarations
├── error.rs         # CoreError type (thiserror-based)
├── types.rs         # Domain models (WifiCredentials, WifiNetwork, etc.)
├── payload.rs       # WIFI: string parsing/formatting
├── qr.rs            # QR encoding (→ PNG/terminal) and decoding
├── platform/        # OS-specific adapters
│   ├── mod.rs       # WifiAdapter trait, factory
│   ├── command.rs   # Shell execution helpers
│   ├── macos.rs     # macOS (networksetup, security, syslog)
│   ├── linux.rs     # Linux (nmcli)
│   └── windows.rs   # Windows (netsh)
├── service.rs       # High-level feature functions
├── ipc.rs           # Native Messaging protocol
└── examples/        # Runnable examples
```

### Module Dependency Rules

- **types** → No dependencies (pure data)
- **payload** → types only
- **qr** → No core dependencies (just qrcode/image/rqrr)
- **platform** → types, error, command helpers
- **service** → types, qr, platform, payload
- **ipc** → types, service

This keeps dependencies acyclic and each module independently testable.

---

## Platform Abstraction

### The WifiAdapter Trait

```rust
pub trait WifiAdapter: Send + Sync {
    fn list_networks(&self) -> Result<Vec<WifiNetwork>>;
    fn current_ssid(&self) -> Result<String>;
    fn credentials(&self, ssid: &str) -> Result<WifiCredentials>;
    fn connect(&self, credentials: &WifiCredentials) -> Result<()>;
}
```

**Why a trait?**

- macOS, Linux, and Windows use completely different commands
- The trait defines **what** we need, not **how** to get it
- Frontends don't care which OS they're running on

### Why a Trait Object (`Box<dyn WifiAdapter>`)?

```rust
pub fn default_adapter() -> Box<dyn WifiAdapter> {
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacosAdapter::new());

    #[cfg(target_os = "linux")]
    return Box::new(linux::LinuxAdapter::new());

    #[cfg(target_os = "windows")]
    return Box::new(windows::WindowsAdapter::new());
}
```

**Why not generics?**

Generics would work, but would require threading the type parameter through every function:

```rust
// With generics (verbose)
fn share_current<A: WifiAdapter>(adapter: &A) -> Result<QrShare> { ... }

// With trait object (clean)
fn share_current(adapter: &dyn WifiAdapter) -> Result<QrShare> { ... }
```

Since we only ever need **one** adapter at runtime (the current OS's), a trait object is simpler.

### Conditional Compilation

Each platform module is compiled **only** on that OS:

```rust
#[cfg(target_os = "macos")]
mod macos;
```

Benefits:
- Windows code can't break macOS builds
- No runtime branching for which adapter to use
- Smaller binaries (only the current OS's code is included)

---

## Frontend Patterns

### CLI Pattern: Flags → Core or TUI

The CLI (`qr-wifi`) has two modes:

1. **One-shot mode:** Flags specify an action directly
   ```sh
   qr-wifi --share          # Call share_current()
   qr-wifi --ssid "Guest"   # Call share_ssid()
   ```

2. **Interactive mode:** No flags → drop into TUI menu
   ```sh
   qr-wifi                  # Call run_menu()
   ```

This is implemented in `crates/cli/src/main.rs`:

```rust
fn dispatch(cli: &Cli, adapter: &dyn WifiAdapter) -> Result<(), String> {
    if cli.list {
        return list_networks(adapter);
    }
    if cli.share {
        return share_current(adapter);
    }
    // ... more flags ...

    // No flags? Run the menu.
    run_menu(adapter);
    Ok(())
}
```

### TUI Pattern: Shared Library

`qr-wifi-tui` is both:
1. A **library** (`lib.rs`) with `run_menu()` that the CLI calls
2. A **binary** (`main.rs`) that runs the menu directly

This avoids duplicating the menu logic.

### Host Pattern: IPC Bridge

`qr-wifi-host` is the thinnest frontend — it's just a protocol translator:

1. Read length-prefixed JSON from stdin
2. Parse as `Request`
3. Call `handle_request(request, adapter)`
4. Write length-prefixed JSON `Response` to stdout

The actual logic lives in `qr-wifi-core::ipc`.

### GUI Pattern: Tauri Commands

The Tauri GUI defines one command per feature:

```rust
#[tauri::command]
fn share_current() -> Result<QrShare, String> {
    let adapter = default_adapter();
    core_share_current(adapter.as_ref()).map_err(to_message)
}
```

Each command is a **thin wrapper** around a core function. The frontend just calls `invoke('share_current')`.

---

## Data Flow

### Example: Sharing Current Wi-Fi

```
User action                  Core                          OS
─────────────────────────────────────────────────────────────
[CLI] qr-wifi --share
    ↓
    current_credentials(adapter)
    └─→ adapter.current_ssid()
        └─→ networksetup -getairportnetwork
    └─→ adapter.credentials("Home")
        └─→ security find-internet-password
    ↓
    share_custom(&credentials)
    └─→ build_payload(&credentials)  → "WIFI:T:WPA;S:Home;..."
    └─→ qrcode::render()             → QR matrix
    └─→ image::png()                  → PNG bytes
    └─→ base64::encode()             → png_base64
    ↓
[Output] QR art + "WIFI:..." string
```

### Example: Browser Extension Connect

```
User action              Extension          Host              Core             OS
───────────────────────────────────────────────────────────────────────────────
[Extension] User clicks "Connect"
    ↓
    send({command: "connect_payload", payload: "WIFI:..."})
    ↓                                                ↓
    [Native Messaging] ──────────→ read_message() ──→
    ↓
    handle_request(request, adapter)
    ↓
    connect_payload(adapter, &payload)
    └─→ parse_payload()          → WifiCredentials
    └─→ adapter.connect(&creds)
        └─→ networksetup -setairportnetwork en0 "Home" "password"
    ↓
    write_message({ok: true, data: {kind: "connected"}})
    ↓                                                ↓
    ←───────────────── [Native Messaging] ←─────────
    ↓
    [Extension] setStatus("Connected")
```

---

## Error Handling Strategy

### The CoreError Type

All core functions return `Result<T, CoreError>`:

```rust
pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error("OS command failed: {0}")]
    CommandFailed(String),

    #[error("No credentials found for SSID: {0}")]
    CredentialsNotFound(String),

    #[error("Invalid WIFI: payload: {0}")]
    InvalidPayload(String),

    // ... more variants
}
```

**Why `thiserror`?**

- Derive macros do the boilerplate (display impls, From conversions)
- Error messages are self-documenting at the definition site
- Easy to add new error variants without changing call sites

### Binary Error Handling

Binary crates (CLI, TUI, host) convert errors to strings:

```rust
fn main() -> ExitCode {
    match do_work() {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
```

This keeps main() simple while still providing clear error messages.

---

## Testing Strategy

### Unit Tests with Fakes

Core functions are tested with a **fake adapter** that doesn't touch the OS:

```rust
struct FakeAdapter {
    networks: Vec<WifiNetwork>,
    // ...
}

impl WifiAdapter for FakeAdapter {
    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        Ok(self.networks.clone())
    }
    // ...
}
```

This allows fast, deterministic testing of service logic.

### Platform-Specific Tests

Platform modules test their **parsing logic** (not the actual OS commands):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ssid_from_networksetup_output() {
        let output = "    Aggregated Wi-Fi";
        assert_eq!(parse_current_ssid(output), Ok("Aggregated Wi-Fi".to_string()));
    }
}
```

### Integration Tests

`crates/core/tests/payload_round_trip.rs` tests end-to-end:

1. Build credentials → payload
2. Parse payload → credentials
3. Encode payload → QR → decode → credentials

This verifies the full pipeline works.

---

## Summary

QR Wi-Fi RS's architecture is built on:

1. **Separation of concerns:** Core logic is separate from all UIs
2. **Trait-based abstraction:** `WifiAdapter` hides OS differences
3. **Thin frontends:** Each UI is a minimal wrapper around core functions
4. **Conditional compilation:** Only the current OS's code is included
5. **Clear error handling:** `thiserror` provides typed errors with good messages
6. **Testable design:** Fakes and parsing tests enable thorough unit testing

This keeps the codebase maintainable, testable, and easy to extend with new frontends.
