use super::client::{self, Lyrics};
use crate::features::library::Track;
use eframe::egui;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Duration,
};

type FetchResult = Result<Option<Lyrics>, String>;

#[derive(Clone, Copy, Debug)]
/// Current outcome of a lyrics lookup for the selected track, borrowed from the session cache.
pub(crate) enum LyricsResult<'a> {
    /// A lyrics request is running.
    Loading,
    /// Lyrics were found.
    Found(&'a Lyrics),
    /// The service has no lyrics for the track.
    Unavailable,
    /// The lyrics service request failed.
    Failed(&'a str),
}

#[derive(Default)]
/// Asynchronous lyrics requests and their session cache.
pub(crate) struct LyricsFeature {
    cache: HashMap<u64, FetchResult>,
    pending: Option<(u64, Receiver<FetchResult>)>,
}

impl LyricsFeature {
    /// Return cached lyrics or start a background lookup for `track`.
    ///
    /// # Arguments
    /// * `ctx` - egui context used to schedule redraws after the request completes.
    /// * `track` - Track metadata used to query the lyrics provider.
    ///
    /// # Returns
    /// The cached result, or the current loading state.
    pub(crate) fn request(&mut self, ctx: &egui::Context, track: &Track) -> LyricsResult<'_> {
        let key = track_key(track);
        if let Some((finished_key, receiver)) = &self.pending {
            let finished = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => Some(Err("Lyrics request stopped".to_owned())),
                Err(TryRecvError::Empty) => None,
            };
            if let Some(result) = finished {
                self.cache.insert(*finished_key, result);
                self.pending = None;
            }
        }
        if !self.cache.contains_key(&key) {
            // One request runs at a time; a different track waits for the current one to finish.
            if let Some((pending_key, _)) = &self.pending {
                ctx.request_repaint_after(Duration::from_millis(if *pending_key == key {
                    250
                } else {
                    100
                }));
                return LyricsResult::Loading;
            }
            let (sender, receiver) = mpsc::channel();
            let request_track = track.clone();
            let request_ctx = ctx.clone();
            match thread::Builder::new()
                .name("viper-lyrics-fetch".to_owned())
                .spawn(move || {
                    let _ = sender.send(client::fetch(&request_track));
                    request_ctx.request_repaint();
                }) {
                Ok(_) => {
                    self.pending = Some((key, receiver));
                    ctx.request_repaint_after(Duration::from_millis(250));
                    return LyricsResult::Loading;
                }
                Err(error) => {
                    self.cache
                        .insert(key, Err(format!("Could not start lyrics request: {error}")));
                }
            }
        }
        match &self.cache[&key] {
            Ok(Some(lyrics)) => LyricsResult::Found(lyrics),
            Ok(None) => LyricsResult::Unavailable,
            Err(error) => LyricsResult::Failed(error),
        }
    }
}

/// Hash the track fields used for a lyrics lookup without allocating a composite key.
fn track_key(track: &Track) -> u64 {
    let mut hasher = DefaultHasher::new();
    (
        &track.path,
        &track.title,
        &track.artist,
        &track.album,
        track.duration_ms,
    )
        .hash(&mut hasher);
    hasher.finish()
}
