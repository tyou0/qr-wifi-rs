//! Thin helpers around [`std::process::Command`].
//!
//! The OS Wi-Fi adapters shell out to the platform's native tooling
//! (`networksetup`/`ipconfig` on macOS, `nmcli` on Linux).
//! These helpers keep that pattern uniform and easy to reason about.

use std::process::Command;

use crate::error::{CoreError, Result};

/// Run a command and return its stdout on success.
///
/// A non-zero exit status becomes a [`CoreError::Command`] that includes the
/// trimmed stderr so callers can surface a useful message.
pub(crate) fn run(program: &str, args: &[&str]) -> Result<String> {
    run_with_label(program, args, display_invocation(program, args))
}

/// Run a command whose arguments may contain credentials.
///
/// The arguments still reach the OS process, but they are never copied into an
/// error returned to a frontend or log.
#[cfg(any(test, target_os = "linux", target_os = "macos"))]
pub(crate) fn run_redacted(program: &str, args: &[&str]) -> Result<String> {
    run_with_label(program, args, format!("{program} [arguments redacted]"))
}

fn run_with_label(program: &str, args: &[&str], command_label: String) -> Result<String> {
    let output = native_command(program, args)
        .output()
        .map_err(|e| CoreError::Command {
            command: command_label.clone(),
            message: e.to_string(),
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            output.status.to_string()
        };
        Err(CoreError::Command {
            command: command_label,
            message,
        })
    }
}

/// Best-effort capture of a command's stdout.
///
/// Returns [`None`] if the program cannot be spawned at all. A non-zero exit
/// still yields stdout (many Wi-Fi tools print useful output before failing),
/// so callers can parse defensively.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn try_capture(program: &str, args: &[&str]) -> Option<String> {
    let output = native_command(program, args).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if stdout.trim().is_empty() {
        None
    } else {
        Some(stdout)
    }
}

fn native_command(program: &str, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    // nmcli's yes/no values are localized, but plain C also replaces Unicode
    // SSIDs with question marks. C.UTF-8 keeps English tokens and UTF-8 text.
    let locale = if program == "nmcli" { "C.UTF-8" } else { "C" };
    command.args(args).env("LC_ALL", locale);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // Commands launched by the desktop GUI should not open console windows.
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
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

    #[cfg(unix)]
    #[test]
    fn run_echo_succeeds_on_unix() {
        let out = run("echo", &["hello"]).unwrap();
        assert!(out.contains("hello"));
    }

    #[test]
    fn try_capture_missing_program_is_none() {
        assert!(try_capture("this-program-does-not-exist-xyz", &[]).is_none());
    }

    #[test]
    fn redacted_command_error_never_contains_arguments() {
        let error = run_redacted(
            "this-program-does-not-exist-xyz",
            &["super-secret-password"],
        )
        .unwrap_err();
        assert!(error.to_string().contains("arguments redacted"));
        assert!(!error.to_string().contains("super-secret-password"));
    }

    #[cfg(unix)]
    #[test]
    fn command_uses_stable_locale_and_keeps_stdout_errors() {
        assert_eq!(run("sh", &["-c", "printf %s \"$LC_ALL\""]).unwrap(), "C");
        let error = run("sh", &["-c", "printf 'native tool failure'; exit 1"]).unwrap_err();
        assert!(error.to_string().contains("native tool failure"));
    }
}
