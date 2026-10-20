use std::path::Path;

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "opus", "wav", "m4a", "aac", "aiff", "wma", "ape", "wv", "dsf",
    "dff", "webm",
];
const IGNORED_FILE_NAMES: &[&str] = &[".DS_Store", "Thumbs.db"];
const IGNORED_FILE_PREFIXES: &[&str] = &["._"];

pub(crate) fn is_audio_path(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if IGNORED_FILE_NAMES
        .iter()
        .any(|ignored| file_name.eq_ignore_ascii_case(ignored))
        || IGNORED_FILE_PREFIXES
            .iter()
            .any(|prefix| file_name.starts_with(prefix))
    {
        return false;
    }

    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            AUDIO_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

/// Return whether a library path is an `smb://` share URL rather than a local path.
pub(crate) fn is_smb_path(path: &Path) -> bool {
    path.to_str().is_some_and(|path| path.starts_with("smb://"))
}

/// Build the grouping key for an album, keeping lossless and lossy copies of the same album apart.
///
/// # Arguments
///
/// * `title` - Album title tag.
/// * `artist` - Album artist tag.
/// * `lossless` - Whether the album's tracks are lossless.
///
/// # Returns
///
/// The normalized title, normalized artist, and lossless flag as a map key.
pub(crate) fn album_key(title: &str, artist: &str, lossless: bool) -> (String, String, bool) {
    (
        normalize_album_key(title),
        normalize_album_key(artist),
        lossless,
    )
}

fn normalize_album_key(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}
