use super::model::AudioProperties;
use lofty::{
    file::{AudioFile, FileType, TaggedFile, TaggedFileExt},
    probe::Probe,
    tag::ItemKey,
};
use std::{
    io::{BufRead, BufReader, Read, Seek},
    path::Path,
};
use symphonia::core::meta::StandardTagKey;

#[derive(Default)]
pub(super) struct TrackMetadata {
    pub(super) title: Option<String>,
    pub(super) artist: Option<String>,
    pub(super) album: Option<String>,
    pub(super) album_artist: Option<String>,
    pub(super) disc_number: Option<u32>,
    pub(super) track_number: Option<u32>,
    pub(super) duration_ms: Option<u64>,
    pub(super) release_year: Option<u32>,
    pub(super) audio: AudioProperties,
    pub(super) artwork: Option<Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum ReaderMetadataError {
    /// The media file is empty and contains no readable audio stream.
    #[error("media file is empty")]
    Empty,
    /// The metadata reader failed to parse the media file.
    #[error("metadata read failed: {0}")]
    Failed(String),
}

impl TrackMetadata {
    pub(super) fn has_tags(&self) -> bool {
        self.title.is_some()
            || self.artist.is_some()
            || self.album.is_some()
            || self.album_artist.is_some()
            || self.disc_number.is_some()
            || self.track_number.is_some()
            || self.release_year.is_some()
    }
}

pub(super) fn read_metadata(path: &Path) -> TrackMetadata {
    if is_asf_path(path) {
        if let Ok(file) = std::fs::File::open(path) {
            if let Ok(metadata) = read_asf_metadata(&mut std::io::BufReader::new(file)) {
                return metadata;
            }
        }
    }
    if let Ok(file) = lofty::read_from_path(path) {
        return metadata_from_tagged_file(file, false);
    }

    let Some(media) = super::symphonia::probe(path) else {
        return TrackMetadata::default();
    };
    let lookup = |standard_key: StandardTagKey, aliases: &[&str]| {
        media
            .tags
            .iter()
            .find(|tag| {
                tag.standard_key == Some(standard_key)
                    || aliases
                        .iter()
                        .any(|alias| tag.key.eq_ignore_ascii_case(alias))
            })
            .map(|tag| tag.value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let number = |standard_key, aliases| {
        lookup(standard_key, aliases).and_then(|value| value.split('/').next()?.trim().parse().ok())
    };

    TrackMetadata {
        title: lookup(StandardTagKey::TrackTitle, &["title", "tracktitle"]),
        artist: lookup(StandardTagKey::Artist, &["artist"]),
        album: lookup(StandardTagKey::Album, &["album"]),
        album_artist: lookup(
            StandardTagKey::AlbumArtist,
            &["album_artist", "albumartist"],
        ),
        disc_number: number(StandardTagKey::DiscNumber, &["disc", "discnumber"]),
        track_number: number(StandardTagKey::TrackNumber, &["track", "tracknumber"]),
        duration_ms: media.duration_ms,
        release_year: lookup(
            StandardTagKey::Date,
            &[
                "date",
                "year",
                "recordingdate",
                "originaldate",
                "releasedate",
            ],
        )
        .and_then(|date| date.get(..4)?.parse().ok()),
        audio: media.audio,
        artwork: None,
    }
}

pub(super) fn read_metadata_from_reader<R: Read + Seek>(
    reader: R,
    path: &Path,
) -> Result<TrackMetadata, ReaderMetadataError> {
    let mut reader = BufReader::with_capacity(128 * 1024, reader);
    if reader
        .fill_buf()
        .map_err(|error| ReaderMetadataError::Failed(error.to_string()))?
        .is_empty()
    {
        return Err(ReaderMetadataError::Empty);
    }
    if is_asf_path(path) {
        return read_asf_metadata(&mut reader).map_err(ReaderMetadataError::Failed);
    }
    reader
        .seek(std::io::SeekFrom::Start(0))
        .map_err(|error| ReaderMetadataError::Failed(error.to_string()))?;
    let probe = match FileType::from_path(path) {
        Some(file_type) => Probe::with_file_type(reader, file_type),
        None => Probe::new(reader)
            .guess_file_type()
            .map_err(|error| ReaderMetadataError::Failed(error.to_string()))?,
    };
    probe
        .read()
        .map(|file| metadata_from_tagged_file(file, true))
        .map_err(|error| ReaderMetadataError::Failed(error.to_string()))
}

fn is_asf_path(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("wma") || extension.eq_ignore_ascii_case("asf")
    })
}

fn read_asf_metadata<R: Read + Seek>(reader: &mut R) -> Result<TrackMetadata, String> {
    let file = <audex::asf::ASF as audex::FileType>::load_from_reader(reader)
        .map_err(|error| error.to_string())?;
    let text = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| file.tags.get(key).first().map(|value| value.to_string()))
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let number =
        |keys: &[&str]| text(keys).and_then(|value| value.split('/').next()?.trim().parse().ok());
    let millis = (file.info.length * 1000.0).max(0.0) as u64;
    Ok(TrackMetadata {
        title: text(&["Title"]),
        artist: text(&["Author"]),
        album: text(&["WM/AlbumTitle", "Album"]),
        album_artist: text(&["WM/AlbumArtist"]),
        disc_number: number(&["WM/PartOfSet"]),
        track_number: number(&["WM/TrackNumber"]),
        duration_ms: (millis > 0).then_some(millis),
        release_year: text(&["WM/Year"]).and_then(|date| date.get(..4)?.parse().ok()),
        audio: AudioProperties {
            bitrate_kbps: (file.info.bitrate > 0).then_some(file.info.bitrate / 1000),
            sample_rate_hz: (file.info.sample_rate > 0).then_some(file.info.sample_rate),
            bit_depth: None,
            channels: (file.info.channels > 0).then_some(file.info.channels as u8),
        },
        artwork: None,
    })
}

fn metadata_from_tagged_file(file: TaggedFile, include_artwork: bool) -> TrackMetadata {
    let properties = file.properties();
    let millis = properties.duration().as_millis() as u64;
    let duration_ms = (millis > 0).then_some(millis);
    let audio = AudioProperties {
        bitrate_kbps: properties.audio_bitrate(),
        sample_rate_hz: properties.sample_rate(),
        bit_depth: properties.bit_depth(),
        channels: properties.channels(),
    };
    let tag = file.primary_tag().or_else(|| file.first_tag());
    let artwork = include_artwork
        .then(|| {
            tag.and_then(|tag| tag.pictures().first())
                .map(|picture| picture.data().to_vec())
        })
        .flatten();
    let Some(tag) = tag else {
        return TrackMetadata {
            duration_ms,
            audio,
            artwork,
            ..TrackMetadata::default()
        };
    };
    let text = |key| {
        tag.get_string(key)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let number = |key| text(key).and_then(|value| value.split('/').next()?.parse().ok());
    TrackMetadata {
        title: text(ItemKey::TrackTitle),
        artist: text(ItemKey::TrackArtist),
        album: text(ItemKey::AlbumTitle),
        album_artist: text(ItemKey::AlbumArtist),
        disc_number: number(ItemKey::DiscNumber),
        track_number: number(ItemKey::TrackNumber),
        duration_ms,
        release_year: text(ItemKey::RecordingDate)
            .and_then(|date| date.get(..4)?.parse::<u32>().ok()),
        audio,
        artwork,
    }
}
