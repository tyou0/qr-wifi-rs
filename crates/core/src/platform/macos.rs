//! macOS Wi-Fi adapter.
//!
//! Active SSID detection uses several methods in order and falls back to the
//! syslog when the OS redacts the name. This mirrors the proven Bun adapter in
//! `qr_wifi_bun/src/platform/macosWifi.ts`, which is why it detects the current
//! network reliably on modern macOS where a single method often returns
//! `<redacted>`:
//!
//! 1. `networksetup -getairportnetwork <device>` (primary)
//! 2. `ipconfig getsummary <device>`
//! 3. `swift` CoreWLAN (live SSID, then first saved profile)
//! 4. `system_profiler SPAirPortDataType -json`
//!
//! If any step reports `<redacted>`, the most recent `airportd` log entry is
//! consulted.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::platform::{run, run_redacted, try_capture, WifiAdapter};
use crate::types::{WifiCredentials, WifiNetwork, WifiSecurity};

const NETWORKSETUP: &str = "/usr/sbin/networksetup";
const IPCONFIG: &str = "/usr/sbin/ipconfig";
const SYSTEM_PROFILER: &str = "/usr/sbin/system_profiler";
const SECURITY: &str = "/usr/bin/security";
const SWIFT: &str = "/usr/bin/swift";
const LOG: &str = "/usr/bin/log";
const DEFAULT_INTERFACE: &str = "en0";

/// CoreWLAN snippet: live SSID first, then the first saved network profile.
/// It is executed through `/usr/bin/swift` and needs no compilation step.
const COREWLAN_SWIFT: &str = r#"
import CoreWLAN
import Foundation

let interface = CWWiFiClient.shared().interface()
if let ssid = interface?.ssid(), !ssid.isEmpty {
  print(ssid)
} else if let configuration = interface?.configuration(),
          let profiles = configuration.value(forKey: "networkProfiles") as? NSOrderedSet,
          let first = profiles.firstObject as? CWNetworkProfile,
          let ssid = first.ssid,
          !ssid.isEmpty {
  print(ssid.replacingOccurrences(of: "\u{2019}", with: "'").replacingOccurrences(of: "\u{2018}", with: "'").trimmingCharacters(in: .whitespacesAndNewlines))
}
"#;

/// A usable SSID is non-empty and not redacted by the OS.
pub fn usable_ssid(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("<redacted>") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse `networksetup -listallhardwareports` to find the Wi-Fi/AirPort device.
pub fn wifi_device(hardware_ports: &str) -> String {
    let mut in_wifi = false;
    for line in hardware_ports.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Hardware Port:") {
            let port = rest.trim();
            in_wifi = port == "Wi-Fi" || port == "AirPort";
        } else if in_wifi {
            if let Some(rest) = trimmed.strip_prefix("Device:") {
                let dev = rest.trim();
                if !dev.is_empty() {
                    return dev.to_string();
                }
            }
        }
    }
    DEFAULT_INTERFACE.to_string()
}

/// Parse `networksetup -getairportnetwork` output for the current SSID.
/// Returns `Some("<redacted>")` so callers can trigger the syslog fallback.
pub fn parse_networksetup_airport(output: &str) -> Option<String> {
    if output.contains("<redacted>") {
        return Some("<redacted>".to_string());
    }
    for line in output.lines() {
        if let Some(idx) = line.find("Current Wi-Fi Network:") {
            return usable_ssid(&line[idx + "Current Wi-Fi Network:".len()..]);
        }
    }
    None
}

/// Parse `ipconfig getsummary` output for the current SSID.
///
/// Lines look like `          SSID : Upstairs`. We left-trim first, then match
/// the `SSID` token and split on the first `:`, so a `BSSID :` line (which
/// starts with `B`) can never be mistaken for the SSID.
pub fn parse_ipconfig_summary(output: &str) -> Option<String> {
    if output.contains("<redacted>") {
        return Some("<redacted>".to_string());
    }
    for line in output.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("SSID") else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        if let Some(ssid) = usable_ssid(value) {
            return Some(ssid);
        }
    }
    None
}

/// Parse `system_profiler SPAirPortDataType -json` for the interface's network.
pub fn parse_system_profiler(output: &str, device: &str) -> Option<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }
    let root: Value = serde_json::from_str(trimmed).ok()?;
    let airport = root.get("SPAirPortDataType")?.as_array()?;
    for item in airport {
        let Some(interfaces) = item
            .get("spairport_airport_interfaces")
            .and_then(|v| v.as_array())
        else {
            continue;
        };
        for interface in interfaces {
            let name = interface.get("_name").and_then(|v| v.as_str());
            if name == Some(device) {
                if let Some(network) = interface.get("spairport_current_network_information") {
                    if let Some(name) = network.get("_name").and_then(|v| v.as_str()) {
                        if let Some(ssid) = usable_ssid(name) {
                            return Some(ssid);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Parse `networksetup -listpreferredwirelessnetworks` into network entries.
pub fn parse_preferred_networks(output: &str) -> Vec<WifiNetwork> {
    let mut networks = Vec::new();
    let mut started = false;
    for line in output.lines() {
        let ssid = line.trim();
        if ssid.is_empty() {
            continue;
        }
        if !started {
            // First non-empty line is a header such as
            // "Preferred networks on en0:".
            started = true;
            continue;
        }
        networks.push(WifiNetwork {
            ssid: ssid.to_string(),
            security: WifiSecurity::Wpa,
            signal: None,
            active: false,
        });
    }
    networks
}

fn ssid_from_syslog() -> Option<String> {
    let output = try_capture(
        LOG,
        &[
            "show",
            "--last",
            "10m",
            "--predicate",
            "process == 'airportd' && message contains 'NetworkName'",
        ],
    )?;
    let mut found: Option<String> = None;
    for chunk in output.split("NetworkName = ").skip(1) {
        let value = chunk.split(';').next().unwrap_or("").trim();
        if !value.is_empty() {
            found = Some(value.trim_matches('"').to_string());
        }
    }
    found.filter(|s| !s.is_empty())
}

/// Resolve a candidate that may be the sentinel `"<redacted>"`.
fn resolve_candidate(candidate: String) -> Result<String> {
    if candidate == "<redacted>" {
        ssid_from_syslog().ok_or(CoreError::NoActiveNetwork)
    } else {
        Ok(candidate)
    }
}

/// macOS Wi-Fi adapter. The Wi-Fi device is constant for a process, so it is
/// resolved once. The active SSID is also cached briefly so repeated list/share
/// calls do not re-run the (slow) multi-method detection chain.
pub struct MacosAdapter {
    ssid_cache: Mutex<Option<(Instant, String)>>,
}

impl Default for MacosAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MacosAdapter {
    pub fn new() -> Self {
        Self {
            ssid_cache: Mutex::new(None),
        }
    }

    /// Resolve the Wi-Fi device once per process (it does not change).
    fn device(&self) -> String {
        static DEVICE: OnceLock<String> = OnceLock::new();
        DEVICE
            .get_or_init(|| {
                try_capture(NETWORKSETUP, &["-listallhardwareports"])
                    .map(|output| wifi_device(&output))
                    .unwrap_or_else(|| DEFAULT_INTERFACE.to_string())
            })
            .clone()
    }

    fn current_ssid_inner(&self) -> Result<String> {
        const TTL: Duration = Duration::from_secs(3);

        if let Ok(cache) = self.ssid_cache.lock() {
            if let Some((at, ssid)) = cache.as_ref() {
                if at.elapsed() < TTL {
                    return Ok(ssid.clone());
                }
            }
        }

        let device = self.device();
        let result = self.detect_ssid(&device);

        if let Ok(ref ssid) = result {
            if let Ok(mut cache) = self.ssid_cache.lock() {
                *cache = Some((Instant::now(), ssid.clone()));
            }
        }
        result
    }

    /// Run the multi-method detection chain for a known device.
    fn detect_ssid(&self, device: &str) -> Result<String> {
        if let Some(output) = try_capture(NETWORKSETUP, &["-getairportnetwork", device]) {
            if let Some(candidate) = parse_networksetup_airport(&output) {
                return resolve_candidate(candidate);
            }
        }

        if let Some(output) = try_capture(IPCONFIG, &["getsummary", device]) {
            if let Some(candidate) = parse_ipconfig_summary(&output) {
                return resolve_candidate(candidate);
            }
        }

        if let Some(output) = try_capture(SWIFT, &["-e", COREWLAN_SWIFT]) {
            for line in output.lines() {
                if let Some(ssid) = usable_ssid(line) {
                    return Ok(ssid);
                }
            }
        }

        if let Some(output) = try_capture(SYSTEM_PROFILER, &["SPAirPortDataType", "-json"]) {
            if let Some(candidate) = parse_system_profiler(&output, device) {
                return resolve_candidate(candidate);
            }
        }

        Err(CoreError::NoActiveNetwork)
    }
}

impl WifiAdapter for MacosAdapter {
    fn current_ssid(&self) -> Result<String> {
        self.current_ssid_inner()
    }

    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        let device = self.device();
        let output = run(NETWORKSETUP, &["-listpreferredwirelessnetworks", &device])?;
        let mut networks = parse_preferred_networks(&output);

        if let Ok(current) = self.current_ssid_inner() {
            let mut seen_active = false;
            for network in &mut networks {
                if network.ssid == current {
                    network.active = true;
                    seen_active = true;
                }
            }
            if !seen_active && !networks.iter().any(|n| n.ssid == current) {
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
        let password = run(SECURITY, &["find-generic-password", "-wa", ssid])
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
        let device = self.device();
        let mut args: Vec<String> = vec![
            "-setairportnetwork".into(),
            device,
            credentials.ssid.clone(),
        ];
        if credentials.security != WifiSecurity::Nopass {
            if let Some(password) = &credentials.password {
                if !password.trim().is_empty() {
                    args.push(password.clone());
                }
            }
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_redacted(NETWORKSETUP, &arg_refs)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usable_ssid_filters_redacted_and_empty() {
        assert_eq!(usable_ssid("Home"), Some("Home".into()));
        assert_eq!(usable_ssid("  Spaced  "), Some("Spaced".into()));
        assert_eq!(usable_ssid("<redacted>"), None);
        assert_eq!(usable_ssid("   "), None);
        assert_eq!(usable_ssid(""), None);
    }

    #[test]
    fn parses_wifi_device() {
        let sample = "\
Hardware Port: Bluetooth
Device: en6
Hardware Port: Wi-Fi
Device: en0
Hardware Port: Thunderbolt 1
Device: bridge0
";
        assert_eq!(wifi_device(sample), "en0");
    }

    #[test]
    fn falls_back_to_en0_when_unknown() {
        let sample = "Hardware Port: Bluetooth\nDevice: en6\n";
        assert_eq!(wifi_device(sample), DEFAULT_INTERFACE);
        assert_eq!(wifi_device(""), DEFAULT_INTERFACE);
    }

    #[test]
    fn parses_airport_network() {
        let out = "Current Wi-Fi Network: Upstairs\n";
        assert_eq!(parse_networksetup_airport(out), Some("Upstairs".into()));
        assert_eq!(
            parse_networksetup_airport("<redacted>"),
            Some("<redacted>".into())
        );
        assert_eq!(parse_networksetup_airport("nothing here"), None);
    }

    #[test]
    fn parses_ipconfig_summary() {
        let out = "\
          SSID : Upstairs
          BSSID : aa:bb:cc:dd:ee:ff
";
        assert_eq!(parse_ipconfig_summary(out), Some("Upstairs".into()));
        assert_eq!(
            parse_ipconfig_summary("  SSID : <redacted>"),
            Some("<redacted>".into())
        );
        // A BSSID line must never be mistaken for the SSID, even if the SSID
        // line is absent or empty.
        assert_eq!(
            parse_ipconfig_summary("  BSSID : aa:bb:cc:dd:ee:ff\n"),
            None
        );
        assert_eq!(parse_ipconfig_summary("  SSID :\n  BSSID : aa:bb\n"), None);
    }

    #[test]
    fn parses_preferred_networks() {
        let out = "\
Preferred networks on en0:
    Home
    Guest
    Coffee Shop
";
        let nets = parse_preferred_networks(out);
        assert_eq!(
            nets.iter().map(|n| n.ssid.as_str()).collect::<Vec<_>>(),
            vec!["Home", "Guest", "Coffee Shop"]
        );
        assert!(nets.iter().all(|n| !n.active));
    }

    #[test]
    fn parses_system_profiler_json() {
        let json = r#"{
  "SPAirPortDataType": [
    {
      "spairport_airport_interfaces": [
        {
          "_name": "en0",
          "spairport_current_network_information": { "_name": "Upstairs" }
        }
      ]
    }
  ]
}"#;
        assert_eq!(parse_system_profiler(json, "en0"), Some("Upstairs".into()));
        assert_eq!(parse_system_profiler(json, "en5"), None);
        assert_eq!(parse_system_profiler("not json", "en0"), None);
        assert_eq!(parse_system_profiler("", "en0"), None);
    }
}
