use super::{
    background::LibraryBackgroundTasks, merge_discovered_tracks, remove_library_tracks, sorting,
    DiscoveredTracks, Library, ScanProgress,
};
use crate::platform::persistence::settings::Settings;
use rayon::prelude::*;
use std::{path::PathBuf, sync::Arc};

pub(crate) struct LibraryFeature {
    pub(crate) model: Library,
    pub(crate) view: LibraryViewState,
    pub(crate) scanning: bool,
    pub(crate) progress: Arc<ScanProgress>,
    pub(crate) background: LibraryBackgroundTasks,
}

impl LibraryFeature {
    pub(crate) fn new(random_sort_seed: u64) -> Self {
        Self {
            model: Library::default(),
            view: LibraryViewState::new(random_sort_seed),
            scanning: false,
            progress: Arc::new(ScanProgress::default()),
            background: LibraryBackgroundTasks::default(),
        }
    }

    pub(crate) fn filtered_album_indices(&mut self, settings: &Settings) -> Arc<Vec<usize>> {
        if self.view.album_filter_dirty
            || self.view.album_sort_dirty
            || self.view.album_filter_query != self.view.search
        {
            let query = self.view.search.to_lowercase();
            let mut matches: Vec<_> = self
                .model
                .albums
                .iter()
                .enumerate()
                .filter(|(_, album)| {
                    !album.tracks.is_empty()
                        && (query.is_empty()
                            || album.title.to_lowercase().contains(&query)
                            || album.artist.to_lowercase().contains(&query))
                })
                .map(|(index, _)| index)
                .collect();
            sorting::sort_indices(
                &mut matches,
                &self.model.albums,
                &self.model.tracks,
                settings,
                self.view.random_sort_seed,
            );
            self.view.filtered_albums = Arc::new(matches);
            self.view.album_filter_query.clone_from(&self.view.search);
            self.view.album_filter_dirty = false;
            self.view.album_sort_dirty = false;
        }
        Arc::clone(&self.view.filtered_albums)
    }

    /// Return library tracks whose title, artist, or album matches the search, in library order.
    ///
    /// # Returns
    /// Shared track indexes, recomputed only when the library or search text changes.
    pub(crate) fn filtered_song_indices(&mut self) -> Arc<Vec<usize>> {
        if self.view.song_filter_dirty || self.view.song_filter_query != self.view.search {
            let query = self.view.search.to_lowercase();
            let matches = if query.is_empty() {
                (0..self.model.tracks.len()).collect()
            } else {
                self.model
                    .tracks
                    .par_iter()
                    .enumerate()
                    .filter(|(_, track)| {
                        [&track.title, &track.artist, &track.album]
                            .iter()
                            .any(|field| field.to_lowercase().contains(&query))
                    })
                    .map(|(index, _)| index)
                    .collect()
            };
            self.view.filtered_songs = Arc::new(matches);
            self.view.song_filter_query.clone_from(&self.view.search);
            self.view.song_filter_dirty = false;
        }
        Arc::clone(&self.view.filtered_songs)
    }

    pub(crate) fn start_scan(
        &mut self,
        root: PathBuf,
        #[cfg(target_os = "linux")] smb_auth: Option<super::smb::SmbAuth>,
    ) -> Result<(), String> {
        self.background.cancel_artwork_scan();
        self.background.clear_watch_state();
        self.model = Library::default();
        self.view.mark_library_changed();
        self.view.selected_album = None;
        self.view.highlighted_album = None;
        self.view.selected_song = None;
        self.scanning = true;
        match self.background.start_scan(
            root,
            #[cfg(target_os = "linux")]
            smb_auth,
        ) {
            Ok(progress) => {
                self.progress = progress;
                Ok(())
            }
            Err(error) => {
                self.scanning = false;
                Err(error)
            }
        }
    }

    pub(crate) fn poll_scan(&mut self) -> Option<ScanCompletion> {
        let result = self.background.poll_scan()?;
        self.scanning = false;
        match result {
            Ok(library) => {
                let artwork_paths = library
                    .albums
                    .iter()
                    .enumerate()
                    .filter(|(_, album)| album.art.is_none())
                    .filter_map(|(album_index, album)| {
                        let track_index = *album.tracks.first()?;
                        let path = library.tracks.get(track_index)?.path.clone();
                        Some((album_index, path))
                    })
                    .collect();
                if let Err(error) = self.background.start_artwork_scan(artwork_paths) {
                    eprintln!("{error}");
                }
                let warning = if let Some(error) = library.scan_error.clone() {
                    Some(error)
                } else if library.unreadable_directories > 0 {
                    Some(format!(
                        "Skipped {} unreadable director{} while scanning",
                        library.unreadable_directories,
                        if library.unreadable_directories == 1 {
                            "y"
                        } else {
                            "ies"
                        }
                    ))
                } else if library.tracks.is_empty() {
                    Some("No supported audio files were found in this folder".into())
                } else {
                    None
                };
                self.model = library;
                self.view.mark_library_changed();
                Some(ScanCompletion::Loaded { warning })
            }
            Err(error) => Some(ScanCompletion::Failed(error)),
        }
    }

    pub(crate) fn apply_watcher_update(
        &mut self,
        discovered: DiscoveredTracks,
        removed_paths: &[PathBuf],
    ) -> LibraryUpdate {
        let track_remap = remove_library_tracks(&mut self.model, removed_paths);
        let remap = |track: Option<usize>| {
            track.and_then(|index| track_remap.get(index).copied().flatten())
        };
        self.view.selected_track = remap(self.view.selected_track);
        self.view.selected_song = remap(self.view.selected_song);
        let selected_album_cleared = self.view.selected_album.is_some_and(|index| {
            self.model
                .albums
                .get(index)
                .is_none_or(|album| album.tracks.is_empty())
        });
        if selected_album_cleared {
            self.view.selected_album = None;
            self.view.highlighted_album = None;
        }

        let removed_any = track_remap.iter().any(Option::is_none);
        let added_tracks = merge_discovered_tracks(&mut self.model, discovered);
        if added_tracks > 0 || removed_any {
            self.view.mark_library_changed();
        }
        LibraryUpdate {
            track_remap,
            added_tracks,
            removed_any,
            selected_album_cleared,
        }
    }
}

pub(crate) struct LibraryUpdate {
    pub(crate) track_remap: Vec<Option<usize>>,
    pub(crate) added_tracks: usize,
    pub(crate) removed_any: bool,
    pub(crate) selected_album_cleared: bool,
}

pub(crate) enum ScanCompletion {
    Loaded { warning: Option<String> },
    Failed(String),
}

pub(crate) struct LibraryViewState {
    pub(crate) search: String,
    pub(crate) selected_album: Option<usize>,
    pub(crate) highlighted_album: Option<usize>,
    pub(crate) selected_track: Option<usize>,
    pub(crate) album_sort_dirty: bool,
    pub(crate) album_filter_query: String,
    pub(crate) filtered_albums: Arc<Vec<usize>>,
    pub(crate) album_filter_dirty: bool,
    pub(crate) random_sort_seed: u64,
    /// Track selected in the songs view.
    pub(crate) selected_song: Option<usize>,
    pub(crate) song_filter_query: String,
    pub(crate) filtered_songs: Arc<Vec<usize>>,
    pub(crate) song_filter_dirty: bool,
    /// Album grid columns from the last frame, used for keyboard navigation.
    pub(crate) grid_columns: usize,
    /// Set by keyboard navigation so the next frame scrolls the selection into view.
    pub(crate) scroll_to_selection: bool,
}

impl LibraryViewState {
    pub(crate) fn new(random_sort_seed: u64) -> Self {
        Self {
            search: String::new(),
            selected_album: None,
            highlighted_album: None,
            selected_track: None,
            album_sort_dirty: true,
            album_filter_query: String::new(),
            filtered_albums: Arc::new(Vec::new()),
            album_filter_dirty: true,
            random_sort_seed,
            selected_song: None,
            song_filter_query: String::new(),
            filtered_songs: Arc::new(Vec::new()),
            song_filter_dirty: true,
            grid_columns: 1,
            scroll_to_selection: false,
        }
    }

    /// Invalidate cached album and song lists after tracks were added, removed, or rescanned.
    fn mark_library_changed(&mut self) {
        self.album_filter_dirty = true;
        self.album_sort_dirty = true;
        self.song_filter_dirty = true;
    }

    /// Select an album, highlight it in the browser, and clear the selected track.
    ///
    /// # Arguments
    /// * `album_index` - Index into the library's albums.
    pub(crate) fn select_album(&mut self, album_index: usize) {
        self.selected_track = None;
        self.selected_album = Some(album_index);
        self.highlighted_album = Some(album_index);
    }
}
