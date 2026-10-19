//! MPRIS media-control bridge between the desktop session bus and shared playback state.
use super::{Playback, RepeatMode};
use crate::features::library::{Album, AlbumIndex, Track};
use mpris_server::{LoopStatus, Metadata, PlaybackStatus, Player, Time, TrackId};
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};

#[derive(Clone, Debug, PartialEq)]
struct MprisSnapshot {
    track: Option<usize>,
    status: PlaybackStatus,
    position: Duration,
    volume: f64,
    shuffle: bool,
    loop_status: LoopStatus,
    can_go_next: bool,
    can_go_previous: bool,
    can_play: bool,
    can_pause: bool,
    can_seek: bool,
}

impl MprisSnapshot {
    fn from_playback(playback: &Playback, library: &MprisLibrary) -> Self {
        let track = playback
            .current
            .filter(|&index| library.tracks.get(index).is_some());
        let has_track = track.is_some();
        let can_seek = track
            .and_then(|index| library.tracks.get(index))
            .is_some_and(|track| track.duration_ms.is_some());
        let can_play = has_track || !library.tracks.is_empty();
        Self {
            track,
            status: if playback.is_playing() {
                PlaybackStatus::Playing
            } else if has_track {
                PlaybackStatus::Paused
            } else {
                PlaybackStatus::Stopped
            },
            position: playback.position(),
            volume: f64::from(playback.volume.min(100)) / 100.0,
            shuffle: playback.shuffle_enabled,
            loop_status: match playback.repeat_mode {
                RepeatMode::Off => LoopStatus::None,
                RepeatMode::One => LoopStatus::Track,
                RepeatMode::All => LoopStatus::Playlist,
            },
            can_go_next: playback.has_next(),
            can_go_previous: playback.current.is_some() || playback.has_history(),
            can_play,
            can_pause: has_track,
            can_seek,
        }
    }
}

#[derive(Default)]
struct MprisLibrary {
    tracks: Arc<Vec<Track>>,
    track_album: Arc<Vec<Option<AlbumIndex>>>,
    art_urls: Vec<Option<Arc<str>>>,
}

impl MprisLibrary {
    fn replace(
        &mut self,
        tracks: Arc<Vec<Track>>,
        track_album: Arc<Vec<Option<AlbumIndex>>>,
        albums: &[Album],
    ) {
        self.tracks = tracks;
        self.track_album = track_album;
        self.art_urls = albums
            .iter()
            .map(|album| album.art.as_deref().and_then(artwork_uri).map(Arc::from))
            .collect();
    }
}

#[derive(Debug)]
enum MprisCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    Seek(i64),
    SetPosition(String, Duration),
    SetVolume(f64),
    SetShuffle(bool),
    SetLoopStatus(String),
}

pub(crate) struct MprisBridge {
    library: Arc<Mutex<MprisLibrary>>,
    shutdown: Arc<AtomicBool>,
}

impl MprisBridge {
    pub(crate) fn new(playback: Arc<Mutex<Playback>>) -> Self {
        fn assert_send<T: Send>() {}
        assert_send::<Playback>();

        let library = Arc::new(Mutex::new(MprisLibrary::default()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_playback = Arc::clone(&playback);
        let worker_library = Arc::clone(&library);
        let worker_shutdown = Arc::clone(&shutdown);
        let (command_sender, command_receiver) = mpsc::channel();
        let _ = thread::Builder::new()
            .name("viper-mpris".into())
            .spawn(move || {
                async_std::task::block_on(async move {
                    let player = match Player::builder("viper")
                        .identity("Viper")
                        .desktop_entry("viper")
                        .can_quit(false)
                        .can_raise(false)
                        .can_control(true)
                        .build()
                        .await
                    {
                        Ok(player) => player,
                        Err(error) => {
                            eprintln!("Could not start MPRIS service: {error}");
                            return;
                        }
                    };
                    connect_commands(&player, command_sender);
                    let mut previous_snapshot: Option<MprisSnapshot> = None;
                    let run_task = player.run();
                    futures_lite::future::or(run_task, async {
                        while !worker_shutdown.load(Ordering::Relaxed) {
                            while let Ok(command) = command_receiver.try_recv() {
                                apply_command(&worker_playback, &worker_library, command);
                            }
                            if let Some(snapshot) =
                                current_snapshot(&worker_playback, &worker_library)
                            {
                                let metadata = (previous_snapshot
                                    .as_ref()
                                    .is_none_or(|previous| previous.track != snapshot.track))
                                .then(|| metadata_for_track(&worker_library, snapshot.track));
                                apply_snapshot(&player, &snapshot, &previous_snapshot, metadata)
                                    .await;
                                previous_snapshot = Some(snapshot);
                            }
                            async_std::task::sleep(Duration::from_millis(20)).await;
                        }
                    })
                    .await;
                });
            })
            .map_err(|error| eprintln!("Could not launch MPRIS service thread: {error}"));

        Self { library, shutdown }
    }

    pub(crate) fn set_library(
        &self,
        tracks: Arc<Vec<Track>>,
        track_album: Arc<Vec<Option<AlbumIndex>>>,
        albums: &[Album],
    ) {
        if let Ok(mut library) = self.library.lock() {
            library.replace(tracks, track_album, albums);
        }
    }
}

impl Drop for MprisBridge {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

fn current_snapshot(
    playback: &Mutex<Playback>,
    library: &Mutex<MprisLibrary>,
) -> Option<MprisSnapshot> {
    let library = library.lock().ok()?;
    let playback = playback.lock().ok()?;
    Some(MprisSnapshot::from_playback(&playback, &library))
}

fn apply_command(playback: &Mutex<Playback>, library: &Mutex<MprisLibrary>, command: MprisCommand) {
    let Ok(library) = library.lock() else {
        return;
    };
    let Ok(mut playback) = playback.lock() else {
        return;
    };
    let tracks = library.tracks.as_slice();
    match command {
        MprisCommand::Play => {
            if playback.current.is_some() {
                if playback.is_paused() {
                    playback.toggle_pause();
                }
            } else {
                playback.play_random_track(tracks);
            }
        }
        MprisCommand::Pause => {
            if playback.current.is_some() && !playback.is_paused() {
                playback.toggle_pause();
            }
        }
        MprisCommand::PlayPause => {
            if playback.current.is_some() {
                playback.toggle_pause();
            } else {
                playback.play_random_track(tracks);
            }
        }
        MprisCommand::Stop => playback.stop(),
        MprisCommand::Next => playback.skip_next(tracks),
        MprisCommand::Previous => playback.previous(tracks),
        MprisCommand::Seek(offset) => {
            let position = playback.position().as_micros() as i128;
            let target = position.saturating_add(i128::from(offset)).max(0) as u64;
            playback.seek(tracks, Duration::from_micros(target));
        }
        MprisCommand::SetPosition(track_id, position) => {
            if playback
                .current
                .is_some_and(|index| track_id == track_id_for(index))
            {
                playback.seek(tracks, position);
            }
        }
        MprisCommand::SetVolume(volume) => {
            playback.set_volume((volume.clamp(0.0, 1.0) * 100.0).round() as u8);
        }
        MprisCommand::SetShuffle(shuffle) => {
            if playback.shuffle_enabled != shuffle {
                playback.toggle_shuffle(tracks);
            }
        }
        MprisCommand::SetLoopStatus(status) => {
            let repeat_mode = match status.as_str() {
                "Track" => RepeatMode::One,
                "Playlist" => RepeatMode::All,
                _ => RepeatMode::Off,
            };
            playback.set_repeat_mode(repeat_mode);
        }
    }
}

fn connect_commands(player: &Player, sender: mpsc::Sender<MprisCommand>) {
    // Forward each D-Bus method call to the polling loop as a command.
    macro_rules! forward {
        ($connect:ident($($argument:pat_param),*) => $command:expr) => {{
            let sender = sender.clone();
            player.$connect(move |_, $($argument),*| {
                let _ = sender.send($command);
            });
        }};
    }
    forward!(connect_next() => MprisCommand::Next);
    forward!(connect_previous() => MprisCommand::Previous);
    forward!(connect_play() => MprisCommand::Play);
    forward!(connect_pause() => MprisCommand::Pause);
    forward!(connect_play_pause() => MprisCommand::PlayPause);
    forward!(connect_stop() => MprisCommand::Stop);
    forward!(connect_seek(offset) => MprisCommand::Seek(offset.as_micros()));
    forward!(connect_set_position(track_id, position) => MprisCommand::SetPosition(
        track_id.as_str().to_owned(),
        Duration::from_micros(position.as_micros().max(0) as u64),
    ));
    forward!(connect_set_volume(volume) => MprisCommand::SetVolume(volume));
    forward!(connect_set_shuffle(shuffle) => MprisCommand::SetShuffle(shuffle));
    forward!(connect_set_loop_status(status) => MprisCommand::SetLoopStatus(
        status.as_str().to_owned()
    ));
}

async fn apply_snapshot(
    player: &Player,
    snapshot: &MprisSnapshot,
    previous: &Option<MprisSnapshot>,
    metadata: Option<Metadata>,
) {
    if let Some(metadata) = metadata {
        let _ = player.set_metadata(metadata).await;
    }
    // Only publish properties that changed since the previous snapshot.
    macro_rules! publish {
        ($($field:ident => $setter:ident),* $(,)?) => {$(
            if previous
                .as_ref()
                .is_none_or(|state| state.$field != snapshot.$field)
            {
                let _ = player.$setter(snapshot.$field).await;
            }
        )*};
    }
    publish!(
        status => set_playback_status,
        volume => set_volume,
        shuffle => set_shuffle,
        loop_status => set_loop_status,
        can_go_next => set_can_go_next,
        can_go_previous => set_can_go_previous,
        can_play => set_can_play,
        can_pause => set_can_pause,
        can_seek => set_can_seek,
    );
    player.set_position(Time::from_micros(
        snapshot.position.as_micros().min(i64::MAX as u128) as i64,
    ));
}

fn metadata_for_track(library: &Mutex<MprisLibrary>, index: Option<usize>) -> Metadata {
    let no_track = || Metadata::builder().trackid(TrackId::NO_TRACK).build();
    let Ok(library) = library.lock() else {
        return no_track();
    };
    let Some((index, track)) = index.and_then(|index| Some((index, library.tracks.get(index)?)))
    else {
        return no_track();
    };
    let id = TrackId::try_from(track_id_for(index)).unwrap_or(TrackId::NO_TRACK);
    let mut metadata = Metadata::builder().trackid(id).title(track.title.clone());
    if !track.artist.is_empty() {
        metadata = metadata.artist([track.artist.clone()]);
    }
    if !track.album_artist.is_empty() {
        metadata = metadata.album_artist([track.album_artist.clone()]);
    }
    if !track.album.is_empty() {
        metadata = metadata.album(track.album.clone());
    }
    if let Some(art_url) = library
        .track_album
        .get(index)
        .and_then(Option::as_ref)
        .map(|album_index| album_index.get())
        .and_then(|album_index| library.art_urls.get(album_index))
        .and_then(Option::as_ref)
    {
        metadata = metadata.art_url(art_url.to_string());
    }
    if let Some(duration_ms) = track.duration_ms {
        metadata = metadata.length(Time::from_millis(duration_ms.min(i64::MAX as u64) as i64));
    }
    metadata.build()
}

fn track_id_for(index: usize) -> String {
    format!("/org/viper/music/track/{index}")
}

fn artwork_uri(bytes: &[u8]) -> Option<String> {
    let cache_path = crate::platform::persistence::settings::Settings::file_path()
        .ok()?
        .with_file_name("cached")
        .join("mpris")
        .join(format!(
            "{:x}.{}",
            md5::compute(bytes),
            if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                "png"
            } else {
                "jpg"
            }
        ));
    if !cache_path.is_file() {
        fs::create_dir_all(cache_path.parent()?).ok()?;
        fs::write(&cache_path, bytes).ok()?;
    }
    url::Url::from_file_path(cache_path)
        .ok()
        .map(|uri| uri.into())
}
