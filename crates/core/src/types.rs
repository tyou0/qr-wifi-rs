//! Domain types shared across CLI, TUI, GUI, and the IPC host.

use serde::{Deserialize, Serialize};

/// Wi-Fi authentication mode.
///
/// Serializes as the tokens used in the `T:` field of a `WIFI:` QR payload.
/// The classic spec only defines `WPA`/`WEP`/`nopass`, but real-world
/// generators and scanners widely accept `WPA2` and `WPA3`, so they are
/// first-class variants here. To stay forward-compatible, [`WifiSecurity::parse`]
/// accepts the lowercase/uppercase variants and falls back to [`WifiSecurity::Wpa`]
/// for anything unknown (most clients treat `WPA` as "any WPA family").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WifiSecurity {
    /// WPA (1). Also the safe fallback for unknown / generic WPA-family tokens.
    #[serde(rename = "WPA")]
    Wpa,
    /// WPA2 (the most common modern personal network).
    #[serde(rename = "WPA2")]
    Wpa2,
    /// WPA3 (newest personal/enterprise standard).
    #[serde(rename = "WPA3")]
    Wpa3,
    /// Legacy WEP.
    #[serde(rename = "WEP")]
    Wep,
    /// Open / unsecured network. No password is emitted in the QR.
    #[serde(rename = "nopass")]
    Nopass,
}

impl WifiSecurity {
    /// Parse a security token from a `WIFI:` payload or user input.
    ///
    /// Matching is case-insensitive. Recognized tokens: `WPA`, `WPA2`, `WPA3`,
    /// `WEP`, `nopass` (also `open`/`none`). Anything else falls back to
    /// [`WifiSecurity::Wpa`].
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_uppercase().as_str() {
            "WPA2" => WifiSecurity::Wpa2,
            "WPA3" => WifiSecurity::Wpa3,
            "WEP" => WifiSecurity::Wep,
            "NOPASS" | "OPEN" | "NONE" => WifiSecurity::Nopass,
            _ => WifiSecurity::Wpa, // "WPA" and unknowns
        }
    }

    /// Token used inside the `T:` field of a `WIFI:` payload.
    pub fn as_token(self) -> &'static str {
        match self {
            WifiSecurity::Wpa => "WPA",
            WifiSecurity::Wpa2 => "WPA2",
            WifiSecurity::Wpa3 => "WPA3",
            WifiSecurity::Wep => "WEP",
            WifiSecurity::Nopass => "nopass",
        }
    }

    /// `true` when this security type needs a password (i.e. not open).
    pub fn requires_password(self) -> bool {
        !matches!(self, WifiSecurity::Nopass)
    }
}

impl std::fmt::Display for WifiSecurity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_token())
    }
}

/// A Wi-Fi network entry returned by the OS list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiNetwork {
    /// The network's SSID (name).
    pub ssid: String,
    /// Authentication mode reported by the OS.
    pub security: WifiSecurity,
    /// Signal strength percentage when the OS reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<u8>,
    /// `true` when this is the currently connected network.
    pub active: bool,
}

/// Credentials used to build a `WIFI:` QR code or to connect.
///
/// This is the central value type of the crate; build one with [`Self::new`]
/// and the fluent `with_password` / `hidden` setters (a "builder" pattern).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiCredentials {
    /// The network's SSID (name).
    pub ssid: String,
    /// Authentication mode to encode / connect with.
    pub security: WifiSecurity,
    /// The pre-shared key, if any. `None` for open networks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// `true` for hidden (non-broadcasting) networks.
    pub hidden: bool,
}

impl WifiCredentials {
    /// Create credentials with an SSID and security, no password, not hidden.
    pub fn new(ssid: impl Into<String>, security: WifiSecurity) -> Self {
        Self {
            ssid: ssid.into(),
            security,
            password: None,
            hidden: false,
        }
    }

    /// Builder setter: attach a password. Consumes and returns `self`.
    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Builder setter: mark the network as hidden (or not). Consumes and returns `self`.
    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }
}

/// Sort networks: the active network first, then the rest alphabetically
/// (case-insensitive). Used by every frontend so the ordering is consistent.
pub fn sort_networks(networks: &mut [WifiNetwork]) {
    networks.sort_by(|a, b| match (a.active, b.active) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_puts_active_first_then_alphabetical() {
        let mut networks = vec![
            WifiNetwork {
                ssid: "Banana".into(),
                security: WifiSecurity::Wpa,
                signal: None,
                active: false,
            },
            WifiNetwork {
                ssid: "apple".into(),
                security: WifiSecurity::Wpa,
                signal: None,
                active: false,
            },
            WifiNetwork {
                ssid: "Current".into(),
                security: WifiSecurity::Wpa,
                signal: None,
                active: true,
            },
            WifiNetwork {
                ssid: "Cherry".into(),
                security: WifiSecurity::Wpa,
                signal: None,
                active: false,
            },
        ];
        sort_networks(&mut networks);
        let names: Vec<&str> = networks.iter().map(|n| n.ssid.as_str()).collect();
        assert_eq!(names, vec!["Current", "apple", "Banana", "Cherry"]);
    }

    #[test]
    fn sort_with_no_active_is_alphabetical() {
        let mut networks = vec![
            WifiNetwork {
                ssid: "Zeta".into(),
                security: WifiSecurity::Wpa,
                signal: None,
                active: false,
            },
            WifiNetwork {
                ssid: "alpha".into(),
                security: WifiSecurity::Wpa,
                signal: None,
                active: false,
            },
        ];
        sort_networks(&mut networks);
        let names: Vec<&str> = networks.iter().map(|n| n.ssid.as_str()).collect();
        assert_eq!(names, vec!["alpha", "Zeta"]);
    }

    #[test]
    fn security_round_trips_through_json() {
        let all = [
            WifiSecurity::Wpa,
            WifiSecurity::Wpa2,
            WifiSecurity::Wpa3,
            WifiSecurity::Wep,
            WifiSecurity::Nopass,
        ];
        for security in all {
            let json = serde_json::to_string(&security).unwrap();
            let back: WifiSecurity = serde_json::from_str(&json).unwrap();
            assert_eq!(security, back);
        }
        assert_eq!(
            serde_json::to_string(&WifiSecurity::Wpa).unwrap(),
            "\"WPA\""
        );
        assert_eq!(
            serde_json::to_string(&WifiSecurity::Wpa2).unwrap(),
            "\"WPA2\""
        );
        assert_eq!(
            serde_json::to_string(&WifiSecurity::Wpa3).unwrap(),
            "\"WPA3\""
        );
        assert_eq!(
            serde_json::to_string(&WifiSecurity::Nopass).unwrap(),
            "\"nopass\""
        );
    }

    #[test]
    fn security_parse_is_lenient() {
        assert_eq!(WifiSecurity::parse("WPA"), WifiSecurity::Wpa);
        assert_eq!(WifiSecurity::parse("wpa2"), WifiSecurity::Wpa2);
        assert_eq!(WifiSecurity::parse("WPA3"), WifiSecurity::Wpa3);
        assert_eq!(WifiSecurity::parse("WEP"), WifiSecurity::Wep);
        assert_eq!(WifiSecurity::parse("nopass"), WifiSecurity::Nopass);
        assert_eq!(WifiSecurity::parse("open"), WifiSecurity::Nopass);
        assert_eq!(WifiSecurity::parse("bogus"), WifiSecurity::Wpa);
        assert_eq!(WifiSecurity::parse(""), WifiSecurity::Wpa);
    }

    #[test]
    fn requires_password_excludes_open() {
        assert!(!WifiSecurity::Nopass.requires_password());
        assert!(WifiSecurity::Wpa.requires_password());
        assert!(WifiSecurity::Wpa2.requires_password());
        assert!(WifiSecurity::Wpa3.requires_password());
        assert!(WifiSecurity::Wep.requires_password());
    }
}
