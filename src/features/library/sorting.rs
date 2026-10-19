use crate::{
    features::library::{Album, Track},
    platform::persistence::settings::{AlbumSort, Settings},
};
use std::{cmp::Reverse, collections::BTreeMap};

/// Sort album indexes by the configured album sort, computing each sort key once.
///
/// Ties are broken by case-insensitive title and then by index, except for random order.
///
/// # Arguments
/// * `indices` - Album indexes to reorder in place.
/// * `albums` - Albums addressed by `indices`.
/// * `tracks` - Library tracks addressed by each album.
/// * `settings` - Sort order, direction, and per-album state.
/// * `seed` - Seed for random ordering.
pub(crate) fn sort_indices(
    indices: &mut [usize],
    albums: &[Album],
    tracks: &[Track],
    settings: &Settings,
    seed: u64,
) {
    let ascending = settings.sort_ascending;
    let title = |index: usize| albums[index].title.to_lowercase();
    let stored = |values: &BTreeMap<String, u64>, index: usize| {
        values.get(&albums[index].key).copied().unwrap_or_default()
    };
    let album_tracks = |index: usize| {
        albums[index]
            .tracks
            .iter()
            .filter_map(|&track| tracks.get(track))
    };
    match settings.album_sort {
        AlbumSort::AlbumArtist => sort_by(indices, ascending, title, |index| {
            albums[index].artist.to_lowercase()
        }),
        AlbumSort::Id => sort_by(indices, ascending, |_| (), |index| index),
        AlbumSort::Artist => sort_by(indices, ascending, title, |index| {
            album_tracks(index)
                .next()
                .map_or_else(String::new, |track| track.artist.to_lowercase())
        }),
        AlbumSort::Duration => sort_by(indices, ascending, title, |index| {
            album_tracks(index)
                .filter_map(|track| track.duration_ms)
                .fold(0, u64::saturating_add)
        }),
        AlbumSort::MostPlayed => sort_by(indices, ascending, title, |index| {
            stored(&settings.album_play_counts, index)
        }),
        AlbumSort::Name => sort_by(indices, ascending, |index| index, title),
        AlbumSort::Random => sort_by(
            indices,
            ascending,
            |_| (),
            |index| random_sort_hash(&albums[index].key, seed),
        ),
        AlbumSort::Rating => sort_by(indices, ascending, title, |index| {
            settings
                .album_ratings
                .get(&albums[index].key)
                .copied()
                .unwrap_or_default()
        }),
        AlbumSort::RecentlyAdded => sort_by(indices, ascending, title, |index| {
            stored(&settings.album_added, index)
        }),
        AlbumSort::RecentlyPlayed => sort_by(indices, ascending, title, |index| {
            stored(&settings.album_last_played, index)
        }),
        AlbumSort::SongCount => sort_by(indices, ascending, title, |index| {
            albums[index].tracks.len()
        }),
        AlbumSort::Favorited => sort_by(indices, ascending, title, |index| {
            settings.favorite_albums.contains(&albums[index].key)
        }),
        AlbumSort::ReleaseYear => sort_by(indices, ascending, title, |index| {
            album_tracks(index)
                .filter_map(|track| track.release_year)
                .min()
                .unwrap_or_default()
        }),
    }
}

/// Sort by `key` in the requested direction, then ascending by `tie` and index.
fn sort_by<K: Ord, T: Ord>(
    indices: &mut [usize],
    ascending: bool,
    tie: impl Fn(usize) -> T,
    key: impl Fn(usize) -> K,
) {
    if ascending {
        indices.sort_by_cached_key(|&index| (key(index), tie(index), index));
    } else {
        indices.sort_by_cached_key(|&index| (Reverse(key(index)), tie(index), index));
    }
}

fn random_sort_hash(key: &str, seed: u64) -> u64 {
    key.bytes().fold(seed ^ 0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
