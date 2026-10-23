//! Interface de ThunderLook : Leptos (rendu côté client) compilé en WebAssembly, hébergé par Tauri.

mod accounts;
mod actions;
mod app;
mod appointment;
mod autoreply;
mod calendar;
mod commandbar;
mod contact_form;
mod contacts;
mod compose;
mod create_rule;
mod dialogs;
mod folders;
mod icons;
mod junk;
mod ipc;
mod list;
mod menu;
mod menus;
mod quicksteps;
mod reading;
mod rules;
mod splitter;
mod state;
mod statusbar;
mod topbar;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
