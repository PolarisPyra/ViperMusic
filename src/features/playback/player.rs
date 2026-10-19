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
        Arc,
    },
    time::Duration,
};

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
    /// Whether upcoming tracks are shuffled.
    pub(crate) shuffle_enabled: bool,
    /// Repeat mode for the current song or listening context.
    pub(crate) repeat_mode: RepeatMode,
    /// Playback volume as a percentage from 0 to 100.
    pub volume: u8,
    #[cfg(target_os = "linux")]
    smb_auth: Option<crate::features::library::smb::SmbAuth>,
}

impl Playback {
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
        let Some((&first, rest)) = album.tracks.split_first() else {
            return;
        };
        self.prepare_selection();
        self.error = None;
        self.context_tracks = album.tracks.clone();
        let first = if self.shuffle_enabled {
            self.auto_queue.extend(album.tracks.iter().copied());
            self.auto_queue.retain(|&index| index != first);
            self.shuffle_queue();
            self.auto_queue.pop_front().unwrap_or(first)
        } else {
            self.auto_queue.extend(rest.iter().copied());
            first
        };
        self.start_track(tracks, first);
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
    pub fn advance_if_finished(&mut self, tracks: &[Track]) {
        if self.is_paused() {
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
        if self.repeat_mode == RepeatMode::One {
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
    pub fn update_gapless(&mut self, tracks: &[Track], enabled: bool) {
        if !enabled {
            self.gapless_preparation = None;
            self.clear_scheduled_pending();
            return;
        }
        if self.current.is_none() {
            self.refill_repeating_queue();
            if let Some(next) = self.pop_next_queued_track() {
                self.start_track(tracks, next);
            }
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
        self.refill_repeating_queue();
        let next_track = self.next_queued_track_with_source();
        if let Some(preparation) = self.gapless_preparation.take() {
            match preparation.receiver.try_recv() {
                Err(TryRecvError::Empty) => {
                    self.gapless_preparation = Some(preparation);
                    return;
                }
                Err(TryRecvError::Disconnected) => {
                    if next_track.map(|(index, _)| index) == Some(preparation.track_index) {
                        self.discard_next_queued_track();
                        self.error = Some("Could not prepare the next audio track".to_owned());
                    }
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
                            Err(error) => {
                                self.discard_next_queued_track();
                                self.error = Some(error);
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

    pub(crate) fn gapless_preparation_pending(&self) -> bool {
        self.gapless_preparation.is_some()
    }

    pub(crate) fn has_playback_work(&self) -> bool {
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
    pub fn remap_tracks(&mut self, remap: &[Option<usize>]) {
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
                self.clear_scheduled_pending();
            }
        }
    }

    fn start_track(&mut self, tracks: &[Track], track_index: usize) {
        self.start_track_at(tracks, track_index, Duration::ZERO);
    }

    fn start_track_at(&mut self, tracks: &[Track], track_index: usize, position: Duration) {
        let Some(track) = tracks.get(track_index) else {
            self.current = None;
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
                            self.error = Some(error);
                            self.current = None;
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
                self.current = Some(track_index);
            }
            Err(error) => {
                self.stop_current();
                self.manual_queue.clear();
                self.auto_queue.clear();
                self.error = Some(error);
            }
        }
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
            shuffle_enabled: false,
            context_tracks: Vec::new(),
            repeat_mode: RepeatMode::Off,
            volume: 70,
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
