use std::sync::{mpsc::Sender, Arc};

use image::DynamicImage;
use ksni::{
    blocking::{Handle, TrayMethods},
    menu::StandardItem,
    Icon, MenuItem, ToolTip,
};

use super::TrayEvent;

/// StatusNotifierItem state read by the desktop host, which calls back on menu selections.
pub(crate) struct LinuxTray {
    sender: Sender<TrayEvent>,
    wake: Arc<dyn Fn() + Send + Sync>,
    icon: Vec<Icon>,
}

impl LinuxTray {
    /// Forward a menu selection to the app and wake its event loop.
    fn send(&self, event: TrayEvent) {
        let _ = self.sender.send(event);
        (self.wake)();
    }
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

/// Create the StatusNotifierItem tray icon with Show, Play, Pause, and Quit menu items.
///
/// The icon is spawned on the D-Bus service thread and stays alive while the returned handle does.
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
) -> Result<Handle<LinuxTray>, String> {
    LinuxTray {
        sender,
        wake: Arc::new(wake),
        icon: vec![argb_icon(logo)],
    }
    .spawn()
    .map_err(|error| error.to_string())
}
