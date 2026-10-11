//! Small owned wrappers around wlanapi.dll. All OS allocations and session
//! handles are released even when profile parsing or a later operation fails.

use std::ffi::c_void;
use std::ptr::{null, null_mut};

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_STATE, HANDLE};
use windows_sys::Win32::NetworkManagement::WiFi::*;

use super::{parse_profile, profile_xml, WindowsProfile};
use crate::error::{CoreError, Result};
use crate::types::{sort_networks, WifiCredentials, WifiNetwork, WifiSecurity};

pub(super) struct Session(HANDLE);

struct WlanMemory(*mut c_void);

impl WlanMemory {
    fn new<T>(pointer: *mut T) -> Result<Self> {
        if pointer.is_null() {
            Err(CoreError::Platform(
                "Windows returned an empty Wi-Fi buffer".into(),
            ))
        } else {
            Ok(Self(pointer.cast()))
        }
    }
}

impl Drop for WlanMemory {
    fn drop(&mut self) {
        // SAFETY: This pointer was allocated by a successful WLAN API call and
        // is owned by exactly one WlanMemory until this drop.
        unsafe { WlanFreeMemory(self.0) };
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: The handle came from WlanOpenHandle and is still owned here.
        unsafe { WlanCloseHandle(self.0, null()) };
    }
}

fn check(code: u32, operation: &str) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(CoreError::Platform(format!(
            "Could not {operation}: {}",
            std::io::Error::from_raw_os_error(code as i32)
        )))
    }
}

fn check_location(code: u32, operation: &str) -> Result<()> {
    if code == ERROR_ACCESS_DENIED {
        return Err(CoreError::Platform(
            "Windows denied Wi-Fi access. Enable Location services and desktop-app location access in Settings > Privacy & security > Location.".into()
        ));
    }
    check(code, operation)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn decode_ssid(ssid: &DOT11_SSID) -> Result<String> {
    let length = ssid.uSSIDLength as usize;
    let bytes = ssid.ucSSID.get(..length).ok_or_else(|| {
        CoreError::Platform("Windows returned an invalid Wi-Fi SSID length".into())
    })?;
    String::from_utf8(bytes.to_vec())
        .map_err(|_| CoreError::Platform("Wi-Fi SSID is not valid UTF-8".into()))
}

fn network_security(network: &WLAN_AVAILABLE_NETWORK) -> WifiSecurity {
    if network.bSecurityEnabled == 0 {
        return WifiSecurity::Nopass;
    }
    if matches!(
        network.dot11DefaultCipherAlgorithm,
        DOT11_CIPHER_ALGO_WEP | DOT11_CIPHER_ALGO_WEP40 | DOT11_CIPHER_ALGO_WEP104
    ) {
        return WifiSecurity::Wep;
    }
    match network.dot11DefaultAuthAlgorithm {
        DOT11_AUTH_ALGO_WPA3_SAE | DOT11_AUTH_ALGO_WPA3_ENT | DOT11_AUTH_ALGO_WPA3_ENT_192 => {
            WifiSecurity::Wpa3
        }
        DOT11_AUTH_ALGO_RSNA | DOT11_AUTH_ALGO_RSNA_PSK => WifiSecurity::Wpa2,
        _ => WifiSecurity::Wpa,
    }
}

struct SavedProfile {
    interface: GUID,
    profile: WindowsProfile,
    flags: u32,
}

impl Session {
    pub(super) fn open() -> Result<Self> {
        let mut handle = null_mut();
        let mut version = 0;
        // SAFETY: All out pointers are valid; reserved parameters must be NULL.
        check(
            unsafe { WlanOpenHandle(2, null(), &mut version, &mut handle) },
            "open Windows Wi-Fi",
        )?;
        Ok(Self(handle))
    }

    fn interfaces(&self) -> Result<Vec<WLAN_INTERFACE_INFO>> {
        let mut pointer = null_mut();
        // SAFETY: The session is open and the out pointer is valid.
        check(
            unsafe { WlanEnumInterfaces(self.0, null(), &mut pointer) },
            "list Wi-Fi adapters",
        )?;
        let _allocation = WlanMemory::new(pointer)?;
        // SAFETY: WlanEnumInterfaces allocates a variable-length array with
        // dwNumberOfItems entries, valid until WlanFreeMemory is called.
        let mut interfaces = unsafe {
            std::slice::from_raw_parts(
                std::ptr::addr_of!((*pointer).InterfaceInfo).cast::<WLAN_INTERFACE_INFO>(),
                (*pointer).dwNumberOfItems as usize,
            )
            .to_vec()
        };
        interfaces.sort_by_key(|interface| interface.isState != wlan_interface_state_connected);
        Ok(interfaces)
    }

    pub(super) fn current_ssid(&self) -> Result<String> {
        for interface in self.interfaces()? {
            if interface.isState != wlan_interface_state_connected {
                continue;
            }
            let mut pointer = null_mut();
            let mut size = 0;
            // SAFETY: The interface comes from this session. Out pointers are
            // valid, and this opcode returns WLAN_CONNECTION_ATTRIBUTES.
            let code = unsafe {
                WlanQueryInterface(
                    self.0,
                    &interface.InterfaceGuid,
                    wlan_intf_opcode_current_connection,
                    null(),
                    &mut size,
                    &mut pointer,
                    null_mut(),
                )
            };
            if code == ERROR_INVALID_STATE {
                continue; // The interface disconnected after enumeration.
            }
            check_location(code, "read the current Wi-Fi network")?;
            let _allocation = WlanMemory::new(pointer)?;
            if (size as usize) < std::mem::size_of::<WLAN_CONNECTION_ATTRIBUTES>() {
                return Err(CoreError::Platform(
                    "Windows returned incomplete Wi-Fi connection data".into(),
                ));
            }
            // SAFETY: The allocation's type and size were established above.
            let attributes = unsafe { &*pointer.cast::<WLAN_CONNECTION_ATTRIBUTES>() };
            if attributes.isState == wlan_interface_state_connected {
                return decode_ssid(&attributes.wlanAssociationAttributes.dot11Ssid);
            }
        }
        Err(CoreError::NoActiveNetwork)
    }

    pub(super) fn networks(&self) -> Result<Vec<WifiNetwork>> {
        let mut networks: Vec<WifiNetwork> = Vec::new();
        for interface in self.interfaces()? {
            let mut pointer = null_mut();
            // SAFETY: The interface comes from this session; the out pointer
            // receives a WLAN_AVAILABLE_NETWORK_LIST allocation.
            check_location(
                unsafe {
                    WlanGetAvailableNetworkList(
                        self.0,
                        &interface.InterfaceGuid,
                        0,
                        null(),
                        &mut pointer,
                    )
                },
                "list Wi-Fi networks",
            )?;
            let _allocation = WlanMemory::new(pointer)?;
            // SAFETY: The API allocates dwNumberOfItems network entries.
            let entries = unsafe {
                std::slice::from_raw_parts(
                    std::ptr::addr_of!((*pointer).Network).cast::<WLAN_AVAILABLE_NETWORK>(),
                    (*pointer).dwNumberOfItems as usize,
                )
            };
            for entry in entries {
                let Ok(ssid) = decode_ssid(&entry.dot11Ssid) else {
                    continue;
                };
                if ssid.is_empty() {
                    continue;
                }
                let active = entry.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED != 0;
                let signal = Some(entry.wlanSignalQuality.min(100) as u8);
                let security = network_security(entry);
                if let Some(existing) = networks.iter_mut().find(|network| network.ssid == ssid) {
                    if active || (!existing.active && signal > existing.signal) {
                        existing.security = security;
                    }
                    existing.active |= active;
                    existing.signal = existing.signal.max(signal);
                } else {
                    networks.push(WifiNetwork {
                        ssid,
                        security,
                        signal,
                        active,
                    });
                }
            }
        }
        sort_networks(&mut networks);
        Ok(networks)
    }

    fn profile(&self, interface: &GUID, name: &str, plaintext: bool) -> Result<WindowsProfile> {
        let name = wide(name);
        let mut flags = if plaintext {
            WLAN_PROFILE_GET_PLAINTEXT_KEY
        } else {
            0
        };
        let mut pointer = null_mut();
        // SAFETY: Name is NUL-terminated UTF-16 and all out pointers are valid.
        check(
            unsafe {
                WlanGetProfile(
                    self.0,
                    interface,
                    name.as_ptr(),
                    null(),
                    &mut pointer,
                    &mut flags,
                    null_mut(),
                )
            },
            "read a saved Wi-Fi profile",
        )?;
        let _allocation = WlanMemory::new(pointer)?;
        // SAFETY: WlanGetProfile returns a NUL-terminated UTF-16 XML string,
        // which remains allocated for this entire read.
        let xml = unsafe {
            let mut length = 0;
            while *pointer.add(length) != 0 {
                length += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length))
        };
        parse_profile(&xml)
    }

    fn find_profile(&self, ssid: &str) -> Result<Option<SavedProfile>> {
        for interface in self.interfaces()? {
            let mut pointer = null_mut();
            // SAFETY: The interface and out pointer are valid for this session.
            check(
                unsafe {
                    WlanGetProfileList(self.0, &interface.InterfaceGuid, null(), &mut pointer)
                },
                "list saved Wi-Fi profiles",
            )?;
            let _allocation = WlanMemory::new(pointer)?;
            // SAFETY: The API allocates dwNumberOfItems profile entries.
            let profiles = unsafe {
                std::slice::from_raw_parts(
                    std::ptr::addr_of!((*pointer).ProfileInfo).cast::<WLAN_PROFILE_INFO>(),
                    (*pointer).dwNumberOfItems as usize,
                )
            };
            for entry in profiles {
                let length = entry
                    .strProfileName
                    .iter()
                    .position(|character| *character == 0)
                    .unwrap_or(entry.strProfileName.len());
                let name = String::from_utf16_lossy(&entry.strProfileName[..length]);
                let profile = self.profile(&interface.InterfaceGuid, &name, false)?;
                if profile.ssid().ok().as_deref() == Some(ssid) {
                    return Ok(Some(SavedProfile {
                        interface: interface.InterfaceGuid,
                        profile,
                        flags: entry.dwFlags,
                    }));
                }
            }
        }
        Ok(None)
    }

    pub(super) fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
        let saved = self
            .find_profile(ssid)?
            .ok_or_else(|| CoreError::NetworkNotFound(ssid.to_string()))?;
        // Windows may still return a protected key when policy denies plaintext
        // access. The XML parser leaves the password absent in that case.
        self.profile(&saved.interface, &saved.profile.name, true)?
            .credentials()
    }

    pub(super) fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
        let saved = self.find_profile(&credentials.ssid)?;
        let has_password = credentials
            .password
            .as_ref()
            .is_some_and(|password| !password.is_empty());
        let install = credentials.security == WifiSecurity::Nopass || has_password;
        let (interface, name, flags) = if let Some(saved) = saved {
            (
                saved.interface,
                saved.profile.name,
                saved.flags & WLAN_PROFILE_USER,
            )
        } else if install {
            let interface = self.interfaces()?.first().copied().ok_or_else(|| {
                CoreError::Platform("No Windows Wi-Fi adapter is available".into())
            })?;
            (
                interface.InterfaceGuid,
                credentials.ssid.clone(),
                WLAN_PROFILE_USER,
            )
        } else {
            return Err(CoreError::NetworkNotFound(credentials.ssid.clone()));
        };
        if install {
            let xml = wide(&profile_xml(credentials, &name)?);
            let mut reason = 0;
            // SAFETY: The profile XML is NUL-terminated and remains alive for
            // this call. Windows stores it directly, without a plaintext file.
            let code = unsafe {
                WlanSetProfile(
                    self.0,
                    &interface,
                    flags,
                    xml.as_ptr(),
                    null(),
                    1,
                    null(),
                    &mut reason,
                )
            };
            check(code, &format!("save the Wi-Fi profile (reason {reason})"))?;
        }
        let name = wide(&name);
        let parameters = WLAN_CONNECTION_PARAMETERS {
            wlanConnectionMode: wlan_connection_mode_profile,
            strProfile: name.as_ptr(),
            dot11BssType: dot11_BSS_type_infrastructure,
            ..Default::default()
        };
        // SAFETY: The interface and profile name are valid for the duration of
        // the call. Like netsh wlan connect, this requests asynchronous joining.
        check(
            unsafe { WlanConnect(self.0, &interface, &parameters, null()) },
            "connect to the Wi-Fi network",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_ssids_preserve_unicode_spaces_and_punctuation() {
        let value = "  카페:Guest\\WiFi  ";
        let mut ssid = DOT11_SSID {
            uSSIDLength: value.len() as u32,
            ..Default::default()
        };
        ssid.ucSSID[..value.len()].copy_from_slice(value.as_bytes());
        assert_eq!(decode_ssid(&ssid).unwrap(), value);
        ssid.uSSIDLength = 33;
        assert!(decode_ssid(&ssid).is_err());
    }

    #[test]
    fn native_security_keeps_open_wep_wpa2_and_wpa3_distinct() {
        let mut network = WLAN_AVAILABLE_NETWORK::default();
        assert_eq!(network_security(&network), WifiSecurity::Nopass);
        network.bSecurityEnabled = 1;
        network.dot11DefaultCipherAlgorithm = DOT11_CIPHER_ALGO_WEP104;
        assert_eq!(network_security(&network), WifiSecurity::Wep);
        network.dot11DefaultCipherAlgorithm = DOT11_CIPHER_ALGO_CCMP;
        network.dot11DefaultAuthAlgorithm = DOT11_AUTH_ALGO_RSNA_PSK;
        assert_eq!(network_security(&network), WifiSecurity::Wpa2);
        network.dot11DefaultAuthAlgorithm = DOT11_AUTH_ALGO_WPA3_SAE;
        assert_eq!(network_security(&network), WifiSecurity::Wpa3);
    }
}
