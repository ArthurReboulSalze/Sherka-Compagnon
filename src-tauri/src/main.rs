#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if sherka_desktop::portable_update::run_if_requested() {
        return;
    }
    sherka_desktop::run();
}
