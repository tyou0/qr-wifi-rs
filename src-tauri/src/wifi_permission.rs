//! Request macOS Wi-Fi identity access only while the app is in use.
//! The manager stays on the main run loop. No coordinates are requested.
#[cfg(target_os = "macos")]
pub fn request() {
    use objc2::{rc::Retained, MainThreadMarker};
    use objc2_core_location::{CLAuthorizationStatus, CLLocationManager};
    use std::cell::RefCell;
    thread_local! {
        static MANAGER: RefCell<Option<Retained<CLLocationManager>>> = const { RefCell::new(None) };
    }
    assert!(
        MainThreadMarker::new().is_some(),
        "Location authorization must start on the main thread"
    );
    MANAGER.with(|slot| {
        let mut slot = slot.borrow_mut();
        // SAFETY: called for a focused window on Tauri's main event loop. The
        // thread-local retains the manager until application exit.
        let manager = slot.get_or_insert_with(|| unsafe { CLLocationManager::new() });
        if unsafe { manager.authorizationStatus() } == CLAuthorizationStatus::NotDetermined {
            unsafe { manager.requestWhenInUseAuthorization() };
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn request() {}
