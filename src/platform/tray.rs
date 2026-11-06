use std::sync::atomic::{AtomicBool, Ordering};

/// Menu selections sent from the system tray to the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrayEvent {
    Show,
    Play,
    Pause,
    Quit,
}

/// Set when the app answers a tray Quit by asking first, so the tray doesn't force the exit.
static QUIT_HELD: AtomicBool = AtomicBool::new(false);

/// Keep the process running after a tray Quit the app has chosen to ask about.
pub(crate) fn hold_quit() {
    QUIT_HELD.store(true, Ordering::Relaxed);
}

#[cfg(target_os = "linux")]
mod ksni_backend;
#[cfg(not(target_os = "linux"))]
mod tray_icon_backend;

#[cfg(target_os = "linux")]
pub(crate) use ksni_backend::{create, Tray};
#[cfg(not(target_os = "linux"))]
pub(crate) use tray_icon_backend::{create, Tray};
