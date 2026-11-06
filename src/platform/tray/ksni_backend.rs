use std::{
    sync::{atomic::Ordering, mpsc::Sender, Arc},
    thread,
    time::Duration,
};

use image::DynamicImage;
use ksni::{
    blocking::{Handle, TrayMethods},
    menu::StandardItem,
    Icon, MenuItem, ToolTip,
};

use super::{TrayEvent, QUIT_HELD};

/// StatusNotifierItem state read by the desktop host, which calls back on menu selections.
pub(crate) struct LinuxTray {
    sender: Sender<TrayEvent>,
    wake: Arc<dyn Fn() + Send + Sync>,
    icon: Vec<Icon>,
}

impl LinuxTray {
    /// Forward a menu selection to the app and wake its event loop.
    fn send(&self, event: TrayEvent) {
        let quit = matches!(event, TrayEvent::Quit);
        if quit {
            QUIT_HELD.store(false, Ordering::Relaxed);
        }
        let _ = self.sender.send(event);
        (self.wake)();
        // On Wayland a minimized window can stay blocked presenting its last frame, so it never
        // sees the quit. Exit anyway if it is still running shortly after, skipping exit handlers
        // because the graphics driver's can wait forever on that blocked frame.
        if quit {
            thread::spawn(|| {
                if exit_after_quit(Duration::from_secs(2), crate::features::ripper::ripping) {
                    // SAFETY: `_exit` ends the process at once and touches no Rust state.
                    unsafe { libc::_exit(0) };
                }
            });
        }
    }
}

/// Wait out a tray Quit, then decide whether the tray should end the process itself.
///
/// A window blocked while minimized can't ask about a rip, so one still running finishes first.
///
/// # Arguments
/// * `delay` - Time the app has to quit on its own.
/// * `ripping` - Whether a disc is still being ripped.
///
/// # Returns
/// Whether to end the process: `false` once the app has held the quit to ask first.
fn exit_after_quit(delay: Duration, ripping: impl Fn() -> bool) -> bool {
    thread::sleep(delay);
    while ripping() && !QUIT_HELD.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(250));
    }
    !QUIT_HELD.load(Ordering::Relaxed)
}

/// Build a menu entry that sends `event` when selected.
///
/// # Arguments
/// * `label` - Text shown in the menu.
/// * `event` - Event sent when the entry is selected.
fn entry(label: &str, event: TrayEvent) -> MenuItem<LinuxTray> {
    MenuItem::Standard(StandardItem {
        label: label.into(),
        activate: Box::new(move |tray: &mut LinuxTray| tray.send(event)),
        ..Default::default()
    })
}

/// Convert the logo into the ARGB32 pixmap the StatusNotifierItem spec expects.
///
/// # Arguments
/// * `logo` - Source image, scaled down to at most 96 pixels per side.
fn argb_icon(logo: &DynamicImage) -> Icon {
    let rgba = logo.thumbnail(96, 96).into_rgba8();
    let (width, height) = rgba.dimensions();
    Icon {
        width: width as i32,
        height: height as i32,
        data: rgba
            .pixels()
            .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
            .collect(),
    }
}

impl ksni::Tray for LinuxTray {
    // Left click opens the menu instead of calling activate, so the menu is the only tray action.
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn title(&self) -> String {
        "Viper Music".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icon.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "Viper Music".into(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            entry("Show Viper Music", TrayEvent::Show),
            entry("Play", TrayEvent::Play),
            entry("Pause", TrayEvent::Pause),
            entry("Quit", TrayEvent::Quit),
        ]
    }
}

/// Handle to the StatusNotifierItem tray icon, which stays alive while this value does.
pub(crate) struct Tray(Handle<LinuxTray>);

impl Tray {
    /// Replace the tray icon with `logo`.
    ///
    /// # Arguments
    /// * `logo` - Image shown from now on, scaled down to at most 96 pixels per side.
    ///
    /// # Errors
    /// Returns a message if the tray service has stopped.
    pub(crate) fn set_logo(&self, logo: &DynamicImage) -> Result<(), String> {
        self.0
            .update(|tray| tray.icon = vec![argb_icon(logo)])
            .ok_or_else(|| "tray service has stopped".to_owned())
    }
}

/// Create the StatusNotifierItem tray icon with Show, Play, Pause, and Quit menu items.
///
/// The icon is spawned on the D-Bus service thread and stays alive while the returned tray does.
///
/// # Arguments
/// * `logo` - Image used for the tray icon.
/// * `sender` - Receives menu selections.
/// * `wake` - Called after each selection so an idle window processes it.
///
/// # Errors
/// Returns a message if the desktop has no usable D-Bus session for the tray.
pub(crate) fn create(
    logo: &DynamicImage,
    sender: Sender<TrayEvent>,
    wake: impl Fn() + Send + Sync + 'static,
) -> Result<Tray, String> {
    LinuxTray {
        sender,
        wake: Arc::new(wake),
        icon: vec![argb_icon(logo)],
    }
    .spawn()
    .map(Tray)
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{exit_after_quit, QUIT_HELD};
    use std::{cell::Cell, sync::atomic::Ordering, time::Duration};

    #[test]
    fn a_quit_the_app_cannot_see_lets_a_rip_finish_and_one_it_holds_stays() {
        QUIT_HELD.store(false, Ordering::Relaxed);
        let checks = Cell::new(0);
        let ripping = || {
            checks.set(checks.get() + 1);
            checks.get() < 3
        };
        assert!(exit_after_quit(Duration::ZERO, ripping));
        assert_eq!(checks.get(), 3);

        let ripping = || {
            QUIT_HELD.store(true, Ordering::Relaxed);
            true
        };
        assert!(!exit_after_quit(Duration::ZERO, ripping));
    }
}
