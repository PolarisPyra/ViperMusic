//! The ripper's state machine. Each phase owns exactly what is valid in it, so a background job,
//! its result, and what the page shows can't disagree, and every transition is in this file.
use super::{
    drive::{paranoia_installed, AudioTrack, Drive, DriveStatus, Toc},
    musicbrainz::{self, Release},
    rip::{self, AlbumTags, PlannedTrack, RipEvent, RipPlan},
};
use crate::shared::ui::toasts::Toasts;
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

/// How often drives are checked for a disc while the ripper is open.
const PROBE_INTERVAL: Duration = Duration::from_millis(1500);

pub(crate) struct RipperFeature {
    /// Drives found by the last probe, each with its status; `None` until the first probe ends.
    pub(crate) drives: Option<Vec<(Drive, DriveStatus)>>,
    /// The drive the ripper reads; `None` until a probe finds one.
    pub(crate) selected: Option<Drive>,
    probe: Probe,
    pub(crate) phase: Phase,
    /// The folder albums are ripped into, when the user chose one other than the library's.
    pub(crate) destination: Option<PathBuf>,
    pub(crate) eject_when_done: bool,
}

enum Probe {
    Idle { due: Instant },
    Running(Receiver<Vec<(Drive, DriveStatus)>>),
}

pub(crate) enum Phase {
    /// No disc to read yet: the page shows the drive's status.
    Waiting,
    /// Reading the table of contents and asking MusicBrainz about the disc.
    Reading(Receiver<Result<DiscRead, String>>),
    /// The disc is read and its tags can be edited.
    Ready(Box<Session>),
    Ripping(Box<Rip>),
    /// Cancel was pressed; the worker is stopping the current track.
    Cancelling(Box<Rip>),
    Done(Box<Rip>),
    /// The disc could not be read.
    Failed(String),
}

pub(crate) struct DiscRead {
    toc: Toc,
    lookup: Result<Vec<Release>, String>,
}

/// A read disc and what the user has made of its tags.
pub(crate) struct Session {
    pub(crate) drive: Drive,
    /// Where to add the disc to MusicBrainz when it isn't known there.
    pub(crate) submission_url: String,
    pub(crate) tracks: Vec<AudioTrack>,
    pub(crate) lookup: Lookup,
    pub(crate) draft: Draft,
    pub(crate) cover: Cover,
}

pub(crate) enum Lookup {
    Searching(Receiver<Result<Vec<Release>, String>>),
    Matched {
        releases: Vec<Release>,
        chosen: usize,
    },
    NotFound,
    Failed(String),
}

/// The editable tags. Strings, so fields can be cleared while typing.
pub(crate) struct Draft {
    pub(crate) album: String,
    pub(crate) artist: String,
    pub(crate) year: String,
    pub(crate) disc_number: u32,
    pub(crate) disc_count: u32,
    pub(crate) release_id: Option<String>,
    pub(crate) tracks: Vec<TrackDraft>,
}

pub(crate) struct TrackDraft {
    pub(crate) include: bool,
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) recording_id: Option<String>,
}

/// A downloaded cover and its decoded image; `None` when the release has none.
type CoverResult = Result<Option<(Vec<u8>, egui::ColorImage)>, String>;

pub(crate) enum Cover {
    /// No release is chosen, so there is no cover to fetch.
    None,
    Loading {
        release_id: String,
        job: Receiver<CoverResult>,
    },
    Ready {
        release_id: String,
        bytes: Vec<u8>,
        texture: egui::TextureHandle,
    },
    /// The Cover Art Archive has no cover for the release, or could not be reached.
    Unavailable { release_id: String },
}

/// A rip in progress or finished, and each track's progress.
pub(crate) struct Rip {
    pub(crate) session: Box<Session>,
    pub(crate) rows: Vec<TrackProgress>,
    pub(crate) folder: PathBuf,
    pub(crate) started: Instant,
    job: Receiver<RipEvent>,
    cancel: Arc<AtomicBool>,
    /// Whether the drive ejected the disc at the end.
    pub(crate) ejected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TrackProgress {
    /// Not chosen for this rip.
    Skipped,
    Queued,
    Reading {
        read: u64,
    },
    Done,
    Failed(String),
}

impl Default for RipperFeature {
    fn default() -> Self {
        Self {
            drives: None,
            selected: None,
            probe: Probe::Idle {
                due: Instant::now(),
            },
            phase: Phase::Waiting,
            destination: None,
            eject_when_done: true,
        }
    }
}

impl RipperFeature {
    /// Advance background work. Runs every frame on every page, so a rip carries on and reports
    /// while the user is elsewhere.
    pub(crate) fn poll(&mut self, ctx: &egui::Context, toasts: &mut Toasts) {
        self.phase = match std::mem::replace(&mut self.phase, Phase::Waiting) {
            Phase::Reading(job) => match job.try_recv() {
                Ok(read) => self.disc_read(ctx, read),
                Err(TryRecvError::Empty) => Phase::Reading(job),
                Err(TryRecvError::Disconnected) => Phase::Failed("Reading the disc stopped".into()),
            },
            Phase::Ready(mut session) => {
                session.poll(ctx);
                Phase::Ready(session)
            }
            Phase::Ripping(rip) => poll_rip(rip, toasts, false),
            Phase::Cancelling(rip) => poll_rip(rip, toasts, true),
            phase => phase,
        };
    }

    /// Check the drives while the page is shown, and react to a disc going in or out.
    pub(crate) fn watch_drives(&mut self, ctx: &egui::Context) {
        if !matches!(
            self.phase,
            Phase::Waiting | Phase::Ready(_) | Phase::Done(_) | Phase::Failed(_)
        ) {
            return;
        }
        match &self.probe {
            Probe::Idle { due } if Instant::now() >= *due => {
                let (sender, receiver) = mpsc::channel();
                let repaint = ctx.clone();
                let spawned = thread::Builder::new()
                    .name("viper-cd-probe".to_owned())
                    .spawn(move || {
                        let drives = Drive::list()
                            .into_iter()
                            .map(|drive| {
                                let status = drive.status();
                                (drive, status)
                            })
                            .collect();
                        let _ = sender.send(drives);
                        repaint.request_repaint();
                    });
                self.probe = match spawned {
                    Ok(_) => Probe::Running(receiver),
                    Err(_) => Probe::Idle {
                        due: Instant::now() + PROBE_INTERVAL,
                    },
                };
            }
            Probe::Idle { due } => {
                ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
            }
            Probe::Running(receiver) => match receiver.try_recv() {
                Ok(drives) => {
                    self.probe = Probe::Idle {
                        due: Instant::now() + PROBE_INTERVAL,
                    };
                    self.drives_changed(drives);
                    ctx.request_repaint_after(PROBE_INTERVAL);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.probe = Probe::Idle {
                        due: Instant::now() + PROBE_INTERVAL,
                    };
                }
            },
        }
    }

    fn drives_changed(&mut self, drives: Vec<(Drive, DriveStatus)>) {
        let before = self.status().cloned();
        if !self
            .selected
            .as_ref()
            .is_some_and(|selected| drives.iter().any(|(drive, _)| drive == selected))
        {
            self.selected = drives.first().map(|(drive, _)| drive.clone());
        }
        self.drives = Some(drives);
        let now = self.status().cloned();
        let inserted =
            now == Some(DriveStatus::DiscPresent) && before != Some(DriveStatus::DiscPresent);
        let removed =
            before == Some(DriveStatus::DiscPresent) && now != Some(DriveStatus::DiscPresent);
        match &self.phase {
            Phase::Waiting if now == Some(DriveStatus::DiscPresent) => self.read_disc(),
            // A new disc replaces a finished rip or a disc that could not be read.
            Phase::Done(_) | Phase::Failed(_) if inserted => self.read_disc(),
            Phase::Ready(_) | Phase::Failed(_) if removed => self.phase = Phase::Waiting,
            _ => {}
        }
    }

    /// The selected drive's last known status.
    pub(crate) fn status(&self) -> Option<&DriveStatus> {
        let selected = self.selected.as_ref()?;
        self.drives
            .as_deref()?
            .iter()
            .find(|(drive, _)| drive == selected)
            .map(|(_, status)| status)
    }

    /// Whether the ripper is reading or writing, so the drive can't be changed.
    pub(crate) fn busy(&self) -> bool {
        matches!(
            self.phase,
            Phase::Reading(_) | Phase::Ripping(_) | Phase::Cancelling(_)
        )
    }

    pub(crate) fn select_drive(&mut self, drive: Drive) {
        if self.busy() || self.selected.as_ref() == Some(&drive) {
            return;
        }
        self.selected = Some(drive);
        self.phase = Phase::Waiting;
        // Check the new drive now rather than on the next round.
        if let Probe::Idle { due } = &mut self.probe {
            *due = Instant::now();
        }
    }

    /// Read the disc in the selected drive again, after a failure or to start over.
    pub(crate) fn read_disc(&mut self) {
        let Some(drive) = self.selected.clone() else {
            self.phase = Phase::Waiting;
            return;
        };
        let (sender, receiver) = mpsc::channel();
        let spawned = thread::Builder::new()
            .name("viper-cd-read".to_owned())
            .spawn(move || {
                let read = drive
                    .read_toc()
                    .map_err(|error| format!("Could not read the disc: {error}"))
                    .map(|toc| {
                        let lookup = musicbrainz::lookup(&toc);
                        DiscRead { toc, lookup }
                    });
                let _ = sender.send(read);
            });
        self.phase = match spawned {
            Ok(_) => Phase::Reading(receiver),
            Err(error) => Phase::Failed(format!("Could not start reading the disc: {error}")),
        };
    }

    fn disc_read(&self, ctx: &egui::Context, read: Result<DiscRead, String>) -> Phase {
        let (read, drive) = match (read, self.selected.clone()) {
            (Ok(read), Some(drive)) => (read, drive),
            (Err(error), _) => return Phase::Failed(error),
            (Ok(_), None) => return Phase::Waiting,
        };
        let tracks = read.toc.audio_tracks();
        if tracks.is_empty() {
            return Phase::Failed("This disc has no audio tracks.".into());
        }
        let mut session = Session {
            drive,
            submission_url: musicbrainz::submission_url(&read.toc),
            draft: Draft::blank(&tracks),
            tracks,
            lookup: Lookup::NotFound,
            cover: Cover::None,
        };
        session.lookup_finished(ctx, read.lookup);
        Phase::Ready(Box::new(session))
    }

    /// Start ripping the chosen tracks into `root`.
    pub(crate) fn start_rip(&mut self, ctx: &egui::Context, root: &Path) {
        let Phase::Ready(session) = std::mem::replace(&mut self.phase, Phase::Waiting) else {
            return;
        };
        self.phase = start(session, root, self.eject_when_done, ctx);
    }

    /// Rip the tracks that failed again, keeping those that worked.
    pub(crate) fn retry_failed(&mut self, ctx: &egui::Context) {
        let Phase::Done(rip) = std::mem::replace(&mut self.phase, Phase::Waiting) else {
            return;
        };
        let Rip {
            mut session,
            rows,
            folder,
            ..
        } = *rip;
        for (track, row) in session.draft.tracks.iter_mut().zip(&rows) {
            track.include = matches!(row, TrackProgress::Failed(_));
        }
        let root = folder
            .parent()
            .and_then(Path::parent)
            .map_or_else(|| folder.clone(), Path::to_path_buf);
        self.phase = start(session, &root, self.eject_when_done, ctx);
    }

    /// Go back to editing the disc's tags after a rip, to rip it again.
    pub(crate) fn edit_again(&mut self) {
        if let Phase::Done(rip) = std::mem::replace(&mut self.phase, Phase::Waiting) {
            self.phase = Phase::Ready(rip.session);
        }
    }

    pub(crate) fn cancel(&mut self) {
        if let Phase::Ripping(rip) = std::mem::replace(&mut self.phase, Phase::Waiting) {
            rip.cancel.store(true, Ordering::Relaxed);
            self.phase = Phase::Cancelling(rip);
        }
    }

    /// Eject the disc, or close the tray when it is open.
    pub(crate) fn toggle_tray(&mut self, toasts: &mut Toasts) {
        let Some(drive) = self.selected.clone() else {
            return;
        };
        let result = if self.status() == Some(&DriveStatus::TrayOpen) {
            drive.close_tray()
        } else {
            drive.eject()
        };
        if let Err(error) = result {
            toasts.push(format!("Could not operate the tray: {error}"));
        }
        if !self.busy() {
            self.phase = match std::mem::replace(&mut self.phase, Phase::Waiting) {
                // Keep a finished rip's summary on screen after its disc comes out.
                done @ Phase::Done(_) => done,
                _ => Phase::Waiting,
            };
        }
        if let Probe::Idle { due } = &mut self.probe {
            *due = Instant::now();
        }
    }

    /// Leave a finished rip for the next disc, ejecting this one if it is still in.
    pub(crate) fn next_disc(&mut self, toasts: &mut Toasts) {
        let Phase::Done(rip) = &self.phase else {
            return;
        };
        if !rip.ejected {
            if let Err(error) = rip.session.drive.eject() {
                toasts.push(format!("Could not eject the disc: {error}"));
            }
        }
        self.phase = Phase::Waiting;
        if let Probe::Idle { due } = &mut self.probe {
            *due = Instant::now();
        }
    }

    /// Whether ripping needs cdparanoia and it is missing.
    pub(crate) fn needs_paranoia(&self) -> bool {
        self.selected
            .as_ref()
            .is_some_and(Drive::reads_with_paranoia)
            && !paranoia_installed()
    }

    /// The rip's progress from 0 to 1 while ripping, for the sidebar.
    pub(crate) fn progress(&self) -> Option<f32> {
        match &self.phase {
            Phase::Ripping(rip) | Phase::Cancelling(rip) => Some(rip.progress()),
            _ => None,
        }
    }
}

fn start(session: Box<Session>, root: &Path, eject_when_done: bool, ctx: &egui::Context) -> Phase {
    let draft = &session.draft;
    let album = AlbumTags {
        title: draft.album.trim().to_owned(),
        artist: draft.artist.trim().to_owned(),
        year: draft.year.trim().parse().ok(),
        disc_number: draft.disc_number,
        disc_count: draft.disc_count,
        track_count: session.tracks.len(),
        release_id: draft.release_id.clone(),
    };
    let folder = rip::album_folder(root, &album.artist, &album.title);
    let tracks: Vec<PlannedTrack> = session
        .tracks
        .iter()
        .zip(&draft.tracks)
        .enumerate()
        .filter(|(_, (_, track))| track.include)
        .map(|(row, (audio, track))| PlannedTrack {
            row,
            track: *audio,
            title: track.title.trim().to_owned(),
            artist: track.artist.trim().to_owned(),
            recording_id: track.recording_id.clone(),
        })
        .collect();
    if tracks.is_empty() {
        return Phase::Ready(session);
    }
    let rows = draft
        .tracks
        .iter()
        .map(|track| {
            if track.include {
                TrackProgress::Queued
            } else {
                TrackProgress::Skipped
            }
        })
        .collect();
    let cancel = Arc::new(AtomicBool::new(false));
    let plan = RipPlan {
        drive: session.drive.clone(),
        folder: folder.clone(),
        album,
        tracks,
        cover: match &session.cover {
            Cover::Ready { bytes, .. } => Some(bytes.clone()),
            _ => None,
        },
        eject_when_done,
    };
    let repaint = ctx.clone();
    match rip::spawn(plan, Arc::clone(&cancel), move || repaint.request_repaint()) {
        Ok(job) => Phase::Ripping(Box::new(Rip {
            session,
            rows,
            folder,
            started: Instant::now(),
            job,
            cancel,
            ejected: false,
        })),
        Err(error) => Phase::Failed(format!("Could not start ripping: {error}")),
    }
}

/// Apply the worker's events; the rip ends when the worker says it has stopped.
fn poll_rip(mut rip: Box<Rip>, toasts: &mut Toasts, cancelling: bool) -> Phase {
    loop {
        match rip.job.try_recv() {
            Ok(RipEvent::Progress { row, read }) => rip.set(row, TrackProgress::Reading { read }),
            Ok(RipEvent::TrackDone { row }) => rip.set(row, TrackProgress::Done),
            Ok(RipEvent::TrackFailed { row, error }) => rip.set(row, TrackProgress::Failed(error)),
            Ok(RipEvent::Finished { ejected }) => {
                rip.ejected = ejected;
                break;
            }
            Err(TryRecvError::Empty) => {
                return if cancelling {
                    Phase::Cancelling(rip)
                } else {
                    Phase::Ripping(rip)
                };
            }
            Err(TryRecvError::Disconnected) => {
                for row in &mut rip.rows {
                    if matches!(row, TrackProgress::Queued | TrackProgress::Reading { .. }) {
                        *row = TrackProgress::Failed("The rip stopped unexpectedly".into());
                    }
                }
                break;
            }
        }
    }
    if cancelling {
        let kept = rip.count(|row| *row == TrackProgress::Done);
        toasts.push(format!(
            "Stopped ripping. {kept} finished {} kept.",
            if kept == 1 {
                "track was"
            } else {
                "tracks were"
            }
        ));
        return Phase::Ready(rip.session);
    }
    let done = rip.count(|row| *row == TrackProgress::Done);
    let failed = rip.count(|row| matches!(row, TrackProgress::Failed(_)));
    toasts.push(if failed == 0 {
        format!("Ripped {}: {done} tracks", rip.session.draft.album.trim())
    } else {
        format!("Ripped {done} tracks; {failed} could not be read")
    });
    Phase::Done(rip)
}

impl Rip {
    fn set(&mut self, row: usize, progress: TrackProgress) {
        if let Some(slot) = self.rows.get_mut(row) {
            *slot = progress;
        }
    }

    pub(crate) fn count(&self, filter: impl Fn(&TrackProgress) -> bool) -> usize {
        self.rows.iter().filter(|row| filter(row)).count()
    }

    /// Bytes read so far against the bytes of every chosen track.
    pub(crate) fn bytes(&self) -> (u64, u64) {
        self.rows
            .iter()
            .zip(&self.session.tracks)
            .fold((0, 0), |(read, total), (row, track)| match row {
                TrackProgress::Skipped => (read, total),
                TrackProgress::Queued => (read, total + track.bytes()),
                TrackProgress::Reading { read: now } => (read + now, total + track.bytes()),
                TrackProgress::Done | TrackProgress::Failed(_) => {
                    (read + track.bytes(), total + track.bytes())
                }
            })
    }

    pub(crate) fn progress(&self) -> f32 {
        let (read, total) = self.bytes();
        if total == 0 {
            1.0
        } else {
            (read as f64 / total as f64) as f32
        }
    }

    /// Time left at the pace so far, once there is enough to judge it.
    pub(crate) fn remaining(&self) -> Option<Duration> {
        let (read, total) = self.bytes();
        let elapsed = self.started.elapsed();
        (read > 0 && elapsed > Duration::from_secs(3)).then(|| {
            Duration::from_secs_f64(elapsed.as_secs_f64() * (total - read) as f64 / read as f64)
        })
    }
}

impl Draft {
    fn blank(tracks: &[AudioTrack]) -> Self {
        Self {
            album: String::new(),
            artist: String::new(),
            year: String::new(),
            disc_number: 1,
            disc_count: 1,
            release_id: None,
            tracks: tracks
                .iter()
                .map(|track| TrackDraft {
                    include: true,
                    title: format!("Track {}", track.number),
                    artist: String::new(),
                    recording_id: None,
                })
                .collect(),
        }
    }

    /// Fill the tags from a release, keeping which tracks are chosen.
    fn apply(&mut self, release: &Release) {
        self.album.clone_from(&release.title);
        self.artist.clone_from(&release.artist);
        self.year = release
            .year
            .map(|year| year.to_string())
            .unwrap_or_default();
        self.disc_number = release.disc_number;
        self.disc_count = release.disc_count;
        self.release_id = Some(release.id.clone());
        for (index, track) in self.tracks.iter_mut().enumerate() {
            let found = release.tracks.get(index);
            track.title = found.map_or_else(|| format!("Track {}", index + 1), |t| t.title.clone());
            track.artist = found.map_or_else(|| release.artist.clone(), |t| t.artist.clone());
            track.recording_id = found.and_then(|t| t.recording_id.clone());
        }
    }

    /// Change the album artist, and with it every track artist that matched the old one.
    pub(crate) fn set_artist(&mut self, artist: String) {
        for track in &mut self.tracks {
            if track.artist == self.artist {
                track.artist.clone_from(&artist);
            }
        }
        self.artist = artist;
    }

    /// Whether any track has an artist of its own, so the track list shows an artist column.
    pub(crate) fn various_artists(&self) -> bool {
        self.tracks.iter().any(|track| track.artist != self.artist)
    }
}

impl Session {
    fn poll(&mut self, ctx: &egui::Context) {
        if let Lookup::Searching(job) = &self.lookup {
            match job.try_recv() {
                Ok(result) => self.lookup_finished(ctx, result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.lookup = Lookup::Failed("The MusicBrainz search stopped".into());
                }
            }
        }
        if let Cover::Loading { release_id, job } = &self.cover {
            match job.try_recv() {
                Ok(Ok(Some((bytes, image)))) => {
                    let texture = ctx.load_texture(
                        format!("ripper-cover-{release_id}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.cover = Cover::Ready {
                        release_id: release_id.clone(),
                        bytes,
                        texture,
                    };
                }
                Ok(Ok(None) | Err(_)) | Err(TryRecvError::Disconnected) => {
                    self.cover = Cover::Unavailable {
                        release_id: release_id.clone(),
                    };
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    fn lookup_finished(&mut self, ctx: &egui::Context, result: Result<Vec<Release>, String>) {
        self.lookup = match result {
            Ok(releases) if releases.is_empty() => Lookup::NotFound,
            Ok(releases) => Lookup::Matched {
                releases,
                chosen: 0,
            },
            Err(error) => Lookup::Failed(error),
        };
        if let Lookup::Matched { releases, chosen } = &self.lookup {
            let release = releases[*chosen].clone();
            self.choose(ctx, &release);
        }
    }

    /// Use another of the releases MusicBrainz found.
    pub(crate) fn choose_release(&mut self, ctx: &egui::Context, index: usize) {
        let Lookup::Matched { releases, chosen } = &mut self.lookup else {
            return;
        };
        let Some(release) = releases.get(index).cloned() else {
            return;
        };
        *chosen = index;
        self.choose(ctx, &release);
    }

    fn choose(&mut self, ctx: &egui::Context, release: &Release) {
        self.draft.apply(release);
        let current = match &self.cover {
            Cover::Loading { release_id, .. }
            | Cover::Ready { release_id, .. }
            | Cover::Unavailable { release_id } => Some(release_id.as_str()),
            Cover::None => None,
        };
        if current != Some(release.id.as_str()) {
            self.cover = fetch_cover(ctx, release.id.clone());
        }
    }

    /// Ask MusicBrainz about the disc again, after it could not be reached.
    pub(crate) fn search_again(&mut self, ctx: &egui::Context) {
        let drive = self.drive.clone();
        let (sender, receiver) = mpsc::channel();
        let repaint = ctx.clone();
        let spawned = thread::Builder::new()
            .name("viper-cd-lookup".to_owned())
            .spawn(move || {
                let result = drive
                    .read_toc()
                    .map_err(|error| format!("Could not read the disc: {error}"))
                    .and_then(|toc| musicbrainz::lookup(&toc));
                let _ = sender.send(result);
                repaint.request_repaint();
            });
        self.lookup = match spawned {
            Ok(_) => Lookup::Searching(receiver),
            Err(error) => Lookup::Failed(format!("Could not start the search: {error}")),
        };
    }

    pub(crate) fn duration(&self) -> Duration {
        self.tracks
            .iter()
            .zip(&self.draft.tracks)
            .filter(|(_, draft)| draft.include)
            .map(|(track, _)| track.duration())
            .sum()
    }
}

fn fetch_cover(ctx: &egui::Context, release_id: String) -> Cover {
    let (sender, job) = mpsc::channel();
    let repaint = ctx.clone();
    let id = release_id.clone();
    let spawned = thread::Builder::new()
        .name("viper-cd-cover".to_owned())
        .spawn(move || {
            let result = musicbrainz::front_cover(&id).map(|cover| {
                cover.and_then(|bytes| {
                    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
                    let size = [image.width() as usize, image.height() as usize];
                    let image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
                    Some((bytes, image))
                })
            });
            let _ = sender.send(result);
            repaint.request_repaint();
        });
    match spawned {
        Ok(_) => Cover::Loading { release_id, job },
        Err(_) => Cover::Unavailable { release_id },
    }
}
