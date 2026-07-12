//! Linux Wi-Fi adapter backed by NetworkManager (`nmcli`).

use crate::error::Result;
use crate::platform::{run, run_redacted, WifiAdapter};
use crate::types::{WifiCredentials, WifiNetwork, WifiSecurity};

const NMCLI: &str = "nmcli";

/// Map an `nmcli` security token to our enum. Open networks are reported as
/// empty / `"--"` / `"none"`.
fn parse_security(value: &str) -> WifiSecurity {
    match value.trim() {
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
        if let Some(rest) = line.strip_prefix("yes:") {
            let ssid = rest.trim();
            if !ssid.is_empty() {
                return Some(ssid.to_string());
            }
        }
    }
    None
}

/// Parse `nmcli -t -f ACTIVE,SSID,SIGNAL,SECURITY dev wifi` into networks.
pub fn parse_nmcli_networks(output: &str) -> Vec<WifiNetwork> {
    let mut networks = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 4 {
            continue;
        }
        let active = parts[0] == "yes";
        let security = parse_security(parts[parts.len() - 1]);
        let signal = parts[parts.len() - 2].trim().parse::<u8>().ok();
        let ssid = parts[1..parts.len() - 2].join(":").trim().to_string();
        if ssid.is_empty() || !seen.insert(ssid.clone()) {
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

pub struct LinuxAdapter;

impl WifiAdapter for LinuxAdapter {
    fn current_ssid(&self) -> Result<String> {
        let output = run(
            NMCLI,
            &[
                "-t",
                "-f",
                "ACTIVE,SSID",
                "device",
                "wifi",
                "--rescan",
                "no",
            ],
        )?;
        parse_nmcli_current_ssid(&output).ok_or(crate::error::CoreError::NoActiveNetwork)
    }

    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        let output = run(
            NMCLI,
            &[
                "-t",
                "-f",
                "ACTIVE,SSID,SIGNAL,SECURITY",
                "device",
                "wifi",
                "--rescan",
                "no",
            ],
        )?;
        let mut networks = parse_nmcli_networks(&output);
        crate::types::sort_networks(&mut networks);
        Ok(networks)
    }

    fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
        let password = run(
            NMCLI,
            &[
                "--show-secrets",
                "-g",
                "802-11-wireless-security.psk",
                "connection",
                "show",
                ssid,
            ],
        )
        .ok()
        .map(|output| output.trim().to_string())
        .filter(|p| !p.is_empty());

        Ok(WifiCredentials {
            ssid: ssid.to_string(),
            security: WifiSecurity::Wpa,
            password,
            hidden: false,
        })
    }

    fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
        let mut args: Vec<String> = vec![
            "device".into(),
            "wifi".into(),
            "connect".into(),
            credentials.ssid.clone(),
        ];
        if credentials.security != WifiSecurity::Nopass {
            if let Some(password) = &credentials.password {
                if !password.trim().is_empty() {
                    args.push("password".into());
                    args.push(password.clone());
                }
            }
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_redacted(NMCLI, &arg_refs)?;
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
        let out = "yes:weird:ssid:77:WPA2\n";
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
}
