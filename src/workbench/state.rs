use crate::features::{library::state::LibraryViewState, tag_editor::TagEditor};

#[derive(Clone, Copy, PartialEq, Eq)]
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
        }
    }

    /// Whether a dialog that takes the keyboard, such as preferences or the tag editor, is open.
    pub(crate) fn dialog_open(&self) -> bool {
        self.show_preferences || self.tag_editor.is_some()
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
