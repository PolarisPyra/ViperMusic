use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::HashMap,
    path::PathBuf,
    sync::{atomic::AtomicUsize, Arc},
};

/// An index into [`Library::albums`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AlbumIndex(usize);

impl AlbumIndex {
    /// Create an album index from its position in a library.
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Return the numeric position used to access an album slice.
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Tagged metadata and playback properties for one audio track.
#[derive(Clone, Deserialize, Serialize)]
pub struct Track {
    /// Track file path, including an SMB URL when the track is remote.
    pub path: PathBuf,
    /// Display title, populated from tags or the file name.
    pub title: String,
    /// Track artist, or an empty string when unavailable.
    pub artist: String,
    /// Album artist, or an empty string when unavailable.
    pub album_artist: String,
    /// Album title, or an empty string when unavailable.
    pub album: String,
    /// Disc number from the track tags, if present.
    pub disc_number: Option<u32>,
    /// Track number from the track tags, if present.
    pub track_number: Option<u32>,
    /// Track duration in milliseconds, if known.
    pub duration_ms: Option<u64>,
    /// Release year from the track tags, if present.
    pub release_year: Option<u32>,
    /// Audio stream properties.
    #[serde(default)]
    pub audio: AudioProperties,
    /// Whether no usable descriptive tags were found.
    #[serde(default)]
    pub metadata_missing: bool,
}

impl Track {
    /// Compare tracks by disc, track number, and path for album playback order.
    ///
    /// # Arguments
    /// * `other` - Track to compare against.
    ///
    /// # Returns
    /// The ordering of `self` relative to `other`.
    pub fn playback_order(&self, other: &Self) -> Ordering {
        self.disc_number
            .unwrap_or(1)
            .cmp(&other.disc_number.unwrap_or(1))
            .then_with(|| {
                self.track_number
                    .unwrap_or(u32::MAX)
                    .cmp(&other.track_number.unwrap_or(u32::MAX))
            })
            .then_with(|| self.path.cmp(&other.path))
    }
}

/// Technical properties of an audio stream.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct AudioProperties {
    /// Average bitrate in kilobits per second, if available.
    pub bitrate_kbps: Option<u32>,
    /// Sample rate in hertz, if available.
    pub sample_rate_hz: Option<u32>,
    /// Sample bit depth, if available.
    pub bit_depth: Option<u8>,
    /// Number of audio channels, if available.
    pub channels: Option<u8>,
}

/// An album and the indexes of its tracks in a library.
#[derive(Clone)]
pub struct Album {
    /// Album title.
    pub title: String,
    /// Album artist.
    pub artist: String,
    /// Stable key for persisted per-album state such as ratings and play counts.
    pub key: String,
    /// Track indexes into [`Library::tracks`], ordered for playback.
    pub tracks: Vec<usize>,
    /// Shared encoded album artwork, if available.
    pub art: Option<Arc<[u8]>>,
}

impl Album {
    /// Create an album from the tags of its first track.
    ///
    /// # Arguments
    /// * `first_track` - Track supplying the album title and album artist.
    /// * `tracks` - Track indexes into [`Library::tracks`], ordered for playback.
    /// * `art` - Shared encoded album artwork, if available.
    ///
    /// # Returns
    /// An album with trimmed metadata and its persisted-state key.
    pub fn new(first_track: &Track, tracks: Vec<usize>, art: Option<Arc<[u8]>>) -> Self {
        let title = first_track.album.trim().to_owned();
        let artist = first_track.album_artist.trim().to_owned();
        Self {
            key: format!("{artist}\0{title}"),
            title,
            artist,
            tracks,
            art,
        }
    }
}

/// Tracks and album groups discovered by a library scan.
#[derive(Default)]
pub struct Library {
    /// All discovered tracks.
    pub tracks: Arc<Vec<Track>>,
    /// Albums grouped from tracks with album metadata.
    pub albums: Vec<Album>,
    /// For each track, its album index, or `None` when it has no album.
    pub track_album: Arc<Vec<Option<AlbumIndex>>>,
    /// Number of directories that could not be scanned.
    pub unreadable_directories: usize,
    pub(crate) scan_error: Option<String>,
    /// Tracks whose tags could not be read.
    pub missing_metadata_tracks: usize,
    /// Empty tracks skipped during scanning.
    pub skipped_empty_files: Vec<PathBuf>,
    /// Nonfatal per-track errors from the scanner.
    pub scan_errors: Vec<String>,
}

/// Newly discovered tracks and associated artwork returned by an incremental scan.
pub struct DiscoveredTracks {
    /// Tracks not yet merged into the loaded library.
    pub tracks: Vec<Track>,
    pub(super) album_art: HashMap<(String, String), Arc<[u8]>>,
}

/// Atomic progress counters for an active library scan.
#[derive(Default)]
pub struct ScanProgress {
    /// Number of scan items completed.
    pub completed: AtomicUsize,
    /// Total number of scan items known so far.
    pub total: AtomicUsize,
    /// Current phase: track scanning or album assembly.
    pub phase: AtomicUsize,
}
