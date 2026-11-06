use crate::features::library::{state::LibraryFeature, Album, Track};
use rodio::{Decoder, MixerDeviceSink, Player, Source};
use std::{
    collections::VecDeque,
    fs::File,
    io::BufReader,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc, Mutex, PoisonError,
    },
    time::Duration,
};

/// How often the driver looks for a finished track while a gapless track is being prepared.
const PREPARING_TICK: Duration = Duration::from_millis(16);
/// How often the driver looks for a finished track while there is playback work.
const PLAYING_TICK: Duration = Duration::from_millis(100);
/// How often the driver wakes with nothing to play; starting playback never waits for it.
const IDLE_TICK: Duration = Duration::from_millis(250);
/// Songs that can't be opened skipped in a row before playback stops, keeping the rest of the
/// queue, so a disconnected drive doesn't empty it or loop through a repeating album.
const MAX_UNREADABLE_IN_A_ROW: usize = 5;

struct GaplessPreparation {
    track_index: usize,
    receiver: Receiver<Result<Decoder<BufReader<File>>, String>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QueueSource {
    Return,
    Manual,
    Automatic,
}

struct ScheduledTrack {
    index: usize,
    source: QueueSource,
    completion: Receiver<()>,
    position_ns: Arc<AtomicU64>,
    started: Arc<AtomicBool>,
    consume_queue: bool,
}

struct PositionSource<S> {
    source: rodio::source::TrackPosition<S>,
    position_ns: Arc<AtomicU64>,
    started: Arc<AtomicBool>,
}

impl<S: Source> PositionSource<S> {
    fn new(source: S, position_ns: Arc<AtomicU64>, started: Arc<AtomicBool>) -> Self {
        Self {
            source: source.track_position(),
            position_ns,
            started,
        }
    }
}

impl<S: Source> Iterator for PositionSource<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.source.next()?;
        self.started.store(true, Ordering::Release);
        self.position_ns.store(
            self.source.get_pos().as_nanos().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.source.size_hint()
    }
}

impl<S: Source> Source for PositionSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.source.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.source.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.source.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }
    fn try_seek(&mut self, position: Duration) -> Result<(), rodio::source::SeekError> {
        self.source.try_seek(position)?;
        self.position_ns.store(
            self.source.get_pos().as_nanos().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        Ok(())
    }
}

/// Repeat behavior for the current track or playback context.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RepeatMode {
    /// Stop when the queue ends.
    #[default]
    Off,
    /// Repeat the current album or library session.
    All,
    /// Repeat the current track.
    One,
}

/// Current audio playback state, queue, history, and volume.
pub struct Playback {
    /// Index of the currently playing track in the supplied library, if any.
    pub current: Option<usize>,
    /// Most recent playback or seek error shown to the user.
    pub error: Option<String>,
    output: Option<MixerDeviceSink>,
    player: Option<Player>,
    queue_input: Option<Arc<rodio::queue::SourcesQueueInput>>,
    manual_queue: VecDeque<usize>,
    auto_queue: VecDeque<usize>,
    return_queue: VecDeque<usize>,
    history: VecDeque<usize>,
    context_tracks: Vec<usize>,
    current_completion: Option<Receiver<()>>,
    current_position_ns: Option<Arc<AtomicU64>>,
    current_started: Option<Arc<AtomicBool>>,
    scheduled_track: Option<ScheduledTrack>,
    gapless_preparation: Option<GaplessPreparation>,
    /// Track whose gapless preparation failed. It isn't prepared again, so the current track's
    /// end starts it the usual way, which skips it if it still can't be opened.
    gapless_failed: Option<usize>,
    /// Whether upcoming tracks are shuffled.
    pub(crate) shuffle_enabled: bool,
    /// Repeat mode for the current song or listening context.
    pub(crate) repeat_mode: RepeatMode,
    /// Playback volume as a percentage from 0 to 100.
    pub volume: u8,
    /// Library tracks the driver thread starts queued tracks from.
    tracks: Arc<Vec<Track>>,
    /// Whether the user turned gapless playback on.
    gapless: bool,
    #[cfg(target_os = "linux")]
    smb_auth: Option<crate::features::library::smb::SmbAuth>,
}

impl Playback {
    /// Start the thread that moves playback on to the next queued track. It runs apart from the
    /// window, which draws no frames while it is minimized or hidden, so playback keeps going.
    /// The thread ends once `playback` is dropped.
    ///
    /// # Arguments
    /// * `playback` - Shared playback state to drive.
    pub(crate) fn spawn_driver(playback: &Arc<Mutex<Self>>) {
        let playback = Arc::downgrade(playback);
        let spawned = std::thread::Builder::new()
            .name("playback-driver".to_owned())
            .spawn(move || {
                while let Some(playback) = playback.upgrade() {
                    let delay = playback
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .tick();
                    drop(playback);
                    std::thread::sleep(delay);
                }
            });
        if let Err(error) = spawned {
            eprintln!("Playback driver could not start: {error}");
        }
    }

    /// Give the driver the current library and gapless setting.
    ///
    /// # Arguments
    /// * `tracks` - Current library tracks, which queued indexes refer to.
    /// * `gapless` - Whether the user turned gapless playback on.
    pub(crate) fn sync_library(&mut self, tracks: &Arc<Vec<Track>>, gapless: bool) {
        if !Arc::ptr_eq(&self.tracks, tracks) {
            self.tracks = Arc::clone(tracks);
        }
        self.gapless = gapless;
    }

    /// Start the next queued track when the current one has ended, and prepare gapless tracks.
    ///
    /// # Returns
    /// How long the driver waits before the next tick.
    fn tick(&mut self) -> Duration {
        if !self.has_playback_work() {
            return IDLE_TICK;
        }
        let tracks = Arc::clone(&self.tracks);
        self.advance_if_finished(&tracks);
        let gapless = self.gapless && self.repeat_mode != RepeatMode::One;
        self.update_gapless(&tracks, gapless);
        if self.gapless_preparation.is_some() {
            PREPARING_TICK
        } else {
            PLAYING_TICK
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn set_smb_auth(&mut self, auth: crate::features::library::smb::SmbAuth) {
        self.smb_auth = Some(auth);
    }

    /// Set the playback volume and apply it to the active player.
    ///
    /// # Arguments
    /// * `volume` - Volume percentage; values above 100 are clamped during playback.
    pub fn set_volume(&mut self, volume: u8) {
        self.volume = volume;
        if let Some(player) = &self.player {
            player.set_volume(self.gain());
        }
    }

    fn gain(&self) -> f32 {
        f32::from(self.volume.min(100)) / 100.0
    }

    /// Return whether a track is currently playing and not paused.
    pub fn is_playing(&self) -> bool {
        self.current.is_some() && !self.is_paused()
    }

    /// Return whether playback is paused.
    pub fn is_paused(&self) -> bool {
        self.player.as_ref().is_some_and(Player::is_paused)
    }

    /// Return the current playback position.
    pub fn position(&self) -> Duration {
        self.current_position_ns
            .as_ref()
            .map_or(Duration::ZERO, |position| {
                Duration::from_nanos(position.load(Ordering::Relaxed))
            })
    }

    /// Return the index and details of the current track.
    pub(crate) fn current_track(&self) -> Option<(usize, &Track)> {
        let index = self.current?;
        Some((index, self.tracks.get(index)?))
    }

    /// Seek within the current track, clamping the position to its known duration.
    ///
    /// # Arguments
    /// * `tracks` - Current library tracks indexed by playback state.
    /// * `position` - Requested position within the current track.
    pub fn seek(&mut self, tracks: &[Track], position: Duration) {
        let Some(index) = self.current else {
            return;
        };
        let duration = tracks
            .get(index)
            .and_then(|track| track.duration_ms)
            .map(Duration::from_millis);
        let position = duration.map_or(position, |duration| position.min(duration));
        let Some(player) = &self.player else {
            return;
        };
        match player.try_seek(position) {
            Ok(()) => {}
            Err(error) => self.error = Some(format!("Could not seek in audio track: {error}")),
        }
    }

    /// Pause playback or resume it when already paused.
    pub fn toggle_pause(&mut self) {
        let Some(player) = &self.player else {
            return;
        };
        if player.is_paused() {
            player.play();
        } else {
            player.pause();
        }
    }

    /// Pause or resume the current track, or start the selected album, the queue, or shuffle.
    ///
    /// # Arguments
    /// * `library` - Loaded library and the user's album and track selection.
    pub(crate) fn play_pause(&mut self, library: &LibraryFeature) {
        let tracks = &library.model.tracks;
        if self.current.is_some() {
            self.toggle_pause();
        } else if let Some(album) = library
            .view
            .selected_album
            .and_then(|index| library.model.albums.get(index))
        {
            match library
                .view
                .selected_track
                .filter(|index| album.tracks.contains(index))
            {
                Some(track_index) => self.play_in_context(&album.tracks, tracks, track_index),
                None => self.play_album(album, tracks),
            }
        } else if self.has_next() {
            self.skip_next(tracks);
        } else if self.shuffle_enabled {
            self.play_random_track(tracks);
        }
    }

    /// Replace the current queue with one track and begin playing it.
    ///
    /// # Arguments
    /// * `tracks` - Current library tracks.
    /// * `track_index` - Index of the track to play.
    pub fn play(&mut self, tracks: &[Track], track_index: usize) {
        self.prepare_selection();
        self.context_tracks.clear();
        self.context_tracks.push(track_index);
        self.error = None;
        self.start_track(tracks, track_index);
    }

    /// Start a random library track and shuffle the remaining tracks.
    ///
    /// # Arguments
    /// * `tracks` - Current library tracks.
    pub fn play_random_track(&mut self, tracks: &[Track]) {
        if tracks.is_empty() {
            return;
        }
        self.prepare_selection();
        self.error = None;
        self.context_tracks = (0..tracks.len()).collect();
        self.auto_queue.extend(0..tracks.len());
        self.shuffle_queue();
        if let Some(track_index) = self.auto_queue.pop_front() {
            self.start_track(tracks, track_index);
        }
    }

    /// Play an album and arrange its remaining tracks according to shuffle settings.
    ///
    /// # Arguments
    /// * `album` - Album whose track indexes define playback order.
    /// * `tracks` - Current library tracks.
    pub fn play_album(&mut self, album: &Album, tracks: &[Track]) {
        if album.tracks.is_empty() {
            return;
        }
        self.prepare_selection();
        self.error = None;
        self.context_tracks = album.tracks.clone();
        if let Some(first) = self.queue_context_from_start() {
            self.start_track(tracks, first);
        }
    }

    /// Queue the whole listening context, shuffled when shuffle is on, and take the track that
    /// plays first, so a shuffled album can start on any of its tracks.
    fn queue_context_from_start(&mut self) -> Option<usize> {
        self.auto_queue.extend(self.context_tracks.iter().copied());
        if self.shuffle_enabled {
            self.shuffle_queue();
        }
        self.auto_queue.pop_front()
    }

    /// Play a track and queue the tracks that follow it in a listening context.
    ///
    /// # Arguments
    /// * `context` - Ordered track indexes, such as an album or a filtered song list.
    /// * `tracks` - Current library tracks.
    /// * `track_index` - Index of the selected track in `tracks`; must appear in `context`.
    pub fn play_in_context(&mut self, context: &[usize], tracks: &[Track], track_index: usize) {
        let Some(position) = context.iter().position(|&index| index == track_index) else {
            return;
        };
        self.prepare_selection();
        self.context_tracks = context.to_vec();
        if self.shuffle_enabled {
            self.auto_queue.extend(
                context
                    .iter()
                    .copied()
                    .filter(|&index| index != track_index),
            );
            self.shuffle_queue();
        } else {
            self.auto_queue
                .extend(context[position + 1..].iter().copied());
        }
        self.start_track(tracks, track_index);
    }

    /// Start the next queued track when the active track has ended.
    ///
    /// # Arguments
    /// * `tracks` - Current library tracks.
    fn advance_if_finished(&mut self, tracks: &[Track]) {
        // A paused track can't reach its end, but the audio thread may have moved on to a
        // scheduled gapless track just before the pause. Follow it there without starting anything.
        if self.is_paused() && self.scheduled_track.is_none() {
            return;
        }
        let completed = self.current_completion.as_ref().is_some_and(|receiver| {
            matches!(
                receiver.try_recv(),
                Ok(()) | Err(TryRecvError::Disconnected)
            )
        });
        if !completed {
            return;
        }
        if let Some(finished_track) = self.current {
            self.push_history(finished_track);
        }
        if self.repeat_mode == RepeatMode::One && !self.is_paused() {
            self.stop_current();
            if let Some(track) = self.history.pop_back() {
                self.start_track(tracks, track);
            }
            return;
        }
        if let Some(scheduled) = self.scheduled_track.take() {
            if scheduled.consume_queue {
                self.consume_scheduled(&scheduled);
            }
            self.current = Some(scheduled.index);
            self.current_completion = Some(scheduled.completion);
            self.current_position_ns = Some(scheduled.position_ns);
            self.current_started = Some(scheduled.started);
            return;
        }
        self.stop_current();
        self.refill_repeating_queue();
        if let Some(next) = self.pop_next_queued_track() {
            self.start_track(tracks, next);
        }
    }

    /// Append queued tracks to the active player for gapless transitions.
    ///
    /// # Arguments
    /// * `tracks` - Current library tracks indexed by the playback queue.
    /// * `enabled` - Whether gapless transitions should be active.
    fn update_gapless(&mut self, tracks: &[Track], enabled: bool) {
        if !enabled {
            self.gapless_preparation = None;
            self.clear_scheduled_pending();
            return;
        }
        // Only the current track's end starts the next one; nothing starts by itself while idle.
        if self.current.is_none() {
            return;
        }
        if self.is_paused()
            || self
                .current_started
                .as_ref()
                .is_some_and(|started| !started.load(Ordering::Acquire))
            || self.scheduled_track.is_some()
        {
            return;
        }
        let Some(queue_input) = self.queue_input.as_ref().map(Arc::clone) else {
            return;
        };
        // Repeat refills the queue only once the context has ended, so turning repeat off while
        // its last track plays still stops there.
        let next_track = self.next_queued_track_with_source();
        if let Some(preparation) = self.gapless_preparation.take() {
            match preparation.receiver.try_recv() {
                Err(TryRecvError::Empty) => {
                    self.gapless_preparation = Some(preparation);
                    return;
                }
                Err(TryRecvError::Disconnected) => {
                    self.gapless_failed = Some(preparation.track_index);
                    return;
                }
                Ok(result) => {
                    let is_current =
                        next_track.map(|(index, _)| index) == Some(preparation.track_index);
                    if is_current {
                        match result {
                            Ok(decoder) => {
                                let position_ns = Arc::new(AtomicU64::new(0));
                                let started = Arc::new(AtomicBool::new(false));
                                let completion =
                                    queue_input.append_with_signal(PositionSource::new(
                                        decoder,
                                        Arc::clone(&position_ns),
                                        Arc::clone(&started),
                                    ));
                                let (_, source) = next_track.expect("prepared queue target exists");
                                self.scheduled_track = Some(ScheduledTrack {
                                    index: preparation.track_index,
                                    source,
                                    completion,
                                    position_ns,
                                    started,
                                    consume_queue: true,
                                });
                                return;
                            }
                            Err(_) => {
                                self.gapless_failed = Some(preparation.track_index);
                                return;
                            }
                        }
                    }
                }
            }
        }

        let Some((track_index, _)) = self.next_queued_track_with_source() else {
            return;
        };
        if self.gapless_failed == Some(track_index) {
            return;
        }
        let Some(track) = tracks.get(track_index) else {
            self.discard_next_queued_track();
            return;
        };
        let path = track.path.clone();
        #[cfg(target_os = "linux")]
        let smb_auth = self.smb_auth.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("music-gapless-prepare".to_owned())
            .spawn(move || {
                let result = Self::open_decoder_with_auth(
                    &path,
                    #[cfg(target_os = "linux")]
                    smb_auth.as_ref(),
                );
                let _ = sender.send(result);
            });
        match worker {
            Ok(_) => {
                self.gapless_preparation = Some(GaplessPreparation {
                    track_index,
                    receiver,
                });
            }
            Err(error) => {
                self.error = Some(format!("Could not prepare next audio track: {error}"));
            }
        }
    }

    fn clear_scheduled_pending(&mut self) {
        self.gapless_preparation = None;
        self.gapless_failed = None;
        if self
            .current_started
            .as_ref()
            .is_some_and(|started| !started.load(Ordering::Acquire))
        {
            return;
        }
        let cleared = self.queue_input.as_ref().map_or(0, |input| input.clear());
        if cleared > 0 {
            self.scheduled_track = None;
        }
    }

    fn consume_scheduled(&mut self, scheduled: &ScheduledTrack) {
        let queue = match scheduled.source {
            QueueSource::Return => &mut self.return_queue,
            QueueSource::Manual => &mut self.manual_queue,
            QueueSource::Automatic => &mut self.auto_queue,
        };
        if let Some(position) = queue.iter().position(|&index| index == scheduled.index) {
            queue.remove(position);
        }
    }

    fn next_queued_track_with_source(&self) -> Option<(usize, QueueSource)> {
        self.return_queue
            .front()
            .copied()
            .map(|i| (i, QueueSource::Return))
            .or_else(|| {
                self.manual_queue
                    .front()
                    .copied()
                    .map(|i| (i, QueueSource::Manual))
            })
            .or_else(|| {
                self.auto_queue
                    .front()
                    .copied()
                    .map(|i| (i, QueueSource::Automatic))
            })
    }

    /// Return manually queued tracks, excluding automatic album and shuffle tracks.
    pub(crate) fn queue(&self) -> &VecDeque<usize> {
        &self.manual_queue
    }

    /// Return whether Previous can move to a track from playback history.
    pub(crate) fn has_history(&self) -> bool {
        !self.history.is_empty()
    }

    /// Return whether Next can play a queued song or repeat the current context.
    pub(crate) fn has_next(&self) -> bool {
        !self.return_queue.is_empty()
            || !self.manual_queue.is_empty()
            || !self.auto_queue.is_empty()
            || (self.repeat_mode == RepeatMode::All && !self.context_tracks.is_empty())
    }

    fn pop_next_queued_track(&mut self) -> Option<usize> {
        self.return_queue
            .pop_front()
            .or_else(|| self.manual_queue.pop_front())
            .or_else(|| self.auto_queue.pop_front())
    }

    fn discard_next_queued_track(&mut self) {
        if !self.return_queue.is_empty() {
            self.return_queue.pop_front();
        } else if self.manual_queue.is_empty() {
            self.auto_queue.pop_front();
        } else {
            self.manual_queue.pop_front();
        }
    }

    /// Toggle shuffle mode and reorder the tracks waiting in the queue.
    pub(crate) fn toggle_shuffle(&mut self, tracks: &[Track]) {
        self.shuffle_enabled = !self.shuffle_enabled;
        if self.shuffle_enabled {
            self.auto_queue.clear();
            if self.context_tracks.is_empty() {
                self.context_tracks.extend(0..tracks.len());
            }
            self.auto_queue.extend(
                self.context_tracks
                    .iter()
                    .copied()
                    .filter(|&index| Some(index) != self.current),
            );
            self.shuffle_queue();
            self.clear_scheduled_pending();
        } else {
            self.auto_queue.clear();
            if !self.context_tracks.is_empty() {
                if let Some(current) = self.current {
                    if let Some(position) = self.context_tracks.iter().position(|&i| i == current) {
                        self.auto_queue
                            .extend(self.context_tracks[position + 1..].iter().copied());
                    }
                }
            }
            self.clear_scheduled_pending();
        }
    }

    /// Toggle repeat for the current album or playback context.
    pub(crate) fn toggle_repeat_all(&mut self) {
        self.set_repeat_mode(if self.repeat_mode == RepeatMode::All {
            RepeatMode::Off
        } else {
            RepeatMode::All
        });
    }

    /// Change the repeat mode and drop any gapless track scheduled under the previous mode.
    pub(crate) fn set_repeat_mode(&mut self, repeat_mode: RepeatMode) {
        self.repeat_mode = repeat_mode;
        self.clear_scheduled_pending();
    }

    fn shuffle_queue(&mut self) {
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos() as u64);
        for index in (1..self.auto_queue.len()).rev() {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.auto_queue.swap(index, (seed as usize) % (index + 1));
        }
    }

    /// Add tracks to the end of the playback queue.
    pub(crate) fn enqueue(&mut self, track_indices: &[usize]) {
        self.manual_queue.extend(track_indices);
        self.clear_scheduled_pending();
    }

    /// Insert tracks at the front of the queue so they play after the current track.
    ///
    /// # Arguments
    /// * `track_indices` - Tracks to play next, in playback order.
    pub(crate) fn play_next(&mut self, track_indices: &[usize]) {
        for &track_index in track_indices.iter().rev() {
            self.manual_queue.push_front(track_index);
        }
        self.clear_scheduled_pending();
    }

    /// Remove every manually queued track while allowing an already-started track to finish.
    pub(crate) fn clear_queue(&mut self) {
        self.manual_queue.clear();
        if let Some(scheduled) = self.scheduled_track.as_mut() {
            if scheduled.source == QueueSource::Manual {
                scheduled.consume_queue = false;
            }
        }
        self.clear_scheduled_pending();
    }

    fn has_playback_work(&self) -> bool {
        self.current.is_some() || self.has_next()
    }

    /// Remove a queued track and return it when the index exists.
    pub(crate) fn remove_queued(&mut self, queue_index: usize) -> Option<usize> {
        let removed = self.manual_queue.remove(queue_index);
        if let (Some(removed), Some(scheduled)) = (removed, self.scheduled_track.as_mut()) {
            if scheduled.source == QueueSource::Manual && scheduled.index == removed {
                scheduled.consume_queue = false;
            }
        }
        self.clear_scheduled_pending();
        removed
    }

    /// Move a queued track to a new position.
    pub(crate) fn move_queued(&mut self, from: usize, to: usize) {
        if from >= self.manual_queue.len() || to >= self.manual_queue.len() || from == to {
            return;
        }
        if let Some(track) = self.manual_queue.remove(from) {
            self.manual_queue.insert(to, track);
        }
        self.clear_scheduled_pending();
    }

    /// Skip the current track and start the next queued track, if one exists.
    ///
    /// # Arguments
    /// * `tracks` - Current library tracks.
    pub fn skip_next(&mut self, tracks: &[Track]) {
        self.refill_repeating_queue();
        if let Some(next) = self.pop_next_queued_track() {
            if let Some(track) = self.current {
                self.push_history(track);
            }
            self.stop_current();
            self.start_track(tracks, next);
        }
    }

    /// Return to the most recently played track, or restart the current track.
    ///
    /// # Arguments
    /// * `tracks` - Current library tracks.
    pub fn previous(&mut self, tracks: &[Track]) {
        if let Some(current) = self.current {
            if self.position() > Duration::from_secs(3) {
                self.stop_current();
                self.start_track(tracks, current);
                return;
            }
        }
        let Some(previous) = self.history.pop_back() else {
            if let Some(current) = self.current {
                self.stop_current();
                self.start_track(tracks, current);
            }
            return;
        };
        if let Some(current) = self.current {
            self.return_queue.push_front(current);
        }
        self.stop_current();
        self.start_track(tracks, previous);
    }

    /// Stop the active track and clear the queue and playback history.
    pub fn stop(&mut self) {
        self.stop_current();
        self.manual_queue.clear();
        self.auto_queue.clear();
        self.return_queue.clear();
        self.history.clear();
        self.context_tracks.clear();
    }

    fn prepare_selection(&mut self) {
        if let Some(current) = self.current {
            self.push_history(current);
        }
        self.stop_current();
        self.manual_queue.clear();
        self.auto_queue.clear();
        self.return_queue.clear();
        self.error = None;
    }

    fn refill_repeating_queue(&mut self) {
        if self.repeat_mode != RepeatMode::All || !self.auto_queue.is_empty() {
            return;
        }
        self.auto_queue.extend(self.context_tracks.iter().copied());
        if self.shuffle_enabled {
            self.shuffle_queue();
        }
    }

    /// Update stored track indexes after the library has been changed.
    ///
    /// # Arguments
    /// * `remap` - Old-to-new track index mapping; `None` marks a removed track.
    /// * `tracks` - The changed library tracks, which the remapped indexes refer to.
    pub fn remap_tracks(&mut self, remap: &[Option<usize>], tracks: &Arc<Vec<Track>>) {
        self.tracks = Arc::clone(tracks);
        self.clear_scheduled_pending();
        if let Some(current) = self.current {
            match remap.get(current).copied().flatten() {
                Some(index) => self.current = Some(index),
                None => {
                    self.stop();
                    return;
                }
            }
        }
        let new_index = |index: &mut usize| match remap.get(*index).copied().flatten() {
            Some(remapped) => {
                *index = remapped;
                true
            }
            None => false,
        };
        for queue in [
            &mut self.manual_queue,
            &mut self.auto_queue,
            &mut self.return_queue,
            &mut self.history,
        ] {
            queue.retain_mut(new_index);
        }
        self.context_tracks.retain_mut(new_index);
        if let Some(scheduled) = self.scheduled_track.as_mut() {
            if let Some(index) = remap.get(scheduled.index).copied().flatten() {
                scheduled.index = index;
            } else {
                // Clearing above kept it, so the audio thread already plays it: stop, as for a
                // removed current track.
                self.stop();
            }
        }
    }

    fn start_track(&mut self, tracks: &[Track], track_index: usize) {
        self.start_track_at(tracks, track_index, Duration::ZERO);
    }

    fn start_track_at(&mut self, tracks: &[Track], track_index: usize, position: Duration) {
        self.start_track_skipping(tracks, track_index, position, 0);
    }

    /// Start a track, or skip to the next queued one if it can't be opened.
    ///
    /// # Arguments
    /// * `tracks` - Current library tracks.
    /// * `track_index` - Track to start.
    /// * `position` - Where to start it.
    /// * `skipped` - Songs that couldn't be opened just before this one.
    fn start_track_skipping(
        &mut self,
        tracks: &[Track],
        track_index: usize,
        position: Duration,
        skipped: usize,
    ) {
        let Some(track) = tracks.get(track_index) else {
            self.stop_current();
            return;
        };
        let result = self.open_decoder(&track.path);
        match result {
            Ok(decoder) => {
                let position_ns = Arc::new(AtomicU64::new(0));
                let started = Arc::new(AtomicBool::new(false));
                let mut source =
                    PositionSource::new(decoder, Arc::clone(&position_ns), Arc::clone(&started));
                if !position.is_zero() {
                    if let Err(error) = source.try_seek(position) {
                        self.error = Some(format!("Could not seek in audio track: {error}"));
                    }
                }
                if self.output.is_none() {
                    match Self::open_output() {
                        Ok(output) => self.output = Some(output),
                        Err(error) => {
                            self.fail_to_start(error);
                            return;
                        }
                    }
                }
                let (queue_input, queue_output) = rodio::queue::queue(true);
                let player = Player::connect_new(
                    self.output
                        .as_ref()
                        .expect("audio output initialized")
                        .mixer(),
                );
                player.set_volume(self.gain());
                player.append(queue_output);
                let completion = queue_input.append_with_signal(source);
                self.player = Some(player);
                self.queue_input = Some(queue_input);
                self.current_completion = Some(completion);
                self.current_position_ns = Some(position_ns);
                self.current_started = Some(started);
                self.scheduled_track = None;
                self.gapless_failed = None;
                self.current = Some(track_index);
            }
            Err(error) => {
                self.stop_current();
                self.error = Some(error);
                if skipped + 1 < MAX_UNREADABLE_IN_A_ROW {
                    self.refill_repeating_queue();
                    if let Some(next) = self.pop_next_queued_track() {
                        self.start_track_skipping(tracks, next, Duration::ZERO, skipped + 1);
                    }
                }
            }
        }
    }

    /// Stop playback, with the queue, when no audio output can be opened.
    fn fail_to_start(&mut self, error: String) {
        self.stop_current();
        self.manual_queue.clear();
        self.auto_queue.clear();
        self.error = Some(error);
    }

    fn open_output() -> Result<MixerDeviceSink, String> {
        let mut output = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|error| format!("Could not open audio output: {error}"))?;
        output.log_on_drop(false);
        Ok(output)
    }

    fn open_decoder(&self, path: &Path) -> Result<Decoder<BufReader<File>>, String> {
        Self::open_decoder_with_auth(
            path,
            #[cfg(target_os = "linux")]
            self.smb_auth.as_ref(),
        )
    }

    fn open_decoder_with_auth(
        path: &Path,
        #[cfg(target_os = "linux")] smb_auth: Option<&crate::features::library::smb::SmbAuth>,
    ) -> Result<Decoder<BufReader<File>>, String> {
        #[cfg(target_os = "linux")]
        let staged_path = if crate::features::library::is_smb_path(path) {
            let auth = smb_auth.ok_or_else(|| {
                "Reconnect to the SMB share in Preferences to play this track".to_owned()
            })?;
            Some(
                crate::features::library::smb::stage_file(path, auth)
                    .map_err(|error| error.to_string())
                    .map_err(|error| format!("Could not read SMB audio file: {error}"))?,
            )
        } else {
            None
        };
        #[cfg(target_os = "linux")]
        let local_path = staged_path.as_deref().unwrap_or(path);
        #[cfg(not(target_os = "linux"))]
        let local_path = path;
        let cleanup_staged = || {
            #[cfg(target_os = "linux")]
            if let Some(staged_path) = staged_path.as_ref() {
                let parent = staged_path.parent().map(std::path::Path::to_owned);
                let _ = std::fs::remove_file(staged_path);
                if let Some(parent) = parent {
                    let _ = std::fs::remove_dir(parent);
                }
            }
        };
        let file = match File::open(local_path) {
            Ok(file) => file,
            Err(error) => {
                cleanup_staged();
                return Err(format!(
                    "Could not open audio file {}: {error}",
                    path.display()
                ));
            }
        };
        let decoder = match Decoder::try_from(file) {
            Ok(decoder) => decoder,
            Err(error) => {
                cleanup_staged();
                return Err(format!(
                    "Could not decode audio file {}: {error}",
                    path.display()
                ));
            }
        };
        cleanup_staged();
        Ok(decoder)
    }

    /// Drop the active player and every handle tied to the current or scheduled track.
    fn stop_current(&mut self) {
        self.gapless_preparation = None;
        self.player = None;
        self.queue_input = None;
        self.current_completion = None;
        self.current_position_ns = None;
        self.current_started = None;
        self.scheduled_track = None;
        self.current = None;
    }

    fn push_history(&mut self, track: usize) {
        self.history.push_back(track);
        if self.history.len() > 100 {
            self.history.pop_front();
        }
    }
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            current: None,
            error: None,
            output: None,
            player: None,
            queue_input: None,
            manual_queue: VecDeque::new(),
            auto_queue: VecDeque::new(),
            return_queue: VecDeque::new(),
            history: VecDeque::new(),
            current_completion: None,
            current_position_ns: None,
            current_started: None,
            scheduled_track: None,
            gapless_preparation: None,
            gapless_failed: None,
            shuffle_enabled: false,
            context_tracks: Vec::new(),
            repeat_mode: RepeatMode::Off,
            volume: 70,
            tracks: Arc::default(),
            gapless: false,
            #[cfg(target_os = "linux")]
            smb_auth: None,
        }
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::{Playback, QueueSource, RepeatMode, ScheduledTrack, MAX_UNREADABLE_IN_A_ROW};
    use crate::features::library::Track;
    use std::{
        path::PathBuf,
        sync::{atomic::AtomicBool, mpsc, Arc},
        thread,
        time::{Duration, Instant},
    };

    fn track(number: usize) -> Track {
        Track {
            path: PathBuf::from(format!("missing/{number}.flac")),
            title: format!("Track {number}"),
            artist: String::new(),
            album_artist: String::new(),
            album: "Album".to_owned(),
            disc_number: None,
            track_number: None,
            duration_ms: Some(180_000),
            release_year: None,
            audio: Default::default(),
            metadata_missing: false,
        }
    }

    /// Track 1 scheduled after track 0 and already reached by the audio thread, as when track 0
    /// has just ended. The returned sender keeps track 1's completion pending.
    fn playing_into_gapless_track() -> (Playback, mpsc::Sender<()>) {
        let (finished, completion) = mpsc::channel();
        finished.send(()).unwrap();
        let (next_finished, next_completion) = mpsc::channel();
        let (queue_input, _queue_output) = rodio::queue::queue(true);
        let mut playback = Playback::default();
        playback.tracks = Arc::new((0..2).map(track).collect());
        playback.current = Some(0);
        playback.current_completion = Some(completion);
        playback.current_started = Some(Arc::new(AtomicBool::new(true)));
        playback.queue_input = Some(queue_input);
        playback.auto_queue = [1].into();
        playback.scheduled_track = Some(ScheduledTrack {
            index: 1,
            source: QueueSource::Automatic,
            completion: next_completion,
            position_ns: Arc::default(),
            started: Arc::new(AtomicBool::new(true)),
            consume_queue: true,
        });
        (playback, next_finished)
    }

    #[test]
    fn pausing_as_a_gapless_track_starts_shows_that_track() {
        let (mut playback, _next_finished) = playing_into_gapless_track();
        let (player, _output) = rodio::Player::new();
        player.pause();
        playback.player = Some(player);
        playback.tick();
        assert_eq!(playback.current, Some(1));
        assert!(playback.is_paused());
        assert!(playback.auto_queue.is_empty());
    }

    #[test]
    fn deleting_the_gapless_track_already_playing_stops_it() {
        let (mut playback, _next_finished) = playing_into_gapless_track();
        let tracks = Arc::new(vec![track(0)]);
        playback.remap_tracks(&[Some(0), None], &tracks);
        assert_eq!(playback.current, None);
        assert!(playback.scheduled_track.is_none());
    }

    #[test]
    fn unreadable_songs_are_skipped_until_several_in_a_row_fail() {
        let tracks: Vec<_> = (0..8).map(track).collect();
        let mut playback = Playback::default();
        playback.auto_queue = (1..8).collect();
        playback.start_track(&tracks, 0);
        assert_eq!(playback.current, None);
        assert!(playback.error.is_some());
        assert_eq!(
            playback.auto_queue,
            (MAX_UNREADABLE_IN_A_ROW..8).collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_unreadable_song_is_skipped_the_same_way_with_gapless_on() {
        let tracks: Vec<_> = (0..3).map(track).collect();
        let (finished, completion) = mpsc::channel();
        let (queue_input, _queue_output) = rodio::queue::queue(true);
        let mut playback = Playback::default();
        playback.current = Some(0);
        playback.current_completion = Some(completion);
        playback.current_started = Some(Arc::new(AtomicBool::new(true)));
        playback.queue_input = Some(queue_input);
        playback.auto_queue = [1, 2].into();
        let start = Instant::now();
        while playback.gapless_failed.is_none() {
            assert!(start.elapsed() < Duration::from_secs(5));
            playback.update_gapless(&tracks, true);
            thread::sleep(Duration::from_millis(5));
        }
        // The song stays queued and isn't prepared again.
        playback.update_gapless(&tracks, true);
        assert!(playback.gapless_preparation.is_none());
        assert_eq!(playback.auto_queue, [1, 2]);
        assert!(playback.error.is_none());

        // Track 0 ends: both songs are skipped as without gapless.
        finished.send(()).unwrap();
        playback.advance_if_finished(&tracks);
        assert_eq!(playback.current, None);
        assert!(playback.error.is_some());
        assert!(playback.auto_queue.is_empty());
    }

    #[test]
    fn shuffled_album_can_start_on_its_first_track() {
        let mut playback = Playback::default();
        playback.shuffle_enabled = true;
        playback.context_tracks = vec![3, 4, 5];
        let first = playback.queue_context_from_start();
        let mut played: Vec<_> = first
            .into_iter()
            .chain(playback.auto_queue.iter().copied())
            .collect();
        played.sort_unstable();
        assert_eq!(played, [3, 4, 5]);
    }

    #[test]
    fn album_in_order_starts_on_its_first_track() {
        let mut playback = Playback::default();
        playback.context_tracks = vec![3, 4, 5];
        assert_eq!(playback.queue_context_from_start(), Some(3));
        assert_eq!(playback.auto_queue, [4, 5]);
    }

    #[test]
    fn repeat_off_during_the_last_gapless_track_stops_there() {
        let tracks: Vec<_> = (0..3).map(track).collect();
        let (queue_input, _queue_output) = rodio::queue::queue(true);
        let mut playback = Playback::default();
        playback.current = Some(2);
        playback.context_tracks = vec![0, 1, 2];
        playback.repeat_mode = RepeatMode::All;
        playback.queue_input = Some(queue_input);
        playback.current_started = Some(Arc::new(AtomicBool::new(true)));
        playback.update_gapless(&tracks, true);
        playback.set_repeat_mode(RepeatMode::Off);
        assert!(!playback.has_next());
    }
}
