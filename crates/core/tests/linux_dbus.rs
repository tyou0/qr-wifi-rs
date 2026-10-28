//! Run with dbus-run-session in an isolated test process. No host Wi-Fi is used.
#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

use qr_wifi_core::{default_adapter, WifiCredentials, WifiSecurity};
use zbus::{
    blocking::connection::Builder,
    zvariant::{OwnedObjectPath, OwnedValue},
};

type Settings = HashMap<String, HashMap<String, OwnedValue>>;
const ROOT: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "/org/freedesktop/NetworkManager/Devices/1";
const AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/1";
const PROFILE: &str = "/org/freedesktop/NetworkManager/Settings/1";
const ACTIVE: &str = "/org/freedesktop/NetworkManager/ActiveConnection/1";
fn path(value: &str) -> OwnedObjectPath {
    value.try_into().unwrap()
}

#[derive(Default)]
struct State {
    visible: AtomicBool,
    scanned: AtomicBool,
    activated: AtomicUsize,
    state_reads: AtomicUsize,
    fail: AtomicBool,
}
struct Manager(Arc<State>);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![path(DEVICE)]
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> bool {
        true
    }
    fn add_and_activate_connection2(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        connection: Settings,
        _device: OwnedObjectPath,
        _specific: OwnedObjectPath,
        _options: HashMap<String, OwnedValue>,
    ) -> (
        OwnedObjectPath,
        OwnedObjectPath,
        HashMap<String, OwnedValue>,
    ) {
        assert!(header
            .primary()
            .flags()
            .contains(zbus::message::Flags::AllowInteractiveAuth));
        assert!(
            !connection.contains_key("802-11-wireless-security"),
            "open QR must not install a password"
        );
        self.0.activated.fetch_add(1, Ordering::SeqCst);
        self.0.state_reads.store(0, Ordering::SeqCst);
        (path(PROFILE), path(ACTIVE), HashMap::new())
    }
    fn deactivate_connection(&self, _active: OwnedObjectPath) {}
}
struct Device;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn managed(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        30
    }
}
struct Wireless(Arc<State>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    #[zbus(property)]
    fn last_scan(&self) -> i64 {
        i64::from(self.0.scanned.load(Ordering::SeqCst))
    }
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        if self.0.activated.load(Ordering::SeqCst) > 0
            && self.0.state_reads.load(Ordering::SeqCst) > 1
        {
            path(AP)
        } else {
            path("/")
        }
    }
    fn request_scan(
        &self,
        _options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) {
        assert!(header
            .primary()
            .flags()
            .contains(zbus::message::Flags::AllowInteractiveAuth));
        self.0.scanned.store(true, Ordering::SeqCst);
    }
    fn get_all_access_points(&self) -> Vec<OwnedObjectPath> {
        if self.0.visible.load(Ordering::SeqCst) && self.0.scanned.load(Ordering::SeqCst) {
            vec![path(AP)]
        } else {
            vec![]
        }
    }
}
struct AccessPoint;
#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        b"Fixture Open".to_vec()
    }
    #[zbus(property)]
    fn flags(&self) -> u32 {
        0
    }
    #[zbus(property)]
    fn wpa_flags(&self) -> u32 {
        0
    }
    #[zbus(property)]
    fn rsn_flags(&self) -> u32 {
        0
    }
    #[zbus(property)]
    fn strength(&self) -> u8 {
        80
    }
}
struct ActiveConnection(Arc<State>);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl ActiveConnection {
    #[zbus(property)]
    fn state(&self) -> u32 {
        let n = self.0.state_reads.fetch_add(1, Ordering::SeqCst);
        if self.0.fail.load(Ordering::SeqCst) {
            4
        } else if n == 0 {
            1
        } else {
            2
        }
    }
}
struct Profile;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Profile {
    fn delete(&self) {}
}

#[test]
#[ignore = "requires dbus-run-session and QR_WIFI_DBUS_FIXTURES=1"]
fn native_bus_scans_before_join_and_waits_for_real_activation() {
    assert_eq!(std::env::var("QR_WIFI_DBUS_FIXTURES").as_deref(), Ok("1"));
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
    // This entire integration test runs in its own disposable process.
    std::env::set_var("DBUS_SYSTEM_BUS_ADDRESS", address);
    std::env::set_var("PATH", "/does-not-exist");
    let state = Arc::new(State::default());
    let _service = Builder::session()
        .unwrap()
        .name("org.freedesktop.NetworkManager")
        .unwrap()
        .serve_at(ROOT, Manager(state.clone()))
        .unwrap()
        .serve_at(DEVICE, Device)
        .unwrap()
        .serve_at(DEVICE, Wireless(state.clone()))
        .unwrap()
        .serve_at(AP, AccessPoint)
        .unwrap()
        .serve_at(ACTIVE, ActiveConnection(state.clone()))
        .unwrap()
        .serve_at(PROFILE, Profile)
        .unwrap()
        .build()
        .unwrap();
    let adapter = default_adapter();
    let credentials = WifiCredentials::new("Fixture Open", WifiSecurity::Nopass);
    let error = adapter.connect(&credentials).unwrap_err();
    assert!(
        error.to_string().contains("not visible"),
        "missing AP must have a reachability error, not an nmcli error"
    );
    assert_eq!(
        state.activated.load(Ordering::SeqCst),
        0,
        "absent network must not replace a working connection"
    );
    state.visible.store(true, Ordering::SeqCst);
    state.scanned.store(false, Ordering::SeqCst);
    adapter.connect(&credentials).unwrap();
    assert!(state.scanned.load(Ordering::SeqCst));
    assert!(
        state.state_reads.load(Ordering::SeqCst) >= 2,
        "request acceptance is not connection success"
    );
    state.fail.store(true, Ordering::SeqCst);
    state.scanned.store(false, Ordering::SeqCst);
    state.state_reads.store(0, Ordering::SeqCst);
    let error = adapter.connect(&credentials).unwrap_err();
    assert!(
        error.to_string().contains("join"),
        "failed activation must not be reported as connected"
    );
}
