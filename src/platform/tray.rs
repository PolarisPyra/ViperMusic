/// Menu selections sent from the system tray to the app.
#[derive(Clone, Copy)]
pub(crate) enum TrayEvent {
    Show,
    Play,
    Pause,
    Quit,
}

#[cfg(target_os = "linux")]
mod ksni_backend;
#[cfg(not(target_os = "linux"))]
mod tray_icon_backend;

#[cfg(target_os = "linux")]
pub(crate) use ksni_backend::{create, Tray};
#[cfg(not(target_os = "linux"))]
pub(crate) use tray_icon_backend::{create, Tray};
