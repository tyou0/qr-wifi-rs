//! `qr-wifi-tui` binary: launches the shared interactive menu.

use qr_wifi_core::default_adapter;

fn main() {
    let adapter = default_adapter();
    qr_wifi_tui::run_menu(adapter.as_ref());
}
