//! Audio library models, scanning, metadata caches, and filesystem watching.
mod audio;
pub(crate) mod background;
pub mod cache;
mod model;
mod modifications;
mod scanner;
#[cfg(target_os = "linux")]
pub(crate) mod smb;
#[cfg(target_os = "linux")]
mod smb_cache;
#[cfg(target_os = "linux")]
mod smb_reader;
pub(crate) mod sorting;
pub(crate) mod state;
pub(crate) mod symphonia;
mod tag_reader;
pub mod watcher;

pub(crate) use audio::{album_key, is_audio_path, is_smb_path};
pub use model::{
    Album, AlbumIndex, AudioProperties, DiscoveredTracks, Library, ScanProgress, Track,
};
pub use modifications::{merge_discovered_tracks, remove_library_tracks};
pub(crate) use scanner::scan_library_with_auth;
pub use scanner::{scan_added_tracks, scan_library};
