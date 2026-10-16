// Pas de console supplémentaire en version publiée sous Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    thunderlook_lib::run();
}
