//! Writing edited tags into audio files.
use lofty::{
    config::WriteOptions,
    file::TaggedFileExt,
    tag::{ItemKey, Tag, TagExt},
};
use std::path::Path;

/// One tag to change: its new text, or empty to remove it.
pub(super) type TagChange = (ItemKey, String);

/// Write tag changes into a file's main tag, keeping every tag that didn't change.
///
/// A file without its format's main tag gets one, seeded from whichever tag it has.
///
/// # Arguments
/// * `path` - Local audio file.
/// * `changes` - Tags to set, or remove when their text is empty.
///
/// # Errors
/// Returns why the file could not be read or written.
pub(super) fn write(path: &Path, changes: &[TagChange]) -> Result<(), String> {
    let mut file = lofty::read_from_path(path).map_err(|error| error.to_string())?;
    let tag_type = file.primary_tag_type();
    if file.tag(tag_type).is_none() {
        let mut tag = file
            .first_tag()
            .cloned()
            .unwrap_or_else(|| Tag::new(tag_type));
        tag.re_map(tag_type);
        file.insert_tag(tag);
    }
    let tag = file
        .tag_mut(tag_type)
        .ok_or_else(|| "This file's format can't hold tags".to_owned())?;
    for &(key, ref value) in changes {
        if key == ItemKey::RecordingDate {
            // Some formats keep the year apart from the date; the new year replaces both.
            tag.remove_key(ItemKey::Year);
        }
        if value.is_empty() {
            tag.remove_key(key);
        } else {
            tag.insert_text(key, value.clone());
        }
    }
    tag.save_to_path(path, WriteOptions::default())
        .map_err(|error| error.to_string())
}
