//! Native CoreWLAN adapter. Never infer the current SSID from saved profiles
//! or historical logs, and never send a password through process arguments.

use std::time::{Duration, Instant};

use objc2::rc::{autoreleasepool, Retained};
use objc2_core_location::{CLAuthorizationStatus, CLLocationManager};
use objc2_core_wlan::{
    CWInterface, CWKeychainDomain, CWKeychainFindWiFiPassword, CWNetwork, CWSecurity, CWWiFiClient,
};
use objc2_foundation::{NSData, NSError, NSString};

use crate::error::{CoreError, Result};
use crate::platform::WifiAdapter;
use crate::types::{WifiCredentials, WifiNetwork, WifiSecurity};

#[derive(Default)]
pub struct MacosAdapter;

const PRIVACY_HELP: &str = "macOS did not expose the Wi-Fi name. Enable Location Services for QR Wi-Fi RS in System Settings > Privacy & Security > Location Services, then retry. For a terminal build, authorize the terminal app.";

impl MacosAdapter {
    pub fn new() -> Self {
        Self
    }
}

fn interface() -> Result<Retained<CWInterface>> {
    // SAFETY: CoreWLAN returns retained framework objects; each operation keeps
    // its interface on its calling worker thread and inside an autorelease pool.
    let interface = unsafe { CWWiFiClient::sharedWiFiClient().interface() }
        .ok_or_else(|| CoreError::Platform("No Wi-Fi interface is available".into()))?;
    if !unsafe { interface.powerOn() } {
        return Err(CoreError::Platform(
            "Wi-Fi is off. Enable Wi-Fi and retry.".into(),
        ));
    }
    Ok(interface)
}

fn platform_error(operation: &str, error: &NSError) -> CoreError {
    // Descriptions/userInfo may contain credentials. Expose only the numeric code.
    CoreError::Platform(format!(
        "CoreWLAN {operation} failed (code {}). Check Wi-Fi and Location Services permissions.",
        error.code()
    ))
}

fn current(interface: &CWInterface) -> Result<String> {
    unsafe { interface.ssid() }
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            if unsafe { interface.wlanChannel() }.is_some() {
                CoreError::Platform(PRIVACY_HELP.into())
            } else {
                CoreError::NoActiveNetwork
            }
        })
}

fn security(value: CWSecurity) -> Option<WifiSecurity> {
    match value {
        CWSecurity::None => Some(WifiSecurity::Nopass),
        CWSecurity::WEP => Some(WifiSecurity::Wep),
        CWSecurity::WPAPersonal | CWSecurity::WPAPersonalMixed | CWSecurity::Personal => {
            Some(WifiSecurity::Wpa)
        }
        CWSecurity::WPA2Personal => Some(WifiSecurity::Wpa2),
        CWSecurity::WPA3Personal | CWSecurity::WPA3Transition => Some(WifiSecurity::Wpa3),
        _ => None, // Enterprise and OWE need a richer credential model.
    }
}

fn compatible(network: &CWNetwork, requested: WifiSecurity) -> bool {
    let kinds: &[CWSecurity] = match requested {
        WifiSecurity::Nopass => &[CWSecurity::None],
        WifiSecurity::Wep => &[CWSecurity::WEP],
        WifiSecurity::Wpa => &[
            CWSecurity::WPAPersonal,
            CWSecurity::WPAPersonalMixed,
            CWSecurity::WPA2Personal,
            CWSecurity::WPA3Personal,
            CWSecurity::WPA3Transition,
        ],
        WifiSecurity::Wpa2 => &[
            CWSecurity::WPA2Personal,
            CWSecurity::WPAPersonalMixed,
            CWSecurity::WPA3Transition,
        ],
        WifiSecurity::Wpa3 => &[CWSecurity::WPA3Personal, CWSecurity::WPA3Transition],
    };
    kinds
        .iter()
        .any(|kind| unsafe { network.supportsSecurity(*kind) })
}

fn network_security(network: &CWNetwork) -> Option<WifiSecurity> {
    [
        CWSecurity::WPA3Personal,
        CWSecurity::WPA2Personal,
        CWSecurity::WPAPersonal,
        CWSecurity::WEP,
        CWSecurity::None,
    ]
    .into_iter()
    .find(|kind| unsafe { network.supportsSecurity(*kind) })
    .and_then(security)
}

fn scan(
    interface: &CWInterface,
    ssid: Option<&str>,
    hidden: bool,
) -> Result<Vec<Retained<CWNetwork>>> {
    let data = ssid.map(|s| NSData::with_bytes(s.as_bytes()));
    let results =
        unsafe { interface.scanForNetworksWithSSID_includeHidden_error(data.as_deref(), hidden) }
            .map_err(|e| platform_error("scan", &e))?;
    let networks = results.to_vec();
    if !networks.is_empty() && networks.iter().all(|n| unsafe { n.ssidData() }.is_none()) {
        return Err(CoreError::Platform(PRIVACY_HELP.into()));
    }
    Ok(networks)
}

fn matches(network: &CWNetwork, ssid: &str) -> bool {
    unsafe { network.ssidData() }.is_some_and(|data| data.to_vec() == ssid.as_bytes())
}

impl WifiAdapter for MacosAdapter {
    fn current_ssid(&self) -> Result<String> {
        autoreleasepool(|_| {
            let interface = interface()?;
            current(&interface)
        })
    }

    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        autoreleasepool(|_| {
            let interface = interface()?;
            let active = current(&interface).ok();
            let mut networks: Vec<WifiNetwork> = Vec::new();
            for network in scan(&interface, None, false)? {
                let Some(ssid) = (unsafe { network.ssid() }).map(|s| s.to_string()) else {
                    continue;
                };
                let Some(security) = network_security(&network) else {
                    continue;
                };
                if networks
                    .iter()
                    .any(|n| n.ssid == ssid && n.security == security)
                {
                    continue;
                }
                networks.push(WifiNetwork {
                    active: active.as_ref() == Some(&ssid),
                    ssid,
                    security,
                    signal: Some(((unsafe { network.rssiValue() } + 100) * 2).clamp(0, 100) as u8),
                });
            }
            crate::types::sort_networks(&mut networks);
            Ok(networks)
        })
    }

    fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
        autoreleasepool(|_| {
            let interface = interface()?;
            let active = current(&interface).ok();
            let security = if active.as_deref() == Some(ssid) {
                security(unsafe { interface.security() })
            } else {
                scan(&interface, Some(ssid), true)?.iter().find(|n| matches(n, ssid)).and_then(|n| network_security(n))
            }.ok_or_else(|| CoreError::Platform("Could not determine a supported personal Wi-Fi security type. Use Custom to enter the network details.".into()))?;
            let mut credentials = WifiCredentials::new(ssid, security);
            if security.requires_password() {
                let data = NSData::with_bytes(ssid.as_bytes());
                for domain in [CWKeychainDomain::User, CWKeychainDomain::System] {
                    let mut pointer: *mut NSString = std::ptr::null_mut();
                    // Find returns a borrowed/autoreleased NSString, kept alive
                    // by this pool. Copy its contents; do not assume +1 ownership.
                    let status = unsafe { CWKeychainFindWiFiPassword(domain, &data, &mut pointer) };
                    if status == 0 && !pointer.is_null() {
                        credentials.password = Some(unsafe { &*pointer }.to_string());
                        break;
                    }
                }
            }
            Ok(credentials)
        })
    }

    fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
        autoreleasepool(|_| {
            let interface = interface()?;
            let networks = scan(&interface, Some(&credentials.ssid), credentials.hidden)?;
            let matching: Vec<_> = networks
                .iter()
                .filter(|n| matches(n, &credentials.ssid))
                .collect();
            if matching.is_empty() {
                return Err(CoreError::NetworkUnavailable(credentials.ssid.clone()));
            }
            let network = matching
                .into_iter()
                .filter(|n| compatible(n, credentials.security))
                .max_by_key(|n| unsafe { n.rssiValue() })
                .ok_or_else(|| {
                    CoreError::Platform(
                        "The QR security type does not match the visible Wi-Fi network".into(),
                    )
                })?;
            // Confirm identity access before changing networks: without it we
            // cannot verify the requested SSID after association. The static
            // query avoids creating a location manager on a worker run loop.
            #[allow(deprecated)]
            let authorization = unsafe { CLLocationManager::authorizationStatus_class() };
            if !matches!(
                authorization,
                CLAuthorizationStatus::AuthorizedAlways
                    | CLAuthorizationStatus::AuthorizedWhenInUse
            ) {
                return Err(CoreError::Platform(PRIVACY_HELP.into()));
            }
            let password = if credentials.security.requires_password() {
                Some(NSString::from_str(
                    credentials
                        .password
                        .as_deref()
                        .filter(|p| !p.is_empty())
                        .ok_or_else(|| {
                            CoreError::Platform("This Wi-Fi network requires a password".into())
                        })?,
                ))
            } else {
                None
            };
            unsafe { interface.associateToNetwork_password_error(network, password.as_deref()) }
                .map_err(|e| platform_error("association", &e))?;
            // Association is synchronous; still verify the live SSID rather than
            // treating acceptance, an old cache, or a saved profile as success.
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                match current(&interface) {
                    Ok(ssid) if ssid == credentials.ssid => return Ok(()),
                    Err(CoreError::Platform(message)) => return Err(CoreError::Platform(message)),
                    _ => {}
                }
                if Instant::now() >= deadline {
                    return Err(CoreError::Platform(
                        "macOS did not confirm connection to the requested Wi-Fi network".into(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wpa_security_types_map_to_wifi_qr_types() {
        for (native, expected) in [
            (CWSecurity::WPAPersonal, WifiSecurity::Wpa),
            (CWSecurity::WPAPersonalMixed, WifiSecurity::Wpa),
            (CWSecurity::Personal, WifiSecurity::Wpa),
            (CWSecurity::WPA2Personal, WifiSecurity::Wpa2),
            (CWSecurity::WPA3Personal, WifiSecurity::Wpa3),
            (CWSecurity::WPA3Transition, WifiSecurity::Wpa3),
        ] {
            assert_eq!(security(native), Some(expected));
        }
    }

    #[test]
    fn open_and_wep_security_types_map_to_wifi_qr_types() {
        assert_eq!(security(CWSecurity::None), Some(WifiSecurity::Nopass));
        assert_eq!(security(CWSecurity::WEP), Some(WifiSecurity::Wep));
    }

    #[test]
    fn enterprise_and_unknown_security_are_not_shared_as_personal() {
        assert_eq!(security(CWSecurity::WPA2Enterprise), None);
        assert_eq!(security(CWSecurity::Unknown), None);
        assert_eq!(security(CWSecurity::WPA2Personal), Some(WifiSecurity::Wpa2));
        assert_eq!(security(CWSecurity::None), Some(WifiSecurity::Nopass));
    }

    #[test]
    fn native_errors_do_not_include_remote_descriptions() {
        use objc2_foundation::ns_string;
        let error = unsafe {
            NSError::errorWithDomain_code_userInfo(ns_string!("secret-domain"), -3905, None)
        };
        let message = platform_error("scan", &error).to_string();
        assert!(message.contains("-3905"));
        assert!(!message.contains("secret-domain"));
    }
}
