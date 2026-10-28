//! Windows Wi-Fi adapter backed by the Native Wi-Fi API.
//!
//! Structured SSIDs and profile XML avoid localized labels and console code pages.

#[cfg(target_os = "windows")]
mod native;

use serde::Deserialize;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::error::{CoreError, Result};
#[cfg(target_os = "windows")]
use crate::platform::WifiAdapter;
#[cfg(target_os = "windows")]
use crate::types::WifiNetwork;
use crate::types::{WifiCredentials, WifiSecurity};

// WlanConnect only accepts a request. Its callback supplies the actual result.
#[derive(Default)]
struct ConnectionCompletion {
    reason: Mutex<Option<u32>>,
    changed: Condvar,
}

impl ConnectionCompletion {
    fn finish(&self, reason: u32) {
        if let Ok(mut value) = self.reason.lock() {
            if value.is_none() {
                *value = Some(reason);
                self.changed.notify_all();
            }
        }
    }

    fn wait(&self, timeout: Duration) -> Result<()> {
        let reason = self.reason.lock().map_err(|_| {
            CoreError::Platform("Windows Wi-Fi completion state was interrupted".into())
        })?;
        let (reason, _) = self
            .changed
            .wait_timeout_while(reason, timeout, |value| value.is_none())
            .map_err(|_| {
                CoreError::Platform("Windows Wi-Fi completion state was interrupted".into())
            })?;
        match *reason {
            Some(0) => Ok(()),
            Some(code) => Err(CoreError::Platform(format!("Windows could not join the Wi-Fi network (reason {code}). Check the network's range, password and security settings."))),
            None => Err(CoreError::Platform("Timed out waiting for Windows to join the Wi-Fi network. Check its range, password and security settings.".into())),
        }
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Build the WLAN profile consumed by WlanSetProfile.
fn profile_xml(credentials: &WifiCredentials, profile_name: &str) -> Result<String> {
    if credentials.ssid.is_empty() || credentials.ssid.len() > 32 {
        return Err(CoreError::Platform(
            "Wi-Fi SSID must contain 1 to 32 bytes".into(),
        ));
    }
    let profile_name = xml_escape(profile_name);
    let ssid = xml_escape(&credentials.ssid);
    let hex: String = credentials
        .ssid
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect();
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
            let key_type = if credentials.security != WifiSecurity::Wpa3
                && password.len() == 64
                && password.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                "networkKey"
            } else {
                "passPhrase"
            };
            (
                authentication,
                "AES",
                format!(
                    "<sharedKey><keyType>{key_type}</keyType><protected>false</protected><keyMaterial>{}</keyMaterial></sharedKey>",
                    xml_escape(password)
                ),
            )
        }
    };

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
  <name>{profile_name}</name>
  <SSIDConfig>
    <SSID><hex>{hex}</hex><name>{ssid}</name></SSID>
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsProfile {
    name: String,
    #[serde(rename = "SSIDConfig")]
    ssid_config: SsidConfig,
    #[serde(rename = "MSM")]
    msm: Msm,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SsidConfig {
    #[serde(rename = "SSID")]
    ssid: Ssid,
    #[serde(default)]
    non_broadcast: bool,
}

#[derive(Deserialize)]
struct Ssid {
    name: Option<String>,
    hex: Option<String>,
}

#[derive(Deserialize)]
struct Msm {
    security: ProfileSecurity,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileSecurity {
    auth_encryption: AuthEncryption,
    shared_key: Option<SharedKey>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthEncryption {
    authentication: String,
    encryption: String,
    #[serde(default)]
    use_one_x: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedKey {
    protected: bool,
    key_material: Option<String>,
}

fn parse_profile(xml: &str) -> Result<WindowsProfile> {
    // Do not include raw XML or a deserialization error that could quote a key.
    quick_xml::de::from_str(xml)
        .map_err(|_| CoreError::Platform("Invalid Windows Wi-Fi profile XML".into()))
}

impl WindowsProfile {
    fn ssid(&self) -> Result<String> {
        if let Some(hex) = &self.ssid_config.ssid.hex {
            if !hex.len().is_multiple_of(2) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(CoreError::Platform("Invalid Windows Wi-Fi SSID hex".into()));
            }
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
                .collect();
            return String::from_utf8(bytes)
                .map_err(|_| CoreError::Platform("Wi-Fi SSID is not valid UTF-8".into()));
        }
        self.ssid_config
            .ssid
            .name
            .clone()
            .ok_or_else(|| CoreError::Platform("Windows Wi-Fi profile has no SSID".into()))
    }

    fn credentials(&self) -> Result<WifiCredentials> {
        let auth = &self.msm.security.auth_encryption;
        let security = match (
            auth.authentication.as_str(),
            auth.encryption.as_str(),
            auth.use_one_x,
        ) {
            ("open" | "shared", "WEP", false) => WifiSecurity::Wep,
            ("open", "none", false) => WifiSecurity::Nopass,
            ("WPAPSK", _, false) => WifiSecurity::Wpa,
            ("WPA2PSK", _, false) => WifiSecurity::Wpa2,
            ("WPA3SAE" | "WPA3SAEEXT", _, false) => WifiSecurity::Wpa3,
            _ => return Err(CoreError::Platform(
                "This Windows Wi-Fi authentication cannot be shared as a personal Wi-Fi QR code"
                    .into(),
            )),
        };
        let password = self
            .msm
            .security
            .shared_key
            .as_ref()
            .filter(|key| !key.protected && security.requires_password())
            .and_then(|key| key.key_material.clone())
            .filter(|password| !password.is_empty());
        Ok(WifiCredentials {
            ssid: self.ssid()?,
            security,
            password,
            hidden: self.ssid_config.non_broadcast,
        })
    }
}

#[cfg(target_os = "windows")]
pub struct WindowsAdapter;

#[cfg(target_os = "windows")]
impl WifiAdapter for WindowsAdapter {
    fn current_ssid(&self) -> Result<String> {
        native::Session::open()?.current_ssid()
    }

    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        native::Session::open()?.networks()
    }

    fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
        native::Session::open()?.credentials(ssid)
    }

    fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
        native::Session::open()?.connect(credentials)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepting_a_request_is_not_a_successful_connection() {
        let completion = ConnectionCompletion::default();
        assert!(completion.wait(Duration::ZERO).is_err());
        completion.finish(0);
        assert!(completion.wait(Duration::ZERO).is_ok());
    }

    #[test]
    fn failed_join_and_timeout_never_report_success() {
        let completion = ConnectionCompletion::default();
        completion.finish(123);
        assert!(completion
            .wait(Duration::ZERO)
            .unwrap_err()
            .to_string()
            .contains("could not join"));
        // A later unrelated success must not overwrite the failed attempt.
        completion.finish(0);
        assert!(completion.wait(Duration::ZERO).is_err());
    }

    #[test]
    fn profiles_round_trip_all_security_types_and_exact_credentials() {
        for security in [
            WifiSecurity::Nopass,
            WifiSecurity::Wep,
            WifiSecurity::Wpa2,
            WifiSecurity::Wpa3,
        ] {
            let mut credentials = WifiCredentials::new("  카페:&WiFi  ", security).hidden(true);
            if security.requires_password() {
                credentials.password = Some("  p:<&\\\"'>  ".into());
            }
            let xml = profile_xml(&credentials, "Renamed saved profile").unwrap();
            let profile = parse_profile(&xml).unwrap();
            assert_eq!(profile.name, "Renamed saved profile");
            assert_eq!(profile.credentials().unwrap(), credentials);
        }
    }

    #[test]
    fn secure_profile_escapes_credentials() {
        let credentials = WifiCredentials::new("Cafe & <Lab>", WifiSecurity::Wpa2)
            .with_password("p<&\"'>")
            .hidden(true);
        let xml = profile_xml(&credentials, &credentials.ssid).unwrap();
        assert!(xml.contains("Cafe &amp; &lt;Lab&gt;"));
        assert!(xml.contains("p&lt;&amp;&quot;&apos;&gt;"));
        assert!(xml.contains("<nonBroadcast>true</nonBroadcast>"));
        assert!(xml.contains("<authentication>WPA2PSK</authentication>"));
    }

    #[test]
    fn open_profile_has_no_shared_key() {
        let credentials = WifiCredentials::new("Guest", WifiSecurity::Nopass);
        let xml = profile_xml(&credentials, &credentials.ssid).unwrap();
        assert!(xml.contains("<authentication>open</authentication>"));
        assert!(xml.contains("<encryption>none</encryption>"));
        assert!(!xml.contains("sharedKey"));
    }

    #[test]
    fn secured_profile_requires_password() {
        let credentials = WifiCredentials::new("Home", WifiSecurity::Wpa3);
        assert!(profile_xml(&credentials, &credentials.ssid).is_err());
    }

    #[test]
    fn encrypted_keys_are_never_returned_as_passwords() {
        let credentials =
            WifiCredentials::new("Home", WifiSecurity::Wpa2).with_password("test-password");
        let xml = profile_xml(&credentials, "Home")
            .unwrap()
            .replace("<protected>false", "<protected>true");
        assert!(parse_profile(&xml)
            .unwrap()
            .credentials()
            .unwrap()
            .password
            .is_none());
    }

    #[test]
    fn raw_psk_uses_network_key_and_utf8_ssids_use_hex() {
        let credentials =
            WifiCredentials::new("카페", WifiSecurity::Wpa2).with_password("a".repeat(64));
        let xml = profile_xml(&credentials, "Home").unwrap();
        assert!(xml.contains("<keyType>networkKey</keyType>"));
        assert!(xml.contains("<hex>ECB9B4ED8E98</hex>"));
        assert_eq!(
            parse_profile(&xml).unwrap().ssid().unwrap(),
            credentials.ssid
        );
        assert!(profile_xml(
            &WifiCredentials::new("가".repeat(11), WifiSecurity::Nopass),
            "Too long"
        )
        .is_err());
    }

    #[test]
    fn enterprise_profiles_and_invalid_xml_report_errors_without_keys() {
        let credentials =
            WifiCredentials::new("Office", WifiSecurity::Wpa2).with_password("secret-fixture");
        let xml = profile_xml(&credentials, "Office")
            .unwrap()
            .replace("WPA2PSK", "WPA2")
            .replace("<useOneX>false", "<useOneX>true");
        assert!(parse_profile(&xml).unwrap().credentials().is_err());
        let error = parse_profile("<keyMaterial>secret-fixture").err().unwrap();
        assert!(!error.to_string().contains("secret-fixture"));
    }
}
