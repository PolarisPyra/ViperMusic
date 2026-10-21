use std::sync::mpsc::Sender;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

use super::TrayEvent;

/// Create the system tray icon with Show, Play, Pause, and Quit menu items.
///
/// # Arguments
/// * `logo` - Image used for the tray icon.
/// * `sender` - Receives menu selections.
/// * `wake` - Called after each selection so an idle window processes it.
///
/// # Errors
/// Returns a message if the desktop has no usable system tray.
pub(crate) fn create(
    logo: &image::DynamicImage,
    sender: Sender<TrayEvent>,
    wake: impl Fn() + Send + Sync + 'static,
) -> Result<TrayIcon, String> {
    let menu = Menu::new();
    let show_item = MenuItem::new("Show Viper Music", true, None);
    let play_item = MenuItem::new("Play", true, None);
    let pause_item = MenuItem::new("Pause", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    menu.append_items(&[&show_item, &play_item, &pause_item, &quit_item])
        .map_err(|error| error.to_string())?;

    let show_id = show_item.id().clone();
    let play_id = play_item.id().clone();
    let pause_id = pause_item.id().clone();
    let quit_id = quit_item.id().clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == show_id {
            let _ = sender.send(TrayEvent::Show);
        } else if event.id == play_id {
            let _ = sender.send(TrayEvent::Play);
        } else if event.id == pause_id {
            let _ = sender.send(TrayEvent::Pause);
        } else if event.id == quit_id {
            let _ = sender.send(TrayEvent::Quit);
        }
        wake();
    }));

    let icon_image = logo.thumbnail(96, 96).into_rgba8();
    let (width, height) = icon_image.dimensions();
    let icon =
        Icon::from_rgba(icon_image.into_raw(), width, height).map_err(|error| error.to_string())?;

    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Viper Music")
        .with_icon(icon)
        .build()
        .map_err(|error| error.to_string())
}
