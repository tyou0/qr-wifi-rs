//! Linux Wi-Fi via NetworkManager's stable D-Bus API. No subprocesses or distro detection.
use crate::error::{CoreError, Result};
use crate::platform::WifiAdapter;
use crate::types::{sort_networks, WifiCredentials, WifiNetwork, WifiSecurity};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zbus::blocking::{connection::Builder, Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const SERVICE: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
const PROFILE: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const PSK: u32 = 0x100;
const SAE: u32 = 0x400;
const SCAN_TIMEOUT: Duration = Duration::from_secs(12);
const JOIN_TIMEOUT: Duration = Duration::from_secs(45);
type Settings = HashMap<String, HashMap<String, OwnedValue>>;

fn bus<T>(result: zbus::Result<T>, operation: &str) -> Result<T> {
    result.map_err(|error| {
        // Remote error descriptions may quote secrets. Keep only the standardized name.
        if let zbus::Error::MethodError(name, _, _) = error {
            if ["AccessDenied", "PermissionDenied", "NotAuthorized"].iter().any(|s| name.as_str().contains(s)) {
                return CoreError::Platform(format!("Permission denied while trying to {operation}. Allow Wi-Fi control in your desktop's authorization prompt."));
            }
            if name.as_str().contains("ServiceUnknown") || name.as_str().contains("NameHasNoOwner") {
                return CoreError::Platform("NetworkManager must be running and managing the Wi-Fi interface. Standalone iwd/wpa_supplicant setups are not supported.".into());
            }
            return CoreError::Platform(format!("Could not {operation} ({name})"));
        }
        CoreError::Platform(format!("Could not {operation} through the system D-Bus. Check that NetworkManager is running."))
    })
}

fn text<'a>(settings: &'a Settings, section: &str, key: &str) -> Option<&'a str> {
    settings.get(section)?.get(key)?.try_into().ok()
}
fn bytes(settings: &Settings, section: &str, key: &str) -> Option<Vec<u8>> {
    settings
        .get(section)?
        .get(key)?
        .try_clone()
        .ok()?
        .try_into()
        .ok()
}
fn profile_credentials(settings: &Settings, secrets: &Settings) -> Result<WifiCredentials> {
    let ssid = String::from_utf8(
        bytes(settings, "802-11-wireless", "ssid")
            .ok_or_else(|| CoreError::Platform("Saved Wi-Fi profile has no SSID".into()))?,
    )
    .map_err(|_| CoreError::Platform("Wi-Fi SSID is not valid UTF-8".into()))?;
    let section = "802-11-wireless-security";
    let security = match text(settings, section, "key-mgmt") {
        None => WifiSecurity::Nopass,
        Some("none") => WifiSecurity::Wep,
        Some("sae") => WifiSecurity::Wpa3,
        Some("wpa-psk") => {
            let protocols: Vec<String> = settings
                .get(section)
                .and_then(|s| s.get("proto"))
                .and_then(|v| v.try_clone().ok())
                .and_then(|v| v.try_into().ok())
                .unwrap_or_default();
            if protocols == ["rsn"] {
                WifiSecurity::Wpa2
            } else {
                WifiSecurity::Wpa
            }
        }
        _ => {
            return Err(CoreError::Platform(
                "This Wi-Fi authentication cannot be shared as a personal Wi-Fi QR code".into(),
            ))
        }
    };
    let key = if security == WifiSecurity::Wep {
        let index = settings
            .get(section)
            .and_then(|s| s.get("wep-tx-keyidx"))
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0)
            .min(3);
        format!("wep-key{index}")
    } else {
        "psk".into()
    };
    let password = if security.requires_password() {
        text(secrets, section, &key)
            .or_else(|| text(settings, section, &key))
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    } else {
        None
    };
    let hidden = settings
        .get("802-11-wireless")
        .and_then(|s| s.get("hidden"))
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false);
    Ok(WifiCredentials {
        ssid,
        security,
        password,
        hidden,
    })
}

#[derive(Debug)]
struct AccessPoint {
    device: OwnedObjectPath,
    path: OwnedObjectPath,
    ssid: String,
    privacy: bool,
    wpa: u32,
    rsn: u32,
    signal: u8,
    active: bool,
}
impl AccessPoint {
    fn supports(&self, security: WifiSecurity) -> bool {
        match security {
            WifiSecurity::Nopass => !self.privacy && self.wpa == 0 && self.rsn == 0,
            WifiSecurity::Wep => self.privacy && self.wpa == 0 && self.rsn == 0,
            WifiSecurity::Wpa => (self.wpa | self.rsn) & (PSK | SAE) != 0,
            WifiSecurity::Wpa2 => self.rsn & PSK != 0,
            WifiSecurity::Wpa3 => self.rsn & SAE != 0,
        }
    }
    fn security(&self) -> WifiSecurity {
        if self.rsn & SAE != 0 {
            WifiSecurity::Wpa3
        } else if self.rsn != 0 {
            WifiSecurity::Wpa2
        } else if self.wpa != 0 {
            WifiSecurity::Wpa
        } else if self.privacy {
            WifiSecurity::Wep
        } else {
            WifiSecurity::Nopass
        }
    }
}
fn value<T: Into<Value<'static>>>(value: T) -> OwnedValue {
    value
        .into()
        .try_into()
        .expect("owned settings contain no file descriptors")
}
fn validate_ssid(credentials: &WifiCredentials) -> Result<()> {
    if credentials.ssid.is_empty() || credentials.ssid.len() > 32 {
        return Err(CoreError::Platform(
            "Wi-Fi SSID must contain 1 to 32 bytes".into(),
        ));
    }
    Ok(())
}
fn connection_settings(
    credentials: &WifiCredentials,
    ap: Option<&AccessPoint>,
) -> Result<Settings> {
    validate_ssid(credentials)?;
    let mut settings = Settings::from([
        (
            "connection".into(),
            HashMap::from([
                ("id".into(), value(format!("QR Wi-Fi {}", credentials.ssid))),
                ("type".into(), value("802-11-wireless".to_owned())),
                ("autoconnect".into(), value(false)),
            ]),
        ),
        (
            "802-11-wireless".into(),
            HashMap::from([
                ("ssid".into(), value(credentials.ssid.as_bytes().to_vec())),
                ("mode".into(), value("infrastructure".to_owned())),
                ("hidden".into(), value(credentials.hidden)),
            ]),
        ),
        (
            "ipv4".into(),
            HashMap::from([("method".into(), value("auto".to_owned()))]),
        ),
        (
            "ipv6".into(),
            HashMap::from([("method".into(), value("auto".to_owned()))]),
        ),
    ]);
    if credentials.security.requires_password() {
        let password = credentials
            .password
            .as_ref()
            .filter(|p| !p.is_empty())
            .ok_or_else(|| {
                CoreError::Platform("A Wi-Fi password or matching saved profile is required".into())
            })?;
        let sae = credentials.security == WifiSecurity::Wpa3
            || (credentials.security == WifiSecurity::Wpa
                && ap.is_some_and(|a| a.rsn & SAE != 0 && (a.wpa | a.rsn) & PSK == 0));
        let key_management = if credentials.security == WifiSecurity::Wep {
            "none"
        } else if sae {
            "sae"
        } else {
            "wpa-psk"
        };
        let mut security = HashMap::from([("key-mgmt".into(), value(key_management.to_owned()))]);
        if credentials.security == WifiSecurity::Wep {
            security.insert("wep-key0".into(), value(password.clone()));
            security.insert("wep-key-type".into(), value(1u32));
        } else {
            security.insert("psk".into(), value(password.clone()));
            if credentials.security == WifiSecurity::Wpa2 {
                security.insert("proto".into(), value(vec!["rsn".to_owned()]));
            }
            if sae {
                security.insert("pmf".into(), value(3i32));
            }
        }
        settings.insert("802-11-wireless-security".into(), security);
    }
    Ok(settings)
}

struct Session(Connection);
impl Session {
    fn open() -> Result<Self> {
        let builder = bus(Builder::system(), "open the system bus")?;
        Ok(Self(bus(
            builder.method_timeout(Duration::from_secs(60)).build(),
            "open the system bus",
        )?))
    }
    fn proxy<'a>(&'a self, path: &'a str, interface: &'a str) -> Result<Proxy<'a>> {
        let builder = zbus::blocking::proxy::Builder::new(&self.0);
        let builder = bus(builder.destination(SERVICE), "access NetworkManager")?;
        let builder = bus(builder.path(path), "access NetworkManager")?;
        let builder = bus(builder.interface(interface), "access NetworkManager")?;
        bus(
            builder
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build(),
            "access NetworkManager",
        )
    }
    fn get<T>(&self, path: &str, interface: &str, property: &str) -> Result<T>
    where
        T: TryFrom<OwnedValue>,
        T::Error: Into<zbus::Error>,
    {
        bus(
            self.proxy(path, interface)?.get_property(property),
            "read Wi-Fi state",
        )
    }
    fn authorized<B, R>(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        body: &B,
        operation: &str,
    ) -> Result<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        bus(
            self.proxy(path, interface)?.call_with_flags(
                method,
                zbus::proxy::MethodFlags::AllowInteractiveAuth.into(),
                body,
            ),
            operation,
        )?
        .ok_or_else(|| CoreError::Platform(format!("No response while trying to {operation}")))
    }
    fn devices(&self) -> Result<Vec<OwnedObjectPath>> {
        let devices: Vec<OwnedObjectPath> = bus(
            self.proxy(ROOT, SERVICE)?.call("GetDevices", &()),
            "list Wi-Fi adapters",
        )?;
        let mut result = Vec::new();
        for device in devices {
            if self.get::<u32>(&device, DEVICE, "DeviceType")? == 2
                && self.get::<bool>(&device, DEVICE, "Managed")?
            {
                result.push(device);
            }
        }
        if result.is_empty() {
            return Err(CoreError::Platform(
                "No Wi-Fi adapter managed by NetworkManager is available".into(),
            ));
        }
        Ok(result)
    }
    fn access_points(&self, devices: &[OwnedObjectPath]) -> Result<Vec<AccessPoint>> {
        let mut result = Vec::new();
        for device in devices {
            let active: OwnedObjectPath = self.get(device, WIRELESS, "ActiveAccessPoint")?;
            let paths: Vec<OwnedObjectPath> = bus(
                self.proxy(device, WIRELESS)?
                    .call("GetAllAccessPoints", &()),
                "list Wi-Fi networks",
            )?;
            for path in paths {
                // An AP may disappear while the scan is being read.
                let Ok(ssid) = self.get::<Vec<u8>>(&path, AP, "Ssid") else {
                    continue;
                };
                let Ok(ssid) = String::from_utf8(ssid) else {
                    continue;
                };
                if ssid.is_empty() {
                    continue;
                }
                result.push(AccessPoint {
                    device: device.clone(),
                    active: path == active,
                    privacy: self.get::<u32>(&path, AP, "Flags")? & 1 != 0,
                    wpa: self.get(&path, AP, "WpaFlags")?,
                    rsn: self.get(&path, AP, "RsnFlags")?,
                    signal: self.get::<u8>(&path, AP, "Strength")?.min(100),
                    ssid,
                    path,
                });
            }
        }
        Ok(result)
    }
    fn scan(&self, devices: &[OwnedObjectPath], credentials: &WifiCredentials) -> Result<()> {
        let mut pending = Vec::new();
        for device in devices {
            let before: i64 = self.get(device, WIRELESS, "LastScan")?;
            // A directed scan also resolves broadcast SSIDs that a stale
            // background scan omitted. Never reinterpret H:false as hidden.
            let options =
                HashMap::from([("ssids", value(vec![credentials.ssid.as_bytes().to_vec()]))]);
            let response: zbus::Result<Option<()>> = self.proxy(device, WIRELESS)?.call_with_flags(
                "RequestScan",
                zbus::proxy::MethodFlags::AllowInteractiveAuth.into(),
                &(options,),
            );
            match response {
                Ok(_) => pending.push((device, before)),
                // An in-progress or rate-limited scan may still update LastScan.
                Err(zbus::Error::MethodError(name, _, _))
                    if name.as_str().ends_with("NotAllowed") =>
                {
                    pending.push((device, before));
                }
                Err(error) => {
                    bus::<()>(Err(error), "scan for Wi-Fi networks")?;
                }
            }
        }
        let deadline = Instant::now() + SCAN_TIMEOUT;
        while !pending.is_empty() && Instant::now() < deadline {
            let mut next = Vec::new();
            for (device, before) in pending {
                if self.get::<i64>(device, WIRELESS, "LastScan")? <= before {
                    next.push((device, before));
                }
            }
            pending = next;
            if !pending.is_empty() {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        if !pending.is_empty() {
            return Err(CoreError::Platform(
                "Wi-Fi scan did not finish. Check the adapter and try again.".into(),
            ));
        }
        Ok(())
    }
    fn profiles(&self) -> Result<Vec<(OwnedObjectPath, Settings)>> {
        let active: Vec<OwnedObjectPath> = self.get(ROOT, SERVICE, "ActiveConnections")?;
        let mut active_profiles = Vec::new();
        for connection in active {
            active_profiles.push(self.get::<OwnedObjectPath>(&connection, ACTIVE, "Connection")?);
        }
        let paths: Vec<OwnedObjectPath> = bus(
            self.proxy("/org/freedesktop/NetworkManager/Settings", SETTINGS)?
                .call("ListConnections", &()),
            "list saved Wi-Fi profiles",
        )?;
        let mut result = Vec::new();
        for path in paths {
            let settings: Settings = bus(
                self.proxy(&path, PROFILE)?.call("GetSettings", &()),
                "read a saved Wi-Fi profile",
            )?;
            if text(&settings, "connection", "type") == Some("802-11-wireless") {
                result.push((path, settings));
            }
        }
        // Duplicate SSIDs can have different saved keys. Share/reuse the
        // profile actually in use before considering inactive alternatives.
        result.sort_by_key(|(path, _)| !active_profiles.contains(path));
        Ok(result)
    }
    fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
        for (path, settings) in self.profiles()? {
            if bytes(&settings, "802-11-wireless", "ssid").as_deref() != Some(ssid.as_bytes()) {
                continue;
            }
            let secrets: Settings = self
                .authorized(
                    &path,
                    PROFILE,
                    "GetSecrets",
                    &("802-11-wireless-security",),
                    "read saved Wi-Fi credentials",
                )
                .unwrap_or_default();
            return profile_credentials(&settings, &secrets);
        }
        Err(CoreError::NetworkNotFound(ssid.into()))
    }
    fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
        validate_ssid(credentials)?;
        if !self.get::<bool>(ROOT, SERVICE, "WirelessEnabled")?
            || !self.get::<bool>(ROOT, SERVICE, "WirelessHardwareEnabled")?
        {
            return Err(CoreError::Platform(
                "Wi-Fi is switched off. Enable Wi-Fi and check the hardware airplane-mode switch."
                    .into(),
            ));
        }
        let devices = self.devices()?;
        let mut points = self.access_points(&devices)?;
        // A known compatible AP can be activated directly. If it is missing,
        // request a directed scan and wait for LastScan before declaring it
        // unavailable. This also avoids NM's repeated-scan rate limit.
        if !points
            .iter()
            .any(|ap| ap.ssid == credentials.ssid && ap.supports(credentials.security))
        {
            self.scan(&devices, credentials)?;
            points = self.access_points(&devices)?;
        }
        let same: Vec<_> = points
            .iter()
            .filter(|a| a.ssid == credentials.ssid)
            .collect();
        let chosen = same
            .iter()
            .copied()
            .filter(|a| a.supports(credentials.security))
            .max_by_key(|a| (a.active, a.signal));
        if chosen.is_none() && !same.is_empty() {
            return Err(CoreError::Platform("The QR code's Wi-Fi security does not match the nearby access point. Use a current QR code from that router.".into()));
        }
        if chosen.is_none() && !credentials.hidden {
            return Err(CoreError::NetworkUnavailable(credentials.ssid.clone()));
        }
        if chosen.is_some_and(|a| a.active) {
            return Ok(());
        }
        let device = chosen.map(|a| &a.device).unwrap_or(&devices[0]);
        let root = OwnedObjectPath::try_from("/").unwrap();
        let specific = chosen.map(|a| &a.path).unwrap_or(&root);
        let manager = self.proxy(ROOT, SERVICE)?;
        let (profile, active, created) = if credentials.security.requires_password()
            && credentials.password.as_ref().is_none_or(|p| p.is_empty())
        {
            let saved = self.profiles()?.into_iter().find(|(_, s)| {
                bytes(s, "802-11-wireless", "ssid").as_deref() == Some(credentials.ssid.as_bytes())
                    && profile_credentials(s, &Settings::new()).is_ok_and(|c| {
                        c.security == credentials.security
                            || (credentials.security == WifiSecurity::Wpa
                                && matches!(c.security, WifiSecurity::Wpa2 | WifiSecurity::Wpa3))
                    })
            });
            let (profile, _) = saved.ok_or_else(|| {
                CoreError::Platform("A Wi-Fi password or matching saved profile is required".into())
            })?;
            let active: OwnedObjectPath = self.authorized(
                ROOT,
                SERVICE,
                "ActivateConnection",
                &(&profile, device, specific),
                "join the Wi-Fi network",
            )?;
            (profile, active, false)
        } else {
            let settings = connection_settings(credentials, chosen)?;
            let options = HashMap::from([("persist", value("disk".to_owned()))]);
            let (profile, active, _): (
                OwnedObjectPath,
                OwnedObjectPath,
                HashMap<String, OwnedValue>,
            ) = self.authorized(
                ROOT,
                SERVICE,
                "AddAndActivateConnection2",
                &(settings, device, specific, options),
                "join the Wi-Fi network",
            )?;
            (profile, active, true)
        };
        let result = self.wait_for_activation(&active, device, &credentials.ssid);
        if result.is_err() {
            let _: zbus::Result<()> = manager.call("DeactivateConnection", &(&active,));
            if created {
                if let Ok(proxy) = self.proxy(&profile, PROFILE) {
                    let _: zbus::Result<()> = proxy.call("Delete", &());
                }
            }
        }
        result
    }
    fn wait_for_activation(&self, active: &str, device: &str, ssid: &str) -> Result<()> {
        let deadline = Instant::now() + JOIN_TIMEOUT;
        loop {
            match self.get::<u32>(active,ACTIVE,"State")? {
                2=>{
                    let ap:OwnedObjectPath=self.get(device,WIRELESS,"ActiveAccessPoint")?;
                    if ap.as_str()!="/" && self.get::<Vec<u8>>(&ap,AP,"Ssid")?==ssid.as_bytes(){return Ok(());}
                    return Err(CoreError::Platform("The OS activated a different Wi-Fi network than the QR code requested".into()));
                }
                3|4=>return Err(CoreError::Platform("The OS could not finish joining the Wi-Fi network. Check the password and signal, then try again.".into())),
                _=>{}
            }
            if Instant::now() >= deadline {
                return Err(CoreError::Platform("Timed out waiting to join the Wi-Fi network. Check the password and signal, then try again.".into()));
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }
}

/// Linux adapter using the same NetworkManager D-Bus interface on every distro.
#[derive(Debug)]
pub struct LinuxAdapter;
impl WifiAdapter for LinuxAdapter {
    fn current_ssid(&self) -> Result<String> {
        let session = Session::open()?;
        session
            .access_points(&session.devices()?)?
            .into_iter()
            .find(|a| a.active)
            .map(|a| a.ssid)
            .ok_or(CoreError::NoActiveNetwork)
    }
    fn list_networks(&self) -> Result<Vec<WifiNetwork>> {
        let session = Session::open()?;
        let mut networks: Vec<WifiNetwork> = Vec::new();
        for ap in session.access_points(&session.devices()?)? {
            if let Some(existing) = networks.iter_mut().find(|n| n.ssid == ap.ssid) {
                if ap.active || (!existing.active && Some(ap.signal) > existing.signal) {
                    existing.security = ap.security();
                }
                existing.active |= ap.active;
                existing.signal = existing.signal.max(Some(ap.signal));
            } else {
                networks.push(WifiNetwork {
                    security: ap.security(),
                    signal: Some(ap.signal),
                    active: ap.active,
                    ssid: ap.ssid,
                });
            }
        }
        sort_networks(&mut networks);
        Ok(networks)
    }
    fn credentials(&self, ssid: &str) -> Result<WifiCredentials> {
        Session::open()?.credentials(ssid)
    }
    fn connect(&self, credentials: &WifiCredentials) -> Result<()> {
        Session::open()?.connect(credentials)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires isolated NetworkManager container"]
    fn real_network_manager_accepts_native_profile_settings() {
        assert!(
            std::path::Path::new("/.dockerenv").exists()
                || std::path::Path::new("/run/.containerenv").exists()
        );
        assert_eq!(std::env::var("QR_WIFI_NMCLI_FIXTURES").as_deref(), Ok("1"));
        let session = Session::open().unwrap();
        for security in [
            WifiSecurity::Nopass,
            WifiSecurity::Wep,
            WifiSecurity::Wpa,
            WifiSecurity::Wpa2,
            WifiSecurity::Wpa3,
        ] {
            let mut credentials = WifiCredentials::new("Native:Fixture", security).hidden(true);
            if security.requires_password() {
                credentials.password = Some(
                    if security == WifiSecurity::Wep {
                        "abcde"
                    } else {
                        "fixture-password"
                    }
                    .into(),
                );
            }
            let settings = connection_settings(&credentials, None).unwrap();
            let options = HashMap::<String, OwnedValue>::new();
            let (path, _): (OwnedObjectPath, HashMap<String, OwnedValue>) = session
                .authorized(
                    "/org/freedesktop/NetworkManager/Settings",
                    SETTINGS,
                    "AddConnection2",
                    &(settings, 2u32, options),
                    "validate native profile",
                )
                .unwrap();
            let settings: Settings = session
                .proxy(&path, PROFILE)
                .unwrap()
                .call("GetSettings", &())
                .unwrap();
            let secrets: Settings = session
                .proxy(&path, PROFILE)
                .unwrap()
                .call("GetSecrets", &("802-11-wireless-security",))
                .unwrap_or_default();
            let actual = profile_credentials(&settings, &secrets).unwrap();
            let _: () = session
                .proxy(&path, PROFILE)
                .unwrap()
                .call("Delete", &())
                .unwrap();
            assert_eq!(actual, credentials);
        }
    }
    fn point(wpa: u32, rsn: u32, privacy: bool) -> AccessPoint {
        AccessPoint {
            device: "/device".try_into().unwrap(),
            path: "/ap".try_into().unwrap(),
            ssid: "Fixture".into(),
            wpa,
            rsn,
            privacy,
            signal: 80,
            active: false,
        }
    }
    #[test]
    fn open_wep_wpa2_wpa3_and_enterprise_are_not_confused() {
        assert!(point(0, 0, false).supports(WifiSecurity::Nopass));
        assert!(!point(0, PSK, true).supports(WifiSecurity::Nopass));
        assert!(point(0, 0, true).supports(WifiSecurity::Wep));
        assert!(point(0, PSK | SAE, true).supports(WifiSecurity::Wpa2));
        assert!(point(0, PSK | SAE, true).supports(WifiSecurity::Wpa3));
        assert!(!point(0, 0x200, true).supports(WifiSecurity::Wpa));
    }
    #[test]
    fn dbus_profiles_preserve_utf8_bytes_spaces_and_hidden_flags() {
        let credentials = WifiCredentials::new("  카페:Guest\\WiFi  ", WifiSecurity::Wpa2)
            .with_password("  fixture:p\\ass  ")
            .hidden(true);
        let settings = connection_settings(&credentials, None).unwrap();
        assert_eq!(
            profile_credentials(&settings, &Settings::new()).unwrap(),
            credentials
        );
        let open = WifiCredentials::new("Guest", WifiSecurity::Nopass).with_password("ignored");
        let settings = connection_settings(&open, None).unwrap();
        assert!(!settings.contains_key("802-11-wireless-security"));
        assert_eq!(
            profile_credentials(&settings, &Settings::new())
                .unwrap()
                .password,
            None
        );
    }
    #[test]
    fn generic_wpa_uses_sae_for_a_wpa3_only_access_point() {
        let credentials =
            WifiCredentials::new("Fixture", WifiSecurity::Wpa).with_password("fixture-password");
        let settings = connection_settings(&credentials, Some(&point(0, SAE, true))).unwrap();
        assert_eq!(
            text(&settings, "802-11-wireless-security", "key-mgmt"),
            Some("sae")
        );
        assert_eq!(
            profile_credentials(&settings, &Settings::new())
                .unwrap()
                .security,
            WifiSecurity::Wpa3
        );
    }
}
