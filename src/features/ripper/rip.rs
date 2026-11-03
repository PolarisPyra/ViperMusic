//! Ripping: each chosen track is read from the disc and encoded to FLAC while it is read, then
//! tagged and moved into place. A track is written under a hidden `.part` name until it is
//! complete, so the library never picks up half a file.
use super::drive::{AudioTrack, Drive, TrackReader};
use flacenc::{component::BitRepr, error::Verify, source::Fill};
use lofty::{
    config::WriteOptions,
    picture::{MimeType, Picture, PictureType},
    tag::{ItemKey, Tag, TagExt, TagType},
};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

/// What a rip writes: where, the album's tags, and the tracks chosen.
pub(crate) struct RipPlan {
    pub(crate) drive: Drive,
    pub(crate) folder: PathBuf,
    pub(crate) album: AlbumTags,
    pub(crate) tracks: Vec<PlannedTrack>,
    pub(crate) cover: Option<Vec<u8>>,
    pub(crate) eject_when_done: bool,
}

pub(crate) struct AlbumTags {
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) year: Option<u32>,
    pub(crate) disc_number: u32,
    pub(crate) disc_count: u32,
    pub(crate) track_count: usize,
    pub(crate) release_id: Option<String>,
}

pub(crate) struct PlannedTrack {
    /// The track's row in the ripper, which progress events name.
    pub(crate) row: usize,
    pub(crate) track: AudioTrack,
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) recording_id: Option<String>,
}

pub(crate) enum RipEvent {
    /// Bytes of the row's audio read so far.
    Progress {
        row: usize,
        read: u64,
    },
    TrackDone {
        row: usize,
    },
    TrackFailed {
        row: usize,
        error: String,
    },
    /// The worker has stopped: every track was tried, or the rip was cancelled.
    Finished {
        ejected: bool,
    },
}

/// How often progress is reported while a track is read.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Start ripping on a background thread.
///
/// # Returns
/// The worker's events; the last one is always [`RipEvent::Finished`].
pub(crate) fn spawn(
    plan: RipPlan,
    cancel: Arc<AtomicBool>,
    repaint: impl Fn() + Send + 'static,
) -> io::Result<Receiver<RipEvent>> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("viper-cd-rip".to_owned())
        .spawn(move || {
            let send = |event| {
                let _ = sender.send(event);
                repaint();
            };
            let all_ripped = run(&plan, &cancel, &send);
            // Keep a disc with tracks that failed in the drive, to try them again.
            let ejected = all_ripped && plan.eject_when_done && plan.drive.eject().is_ok();
            send(RipEvent::Finished { ejected });
        })?;
    Ok(receiver)
}

/// Rip each track in turn.
///
/// # Returns
/// Whether every track was ripped.
fn run(plan: &RipPlan, cancel: &AtomicBool, send: &dyn Fn(RipEvent)) -> bool {
    if let Err(error) = fs::create_dir_all(&plan.folder) {
        for track in &plan.tracks {
            send(RipEvent::TrackFailed {
                row: track.row,
                error: format!("Could not create {}: {error}", plan.folder.display()),
            });
        }
        return false;
    }
    let mut all_ripped = true;
    for track in &plan.tracks {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        let mut last_report = Instant::now();
        let mut report = |read: u64| {
            if last_report.elapsed() >= PROGRESS_INTERVAL {
                last_report = Instant::now();
                send(RipEvent::Progress {
                    row: track.row,
                    read,
                });
            }
        };
        match rip_track(plan, track, cancel, &mut report) {
            Ok(()) => send(RipEvent::TrackDone { row: track.row }),
            Err(_) if cancel.load(Ordering::Relaxed) => return false,
            Err(error) => {
                all_ripped = false;
                send(RipEvent::TrackFailed {
                    row: track.row,
                    error: error.to_string(),
                });
            }
        }
    }
    all_ripped
}

/// The file name of a track: `07 - Title.flac`, or `2-07 - Title.flac` on a multi-disc release.
pub(crate) fn file_name(album: &AlbumTags, track: u8, title: &str) -> String {
    let disc = if album.disc_count > 1 {
        format!("{}-", album.disc_number)
    } else {
        String::new()
    };
    format!("{disc}{track:02} - {}.flac", path_part(title))
}

/// The album's folder under `root`: `Artist/Album`.
pub(crate) fn album_folder(root: &Path, artist: &str, album: &str) -> PathBuf {
    root.join(path_part(artist)).join(path_part(album))
}

/// Make a tag safe as one path component on Linux and on the Windows and SMB shares music often
/// lives on.
fn path_part(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect();
    let cleaned = cleaned
        .trim()
        .trim_start_matches('.')
        .trim_end_matches(['.', ' ']);
    if cleaned.is_empty() {
        "Unknown".to_owned()
    } else {
        cleaned.to_owned()
    }
}

fn rip_track(
    plan: &RipPlan,
    planned: &PlannedTrack,
    cancel: &AtomicBool,
    report: &mut dyn FnMut(u64),
) -> io::Result<()> {
    let name = file_name(&plan.album, planned.track.number, &planned.title);
    let path = plan.folder.join(&name);
    let part = plan.folder.join(format!(".{name}.part"));
    let result = (|| {
        let mut reader = plan.drive.open_track(planned.track)?;
        let config = flacenc::config::Encoder::default()
            .into_verified()
            .map_err(|(_, error)| io::Error::other(format!("{error:?}")))?;
        let source = PcmSource {
            reader: &mut reader,
            remaining: planned.track.bytes(),
            read: 0,
            buffer: Vec::new(),
            cancel,
            report,
        };
        let mut stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
            .map_err(|error| io::Error::other(format!("Could not encode the track: {error}")))?;
        // flacenc counts the short last block in the minimum block size, which FLAC leaves out.
        // With the minimum below the maximum, decoders can't place frames, so seeking fails.
        stream
            .stream_info_mut()
            .set_block_sizes(config.block_size, config.block_size)
            .map_err(|error| io::Error::other(format!("Could not encode the track: {error:?}")))?;
        reader.finish()?;
        let mut sink = flacenc::bitsink::ByteSink::new();
        stream
            .write(&mut sink)
            .map_err(|error| io::Error::other(format!("Could not encode the track: {error:?}")))?;
        fs::write(&part, sink.as_slice())?;
        tag(&part, plan, planned)?;
        fs::rename(&part, &path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

fn tag(path: &Path, plan: &RipPlan, planned: &PlannedTrack) -> io::Result<()> {
    let album = &plan.album;
    let mut tag = Tag::new(TagType::VorbisComments);
    let mut text = |key, value: String| {
        if !value.trim().is_empty() {
            tag.insert_text(key, value);
        }
    };
    text(ItemKey::TrackTitle, planned.title.clone());
    text(ItemKey::TrackArtist, planned.artist.clone());
    text(ItemKey::AlbumTitle, album.title.clone());
    text(ItemKey::AlbumArtist, album.artist.clone());
    text(ItemKey::TrackNumber, planned.track.number.to_string());
    text(ItemKey::TrackTotal, album.track_count.to_string());
    text(ItemKey::DiscNumber, album.disc_number.to_string());
    text(ItemKey::DiscTotal, album.disc_count.to_string());
    if let Some(year) = album.year {
        text(ItemKey::RecordingDate, year.to_string());
    }
    if let Some(id) = &album.release_id {
        text(ItemKey::MusicBrainzReleaseId, id.clone());
    }
    if let Some(id) = &planned.recording_id {
        text(ItemKey::MusicBrainzRecordingId, id.clone());
    }
    if let Some(cover) = &plan.cover {
        let mime = if cover.starts_with(&[0x89, b'P', b'N', b'G']) {
            MimeType::Png
        } else {
            MimeType::Jpeg
        };
        tag.push_picture(
            Picture::unchecked(cover.clone())
                .pic_type(PictureType::CoverFront)
                .mime_type(mime)
                .build(),
        );
    }
    tag.save_to_path(path, WriteOptions::default())
        .map_err(|error| io::Error::other(format!("Could not tag the track: {error}")))
}

/// Feeds the FLAC encoder from a track as it is read: 16-bit little-endian stereo at 44.1 kHz.
struct PcmSource<'a> {
    reader: &'a mut TrackReader,
    remaining: u64,
    read: u64,
    buffer: Vec<u8>,
    cancel: &'a AtomicBool,
    report: &'a mut dyn FnMut(u64),
}

impl flacenc::source::Source for PcmSource<'_> {
    fn channels(&self) -> usize {
        2
    }

    fn bits_per_sample(&self) -> usize {
        16
    }

    fn sample_rate(&self) -> usize {
        44_100
    }

    fn read_samples<F: Fill>(
        &mut self,
        block_size: usize,
        dest: &mut F,
    ) -> Result<usize, flacenc::error::SourceError> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(flacenc::error::SourceError::from_io_error(io::Error::new(
                io::ErrorKind::Interrupted,
                "The rip was cancelled",
            )));
        }
        let wanted = (block_size as u64 * 4).min(self.remaining) as usize;
        self.buffer.resize(wanted, 0);
        let mut filled = 0;
        while filled < wanted {
            match self.reader.read(&mut self.buffer[filled..]) {
                Ok(0) => break,
                Ok(count) => filled += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(flacenc::error::SourceError::from_io_error(error)),
            }
        }
        // A short read ends the track; a partial stereo frame at the very end is dropped.
        let filled = filled - filled % 4;
        self.remaining -= filled as u64;
        self.read += filled as u64;
        (self.report)(self.read);
        dest.fill_le_bytes(&self.buffer[..filled], 2)?;
        Ok(filled / 4)
    }

    fn len_hint(&self) -> Option<usize> {
        Some(((self.remaining + self.read) / 4) as usize)
    }
}
