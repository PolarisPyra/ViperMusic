//! Persistent metadata cache for local audio tracks.
use crate::{features::library::Track, platform::persistence::database};
use rusqlite::{params, types::Type, Row};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const TRACK_METADATA_VERSION: u8 = 1;

#[derive(Default)]
/// Persistent metadata cache for local audio tracks.
pub struct TrackMetadataCache {
    entries: BTreeMap<PathBuf, CachedTrack>,
}

#[derive(Clone, Deserialize, Serialize)]
struct CachedTrack {
    #[serde(default)]
    metadata_version: u8,
    size: u64,
    modified_secs: u64,
    modified_nanos: u32,
    track: Track,
}

impl TrackMetadataCache {
    /// Load cached track metadata from SQLite or the legacy cache.
    ///
    /// # Returns
    /// A cache that can serve metadata for unchanged files. Unreadable cache data is ignored.
    pub fn load() -> Self {
        let mut cache = Self {
            entries: load_database_entries(),
        };
        if cache.entries.is_empty() {
            if let Some((path, entries)) = load_legacy_cache() {
                cache.entries = entries;
                if cache.persist() {
                    let _ = fs::remove_file(path);
                }
            }
        }
        cache
    }

    /// Get cached metadata when the file size and modification time still match.
    ///
    /// # Arguments
    /// * `path` - Track path.
    /// * `metadata` - Current filesystem metadata.
    ///
    /// # Returns
    /// A cloned track with its current path, or `None` when the entry is stale or absent.
    pub fn get(&self, path: &Path, metadata: &fs::Metadata) -> Option<Track> {
        let entry = self.entries.get(path)?;
        let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
        (entry.metadata_version == TRACK_METADATA_VERSION
            && entry.size == metadata.len()
            && entry.modified_secs == modified.as_secs()
            && entry.modified_nanos == modified.subsec_nanos())
        .then(|| {
            let mut track = entry.track.clone();
            track.path = path.to_owned();
            track
        })
    }

    /// Add or replace a cached track using its current filesystem signature.
    ///
    /// # Arguments
    /// * `path` - Track path.
    /// * `metadata` - Filesystem metadata used to validate future cache hits.
    /// * `track` - Track data to cache.
    pub fn insert(&mut self, path: PathBuf, metadata: &fs::Metadata, mut track: Track) {
        let Ok(modified) = metadata.modified().and_then(|time| {
            time.duration_since(UNIX_EPOCH)
                .map_err(std::io::Error::other)
        }) else {
            return;
        };
        track.path = PathBuf::new();
        self.entries.insert(
            path,
            CachedTrack {
                metadata_version: TRACK_METADATA_VERSION,
                size: metadata.len(),
                modified_secs: modified.as_secs(),
                modified_nanos: modified.subsec_nanos(),
                track,
            },
        );
    }

    /// Persist the cache to SQLite.
    pub fn save(self) {
        self.persist();
    }

    fn persist(&self) -> bool {
        let Ok(mut connection) = database::open() else {
            return false;
        };
        let Ok(transaction) = connection.transaction() else {
            return false;
        };
        if transaction
            .execute("DELETE FROM track_metadata", [])
            .is_err()
        {
            return false;
        }
        let Ok(mut insert) = transaction.prepare(
            "INSERT INTO track_metadata
             (path, metadata_version, file_size, modified_secs, modified_nanos, track)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        ) else {
            return false;
        };
        let mut payload = Vec::new();
        for (path, entry) in &self.entries {
            payload.clear();
            if serde_json::to_writer(&mut payload, &entry.track).is_err()
                || insert
                    .execute(params![
                        path.to_string_lossy(),
                        entry.metadata_version,
                        i64::try_from(entry.size).unwrap_or(i64::MAX),
                        i64::try_from(entry.modified_secs).unwrap_or(i64::MAX),
                        i64::from(entry.modified_nanos),
                        payload,
                    ])
                    .is_err()
            {
                return false;
            }
        }
        drop(insert);
        transaction.commit().is_ok()
    }
}

fn load_database_entries() -> BTreeMap<PathBuf, CachedTrack> {
    let Ok(connection) = database::open() else {
        return BTreeMap::new();
    };
    let Ok(mut statement) = connection.prepare(
        "SELECT path, metadata_version, file_size, modified_secs, modified_nanos, track
         FROM track_metadata",
    ) else {
        return BTreeMap::new();
    };
    let Ok(rows) = statement.query_map([], |row| {
        let payload: Vec<u8> = row.get(5)?;
        let track = serde_json::from_slice(&payload).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(5, Type::Blob, Box::new(error))
        })?;
        Ok((
            PathBuf::from(row.get::<_, String>(0)?),
            CachedTrack {
                metadata_version: row.get(1)?,
                size: read_unsigned(row, 2)?,
                modified_secs: read_unsigned(row, 3)?,
                modified_nanos: read_unsigned(row, 4)?,
                track,
            },
        ))
    }) else {
        return BTreeMap::new();
    };
    rows.filter_map(Result::ok).collect()
}

fn load_legacy_cache() -> Option<(PathBuf, BTreeMap<PathBuf, CachedTrack>)> {
    let database_path = crate::platform::persistence::settings::Settings::file_path().ok()?;
    let path = database_path.with_file_name("cached").join("tracks.json");
    let bytes = fs::read(&path).ok()?;
    let entries: Vec<CachedTrack> = serde_json::from_slice(&bytes).ok()?;
    let entries = entries
        .into_iter()
        .map(|mut entry| {
            let path = std::mem::take(&mut entry.track.path);
            (path, entry)
        })
        .collect();
    Some((path, entries))
}

/// Read a non-negative SQLite integer column into a narrower unsigned type.
fn read_unsigned<T: TryFrom<i64, Error = std::num::TryFromIntError>>(
    row: &Row<'_>,
    column: usize,
) -> rusqlite::Result<T> {
    T::try_from(row.get::<_, i64>(column)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, Box::new(error))
    })
}
