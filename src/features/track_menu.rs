//! Context menu shared by every view that lists individual tracks.
use crate::{app::ViperApp, platform::desktop, shared::ui::controls::menu_item};
use gpui::{App, WeakEntity};
use gpui_component::menu::PopupMenu;

/// Add queue, navigation, and folder entries for one track to a context menu.
///
/// # Arguments
/// * `menu` - Menu being built.
/// * `app` - Application the entries act on.
/// * `track_index` - Track the menu was opened for.
/// * `offer_go_to_album` - Whether to show "Go to album" when the track belongs to one.
/// * `cx` - Application context, to read the library.
///
/// # Returns
/// The menu with the track's entries added.
pub(crate) fn build(
    menu: PopupMenu,
    app: &WeakEntity<ViperApp>,
    track_index: usize,
    offer_go_to_album: bool,
    cx: &App,
) -> PopupMenu {
    let Some(entity) = app.upgrade() else {
        return menu;
    };
    let library = &entity.read(cx).library.model;
    let album = library
        .track_album
        .get(track_index)
        .copied()
        .flatten()
        .map(|album| album.get())
        .filter(|_| offer_go_to_album);
    let folder = library
        .tracks
        .get(track_index)
        .and_then(|track| track.path.parent())
        .filter(|folder| !crate::features::library::is_smb_path(folder))
        .map(std::path::Path::to_path_buf);
    let mut menu = menu
        .item(menu_item("Play next", app, move |app, _, _| {
            app.playback().play_next(&[track_index]);
        }))
        .item(menu_item("Add to queue", app, move |app, _, _| {
            app.playback().enqueue(&[track_index]);
        }));
    if let Some(album) = album {
        menu = menu.item(menu_item("Go to album", app, move |app, _, _| {
            app.workbench.open_album(&mut app.library.view, album);
        }));
    }
    let has_folder = folder.is_some();
    menu.item(
        menu_item("Open containing folder", app, move |app, _, _| {
            if let Some(folder) = &folder {
                if let Err(error) = desktop::open_folder(folder) {
                    app.toasts
                        .push(format!("Could not open {}: {error}", folder.display()));
                }
            }
        })
        .disabled(!has_folder),
    )
}
