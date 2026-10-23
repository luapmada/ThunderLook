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
mod exchange;
mod folders;
mod global_search;
mod icons;
mod junk;
mod ipc;
mod list;
mod menu;
mod menus;
mod notes;
mod options;
mod print;
mod quicksteps;
mod reading;
mod reminders;
mod rules;
mod safety;
mod splitter;
mod state;
mod statusbar;
mod tasks;
mod topbar;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
