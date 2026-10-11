//! Linux Wi-Fi adapter backed by NetworkManager (`nmcli`).

use crate::error::{CoreError, Result};
use crate::platform::{run, run_redacted, WifiAdapter};
use crate::types::{WifiCredentials, WifiNetwork, WifiSecurity};

const NMCLI: &str = "nmcli";

fn nmcli(args: &[&str], redacted: bool) -> Result<String> {
    let mut arguments = vec!["--colors", "no"];
    arguments.extend_from_slice(args);
    if redacted {
        run_redacted(NMCLI, &arguments)
    } else {
        run(NMCLI, &arguments)
    }
}

/// nmcli escapes both separators and backslashes in terse tabular output.
fn split_nmcli_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            ':' => fields.push(String::new()),
            '\\' if matches!(characters.peek(), Some(':' | '\\')) => {
                fields.last_mut().unwrap().push(characters.next().unwrap());
            }
            other => fields.last_mut().unwrap().push(other),
        }
    }
    fields
}

/// Map an `nmcli` security token to our enum. Open networks are reported as
/// empty / `"--"` / `"none"`.
fn parse_security(value: &str) -> WifiSecurity {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "--" | "none" => WifiSecurity::Nopass,
        other => {
            let lower = other.to_lowercase();
            if lower.contains("wpa3") {
                WifiSecurity::Wpa3
            } else if lower.contains("wpa2") {
                WifiSecurity::Wpa2
            } else if lower.contains("wep") {
                WifiSecurity::Wep
            } else {
                WifiSecurity::Wpa
            }
        }
    }
}

/// Find the active SSID in `nmcli -t -f ACTIVE,SSID dev wifi` output.
pub fn parse_nmcli_current_ssid(output: &str) -> Option<String> {
    for line in output.lines() {
        let fields = split_nmcli_fields(line);
        if fields.len() == 2 && fields[0] == "yes" && !fields[1].is_empty() {
            return Some(fields[1].clone());
        }
    }
    None
}

/// Parse `nmcli -t -f ACTIVE,SSID,SIGNAL,SECURITY dev wifi` into networks.
pub fn parse_nmcli_networks(output: &str) -> Vec<WifiNetwork> {
    let mut networks: Vec<WifiNetwork> = Vec::new();
    for line in output.lines() {
        let parts = split_nmcli_fields(line);
        if parts.len() != 4 {
            continue;
        }
        let active = parts[0] == "yes";
        let security = parse_security(&parts[3]);
        let signal = parts[2].parse::<u8>().ok().filter(|value| *value <= 100);
        let ssid = parts[1].clone();
        if ssid.is_empty() {
            continue;
        }
        if let Some(existing) = networks.iter_mut().find(|network| network.ssid == ssid) {
            if active || (!existing.active && signal > existing.signal) {
                existing.security = security;
            }
            existing.active |= active;
            existing.signal = existing.signal.max(signal);
            continue;
        }
        networks.push(WifiNetwork {
            ssid,
            security,
            signal,
            active,
        });
    }
    networks
}

fn profile_field<'a>(output: &'a str, field: &str) -> &'a str {
    output
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find_map(|(key, value)| (key == field).then_some(value))
        .unwrap_or("")
}

/// Multiline property output is unescaped, with canonical property names.
fn parse_nmcli_credentials(output: &str, ssid: &str) -> Result<WifiCredentials> {
    let key_management = profile_field(output, "802-11-wireless-security.key-mgmt");
    let security = match key_management {
        "" | "--" => WifiSecurity::Nopass,
        "none" => WifiSecurity::Wep,
        "sae" => WifiSecurity::Wpa3,
        "wpa-psk" => {
            if profile_field(output, "802-11-wireless-security.proto") == "rsn" {
                WifiSecurity::Wpa2
            } else {
                WifiSecurity::Wpa
            }
        }
        other => {
            return Err(CoreError::Platform(format!(
                "Wi-Fi authentication {other} cannot be shared as a personal Wi-Fi QR code"
            )));
        }
    };
    let password = match security {
        WifiSecurity::Nopass => "",
        WifiSecurity::Wep => {
            let index = profile_field(output, "802-11-wireless-security.wep-tx-keyidx");
            let index = if matches!(index, "0" | "1" | "2" | "3") {
                index
            } else {
                "0"
            };
            profile_field(output, &format!("802-11-wireless-security.wep-key{index}"))
        }
        _ => profile_field(output, "802-11-wireless-security.psk"),
    };
    Ok(WifiCredentials {
        ssid: ssid.to_string(),
        security,
        password: (!password.is_empty()).then(|| password.to_string()),
        hidden: matches!(
            profile_field(output, "802-11-wireless.hidden"),
            "yes" | "true"
        ),
    })
}

fn read_credentials(
    ssid: &str,
    mut execute: impl FnMut(&[&str]) -> Result<String>,
) -> Result<WifiCredentials> {
    // Profile names are user-editable and need not match their Wi-Fi SSIDs.
    // nmcli orders active profiles first; UUIDs also disambiguate duplicate names.
    let profiles = execute(&["-t", "-f", "UUID,TYPE", "connection", "show"])?;
    for line in profiles.lines() {
        let fields = split_nmcli_fields(line);
        if fields.len() != 2 || !matches!(fields[1].as_str(), "802-11-wireless" | "wifi") {
            continue;
        }
        let uuid = &fields[0];
        let saved_ssid = execute(&[
            "--escape",
            "no",
            "-g",
            "802-11-wireless.ssid",
            "connection",
            "show",
            "uuid",
            uuid,
        ])?;
        // Remove the record terminator, preserving spaces in the actual SSID.
        if saved_ssid.strip_suffix('\n').unwrap_or(&saved_ssid) != ssid {
            continue;
        }
        let profile = execute(&[
            "--show-secrets",
            "--escape",
            "no",
            "-t",
            "-m",
            "multiline",
            "-f",
            "802-11-wireless,802-11-wireless-security",
            "connection",
            "show",
            "uuid",
            uuid,
        ])?;
        return parse_nmcli_credentials(&profile, ssid);
    }
    Err(CoreError::NetworkNotFound(ssid.to_string()))
}

fn connect_arguments(credentials: &WifiCredentials) -> Vec<String> {
    let mut args = vec![
        "device".into(),
        "wifi".into(),
        "connect".into(),
        credentials.ssid.clone(),
    ];
    if credentials.security.requires_password() {
        if let Some(password) = credentials
            .password
            .as_ref()
            .filter(|password| !password.is_empty())
        {
            args.extend(["password".into(), password.clone()]);
        }
    }
    if credentials.hidden {
        args.extend(["hidden".into(), "yes".into()]);
    }
    args
}

pub struct LinuxAdapter;

impl WifiAdapter for LinuxAdapter {
    fn current_ssid(&self) -> Result<String> {
        let output = nmcli(
            &[
                "-t",
                "-f",
                "ACTIVE,SSID",
                "device",
                "wifi",
                "--rescan",
                "no",
            ],
            false,
        )?;
        parse_nmcli_current_ssid(&output).ok_or(crate::error::CoreError::NoActiveNetwork)
    }

    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        let output = nmcli(
            &[
                "-t",
                "-f",
                "ACTIVE,SSID,SIGNAL,SECURITY",
                "device",
                "wifi",
                "--rescan",
                "no",
            ],
            false,
        )?;
        let mut networks = parse_nmcli_networks(&output);
        crate::types::sort_networks(&mut networks);
        Ok(networks)
    }

    fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
        read_credentials(ssid, |args| nmcli(args, false))
    }

    fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
        let args = connect_arguments(credentials);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        nmcli(&arg_refs, true)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_active_ssid() {
        let out = "no:Other\nyes:Home\nno:Guest\n";
        assert_eq!(parse_nmcli_current_ssid(out), Some("Home".into()));
        assert_eq!(parse_nmcli_current_ssid("no:Only\n"), None);
    }

    #[test]
    fn maps_security_tokens() {
        assert_eq!(parse_security("WPA2"), WifiSecurity::Wpa2);
        assert_eq!(parse_security("WPA3"), WifiSecurity::Wpa3);
        assert_eq!(parse_security("WPA"), WifiSecurity::Wpa);
        assert_eq!(parse_security("WEP"), WifiSecurity::Wep);
        assert_eq!(parse_security("--"), WifiSecurity::Nopass);
        assert_eq!(parse_security(""), WifiSecurity::Nopass);
    }

    #[test]
    fn parses_network_rows() {
        let out = "yes:Home:88:WPA2\nno:Guest:42:\nno:Cafe:50:WEP\n";
        let nets = parse_nmcli_networks(out);
        assert_eq!(nets.len(), 3);
        assert_eq!(nets[0].ssid, "Home");
        assert!(nets[0].active);
        assert_eq!(nets[0].signal, Some(88));
        assert_eq!(nets[1].security, WifiSecurity::Nopass);
        assert_eq!(nets[2].security, WifiSecurity::Wep);
    }

    #[test]
    fn handles_colons_in_ssid() {
        let out = "yes:weird\\:ssid:77:WPA2\n";
        let nets = parse_nmcli_networks(out);
        assert_eq!(nets.len(), 1);
        assert_eq!(nets[0].ssid, "weird:ssid");
        assert_eq!(nets[0].signal, Some(77));
    }

    #[test]
    fn deduplicates_networks() {
        let out = "yes:Home:80:WPA2\nno:Home:30:WPA2\n";
        let nets = parse_nmcli_networks(out);
        assert_eq!(nets.len(), 1);
    }

    #[test]
    fn preserves_unicode_spaces_and_escaped_backslashes() {
        let out = "yes:  카페\\:Guest\\\\WiFi  :77:WPA2\n";
        let ssid = "  카페:Guest\\WiFi  ";
        assert_eq!(parse_nmcli_networks(out)[0].ssid, ssid);
        assert_eq!(
            parse_nmcli_current_ssid("yes:  카페\\:Guest\\\\WiFi  \n"),
            Some(ssid.into())
        );
    }

    #[test]
    fn duplicate_access_points_keep_active_state_and_best_signal() {
        let nets = parse_nmcli_networks("no:Home:80:WPA2\nyes:Home:30:WPA3\n");
        assert_eq!(nets.len(), 1);
        assert!(nets[0].active);
        assert_eq!(nets[0].signal, Some(80));
        assert_eq!(nets[0].security, WifiSecurity::Wpa3);
    }

    #[test]
    fn resolves_profile_by_ssid_and_uuid_and_preserves_password() {
        let mut responses = [
            "wired:802-3-ethernet\nother:802-11-wireless\nselected:802-11-wireless\n",
            "Other\n", "  Cafe:WiFi  \n",
            "802-11-wireless.hidden:yes\n802-11-wireless-security.key-mgmt:sae\n802-11-wireless-security.psk:  p:a\\ss  \n",
        ].into_iter();
        let mut calls = Vec::new();
        let credentials = read_credentials("  Cafe:WiFi  ", |args| {
            calls.push(args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>());
            Ok(responses.next().unwrap().to_string())
        })
        .unwrap();
        assert_eq!(credentials.security, WifiSecurity::Wpa3);
        assert_eq!(credentials.password.as_deref(), Some("  p:a\\ss  "));
        assert!(credentials.hidden);
        assert_eq!(&calls[3][calls[3].len() - 2..], &["uuid", "selected"]);
        assert!(calls[3].iter().any(|arg| arg == "--show-secrets"));
        assert!(responses.next().is_none());
    }

    #[test]
    fn saved_open_wep_and_wpa2_profiles_keep_security() {
        let open = parse_nmcli_credentials("802-11-wireless.hidden:no\n", "Guest").unwrap();
        assert_eq!(open.security, WifiSecurity::Nopass);
        assert!(open.password.is_none());
        let wep = parse_nmcli_credentials(
            "802-11-wireless-security.key-mgmt:none\n802-11-wireless-security.wep-tx-keyidx:2\n802-11-wireless-security.wep-key2:abcde\n", "Legacy"
        ).unwrap();
        assert_eq!(wep.security, WifiSecurity::Wep);
        assert_eq!(wep.password.as_deref(), Some("abcde"));
        let wpa2 = parse_nmcli_credentials(
            "802-11-wireless-security.key-mgmt:wpa-psk\n802-11-wireless-security.proto:rsn\n",
            "Home",
        )
        .unwrap();
        assert_eq!(wpa2.security, WifiSecurity::Wpa2);
        assert!(wpa2.password.is_none());
    }

    #[test]
    fn unknown_networks_and_command_failures_are_reported() {
        assert!(matches!(
            read_credentials("Missing", |_| Ok(String::new())),
            Err(CoreError::NetworkNotFound(_))
        ));
        assert!(matches!(
            read_credentials("Home", |_| Err(CoreError::Platform(
                "NetworkManager unavailable".into()
            ))),
            Err(CoreError::Platform(_))
        ));
        assert!(
            parse_nmcli_credentials("802-11-wireless-security.key-mgmt:wpa-eap\n", "Office")
                .is_err()
        );
    }

    #[test]
    fn connecting_hidden_network_passes_hidden_flag_and_exact_password() {
        let credentials = WifiCredentials::new("Hidden:WiFi", WifiSecurity::Wpa2)
            .with_password("  password  ")
            .hidden(true);
        assert_eq!(
            connect_arguments(&credentials),
            [
                "device",
                "wifi",
                "connect",
                "Hidden:WiFi",
                "password",
                "  password  ",
                "hidden",
                "yes"
            ]
        );
        let open = WifiCredentials::new("Guest", WifiSecurity::Nopass).with_password("ignored");
        assert_eq!(
            connect_arguments(&open),
            ["device", "wifi", "connect", "Guest"]
        );
    }
}
