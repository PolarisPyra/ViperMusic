use super::{album_key, is_audio_path};
use crate::features::library::{
    cache::TrackMetadataCache, Album, DiscoveredTracks, Library, ScanProgress, Track,
};
use crate::shared::artwork::load_artwork;
use lofty::file::TaggedFileExt;
use rayon::prelude::*;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use super::tag_reader::{read_metadata, ReaderMetadataError, TrackMetadata};

/// Result of reading one local track during a full scan.
// Nearly every entry is a track, so boxing it would only add an allocation per file.
#[allow(clippy::large_enum_variant)]
enum LocalScan {
    /// The scan was cancelled before this path was read.
    Cancelled,
    /// The file exists but is empty.
    Empty(PathBuf),
    /// Track metadata and the filesystem metadata used to validate future cache hits.
    Track(Track, Option<fs::Metadata>),
}

pub(crate) fn load_track_artwork(path: &Path) -> Option<Vec<u8>> {
    load_artwork(path, |path| {
        if let Ok(file) = lofty::read_from_path(path) {
            if let Some(bytes) = file
                .primary_tag()
                .or_else(|| file.first_tag())
                .and_then(|tag| tag.pictures().first())
                .map(|picture| picture.data().to_vec())
            {
                return Some(bytes);
            }
        }
        let media = super::symphonia::probe(path)?;
        let cover_index = media
            .visuals
            .iter()
            .position(|visual| {
                visual.usage == Some(symphonia::core::meta::StandardVisualKey::FrontCover)
            })
            .unwrap_or(0);
        media
            .visuals
            .into_iter()
            .nth(cover_index)
            .map(|visual| visual.data)
    })
}

/// Scan a local or SMB library root and return its tracks and albums.
///
/// # Arguments
/// * `root` - Folder path or SMB URL to scan.
/// * `cancel` - Atomic flag set to stop the scan.
/// * `progress` - Shared progress counters updated during the scan.
///
/// # Returns
/// The scanned library. Scan-level failures are recorded in the returned value.
///
/// # Examples
/// ```no_run
/// use std::{path::Path, sync::atomic::AtomicBool};
/// use viper::features::library::{scan_library, ScanProgress};
///
/// let cancel = AtomicBool::new(false);
/// let progress = ScanProgress::default();
/// let library = scan_library(Path::new("/music"), &cancel, &progress);
/// println!("Found {} tracks", library.tracks.len());
/// ```
pub fn scan_library(root: &Path, cancel: &AtomicBool, progress: &ScanProgress) -> Library {
    #[cfg(target_os = "linux")]
    return scan_library_with_auth(root, cancel, progress, None);
    #[cfg(not(target_os = "linux"))]
    scan_library_with_auth(root, cancel, progress)
}

pub(crate) fn scan_library_with_auth(
    root: &Path,
    cancel: &AtomicBool,
    progress: &ScanProgress,
    #[cfg(target_os = "linux")] smb_auth: Option<super::smb::SmbAuth>,
) -> Library {
    let mut refreshed_cache = TrackMetadataCache::default();
    let missing_metadata_tracks = AtomicUsize::new(0);
    let mut audio_paths = Vec::new();
    #[cfg(target_os = "linux")]
    let mut smb_fingerprints = HashMap::new();
    let mut skipped_empty_files = Vec::new();
    let mut scan_errors = Vec::new();
    let mut unreadable_directories = 0;
    let is_smb = cfg!(target_os = "linux") && super::is_smb_path(root);
    let cache = if is_smb {
        TrackMetadataCache::default()
    } else {
        TrackMetadataCache::load()
    };
    if is_smb {
        #[cfg(target_os = "linux")]
        {
            if let Some(auth) = smb_auth.as_ref() {
                match super::smb::scan(&root.to_string_lossy(), auth, cancel) {
                    Ok(entries) => {
                        for entry in entries {
                            if let Some(fingerprint) = entry.fingerprint {
                                smb_fingerprints.insert(entry.path.clone(), fingerprint);
                            }
                            audio_paths.push(entry.path);
                        }
                    }
                    Err(error) => {
                        eprintln!("Could not scan SMB music share: {error}");
                        unreadable_directories = 1;
                    }
                }
            } else {
                unreadable_directories = 1;
            }
        }
    } else {
        scan_dir(
            root,
            &mut audio_paths,
            &mut unreadable_directories,
            &mut HashSet::new(),
            cancel,
        );
    }
    if cancel.load(Ordering::Relaxed) {
        return Library::default();
    }
    #[cfg(target_os = "linux")]
    let mut smb_album_art: HashMap<(String, String, bool), Arc<[u8]>> = HashMap::new();
    progress.phase.store(1, Ordering::Relaxed);
    progress.total.store(audio_paths.len(), Ordering::Relaxed);
    let scan_path = |path: &PathBuf| {
        if cancel.load(Ordering::Relaxed) {
            return LocalScan::Cancelled;
        }
        let metadata = fs::metadata(path).ok();
        if metadata
            .as_ref()
            .is_some_and(|metadata| metadata.len() == 0)
        {
            progress.completed.fetch_add(1, Ordering::Relaxed);
            return LocalScan::Empty(path.clone());
        }
        let mut track = metadata
            .as_ref()
            .and_then(|metadata| cache.get(path, metadata))
            .unwrap_or_else(|| scan_track(path.clone()));
        if !track.metadata_missing && track_has_no_metadata(&track) {
            track.metadata_missing = true;
        }
        fill_missing_display_metadata(&mut track);
        if track.metadata_missing {
            missing_metadata_tracks.fetch_add(1, Ordering::Relaxed);
        }
        progress.completed.fetch_add(1, Ordering::Relaxed);
        LocalScan::Track(track, metadata)
    };
    #[cfg(target_os = "linux")]
    let scanned: Vec<_> = if is_smb {
        let Some(auth) = smb_auth.as_ref() else {
            return Library {
                scan_error: Some("Reconnect to the SMB share before scanning".into()),
                ..Library::default()
            };
        };
        let smb_root = root.to_string_lossy().into_owned();
        let smb_cache = super::smb_cache::SmbMetadataCache::load(root);
        let mut refreshed_smb_cache = super::smb_cache::SmbMetadataCache::default();
        let mut cache_changed = false;
        let mut scanned = Vec::with_capacity(audio_paths.len());
        let scan_smb_path = |session: &mut Option<Result<super::smb::SmbSession, String>>,
                             path: &PathBuf| {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let fingerprint = smb_fingerprints.get(path);
            if let Some((track, artwork)) =
                fingerprint.and_then(|fingerprint| smb_cache.get(path, fingerprint))
            {
                return Some((
                    track,
                    artwork,
                    fingerprint.cloned(),
                    true,
                    None,
                    false,
                    false,
                ));
            }

            let session = session.get_or_insert_with(|| {
                super::smb::SmbSession::new(&smb_root, auth).map_err(|error| error.to_string())
            });
            let (mut metadata, cacheable, error, skipped_empty) = match session {
                Ok(session) => match session.read_metadata(path) {
                    Ok(metadata) => (metadata, true, None, false),
                    Err(ReaderMetadataError::Empty) => {
                        (TrackMetadata::default(), false, None, true)
                    }
                    Err(ReaderMetadataError::Failed(error)) => {
                        (TrackMetadata::default(), false, Some(error), false)
                    }
                },
                Err(error) => (
                    TrackMetadata::default(),
                    false,
                    Some(format!("Could not open SMB share for scanning: {error}")),
                    false,
                ),
            };
            let artwork = metadata.artwork.take().map(Arc::<[u8]>::from);
            Some((
                track_from_metadata(path.clone(), metadata),
                artwork,
                fingerprint.cloned(),
                cacheable,
                error,
                skipped_empty,
                true,
            ))
        };
        // Limit concurrent SMB sessions so large shares do not overwhelm the server.
        let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build();
        let results: Vec<_> = match pool {
            Ok(pool) => pool.install(|| {
                audio_paths
                    .par_iter()
                    .map_init(|| None, |session, path| scan_smb_path(session, path))
                    .collect()
            }),
            Err(_) => {
                let mut session = None;
                audio_paths
                    .iter()
                    .map(|path| scan_smb_path(&mut session, path))
                    .collect()
            }
        };
        for result in results {
            if cancel.load(Ordering::Relaxed) {
                return Library::default();
            }
            let Some((
                mut track,
                embedded_art,
                fingerprint,
                cacheable,
                error,
                skipped_empty,
                cache_miss,
            )) = result
            else {
                continue;
            };
            cache_changed |= cache_miss;
            if skipped_empty {
                skipped_empty_files.push(track.path.clone());
                progress.completed.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            if let Some(error) = error {
                scan_errors.push(format!(
                    "Could not read tags from SMB track {}: {error}",
                    track.path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            // Failures must be retried on the next scan rather than becoming cache hits.
            if cacheable {
                if let Some(fingerprint) = fingerprint {
                    refreshed_smb_cache.insert(fingerprint, track.clone(), embedded_art.clone());
                }
            }
            fill_missing_display_metadata(&mut track);
            if track.metadata_missing {
                missing_metadata_tracks.fetch_add(1, Ordering::Relaxed);
            }
            if !track.album.trim().is_empty() {
                if let Some(art) = embedded_art {
                    smb_album_art
                        .entry(album_key(&track.album, &track.album_artist, track.is_lossless()))
                        .or_insert(art);
                }
            }
            progress.completed.fetch_add(1, Ordering::Relaxed);
            scanned.push(LocalScan::Track(track, None));
        }
        // An unchanged rescan should not rewrite potentially large artwork blobs.
        if !cancel.load(Ordering::Relaxed)
            && unreadable_directories == 0
            && (cache_changed || refreshed_smb_cache.len() != smb_cache.len())
        {
            refreshed_smb_cache.save(root);
        }
        scanned
    } else {
        audio_paths.par_iter().map(scan_path).collect()
    };
    #[cfg(not(target_os = "linux"))]
    let scanned: Vec<_> = audio_paths.par_iter().map(scan_path).collect();
    // The paths are now owned by the scanned Track values; release the original
    // directory-walk list before assembling the final library and cover art.
    drop(audio_paths);
    drop(cache);
    if cancel.load(Ordering::Relaxed) {
        return Library::default();
    }
    let mut tracks = Vec::with_capacity(scanned.len());
    for scan in scanned {
        match scan {
            LocalScan::Cancelled => {}
            LocalScan::Empty(path) => skipped_empty_files.push(path),
            LocalScan::Track(track, cache_metadata) => {
                if let Some(metadata) = cache_metadata {
                    refreshed_cache.insert(track.path.clone(), &metadata, track.clone());
                }
                tracks.push(track);
            }
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Library::default();
    }
    if !is_smb {
        refreshed_cache.save();
    }
    tracks.par_sort_unstable_by(|left, right| left.path.cmp(&right.path));

    let mut groups: BTreeMap<(String, String, bool), Vec<usize>> = BTreeMap::new();
    for (index, track) in tracks.iter().enumerate() {
        // Keep tracks with missing album tags playable, but don't manufacture an album for them.
        if track.album.trim().is_empty() {
            continue;
        }
        groups
            .entry(album_key(&track.album, &track.album_artist, track.is_lossless()))
            .or_default()
            .push(index);
    }

    let mut albums = Vec::with_capacity(groups.len());
    let mut track_album = vec![None; tracks.len()];
    progress.total.fetch_add(groups.len(), Ordering::Relaxed);
    progress.phase.store(2, Ordering::Relaxed);
    for (_key, mut album_tracks) in groups {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        album_tracks.sort_by(|&left, &right| tracks[left].playback_order(&tracks[right]));
        #[cfg(target_os = "linux")]
        let art = smb_album_art.remove(&_key);
        #[cfg(not(target_os = "linux"))]
        let art = None;
        let album_index = albums.len();
        for &track_index in &album_tracks {
            track_album[track_index] = Some(super::model::AlbumIndex::new(album_index));
        }
        albums.push(Album::new(&tracks[album_tracks[0]], album_tracks, art));
        progress.completed.fetch_add(1, Ordering::Relaxed);
    }

    Library {
        tracks: std::sync::Arc::new(tracks),
        albums,
        track_album: std::sync::Arc::new(track_album),
        unreadable_directories,
        scan_error: None,
        missing_metadata_tracks: missing_metadata_tracks.load(Ordering::Relaxed),
        skipped_empty_files,
        scan_errors,
    }
}

/// Scan changed local paths for tracks and album artwork.
///
/// # Arguments
/// * `paths` - Paths to inspect.
/// * `cancel` - Atomic flag set to stop the scan.
///
/// # Returns
/// Newly discovered tracks and their album artwork.
pub fn scan_added_tracks(paths: &[PathBuf], cancel: &AtomicBool) -> DiscoveredTracks {
    let mut candidates = Vec::new();
    let mut visited = HashSet::new();
    let mut unreadable_directories = 0;
    for path in paths {
        if cancel.load(Ordering::Relaxed) {
            return DiscoveredTracks {
                tracks: Vec::new(),
                album_art: HashMap::new(),
            };
        }
        if path.is_dir() {
            scan_dir(
                path,
                &mut candidates,
                &mut unreadable_directories,
                &mut visited,
                cancel,
            );
        } else if path.is_file() && is_audio_path(path) {
            candidates.push(path.clone());
        }
    }
    candidates.sort();
    candidates.dedup();
    wait_for_stable_files(&candidates, cancel);
    let tracks: Vec<_> = candidates
        .par_iter()
        .filter(|path| !cancel.load(Ordering::Relaxed) && path.is_file())
        .map(|path| scan_track(path.clone()))
        .collect();
    let mut album_art = HashMap::new();
    let mut seen_album_art = HashSet::new();
    for track in &tracks {
        if track.album.trim().is_empty() {
            continue;
        }
        let key = album_key(&track.album, &track.album_artist, track.is_lossless());
        if seen_album_art.insert(key.clone()) {
            if let Some(art) = load_track_artwork(&track.path) {
                album_art.insert(key, Arc::from(art));
            }
        }
    }
    DiscoveredTracks { tracks, album_art }
}

fn wait_for_stable_files(paths: &[PathBuf], cancel: &AtomicBool) {
    let signature = |path: &Path| {
        let metadata = fs::metadata(path).ok()?;
        Some((metadata.len(), metadata.modified().ok()?))
    };
    let mut previous: Vec<_> = paths.iter().map(|path| signature(path)).collect();
    for _ in 0..20 {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_millis(150));
        let current: Vec<_> = paths.iter().map(|path| signature(path)).collect();
        if current == previous {
            return;
        }
        previous = current;
    }
}

fn scan_dir(
    path: &Path,
    audio_paths: &mut Vec<PathBuf>,
    unreadable_directories: &mut usize,
    visited: &mut HashSet<PathBuf>,
    cancel: &AtomicBool,
) {
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let Ok(canonical_path) = fs::canonicalize(path) else {
        *unreadable_directories += 1;
        return;
    };
    if !visited.insert(canonical_path) {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        *unreadable_directories += 1;
        return;
    };
    for entry in entries.flatten() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let path = entry.path();
        // Directory entries report their type without a stat; only symlinks need resolving.
        let is_dir = entry
            .file_type()
            .is_ok_and(|kind| kind.is_dir() || (kind.is_symlink() && path.is_dir()));
        if is_dir {
            scan_dir(&path, audio_paths, unreadable_directories, visited, cancel);
            continue;
        }
        if !is_audio_path(&path) {
            continue;
        }

        audio_paths.push(path);
    }
}

fn scan_track(path: PathBuf) -> Track {
    let metadata = read_metadata(&path);
    track_from_metadata(path, metadata)
}

fn track_from_metadata(path: PathBuf, metadata: TrackMetadata) -> Track {
    let metadata_missing = !metadata.has_tags();
    Track {
        path,
        title: metadata.title.unwrap_or_default(),
        artist: metadata.artist.unwrap_or_default(),
        album_artist: metadata.album_artist.unwrap_or_default(),
        album: metadata.album.unwrap_or_default(),
        disc_number: metadata.disc_number,
        track_number: metadata.track_number,
        duration_ms: metadata.duration_ms,
        release_year: metadata.release_year,
        audio: metadata.audio,
        metadata_missing,
    }
}

fn track_has_no_metadata(track: &Track) -> bool {
    track.title.trim().is_empty()
        && track.artist.trim().is_empty()
        && track.album_artist.trim().is_empty()
        && track.album.trim().is_empty()
        && track.disc_number.is_none()
        && track.track_number.is_none()
        && track.release_year.is_none()
}

fn fill_missing_display_metadata(track: &mut Track) {
    if track.title.trim().is_empty() {
        track.title = track
            .path
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    if track.album.trim().is_empty() {
        track.album = track
            .path
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
}
