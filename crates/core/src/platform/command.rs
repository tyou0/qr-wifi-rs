//! Thin helpers around [`std::process::Command`].
//!
//! The OS Wi-Fi adapters shell out to the platform's native tooling
//! (`networksetup`/`ipconfig` on macOS, `nmcli` on Linux, `netsh` on Windows).
//! These helpers keep that pattern uniform and easy to reason about.

use std::process::Command;

use crate::error::{CoreError, Result};

/// Run a command and return its stdout on success.
///
/// A non-zero exit status becomes a [`CoreError::Command`] that includes the
/// trimmed stderr so callers can surface a useful message.
pub(crate) fn run(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| CoreError::Command {
            command: program.to_string(),
            message: e.to_string(),
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(CoreError::Command {
            command: display_invocation(program, args),
            message,
        })
    }
}

/// Best-effort capture of a command's stdout.
///
/// Returns [`None`] if the program cannot be spawned at all. A non-zero exit
/// still yields stdout (many Wi-Fi tools print useful output before failing),
/// so callers can parse defensively.
pub(crate) fn try_capture(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if stdout.trim().is_empty() {
        None
    } else {
        Some(stdout)
    }
}

fn display_invocation(program: &str, args: &[&str]) -> String {
    let mut out = program.to_string();
    for arg in args {
        out.push(' ');
        out.push_str(arg);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_echo_succeeds_on_unix() {
        if cfg!(windows) {
            return;
        }
        let out = run("echo", &["hello"]).unwrap();
        assert!(out.contains("hello"));
    }

    #[test]
    fn try_capture_missing_program_is_none() {
        assert!(try_capture("this-program-does-not-exist-xyz", &[]).is_none());
    }
}
