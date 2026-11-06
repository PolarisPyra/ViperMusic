use crate::{platform::persistence::settings::Settings, shared::artwork::prepare_cover};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::Hasher,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::UNIX_EPOCH,
};

static ARTWORK_CACHE_TEMP_ID: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn load_artwork(
    path: &Path,
    embedded_art: impl FnOnce(&Path) -> Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    let folder = path.parent()?;
    find_cover_in(folder)
        .or_else(|| {
            if is_disc_directory(folder) {
                folder.parent().and_then(find_cover_in)
            } else {
                None
            }
        })
        .or_else(|| prepared_cover_for_track(path, || embedded_art(path)))
}

fn is_disc_directory(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let normalized: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect();
    ["disc", "disk", "cd"].iter().any(|prefix| {
        normalized
            .strip_prefix(prefix)
            .is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
    })
}

fn find_cover_in(folder: &Path) -> Option<Vec<u8>> {
    [
        "cover.jpg",
        "cover.jpeg",
        "cover.png",
        "Cover.jpg",
        "front.jpg",
        "front.png",
        "folder.jpg",
        "folder.jpeg",
        "folder.png",
        "album.jpg",
        "album.png",
        "artwork.jpg",
    ]
    .iter()
    .find_map(|name| prepared_cover_for_source(&folder.join(name)))
}

fn prepared_cover_for_track(
    path: &Path,
    embedded_art: impl FnOnce() -> Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!(
                "Could not inspect audio file for artwork cache {}: {error}",
                path.display()
            );
            return embedded_art().and_then(|bytes| prepare_cover(&bytes));
        }
    };
    let cache_path = match artwork_cache_path(path, &metadata) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Could not locate album artwork cache: {error}");
            return embedded_art().and_then(|bytes| prepare_cover(&bytes));
        }
    };
    read_or_create_prepared_cover(&cache_path, || {
        embedded_art().and_then(|bytes| prepare_cover(&bytes))
    })
}

fn prepared_cover_for_source(path: &Path) -> Option<Vec<u8>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return None,
        Err(error) if error.kind() == ErrorKind::NotFound => return None,
        Err(error) => {
            eprintln!(
                "Could not inspect album artwork source {}: {error}",
                path.display()
            );
            return None;
        }
    };
    let cache_path = match artwork_cache_path(path, &metadata) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Could not locate album artwork cache: {error}");
            return fs::read(path).ok().and_then(|bytes| prepare_cover(&bytes));
        }
    };
    read_or_create_prepared_cover(&cache_path, || match fs::read(path) {
        Ok(bytes) => prepare_cover(&bytes),
        Err(error) => {
            eprintln!(
                "Could not read album artwork source {}: {error}",
                path.display()
            );
            None
        }
    })
}

fn read_or_create_prepared_cover(
    cache_path: &Path,
    create: impl FnOnce() -> Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    match fs::read(cache_path) {
        Ok(bytes) => {
            // Reading the header is enough to catch a damaged file; covers are decoded when drawn.
            if image::ImageReader::new(io::Cursor::new(&bytes))
                .with_guessed_format()
                .is_ok_and(|reader| reader.into_dimensions().is_ok())
            {
                return Some(bytes);
            }
            eprintln!(
                "Invalid prepared artwork cache {}, rebuilding it",
                cache_path.display()
            );
            if let Err(error) = fs::remove_file(cache_path) {
                if error.kind() != ErrorKind::NotFound {
                    eprintln!(
                        "Could not remove invalid album artwork cache {}: {error}",
                        cache_path.display()
                    );
                }
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => eprintln!(
            "Could not read album artwork cache {}: {error}",
            cache_path.display()
        ),
    }

    let bytes = create()?;
    let Some(directory) = cache_path.parent() else {
        eprintln!("Album artwork cache path has no parent directory");
        return Some(bytes);
    };
    if let Err(error) = fs::create_dir_all(directory) {
        eprintln!(
            "Could not create album artwork cache {}: {error}",
            directory.display()
        );
        return Some(bytes);
    }
    let temporary_id = ARTWORK_CACHE_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let temporary_path = cache_path.with_extension(format!("{temporary_id}.tmp"));
    if let Err(error) = fs::write(&temporary_path, &bytes) {
        eprintln!(
            "Could not write album artwork cache {}: {error}",
            temporary_path.display()
        );
        return Some(bytes);
    }
    if let Err(error) = fs::rename(&temporary_path, cache_path) {
        eprintln!(
            "Could not save album artwork cache {}: {error}",
            cache_path.display()
        );
        if let Err(remove_error) = fs::remove_file(&temporary_path) {
            if remove_error.kind() != ErrorKind::NotFound {
                eprintln!(
                    "Could not remove temporary album artwork cache {}: {remove_error}",
                    temporary_path.display()
                );
            }
        }
    }
    Some(bytes)
}

fn artwork_cache_path(source_path: &Path, metadata: &fs::Metadata) -> io::Result<PathBuf> {
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    let settings_path = Settings::file_path()?;
    let mut hasher = DefaultHasher::new();
    hasher.write(source_path.as_os_str().to_string_lossy().as_bytes());
    hasher.write_u64(metadata.len());
    hasher.write_u64(modified.as_secs());
    hasher.write_u32(modified.subsec_nanos());
    Ok(settings_path
        .with_file_name("cached")
        .join("jackets")
        .join(format!("{:016x}.img", hasher.finish())))
}
