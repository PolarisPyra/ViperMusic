use crate::features::{library::state::LibraryViewState, tag_editor::TagEditor};
use gpui::{Bounds, Pixels, ScrollHandle, UniformListScrollHandle};
use serde::{Deserialize, Serialize};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Page {
    Home,
    Albums,
    Songs,
    Ripper,
}

impl Page {
    /// Every page reachable from the sidebar and View menu, in display order.
    pub(crate) const ALL: [Self; 4] = [Self::Home, Self::Albums, Self::Songs, Self::Ripper];

    /// Return the page name shown in navigation.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Albums => "Albums",
            Self::Songs => "Songs",
            Self::Ripper => "Rip CD",
        }
    }
}

pub(crate) struct WorkbenchState {
    pub(crate) page: Page,
    pub(crate) show_preferences: bool,
    pub(crate) show_album_details: bool,
    pub(crate) show_queue: bool,
    pub(crate) show_shortcuts: bool,
    /// The album tag editor, while it is open.
    pub(crate) tag_editor: Option<TagEditor>,
    /// Set when the user asks to exit, so closing the window does not just hide it to the tray.
    pub(crate) quit_requested: bool,
    /// Whether the dialog asking about quitting during a rip is open.
    pub(crate) confirm_quit: bool,
}

impl WorkbenchState {
    pub(crate) fn new(page: Page) -> Self {
        Self {
            page,
            show_preferences: false,
            show_album_details: false,
            show_queue: false,
            show_shortcuts: false,
            tag_editor: None,
            quit_requested: false,
            confirm_quit: false,
        }
    }

    /// Whether a dialog that takes the keyboard, such as preferences or the tag editor, is open.
    pub(crate) fn dialog_open(&self) -> bool {
        self.show_preferences || self.tag_editor.is_some() || self.confirm_quit
    }

    /// Switch to the albums page with an album's track panel open.
    ///
    /// # Arguments
    /// * `view` - Library selection state to update.
    /// * `album_index` - Index into the library's albums.
    pub(crate) fn open_album(&mut self, view: &mut LibraryViewState, album_index: usize) {
        view.select_album(album_index);
        view.scroll_to_selection = true;
        self.page = Page::Albums;
        self.show_album_details = true;
    }
}

/// Transient interface state that only lives while the window is drawn, such as drags in progress.
#[derive(Default)]
pub(crate) struct WorkbenchUi {
    /// Focus the search field the next time it is drawn.
    pub(crate) focus_search: bool,
    /// Sidebar width while its edge is being dragged.
    pub(crate) sidebar_drag: Option<f32>,
    /// Track the seek bar is being dragged on, and the position it is dragged to as a fraction of
    /// that track.
    pub(crate) seek_preview: Option<(usize, f32)>,
    /// Where the pointer rests over the seek bar, as a fraction of the track.
    pub(crate) seek_hover: Option<f32>,
    /// Smooths the playback position the seek bar draws.
    pub(crate) seek_clock: crate::workbench::shell::player_bar::SeekClock,
    /// Where the seek bar was last drawn, to turn clicks into positions.
    pub(crate) seek_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Where the volume slider was last drawn.
    pub(crate) volume_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Set while the volume slider is dragged, so the volume is saved once it is let go.
    pub(crate) volume_dragging: bool,
    /// Where the interface scale slider in Preferences was last drawn.
    pub(crate) zoom_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Set while the interface scale slider is dragged, so the scale is saved once it is let go.
    pub(crate) zoom_dragging: bool,
    /// Table and divider being dragged, so the column widths are saved once it is let go.
    pub(crate) column_drag: Option<(&'static str, usize)>,
    /// Queue position a dragged song would be dropped at.
    pub(crate) queue_drop: Option<usize>,
    /// Album track panel width while its edge is being dragged.
    pub(crate) track_panel_drag: Option<f32>,
    /// Height the lyrics were resized to; the default height until then.
    pub(crate) lyrics_height: Option<f32>,
    /// Set while the lyrics are being resized.
    pub(crate) lyrics_resizing: bool,
    /// Whether the pointer is over the lyrics, which stops them following playback.
    pub(crate) lyrics_hovered: bool,
    /// Track and line the lyrics last scrolled to.
    pub(crate) lyrics_followed: Option<(Option<usize>, Option<usize>)>,
    pub(crate) albums_scroll: UniformListScrollHandle,
    pub(crate) album_tracks_scroll: UniformListScrollHandle,
    pub(crate) songs_scroll: UniformListScrollHandle,
    pub(crate) lyrics_scroll: ScrollHandle,
    pub(crate) home_scroll: ScrollHandle,
    /// Horizontal scroll of each Home album shelf.
    pub(crate) home_shelves: [ScrollHandle; 3],
}
