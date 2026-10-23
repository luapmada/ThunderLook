//! Lien avec Outlook (COM) : calendrier commun, lu et écrit par Outlook.

#[cfg(windows)]
pub mod calendar;
#[cfg(windows)]
pub mod com;
#[cfg(windows)]
pub mod contacts;
#[cfg(windows)]
pub mod msg;
