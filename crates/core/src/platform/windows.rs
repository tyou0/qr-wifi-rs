//! Windows Wi-Fi adapter backed by `netsh wlan`.

use crate::error::Result;
use crate::platform::{run, WifiAdapter};
use crate::types::{WifiCredentials, WifiNetwork, WifiSecurity};

const NETSH: &str = "netsh";

/// Find the current SSID in `netsh wlan show interfaces` output.
///
/// netsh pads the label with many spaces (`SSID            : Name`), so we
/// match the `SSID` token directly and split on the first `:`. Left-trimming
/// first means a `BSSID` line can never match (it starts with `B`).
pub fn parse_netsh_current_ssid(output: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("SSID") else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        let ssid = value.trim();
        if !ssid.is_empty() {
            return Some(ssid.to_string());
        }
    }
    None
}

/// Parse `netsh wlan show profiles` into saved SSID names.
pub fn parse_netsh_profiles(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let prefix = trimmed.find(": ")?;
            let after = &trimmed[prefix + 2..];
            let ssid = after.trim();
            if ssid.is_empty() {
                None
            } else {
                Some(ssid.to_string())
            }
        })
        .filter(|line| {
            // Skip the header-ish lines that netsh prints without a profile.
            !line.contains("profiles on interface")
                && !line.eq_ignore_ascii_case("Profiles on interface Wi-Fi:")
        })
        .collect()
}

/// Parse `netsh wlan show networks mode=Bssid` into visible networks.
pub fn parse_netsh_networks(output: &str, active: Option<&str>) -> Vec<WifiNetwork> {
    let mut networks = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("SSID ") {
            // Lines look like "SSID 1 : MyNetwork" or "SSID 2 : Other".
            if let Some(colon) = rest.find(" : ") {
                let ssid = rest[colon + 3..].trim();
                if !ssid.is_empty() {
                    let is_active = active.is_some_and(|a| a == ssid);
                    networks.push(WifiNetwork {
                        ssid: ssid.to_string(),
                        security: WifiSecurity::Wpa,
                        signal: None,
                        active: is_active,
                    });
                }
            }
        }
    }
    networks
}

pub struct WindowsAdapter;

impl WifiAdapter for WindowsAdapter {
    fn current_ssid(&self) -> Result<String> {
        let output = run(NETSH, &["wlan", "show", "interfaces"])?;
        parse_netsh_current_ssid(&output).ok_or(crate::error::CoreError::NoActiveNetwork)
    }

    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        let active = self.current_ssid().ok();
        let output = run(NETSH, &["wlan", "show", "networks", "mode=Bssid"])?;
        let mut networks = parse_netsh_networks(&output, active.as_deref());

        if let Some(current) = active {
            if !networks.iter().any(|n| n.ssid == current) {
                networks.insert(
                    0,
                    WifiNetwork {
                        ssid: current,
                        security: WifiSecurity::Wpa,
                        signal: None,
                        active: true,
                    },
                );
            }
        }
        crate::types::sort_networks(&mut networks);
        Ok(networks)
    }

    fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
        // `name=<ssid>` and `key=clear` are passed as separate argv elements so
        // the SSID cannot alter netsh's tokenization (no shell, no injection).
        let name_arg = format!("name={ssid}");
        let output = run(NETSH, &["wlan", "show", "profile", &name_arg, "key=clear"])?;
        let password = output
            .lines()
            .find_map(|line| {
                let trimmed = line.trim();
                trimmed
                    .strip_prefix("Key Content")
                    .and_then(|rest| rest.trim_start_matches(':').trim().parse().ok())
            })
            .filter(|p: &String| !p.is_empty());

        Ok(WifiCredentials {
            ssid: ssid.to_string(),
            security: WifiSecurity::Wpa,
            password,
            hidden: false,
        })
    }

    fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
        let name_arg = format!("name={}", credentials.ssid);
        run(NETSH, &["wlan", "connect", &name_arg])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_ssid() {
        let out = "\
There is 1 interface on the system:

    Name                   : Wi-Fi
    SSID                   : Upstairs
    BSSID                  : aa:bb:cc:dd:ee:ff
";
        assert_eq!(parse_netsh_current_ssid(out), Some("Upstairs".into()));
    }

    #[test]
    fn ignores_bssid_line() {
        let out = "    BSSID                  : aa:bb:cc:dd:ee:ff\n";
        assert_eq!(parse_netsh_current_ssid(out), None);
    }

    #[test]
    fn parses_network_rows_and_marks_active() {
        let out = "\
SSID 1 : Home
    BSSID 1             : aa:bb:cc:dd:ee:ff
SSID 2 : Guest
";
        let nets = parse_netsh_networks(out, Some("Home"));
        assert_eq!(nets.len(), 2);
        assert_eq!(nets[0].ssid, "Home");
        assert!(nets[0].active);
        assert!(!nets[1].active);
    }
}
