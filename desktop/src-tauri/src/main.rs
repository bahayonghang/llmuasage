#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    llmusage::sync::enable_live_pricing_refresh();
    llmusage_desktop_lib::run();
}
