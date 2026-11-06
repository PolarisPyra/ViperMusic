//! The album being edited: its tags as they were and as the user has changed them, and the
//! MusicBrainz search or save running in the background.
use super::writer::{self, TagChange};
use crate::shared::repaint::Repaint;
use crate::{
    features::{
        library::{read_track, state::LibraryFeature, Track},
        ripper::musicbrainz::{self, Release, ReleaseMatch},
    },
    platform::persistence::settings::Settings,
    shared::ui::toasts::Toasts,
};
use lofty::tag::ItemKey;
use rayon::prelude::*;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
};

/// Each written track read back from its file, or why it could not be written.
type SaveResult = Vec<Result<(usize, Track), String>>;

/// The tag editor dialog's state while it is open.
pub(crate) struct TagEditor {
    /// The album the dialog was opened for.
    album_index: usize,
    /// The album's tags when the dialog opened; only fields that differ from these are written.
    original: AlbumFields,
    pub(crate) album: AlbumFields,
    pub(crate) tracks: Vec<TrackDraft>,
    /// The MusicBrainz release the tags came from, written with them.
    release_id: Option<String>,
    /// Releases the last MusicBrainz search found.
    pub(crate) releases: Vec<ReleaseMatch>,
    /// Which of [`Self::releases`] filled in the tags.
    pub(crate) chosen: Option<usize>,
    pub(crate) job: Job,
    /// The last search's or save's problem, shown above the tags.
    pub(crate) message: Option<String>,
}

/// Album-wide tags, as text so fields can be cleared while typing.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AlbumFields {
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) year: String,
}

/// One track's own tags, as text so fields can be cleared while typing.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct TrackFields {
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) disc: String,
    pub(crate) number: String,
}

/// One track of the album: its tags when the dialog opened and as the user has changed them.
pub(crate) struct TrackDraft {
    /// Index into the library's tracks.
    index: usize,
    path: PathBuf,
    original: TrackFields,
    pub(crate) fields: TrackFields,
    recording_id: Option<String>,
}

/// What the dialog is waiting for.
pub(crate) enum Job {
    Idle,
    Searching(Receiver<Result<Vec<ReleaseMatch>, String>>),
    Fetching(Receiver<Result<Vec<Release>, String>>),
    Saving(Receiver<SaveResult>),
}

impl TagEditor {
    /// Open the editor on an album's current tags.
    ///
    /// # Arguments
    /// * `library` - Library holding the album.
    /// * `album_index` - Index into the library's albums.
    ///
    /// # Returns
    /// The editor, or `None` when there is no such album.
    pub(crate) fn new(library: &LibraryFeature, album_index: usize) -> Option<Self> {
        let album = library.model.albums.get(album_index)?;
        let tracks: Vec<_> = album
            .tracks
            .iter()
            .filter_map(|&index| Some((index, library.model.tracks.get(index)?)))
            .collect();
        let number = |number: Option<u32>| number.map(|number| number.to_string());
        let original = AlbumFields {
            title: album.title.clone(),
            artist: album.artist.clone(),
            year: number(
                tracks
                    .iter()
                    .filter_map(|(_, track)| track.release_year)
                    .min(),
            )
            .unwrap_or_default(),
        };
        Some(Self {
            album_index,
            album: original.clone(),
            original,
            tracks: tracks
                .into_iter()
                .map(|(index, track)| {
                    let original = TrackFields {
                        title: track.title.clone(),
                        artist: track.artist.clone(),
                        disc: number(track.disc_number).unwrap_or_default(),
                        number: number(track.track_number).unwrap_or_default(),
                    };
                    TrackDraft {
                        index,
                        path: track.path.clone(),
                        fields: original.clone(),
                        original,
                        recording_id: None,
                    }
                })
                .collect(),
            release_id: None,
            releases: Vec::new(),
            chosen: None,
            job: Job::Idle,
            message: None,
        })
    }

    /// Whether the dialog is waiting on MusicBrainz or a save.
    pub(crate) fn busy(&self) -> bool {
        !matches!(self.job, Job::Idle)
    }

    /// Search MusicBrainz for releases with the album title and artist typed in.
    ///
    /// # Arguments
    /// * `ctx` - Repainted when the search finishes.
    pub(crate) fn search(&mut self, ctx: &Repaint) {
        let (album, artist) = (self.album.title.clone(), self.album.artist.clone());
        self.start(spawn(ctx, "viper-tag-search", Job::Searching, move || {
            musicbrainz::search(&album, &artist)
        }));
    }

    /// Fill in the tags from one of the releases found.
    ///
    /// # Arguments
    /// * `ctx` - Repainted when the release's tracks arrive.
    /// * `index` - Index into [`Self::releases`].
    pub(crate) fn choose(&mut self, ctx: &Repaint, index: usize) {
        let Some(release) = self.releases.get(index) else {
            return;
        };
        let id = release.id.clone();
        self.chosen = Some(index);
        self.start(spawn(ctx, "viper-tag-release", Job::Fetching, move || {
            musicbrainz::release_media(&id)
        }));
    }

    /// Write every changed tag into the album's files.
    ///
    /// # Arguments
    /// * `ctx` - Repainted when the files are written.
    ///
    /// # Returns
    /// `true` when nothing changed, so there is nothing to wait for.
    pub(crate) fn save(&mut self, ctx: &Repaint) -> bool {
        let year = self.album.year.trim();
        let numbers = self
            .tracks
            .iter()
            .flat_map(|track| [track.fields.disc.trim(), track.fields.number.trim()]);
        if std::iter::once(year)
            .chain(numbers)
            .any(|text| !text.is_empty() && text.parse::<u32>().is_err())
        {
            self.message =
                Some("The year and the disc and track numbers must be whole numbers.".into());
            return false;
        }
        let album_changes = self.album_changes();
        let writes: Vec<_> = self
            .tracks
            .iter()
            .filter_map(|track| {
                let mut changes = album_changes.clone();
                track.push_changes(&mut changes);
                (!changes.is_empty()).then(|| (track.index, track.path.clone(), changes))
            })
            .collect();
        if writes.is_empty() {
            return true;
        }
        self.message = None;
        self.start(spawn(ctx, "viper-tag-save", Job::Saving, move || {
            writes
                .into_par_iter()
                .map(|(index, path, changes)| {
                    writer::write(&path, &changes)
                        .map(|()| (index, read_track(path.clone())))
                        .map_err(|error| {
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            format!("Could not save the tags of {name}: {error}")
                        })
                })
                .collect()
        }));
        false
    }

    fn start(&mut self, job: Result<Job, String>) {
        match job {
            Ok(job) => self.job = job,
            Err(error) => self.message = Some(error),
        }
    }

    /// The album-wide tags the user changed, written to every track.
    fn album_changes(&self) -> Vec<TagChange> {
        let mut changes = Vec::with_capacity(4);
        for (key, now, before) in [
            (ItemKey::AlbumTitle, &self.album.title, &self.original.title),
            (
                ItemKey::AlbumArtist,
                &self.album.artist,
                &self.original.artist,
            ),
            (
                ItemKey::RecordingDate,
                &self.album.year,
                &self.original.year,
            ),
        ] {
            if now.trim() != before.trim() {
                changes.push((key, now.trim().to_owned()));
            }
        }
        if let Some(id) = &self.release_id {
            changes.push((ItemKey::MusicBrainzReleaseId, id.clone()));
        }
        changes
    }

    /// Take in what the background job produced.
    ///
    /// # Arguments
    /// * `library` - Shows the saved tags.
    /// * `settings` - Keeps a renamed album's rating and history.
    /// * `toasts` - Receives save errors.
    ///
    /// # Returns
    /// `true` when the tags are saved and the dialog should close.
    pub(crate) fn poll(
        &mut self,
        library: &mut LibraryFeature,
        settings: &mut Settings,
        toasts: &mut Toasts,
    ) -> bool {
        match &self.job {
            Job::Idle => false,
            Job::Searching(job) => match job.try_recv() {
                Ok(result) => {
                    self.job = Job::Idle;
                    self.chosen = None;
                    match result {
                        Ok(releases) => {
                            self.message = releases.is_empty().then(|| {
                                "MusicBrainz has no release with this title and artist.".to_owned()
                            });
                            self.releases = releases;
                        }
                        Err(error) => self.message = Some(error),
                    }
                    false
                }
                Err(TryRecvError::Empty) => false,
                Err(TryRecvError::Disconnected) => self.stopped(),
            },
            Job::Fetching(job) => match job.try_recv() {
                Ok(result) => {
                    self.job = Job::Idle;
                    match result {
                        Ok(media) => self.apply(&media),
                        Err(error) => {
                            self.chosen = None;
                            self.message = Some(error);
                        }
                    }
                    false
                }
                Err(TryRecvError::Empty) => false,
                Err(TryRecvError::Disconnected) => self.stopped(),
            },
            Job::Saving(job) => match job.try_recv() {
                Ok(results) => {
                    // A rescan may have moved tracks since the dialog opened; skip any that did.
                    let (saved, failed): (Vec<_>, Vec<_>) =
                        results.into_iter().partition(|result| {
                            result.as_ref().is_ok_and(|(index, track)| {
                                library
                                    .model
                                    .tracks
                                    .get(*index)
                                    .is_some_and(|current| current.path == track.path)
                            })
                        });
                    let saved = saved.into_iter().filter_map(Result::ok).collect();
                    if let Some((from, to)) = library.apply_tag_edits(self.album_index, saved) {
                        if let Err(error) = settings.move_album_state(&from, &to) {
                            toasts.push(format!("Could not keep the album's rating: {error}"));
                        }
                    }
                    for error in failed.into_iter().filter_map(Result::err) {
                        toasts.push(error);
                    }
                    true
                }
                Err(TryRecvError::Empty) => false,
                Err(TryRecvError::Disconnected) => {
                    toasts.push("Saving the tags stopped unexpectedly".to_owned());
                    true
                }
            },
        }
    }

    fn stopped(&mut self) -> bool {
        self.job = Job::Idle;
        self.message = Some("The MusicBrainz search stopped".into());
        false
    }

    /// Fill the tags from a release's discs, matching tracks in order when the counts agree and
    /// by disc and track number otherwise.
    fn apply(&mut self, media: &[Release]) {
        let Some(first) = media.first() else {
            self.message = Some("MusicBrainz lists no tracks for this release.".into());
            return;
        };
        self.album.title.clone_from(&first.title);
        self.album.artist.clone_from(&first.artist);
        self.album.year = first.year.map(|year| year.to_string()).unwrap_or_default();
        self.release_id = Some(first.id.clone());
        let release_tracks: Vec<_> = media
            .iter()
            .flat_map(|medium| {
                medium
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(position, track)| (medium.disc_number, position as u32 + 1, track))
            })
            .collect();
        let in_order = release_tracks.len() == self.tracks.len();
        let mut unmatched = 0;
        for (position, track) in self.tracks.iter_mut().enumerate() {
            let found = if in_order {
                release_tracks.get(position)
            } else {
                let disc = track.fields.disc.trim().parse().unwrap_or(1);
                let number = track.fields.number.trim().parse().ok();
                release_tracks.iter().find(|(found_disc, found_number, _)| {
                    *found_disc == disc && Some(*found_number) == number
                })
            };
            let Some(&(disc, number, found)) = found else {
                unmatched += 1;
                continue;
            };
            track.fields = TrackFields {
                title: found.title.clone(),
                artist: found.artist.clone(),
                disc: disc.to_string(),
                number: number.to_string(),
            };
            track.recording_id.clone_from(&found.recording_id);
        }
        self.message = (unmatched > 0).then(|| {
            format!(
                "{unmatched} of the album's tracks have no match on this release and keep their tags."
            )
        });
    }
}

impl TrackDraft {
    /// Add the tags of this track the user changed.
    fn push_changes(&self, changes: &mut Vec<TagChange>) {
        for (key, now, before) in [
            (
                ItemKey::TrackTitle,
                &self.fields.title,
                &self.original.title,
            ),
            (
                ItemKey::TrackArtist,
                &self.fields.artist,
                &self.original.artist,
            ),
            (ItemKey::DiscNumber, &self.fields.disc, &self.original.disc),
            (
                ItemKey::TrackNumber,
                &self.fields.number,
                &self.original.number,
            ),
        ] {
            if now.trim() != before.trim() {
                changes.push((key, now.trim().to_owned()));
            }
        }
        if let Some(id) = &self.recording_id {
            changes.push((ItemKey::MusicBrainzRecordingId, id.clone()));
        }
    }
}

/// Run a job on its own thread and repaint when it is done.
///
/// # Errors
/// Returns a message for the user when the thread could not start.
fn spawn<T: Send + 'static>(
    ctx: &Repaint,
    name: &str,
    job: fn(Receiver<T>) -> Job,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<Job, String> {
    let (sender, receiver) = mpsc::channel();
    let repaint = ctx.clone();
    let spawned = thread::Builder::new().name(name.to_owned()).spawn(move || {
        let _ = sender.send(work());
        repaint.request_repaint();
    });
    spawned
        .map(|_| job(receiver))
        .map_err(|error| format!("Could not start working on the tags: {error}"))
}
