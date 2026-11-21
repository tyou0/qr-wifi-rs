//! Small owned wrappers around wlanapi.dll. All OS allocations and session
//! handles are released even when profile parsing or a later operation fails.

use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{self, SyncSender};
use std::time::Duration;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_STATE, HANDLE};
use windows_sys::Win32::NetworkManagement::WiFi::*;

use super::{parse_profile, profile_xml, ConnectionCompletion, WindowsProfile};
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

struct ConnectionNotice {
    interface: GUID,
    ssid: Vec<u8>,
    completion: ConnectionCompletion,
}

// The heap address stays stable until notification unregistration has waited
// for every in-flight callback. Never hold the completion lock in Drop.
struct AcmNotifications<'a, T> {
    session: &'a Session,
    context: Option<Box<T>>,
}

fn is_acm_notice(data: &L2_NOTIFICATION_DATA, interface: &GUID) -> bool {
    let a = &data.InterfaceGuid;
    let b = interface;
    data.NotificationSource == WLAN_NOTIFICATION_SOURCE_ACM
        && (a.data1, a.data2, a.data3, a.data4) == (b.data1, b.data2, b.data3, b.data4)
}

unsafe extern "system" fn connection_notice(data: *mut L2_NOTIFICATION_DATA, context: *mut c_void) {
    if data.is_null() || context.is_null() {
        return;
    }
    // SAFETY: WLAN owns data during the call; registration owns context until
    // unregistration returns, which waits for pending callbacks to finish.
    let data = unsafe { &*data };
    let context = unsafe { &*context.cast::<ConnectionNotice>() };
    if !is_acm_notice(data, &context.interface)
        || data.NotificationCode != wlan_notification_acm_connection_complete as u32
        || data.pData.is_null()
        || (data.dwDataSize as usize) < std::mem::size_of::<WLAN_CONNECTION_NOTIFICATION_DATA>()
    {
        return;
    }
    // SAFETY: The source, event and minimum allocation size were checked above.
    let details = unsafe { &*data.pData.cast::<WLAN_CONNECTION_NOTIFICATION_DATA>() };
    let length = details.dot11Ssid.uSSIDLength as usize;
    if details.dot11Ssid.ucSSID.get(..length) != Some(context.ssid.as_slice()) {
        return;
    }
    context.completion.finish(details.wlanReasonCode);
}

struct ScanNotice {
    interface: GUID,
    finished: SyncSender<()>,
}

unsafe extern "system" fn scan_notice(data: *mut L2_NOTIFICATION_DATA, context: *mut c_void) {
    if data.is_null() || context.is_null() {
        return;
    }
    // SAFETY: WLAN owns data during the call; the registration owns context.
    let data = unsafe { &*data };
    let context = unsafe { &*context.cast::<ScanNotice>() };
    if is_acm_notice(data, &context.interface)
        && (data.NotificationCode == wlan_notification_acm_scan_complete as u32
            || data.NotificationCode == wlan_notification_acm_scan_fail as u32)
    {
        let _ = context.finished.try_send(());
    }
}

impl<'a, T> AcmNotifications<'a, T> {
    // SAFETY: The callback must interpret the context pointer as a shared T.
    unsafe fn register(
        session: &'a Session,
        context: T,
        callback: WLAN_NOTIFICATION_CALLBACK,
    ) -> Result<Self> {
        let context = Box::new(context);
        // SAFETY: Box keeps the callback context at a stable address for the
        // lifetime of the ACM registration.
        check(
            unsafe {
                WlanRegisterNotification(
                    session.0,
                    WLAN_NOTIFICATION_SOURCE_ACM,
                    0,
                    callback,
                    (&*context as *const T).cast(),
                    null(),
                    null_mut(),
                )
            },
            "observe Windows Wi-Fi notifications",
        )?;
        Ok(Self {
            session,
            context: Some(context),
        })
    }
}

impl AcmNotifications<'_, ConnectionNotice> {
    fn wait(&self) -> Result<()> {
        self.context
            .as_ref()
            .expect("registered context")
            .completion
            .wait(Duration::from_secs(45))
    }

    fn timed_out(&self) -> bool {
        self.context.as_ref().is_some_and(|context| {
            context
                .completion
                .reason
                .lock()
                .map(|value| value.is_none())
                .unwrap_or(true)
        })
    }
}

impl<T> Drop for AcmNotifications<'_, T> {
    fn drop(&mut self) {
        // SAFETY: Called outside the callback and without its mutex held.
        let code = unsafe {
            WlanRegisterNotification(
                self.session.0,
                WLAN_NOTIFICATION_SOURCE_NONE,
                0,
                None,
                null(),
                null(),
                null_mut(),
            )
        };
        if code != 0 {
            // A failed OS unregistration cannot prove that callbacks stopped.
            // Retain this small allocation until process exit rather than free
            // a context the OS might still reference. The session closes next.
            if let Some(context) = self.context.take() {
                Box::leak(context);
            }
        }
    }
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
            self.refresh_scan(interface.InterfaceGuid)?;
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

    fn refresh_scan(&self, interface: GUID) -> Result<()> {
        let (finished, completion) = mpsc::sync_channel(1);
        let context = ScanNotice {
            interface,
            finished,
        };
        // SAFETY: scan_notice uses a shared ScanNotice. Register before WlanScan
        // so an immediate notification can be received.
        let _notifications =
            unsafe { AcmNotifications::register(self, context, Some(scan_notice)) }?;
        // SAFETY: The interface came from this session. Null filters request all SSIDs.
        check_location(
            unsafe { WlanScan(self.0, &interface, null(), null(), null()) },
            "scan for nearby Wi-Fi networks",
        )?;
        // Windows recommends querying the available list after completion or a
        // four-second timeout. A failed scan also leaves the OS list queryable.
        let _ = completion.recv_timeout(Duration::from_secs(4));
        Ok(())
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
        let context = ConnectionNotice {
            interface,
            ssid: credentials.ssid.as_bytes().to_vec(),
            completion: ConnectionCompletion::default(),
        };
        // SAFETY: connection_notice uses a shared ConnectionNotice.
        let notifications =
            unsafe { AcmNotifications::register(self, context, Some(connection_notice)) }?;
        // SAFETY: The interface and profile name are valid for the duration of
        // the call. Acceptance is asynchronous; success comes from the callback.
        check(
            unsafe { WlanConnect(self.0, &interface, &parameters, null()) },
            "connect to the Wi-Fi network",
        )?;
        let result = notifications.wait();
        if result.is_err() && notifications.timed_out() {
            // Prevent a timed-out request from switching networks later, after
            // the frontend has already reported failure.
            unsafe { WlanDisconnect(self.0, &interface, null()) };
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_ignores_unrelated_notifications() {
        let (finished, completion) = mpsc::sync_channel(1);
        let mut context = ScanNotice {
            interface: GUID::from_u128(1),
            finished,
        };
        let pointer = std::ptr::from_mut(&mut context).cast();
        let mut notice = L2_NOTIFICATION_DATA {
            NotificationSource: WLAN_NOTIFICATION_SOURCE_ACM,
            NotificationCode: wlan_notification_acm_scan_complete as u32,
            InterfaceGuid: GUID::from_u128(2),
            ..Default::default()
        };
        // SAFETY: Both pointers refer to live test values of the expected types.
        unsafe { scan_notice(&mut notice, pointer) };
        assert_eq!(completion.try_recv(), Err(mpsc::TryRecvError::Empty));

        notice.InterfaceGuid = context.interface;
        notice.NotificationSource = WLAN_NOTIFICATION_SOURCE_MSM;
        unsafe { scan_notice(&mut notice, pointer) };
        assert_eq!(completion.try_recv(), Err(mpsc::TryRecvError::Empty));

        notice.NotificationSource = WLAN_NOTIFICATION_SOURCE_ACM;
        notice.NotificationCode = wlan_notification_acm_connection_complete as u32;
        unsafe { scan_notice(&mut notice, pointer) };
        assert_eq!(completion.try_recv(), Err(mpsc::TryRecvError::Empty));
    }

    #[test]
    fn scan_complete_and_scan_fail_release_the_waiter() {
        let (finished, completion) = mpsc::sync_channel(1);
        let mut context = ScanNotice {
            interface: GUID::from_u128(1),
            finished,
        };
        let mut notice = L2_NOTIFICATION_DATA {
            NotificationSource: WLAN_NOTIFICATION_SOURCE_ACM,
            InterfaceGuid: context.interface,
            ..Default::default()
        };
        let pointer = std::ptr::from_mut(&mut context).cast();
        for code in [
            wlan_notification_acm_scan_complete,
            wlan_notification_acm_scan_fail,
        ] {
            notice.NotificationCode = code as u32;
            // SAFETY: Both pointers refer to live test values of the expected types.
            unsafe { scan_notice(&mut notice, pointer) };
            assert_eq!(completion.try_recv(), Ok(()));
        }
    }

    #[test]
    fn completion_callback_ignores_other_interfaces_and_ssids() {
        let context = ConnectionNotice {
            interface: GUID::from_u128(1),
            ssid: b"Fixture".to_vec(),
            completion: ConnectionCompletion::default(),
        };
        let mut details = WLAN_CONNECTION_NOTIFICATION_DATA::default();
        details.dot11Ssid.uSSIDLength = 7;
        details.dot11Ssid.ucSSID[..7].copy_from_slice(b"Fixture");
        let mut notice = L2_NOTIFICATION_DATA {
            NotificationSource: WLAN_NOTIFICATION_SOURCE_ACM,
            NotificationCode: wlan_notification_acm_connection_complete as u32,
            InterfaceGuid: GUID::from_u128(2),
            dwDataSize: std::mem::size_of_val(&details) as u32,
            pData: (&mut details as *mut WLAN_CONNECTION_NOTIFICATION_DATA).cast(),
        };
        let pointer = (&context as *const ConnectionNotice).cast_mut().cast();
        // SAFETY: Both pointers refer to live test allocations of the checked types.
        unsafe { connection_notice(&mut notice, pointer) };
        assert!(context.completion.wait(Duration::ZERO).is_err());
        notice.InterfaceGuid = context.interface;
        details.dot11Ssid.uSSIDLength = 33;
        notice.pData = std::ptr::from_mut(&mut details).cast();
        unsafe { connection_notice(&mut notice, pointer) };
        assert!(context.completion.wait(Duration::ZERO).is_err());
        details.dot11Ssid.uSSIDLength = 7;
        notice.pData = std::ptr::from_mut(&mut details).cast();
        unsafe { connection_notice(&mut notice, pointer) };
        assert!(context.completion.wait(Duration::ZERO).is_ok());
    }

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
