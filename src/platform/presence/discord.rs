use crate::features::library::Track;
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Discord application the presence is published as, compiled in by `build.rs`.
const APPLICATION_ID: Option<&str> = option_env!("DISCORD_APPLICATION_ID");

/// Whether this build can publish a Discord presence.
pub(crate) const fn available() -> bool {
    APPLICATION_ID.is_some()
}

/// What the "Listening to …" line on the Discord profile names.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceStatus {
    /// "Listening to Viper Music".
    App,
    /// The artist and album line.
    Artist,
    /// The song title.
    #[default]
    Song,
}

/// Which details the Discord presence shows.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default)]
pub struct PresenceOptions {
    /// What the "Listening to …" line names.
    pub status: PresenceStatus,
    /// Whether the artist is shown under the song title.
    pub show_artist: bool,
    /// Whether the album is shown under the song title.
    pub show_album: bool,
    /// Whether Discord shows the song's elapsed and remaining time.
    pub show_progress: bool,
    /// Whether a paused song stays on the profile.
    pub show_when_paused: bool,
}

impl Default for PresenceOptions {
    fn default() -> Self {
        Self {
            status: PresenceStatus::Song,
            show_artist: true,
            show_album: true,
            show_progress: true,
            show_when_paused: true,
        }
    }
}

/// Track details last published to Discord, used to skip redundant activity updates.
struct PresenceKey {
    track_index: usize,
    title: String,
    artist: String,
    album: String,
    paused: bool,
    options: PresenceOptions,
}

impl PresenceKey {
    fn matches(
        &self,
        track_index: usize,
        track: &Track,
        paused: bool,
        options: PresenceOptions,
    ) -> bool {
        self.track_index == track_index
            && self.paused == paused
            && self.options == options
            && self.title == track.title
            && self.artist == track.artist
            && self.album == track.album
    }
}

pub(crate) struct DiscordPresence {
    client: Option<DiscordIpcClient>,
    enabled: bool,
    published: Option<PresenceKey>,
    retry_after: Instant,
    refresh_after: Instant,
}

impl Default for DiscordPresence {
    fn default() -> Self {
        Self {
            client: None,
            enabled: false,
            published: None,
            retry_after: Instant::now(),
            refresh_after: Instant::now(),
        }
    }
}

impl DiscordPresence {
    /// Publish the playing song to Discord, or clear it.
    ///
    /// # Arguments
    /// * `options` - What to show, or `None` while the presence is turned off.
    /// * `current` - Index and details of the playing track.
    /// * `paused` - Whether playback is paused.
    /// * `position` - How far into the track playback is.
    pub(crate) fn update(
        &mut self,
        options: Option<PresenceOptions>,
        current: Option<(usize, &Track)>,
        paused: bool,
        position: Duration,
    ) {
        if self.enabled != options.is_some() {
            self.disconnect();
            self.enabled = options.is_some();
            self.retry_after = Instant::now();
        }

        let (Some(options), Some(application_id)) = (options, APPLICATION_ID) else {
            return;
        };
        let current = current.filter(|_| options.show_when_paused || !paused);
        let Some((track_index, track)) = current else {
            if self.published.take().is_some() {
                if let Some(client) = &mut self.client {
                    if client.clear_activity().is_err() {
                        self.disconnect();
                    }
                }
            }
            return;
        };

        let now = Instant::now();
        if now < self.refresh_after
            && self
                .published
                .as_ref()
                .is_some_and(|published| published.matches(track_index, track, paused, options))
        {
            return;
        }
        if now < self.retry_after {
            return;
        }

        if self.client.is_none() {
            let mut client = DiscordIpcClient::new(application_id);
            match client.connect() {
                Ok(()) => self.client = Some(client),
                Err(_) => {
                    self.retry_after = Instant::now() + Duration::from_secs(10);
                    return;
                }
            }
        }

        let (title, artist, album) = (
            track.title.as_str(),
            track.artist.as_str(),
            track.album.as_str(),
        );
        let duration = track.duration_ms.map(Duration::from_millis);
        let shown_artist = (options.show_artist && !artist.is_empty()).then_some(artist);
        let shown_album = (options.show_album && !album.is_empty()).then_some(album);
        let state = match (shown_artist, shown_album) {
            (None, None) if options.show_artist => Some("Unknown artist".to_owned()),
            (None, None) => None,
            (Some(text), None) | (None, Some(text)) => Some(text.to_owned()),
            (Some(artist), Some(album)) => Some(format!("{artist} • {album}")),
        };
        let state = match (paused, state) {
            (true, Some(state)) => Some(format!("Paused · {state}")),
            (true, None) => Some("Paused".to_owned()),
            (false, state) => state,
        };
        let status = match options.status {
            PresenceStatus::App => activity::StatusDisplayType::Name,
            PresenceStatus::Artist if state.is_some() => activity::StatusDisplayType::State,
            PresenceStatus::Artist => activity::StatusDisplayType::Name,
            PresenceStatus::Song => activity::StatusDisplayType::Details,
        };
        let mut activity = activity::Activity::new()
            .name("Viper Music")
            .activity_type(activity::ActivityType::Listening)
            .status_display_type(status)
            .details(title);
        if let Some(state) = &state {
            activity = activity.state(state.as_str());
        }
        if !paused && options.show_progress {
            if let Some(duration) = duration {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |time| time.as_secs());
                let start = i64::try_from(now)
                    .unwrap_or(i64::MAX)
                    .saturating_sub(i64::try_from(position.as_secs()).unwrap_or(i64::MAX));
                if let Ok(duration) = i64::try_from(duration.as_secs()) {
                    if let Some(end) = start.checked_add(duration) {
                        activity =
                            activity.timestamps(activity::Timestamps::new().start(start).end(end));
                    }
                }
            }
        }

        match self
            .client
            .as_mut()
            .map(|client| client.set_activity(activity))
        {
            Some(Ok(())) => {
                self.published = Some(PresenceKey {
                    track_index,
                    title: title.to_owned(),
                    artist: artist.to_owned(),
                    album: album.to_owned(),
                    paused,
                    options,
                });
                self.refresh_after = Instant::now() + Duration::from_secs(60);
            }
            Some(Err(_)) | None => {
                self.disconnect();
                self.retry_after = Instant::now() + Duration::from_secs(10);
            }
        }
    }

    fn disconnect(&mut self) {
        if let Some(mut client) = self.client.take() {
            let _ = client.clear_activity();
            let _ = client.close();
        }
        self.published = None;
    }
}

impl Drop for DiscordPresence {
    fn drop(&mut self) {
        self.disconnect();
    }
}
