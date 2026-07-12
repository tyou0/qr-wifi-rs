//! Windows Wi-Fi adapter backed by `netsh wlan`.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{CoreError, Result};
use crate::platform::{run, WifiAdapter};
use crate::types::{WifiCredentials, WifiNetwork, WifiSecurity};

const NETSH: &str = "netsh";

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Build the WLAN profile consumed by `netsh wlan add profile`.
fn profile_xml(credentials: &WifiCredentials) -> Result<String> {
    let ssid = xml_escape(&credentials.ssid);
    let hidden = if credentials.hidden { "true" } else { "false" };

    let (authentication, encryption, shared_key) = match credentials.security {
        WifiSecurity::Nopass => ("open", "none", String::new()),
        WifiSecurity::Wep => {
            let password = credentials
                .password
                .as_deref()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| CoreError::Platform("WEP password is required".into()))?;
            (
                "open",
                "WEP",
                format!(
                    "<sharedKey><keyType>networkKey</keyType><protected>false</protected><keyMaterial>{}</keyMaterial></sharedKey>",
                    xml_escape(password)
                ),
            )
        }
        WifiSecurity::Wpa | WifiSecurity::Wpa2 | WifiSecurity::Wpa3 => {
            let password = credentials
                .password
                .as_deref()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| CoreError::Platform("Wi-Fi password is required".into()))?;
            let authentication = if credentials.security == WifiSecurity::Wpa3 {
                "WPA3SAE"
            } else {
                "WPA2PSK"
            };
            (
                authentication,
                "AES",
                format!(
                    "<sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>{}</keyMaterial></sharedKey>",
                    xml_escape(password)
                ),
            )
        }
    };

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
  <name>{ssid}</name>
  <SSIDConfig>
    <SSID><name>{ssid}</name></SSID>
    <nonBroadcast>{hidden}</nonBroadcast>
  </SSIDConfig>
  <connectionType>ESS</connectionType>
  <connectionMode>auto</connectionMode>
  <MSM>
    <security>
      <authEncryption>
        <authentication>{authentication}</authentication>
        <encryption>{encryption}</encryption>
        <useOneX>false</useOneX>
      </authEncryption>
      {shared_key}
    </security>
  </MSM>
</WLANProfile>
"#
    ))
}

fn temporary_profile_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("qr-wifi-rs-{}-{nonce}.xml", std::process::id()))
}

fn install_profile(credentials: &WifiCredentials) -> Result<()> {
    let path = temporary_profile_path();
    std::fs::write(&path, profile_xml(credentials)?)?;

    let filename = format!("filename={}", path.display());
    let add_result = run(
        NETSH,
        &["wlan", "add", "profile", &filename, "user=current"],
    );
    let remove_result = std::fs::remove_file(&path);

    if let Err(error) = remove_result {
        return Err(CoreError::Platform(format!(
            "could not remove temporary Windows Wi-Fi profile: {error}"
        )));
    }
    add_result.map(|_| ())
}

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
        let has_password = credentials
            .password
            .as_deref()
            .is_some_and(|password| !password.is_empty());
        if credentials.security == WifiSecurity::Nopass || has_password {
            install_profile(credentials)?;
        }

        let name_arg = format!("name={}", credentials.ssid);
        let ssid_arg = format!("ssid={}", credentials.ssid);
        run(NETSH, &["wlan", "connect", &name_arg, &ssid_arg])?;
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

    #[test]
    fn secure_profile_escapes_credentials() {
        let credentials = WifiCredentials::new("Cafe & <Lab>", WifiSecurity::Wpa2)
            .with_password("p<&\"'>")
            .hidden(true);
        let xml = profile_xml(&credentials).unwrap();
        assert!(xml.contains("Cafe &amp; &lt;Lab&gt;"));
        assert!(xml.contains("p&lt;&amp;&quot;&apos;&gt;"));
        assert!(xml.contains("<nonBroadcast>true</nonBroadcast>"));
        assert!(xml.contains("<authentication>WPA2PSK</authentication>"));
    }

    #[test]
    fn open_profile_has_no_shared_key() {
        let credentials = WifiCredentials::new("Guest", WifiSecurity::Nopass);
        let xml = profile_xml(&credentials).unwrap();
        assert!(xml.contains("<authentication>open</authentication>"));
        assert!(xml.contains("<encryption>none</encryption>"));
        assert!(!xml.contains("sharedKey"));
    }

    #[test]
    fn secured_profile_requires_password() {
        let credentials = WifiCredentials::new("Home", WifiSecurity::Wpa3);
        assert!(profile_xml(&credentials).is_err());
    }
}
