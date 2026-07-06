//! Browser Native Messaging host.
//!
//! Reads length-prefixed JSON requests on stdin, dispatches them through the
//! shared [`qr_wifi_core`] logic, and writes length-prefixed JSON responses on
//! stdout. This is the bridge that lets a Chrome/Firefox extension share or
//! connect to Wi-Fi via the OS.
//!
//! Register the compiled binary with the browser's native messaging
//! (`com.thetomyou.qrwifi`); see `README.md` and `extension/`.

use std::io::{stdin, stdout};
use std::process::ExitCode;

use qr_wifi_core::{default_adapter, run_loop};

fn main() -> ExitCode {
    let adapter = default_adapter();
    let mut input = stdin().lock();
    let mut output = stdout().lock();

    match run_loop(&mut input, &mut output, adapter.as_ref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Native messaging forbids any stray bytes on stdout, so errors
            // go to stderr only.
            eprintln!("qr-wifi-host: {error}");
            ExitCode::FAILURE
        }
    }
}
