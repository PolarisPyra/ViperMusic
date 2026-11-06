use crate::{
    features::{library::Track, playback::Playback},
    platform::persistence::settings::LastFmSettings,
};
use secrecy::ExposeSecret;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex, MutexGuard, PoisonError,
    },
    thread,
    time::Duration,
};

const API_URL: &str = "https://ws.audioscrobbler.com/2.0/";

/// How often the scrobbler thread checks playback for a new song or the scrobble point.
const TICK: Duration = Duration::from_secs(1);

enum Event {
    Token(Result<String, String>),
    Session(Result<(String, String), String>),
    NowPlaying(Result<(), String>),
    Scrobble(Result<(), String>),
}

/// Where signing in to Last.fm stands. A reply that arrives in any other step is stale, such as
/// one from a sign-in the user disconnected during, and is dropped.
enum SignIn {
    Idle,
    /// Asking Last.fm for a token and opening its authorization page.
    RequestingToken,
    /// The user approves access in the browser, then finishes sign-in with this token.
    AwaitingApproval(String),
    /// Trading the approved token for a session.
    Exchanging,
}

/// Last.fm authorization and asynchronous playback reporting.
pub(crate) struct LastFmService {
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    sign_in: SignIn,
    scrobbler: Arc<Mutex<Scrobbler>>,
    status: String,
}

/// Reports playback to Last.fm from its own thread. It runs apart from the window, which draws no
/// frames while it is minimized, so songs played then are still scrobbled.
struct Scrobbler {
    sender: Sender<Event>,
    enabled: bool,
    settings: LastFmSettings,
    current_track: Option<usize>,
    last_position: Duration,
    now_playing_track: Option<usize>,
    scrobbled: bool,
    track_started_at: u64,
}

impl LastFmService {
    /// Start the thread that reports playback. It stops once the service is dropped.
    ///
    /// # Arguments
    /// * `playback` - Shared playback state to report.
    pub(crate) fn new(playback: &Arc<Mutex<Playback>>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let scrobbler = Arc::new(Mutex::new(Scrobbler {
            sender: sender.clone(),
            enabled: false,
            settings: LastFmSettings::default(),
            current_track: None,
            last_position: Duration::ZERO,
            now_playing_track: None,
            scrobbled: false,
            track_started_at: 0,
        }));
        let worker_playback = Arc::clone(playback);
        let worker_scrobbler = Arc::downgrade(&scrobbler);
        let spawned = thread::Builder::new()
            .name("lastfm-scrobbler".to_owned())
            .spawn(move || {
                while let Some(scrobbler) = worker_scrobbler.upgrade() {
                    let playback = worker_playback
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    let current = playback
                        .current_track()
                        .map(|(index, track)| (index, track.clone()));
                    let position = playback.position();
                    drop(playback);
                    scrobbler
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .update_playback(
                            current.as_ref().map(|(index, track)| (*index, track)),
                            position,
                        );
                    drop(scrobbler);
                    thread::sleep(TICK);
                }
            });
        if let Err(error) = spawned {
            eprintln!("Last.fm scrobbler could not start: {error}");
        }
        Self {
            sender,
            receiver,
            sign_in: SignIn::Idle,
            scrobbler,
            status: "Not connected".to_owned(),
        }
    }

    fn scrobbler(&self) -> MutexGuard<'_, Scrobbler> {
        self.scrobbler
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Give the scrobbler thread the current settings.
    ///
    /// # Arguments
    /// * `enabled` - Whether the user turned scrobbling on.
    /// * `settings` - Credentials and session to report with.
    pub(crate) fn configure(&self, enabled: bool, settings: &LastFmSettings) {
        let mut scrobbler = self.scrobbler();
        scrobbler.enabled = enabled;
        scrobbler.settings.clone_from(settings);
    }

    pub(crate) fn status(&self) -> &str {
        &self.status
    }

    pub(crate) fn can_finish_auth(&self) -> bool {
        matches!(self.sign_in, SignIn::AwaitingApproval(_))
    }

    pub(crate) fn start_auth(&mut self, settings: &LastFmSettings) {
        if settings.api_key.expose_secret().trim().is_empty()
            || settings.api_secret.expose_secret().is_empty()
        {
            self.status = "Enter an API key and shared secret first".to_owned();
            return;
        }
        self.sign_in = SignIn::RequestingToken;
        self.status = "Requesting authorization...".to_owned();
        let sender = self.sender.clone();
        let credentials = settings.clone();
        thread::spawn(move || {
            let result = get_auth_token(&credentials).and_then(|token| {
                webbrowser::open(&format!(
                    "https://www.last.fm/api/auth/?api_key={}&token={token}",
                    credentials.api_key.expose_secret()
                ))
                .map_err(|error| {
                    format!("Could not open the Last.fm authorization page: {error}")
                })?;
                Ok(token)
            });
            let _ = sender.send(Event::Token(result));
        });
    }

    pub(crate) fn finish_auth(&mut self, settings: &LastFmSettings) {
        let SignIn::AwaitingApproval(token) = &self.sign_in else {
            self.status = "Start authorization before finishing sign-in".to_owned();
            return;
        };
        let token = token.clone();
        self.sign_in = SignIn::Exchanging;
        self.status = "Finishing Last.fm sign-in...".to_owned();
        let sender = self.sender.clone();
        let credentials = settings.clone();
        thread::spawn(move || {
            let result = exchange_auth_token(&credentials, &token);
            let _ = sender.send(Event::Session(result));
        });
    }

    pub(crate) fn disconnect(&mut self) {
        self.sign_in = SignIn::Idle;
        self.scrobbler().now_playing_track = None;
        self.status = "Not connected".to_owned();
    }

    pub(crate) fn poll(&mut self, settings: &mut LastFmSettings) -> bool {
        let mut changed = false;
        while let Ok(event) = self.receiver.try_recv() {
            let stale = match (&event, &self.sign_in) {
                (Event::Token(_), SignIn::RequestingToken)
                | (Event::Session(_), SignIn::Exchanging) => false,
                (Event::Token(_) | Event::Session(_), _) => true,
                _ => false,
            };
            if stale {
                continue;
            }
            match event {
                Event::Token(Ok(token)) => {
                    self.sign_in = SignIn::AwaitingApproval(token);
                    self.status = "Authorization page opened. Approve access, then finish sign-in."
                        .to_owned();
                }
                Event::Token(Err(error)) => {
                    self.sign_in = SignIn::Idle;
                    self.status = error;
                }
                Event::Session(Ok((session_key, username))) => {
                    settings.session_key = Some(session_key.into());
                    settings.username = Some(username.clone());
                    self.sign_in = SignIn::Idle;
                    self.status = format!("Connected as {username}");
                    self.scrobbler().now_playing_track = None;
                    changed = true;
                }
                Event::Session(Err(error)) => {
                    self.sign_in = SignIn::Idle;
                    self.status = error;
                }
                Event::NowPlaying(Ok(())) => {
                    self.status = "Now playing updated on Last.fm".to_owned()
                }
                Event::NowPlaying(Err(error)) => self.status = error,
                Event::Scrobble(Ok(())) => self.status = "Track scrobbled to Last.fm".to_owned(),
                Event::Scrobble(Err(error)) => self.status = error,
            }
        }
        changed
    }
}

impl Scrobbler {
    fn update_playback(&mut self, current: Option<(usize, &Track)>, position: Duration) {
        let track_index = current.map(|(index, _)| index);
        let track = current.map(|(_, track)| track);
        if track_index != self.current_track {
            self.current_track = track_index;
            self.now_playing_track = None;
            self.scrobbled = false;
            self.track_started_at = crate::shared::clock::unix_time_seconds();
        } else if track
            .and_then(|track| track.duration_ms)
            .map(Duration::from_millis)
            .is_some_and(|duration| playback_restarted(self.last_position, position, duration))
        {
            self.now_playing_track = None;
            self.scrobbled = false;
            self.track_started_at = crate::shared::clock::unix_time_seconds();
        }
        self.last_position = position;
        let Some((track_index, track)) = current else {
            return;
        };
        if !self.enabled || self.settings.session_key.is_none() {
            return;
        }

        if self.now_playing_track != Some(track_index) {
            self.now_playing_track = Some(track_index);
            let sender = self.sender.clone();
            let track = track.clone();
            let credentials = self.settings.clone();
            thread::spawn(move || {
                let result = submit_now_playing(&credentials, &track);
                let _ = sender.send(Event::NowPlaying(result));
            });
        }

        if self.scrobbled {
            return;
        }
        let Some(duration_ms) = track.duration_ms else {
            return;
        };
        let Some(threshold) = scrobble_threshold(duration_ms) else {
            return;
        };
        if position < threshold {
            return;
        }

        self.scrobbled = true;
        let timestamp = self.track_started_at;
        let sender = self.sender.clone();
        let track = track.clone();
        let credentials = self.settings.clone();
        thread::spawn(move || {
            let result = submit_scrobble(&credentials, &track, timestamp);
            let _ = sender.send(Event::Scrobble(result));
        });
    }
}

fn scrobble_threshold(duration_ms: u64) -> Option<Duration> {
    if duration_ms <= 30_000 {
        return None;
    }
    Some((Duration::from_millis(duration_ms) / 2).min(Duration::from_secs(4 * 60)))
}

fn playback_restarted(previous: Duration, current: Duration, duration: Duration) -> bool {
    previous >= duration.saturating_sub(Duration::from_secs(2)) && current <= Duration::from_secs(2)
}

fn session_key_value(settings: &LastFmSettings) -> String {
    settings
        .session_key
        .as_ref()
        .map(|key| key.expose_secret().to_owned())
        .unwrap_or_default()
}

fn get_auth_token(settings: &LastFmSettings) -> Result<String, String> {
    let mut parameters = BTreeMap::new();
    parameters.insert("method".to_owned(), "auth.getToken".to_owned());
    api_call(settings, parameters).and_then(|response| {
        response["token"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "Last.fm did not return an authorization token".to_owned())
    })
}

fn exchange_auth_token(settings: &LastFmSettings, token: &str) -> Result<(String, String), String> {
    let mut parameters = BTreeMap::new();
    parameters.insert("method".to_owned(), "auth.getSession".to_owned());
    parameters.insert("token".to_owned(), token.to_owned());
    api_call(settings, parameters).and_then(|response| {
        let session = &response["session"];
        let key = session["key"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "Last.fm did not return a session key".to_owned())?;
        let username = session["name"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "Last.fm did not return an account name".to_owned())?;
        Ok((key, username))
    })
}

fn submit_now_playing(settings: &LastFmSettings, track: &Track) -> Result<(), String> {
    let mut parameters = track_parameters(track);
    parameters.insert("method".to_owned(), "track.updateNowPlaying".to_owned());
    parameters.insert("sk".to_owned(), session_key_value(settings));
    api_call(settings, parameters).map(|_| ())
}

fn submit_scrobble(settings: &LastFmSettings, track: &Track, timestamp: u64) -> Result<(), String> {
    let mut parameters = track_parameters(track);
    parameters.insert("method".to_owned(), "track.scrobble".to_owned());
    parameters.insert("sk".to_owned(), session_key_value(settings));
    parameters.insert("timestamp".to_owned(), timestamp.to_string());
    api_call(settings, parameters).map(|_| ())
}

fn track_parameters(track: &Track) -> BTreeMap<String, String> {
    let mut parameters = BTreeMap::new();
    parameters.insert("artist".to_owned(), track.artist.clone());
    parameters.insert("track".to_owned(), track.title.clone());
    if !track.album.is_empty() {
        parameters.insert("album".to_owned(), track.album.clone());
    }
    if let Some(duration) = track.duration_ms {
        parameters.insert("duration".to_owned(), (duration / 1000).to_string());
    }
    parameters
}

fn api_call(
    settings: &LastFmSettings,
    mut parameters: BTreeMap<String, String>,
) -> Result<Value, String> {
    parameters.insert(
        "api_key".to_owned(),
        settings.api_key.expose_secret().to_owned(),
    );
    let signature = parameters
        .iter()
        .map(|(key, value)| format!("{key}{value}"))
        .collect::<String>()
        + settings.api_secret.expose_secret();
    parameters.insert(
        "api_sig".to_owned(),
        format!("{:x}", md5::compute(signature)),
    );
    parameters.insert("format".to_owned(), "json".to_owned());

    let response: Value = ureq::post(API_URL)
        .send_form(&parameters)
        .map_err(|error| format!("Last.fm request failed: {error}"))?
        .body_mut()
        .read_json()
        .map_err(|error| format!("Could not read Last.fm response: {error}"))?;
    if let Some(message) = response["message"]
        .as_str()
        .filter(|_| response.get("error").is_some())
    {
        return Err(format!("Last.fm: {message}"));
    }
    Ok(response)
}
