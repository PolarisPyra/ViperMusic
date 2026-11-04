use super::{
    album_key,
    model::{Album, AlbumIndex, DiscoveredTracks, Library, Track},
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

/// Merge newly discovered tracks into an existing library.
///
/// # Arguments
/// * `library` - Library to update.
/// * `discovered` - Tracks and artwork from an incremental scan.
///
/// # Returns
/// Number of tracks added.
pub fn merge_discovered_tracks(library: &mut Library, discovered: DiscoveredTracks) -> usize {
    let mut album_indices = HashMap::with_capacity(library.albums.len());
    let mut existing_paths = HashSet::with_capacity(library.tracks.len());
    existing_paths.extend(library.tracks.iter().map(|track| track.path.clone()));
    for (index, album) in library.albums.iter().enumerate() {
        album_indices
            .entry(album_key(&album.title, &album.artist, album.lossless))
            .or_insert(index);
    }
    let mut added_paths = HashSet::new();
    let mut updated_albums = HashSet::new();
    let mut added_count = 0;
    for track in discovered.tracks {
        if existing_paths.contains(&track.path) || !added_paths.insert(track.path.clone()) {
            continue;
        }
        let track_index = library.tracks.len();
        let key = (!track.album.trim().is_empty())
            .then(|| album_key(&track.album, &track.album_artist, track.is_lossless()));
        Arc::make_mut(&mut library.tracks).push(track);
        added_count += 1;
        let Some(key) = key else {
            Arc::make_mut(&mut library.track_album).push(None);
            continue;
        };
        let art = discovered.album_art.get(&key).cloned();
        let album_index = if let Some(&index) = album_indices.get(&key) {
            if library.albums[index].art.is_none() {
                library.albums[index].art = art;
            }
            library.albums[index].tracks.push(track_index);
            index
        } else {
            let index = library.albums.len();
            library.albums.push(Album::new(
                &library.tracks[track_index],
                vec![track_index],
                art,
            ));
            album_indices.insert(key, index);
            index
        };
        Arc::make_mut(&mut library.track_album).push(Some(AlbumIndex::new(album_index)));
        updated_albums.insert(album_index);
    }
    for album_index in updated_albums {
        library.albums[album_index]
            .tracks
            .sort_by(|&left, &right| library.tracks[left].playback_order(&library.tracks[right]));
    }
    added_count
}

/// Remove tracks under the supplied paths and return old-to-new track indexes.
///
/// # Arguments
/// * `library` - Library to update.
/// * `removed_paths` - Removed file or directory paths.
///
/// # Returns
/// An index remap with `None` for removed tracks.
pub fn remove_library_tracks(
    library: &mut Library,
    removed_paths: &[PathBuf],
) -> Vec<Option<usize>> {
    let mut remap = vec![None; library.tracks.len()];
    let mut tracks = Vec::with_capacity(library.tracks.len());
    for (old_index, track) in Arc::make_mut(&mut library.tracks).drain(..).enumerate() {
        if removed_paths
            .iter()
            .any(|removed| track.path == *removed || track.path.starts_with(removed))
        {
            continue;
        }
        remap[old_index] = Some(tracks.len());
        tracks.push(track);
    }
    for album in &mut library.albums {
        album.tracks = album
            .tracks
            .iter()
            .filter_map(|&old_index| remap.get(old_index).copied().flatten())
            .collect();
    }
    library.track_album = Arc::new(
        library
            .track_album
            .iter()
            .enumerate()
            .filter_map(|(old_index, &album_index)| remap[old_index].map(|_| album_index))
            .collect(),
    );
    library.tracks = Arc::new(tracks);
    remap
}

/// Replace tracks whose tags changed and move them to the albums their new tags name.
///
/// Track indexes stay the same, so queues and playback keep pointing at the same files. An album
/// left empty is reused when its tracks all move to an album that doesn't exist yet.
///
/// # Arguments
/// * `library` - Library to update.
/// * `retagged` - Track indexes and the tracks read back from their files.
pub fn retag_library_tracks(library: &mut Library, retagged: Vec<(usize, Track)>) {
    let tracks = Arc::make_mut(&mut library.tracks);
    let track_album = Arc::make_mut(&mut library.track_album);
    let mut indices = Vec::with_capacity(retagged.len());
    for (index, track) in retagged {
        let (Some(slot), Some(album)) = (tracks.get_mut(index), track_album.get(index)) else {
            continue;
        };
        *slot = track;
        if let Some(album) = album.and_then(|album| library.albums.get_mut(album.get())) {
            album.tracks.retain(|&track| track != index);
        }
        indices.push(index);
    }
    let mut album_indices: HashMap<_, _> = library
        .albums
        .iter()
        .enumerate()
        .filter(|(_, album)| !album.tracks.is_empty())
        .map(|(index, album)| {
            (
                album_key(&album.title, &album.artist, album.lossless),
                index,
            )
        })
        .collect();
    let mut updated_albums = HashSet::new();
    for index in indices {
        let track = &tracks[index];
        let previous = track_album[index].map(AlbumIndex::get);
        if track.album.trim().is_empty() {
            track_album[index] = None;
            continue;
        }
        let key = album_key(&track.album, &track.album_artist, track.is_lossless());
        let album_index = *album_indices.entry(key).or_insert_with(|| {
            let art = previous.and_then(|album| library.albums[album].art.clone());
            let album = Album::new(track, Vec::new(), art);
            match previous.filter(|&album| library.albums[album].tracks.is_empty()) {
                Some(reused) => {
                    library.albums[reused] = album;
                    reused
                }
                None => {
                    library.albums.push(album);
                    library.albums.len() - 1
                }
            }
        });
        library.albums[album_index].tracks.push(index);
        track_album[index] = Some(AlbumIndex::new(album_index));
        updated_albums.insert(album_index);
    }
    for album_index in updated_albums {
        library.albums[album_index]
            .tracks
            .sort_by(|&left, &right| tracks[left].playback_order(&tracks[right]));
    }
}
