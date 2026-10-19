//! Context menu shared by every view that lists individual tracks.
use crate::{
    features::{library::Library, playback::Playback},
    platform::desktop,
    shared::ui::{controls::menu_entry, toasts::Toasts},
};
use eframe::egui;

/// Navigation requested from a track context menu.
pub(crate) enum TrackMenuAction {
    /// Open the album at this index.
    GoToAlbum(usize),
}

/// Draw queue, navigation, and folder actions for one track inside an open context menu.
///
/// # Arguments
/// * `ui` - Context menu UI.
/// * `playback` - Playback queue to modify.
/// * `library` - Library containing the track.
/// * `track_index` - Track the menu was opened for.
/// * `offer_go_to_album` - Whether to show "Go to album" when the track belongs to one.
/// * `toasts` - Receives an error if the folder cannot be opened.
///
/// # Returns
/// A navigation request the caller should apply.
pub(crate) fn show(
    ui: &mut egui::Ui,
    playback: &mut Playback,
    library: &Library,
    track_index: usize,
    offer_go_to_album: bool,
    toasts: &mut Toasts,
) -> Option<TrackMenuAction> {
    crate::shared::ui::theme::style_menu_contents(ui, 220.0);
    let mut action = None;
    if menu_entry(ui, true, "Play next") {
        playback.play_next(&[track_index]);
    }
    if menu_entry(ui, true, "Add to queue") {
        playback.enqueue(&[track_index]);
    }
    let album = library
        .track_album
        .get(track_index)
        .copied()
        .flatten()
        .map(|album| album.get());
    if offer_go_to_album {
        if let Some(album) = album {
            if menu_entry(ui, true, "Go to album") {
                action = Some(TrackMenuAction::GoToAlbum(album));
            }
        }
    }
    let folder = library
        .tracks
        .get(track_index)
        .and_then(|track| track.path.parent())
        .filter(|folder| !crate::features::library::is_smb_path(folder));
    if menu_entry(ui, folder.is_some(), "Open containing folder") {
        if let Some(folder) = folder {
            if let Err(error) = desktop::open_folder(folder) {
                toasts.push(format!("Could not open {}: {error}", folder.display()));
            }
        }
    }
    action
}
