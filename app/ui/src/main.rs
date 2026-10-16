//! Interface de ThunderLook : Leptos (rendu côté client) compilé en WebAssembly, hébergé par Tauri.

mod app;
mod commandbar;
mod folders;
mod icons;
mod ipc;
mod list;
mod menu;
mod reading;
mod splitter;
mod state;
mod statusbar;
mod topbar;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
