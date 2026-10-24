use crate::platform::presence::discord::DiscordPresence;
use crate::platform::presence::lastfm::LastFmService;
use crate::{
    features::playback::Playback,
    features::{
        library::state::LibraryFeature, lyrics::LyricsFeature, preferences::PreferencesState,
    },
    platform::{
        persistence::settings::{LastFmSettings, Settings, StartupView},
        tray,
    },
    shared::{
        artwork::ArtworkCache, clock::unix_time_seconds, logo::LogoColorway, ui::toasts::Toasts,
    },
    workbench::{Page, WorkbenchState},
};
use anyhow::Context;
use eframe::egui;
use secrecy::ExposeSecret;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

/// Main application state and egui runtime integration.
pub struct ViperApp {
    pub(crate) workbench: WorkbenchState,
    pub(crate) library: LibraryFeature,
    pub(crate) playback: Arc<Mutex<Playback>>,
    pub(crate) lyrics: LyricsFeature,
    pub(crate) settings: Settings,
    pub(crate) preferences: PreferencesState,
    pub(super) discord_presence: DiscordPresence,
    pub(crate) lastfm_service: LastFmService,
    pub(crate) lastfm_settings: LastFmSettings,
    pub(super) last_observed_track: Option<usize>,
    pub(crate) toasts: Toasts,
    pub(crate) artwork_cache: ArtworkCache,
    /// System tray icon, absent when the desktop has no usable tray.
    pub(crate) tray: Option<tray::Tray>,
    /// Color way last applied to the window and tray icons.
    pub(super) applied_logo: Option<LogoColorway>,
    #[cfg(target_os = "linux")]
    pub(super) smb_auth: Option<crate::features::library::smb::SmbAuth>,
    #[cfg(target_os = "linux")]
    pub(crate) mpris: crate::features::playback::mpris::MprisBridge,
}

impl ViperApp {
    /// Create the application from persisted settings, falling back to defaults on load errors.
    ///
    /// # Returns
    /// A ready-to-run application instance.
    pub fn new() -> Self {
        Self::with_settings_result(Settings::load())
    }

    pub(crate) fn with_settings_result(settings_result: std::io::Result<Settings>) -> Self {
        let (settings, settings_error) = match settings_result {
            Ok(settings) => (settings, None),
            Err(error) => (
                Settings::default(),
                Some(format!("Could not read settings: {error}")),
            ),
        };
        let (lastfm_settings, lastfm_error) =
            match crate::platform::persistence::settings::load_lastfm_settings() {
                Ok(settings) => (settings, None),
                Err(error) => (
                    LastFmSettings::default(),
                    Some(format!("Could not read Last.fm settings: {error}")),
                ),
            };
        let saved_path = settings.music_path.clone();
        #[cfg(target_os = "linux")]
        let saved_samba = crate::platform::persistence::settings::load_samba_settings()
            .ok()
            .flatten();
        #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
        let mut preferences = PreferencesState::new(
            settings.discord_application_id.clone().unwrap_or_default(),
            lastfm_settings.api_key.expose_secret().to_owned(),
            lastfm_settings.api_secret.expose_secret().to_owned(),
        );
        #[cfg(target_os = "linux")]
        {
            if let Some(samba) = &saved_samba {
                preferences.smb_url_draft.clone_from(&samba.share_url);
                preferences.smb_username.clone_from(&samba.username);
                preferences.smb_password = samba.password.expose_secret().to_owned();
                preferences.smb_workgroup.clone_from(&samba.workgroup);
            } else if let Some(path) = saved_path
                .as_deref()
                .filter(|path| crate::features::library::is_smb_path(path))
            {
                preferences.smb_url_draft = path.to_string_lossy().into_owned();
            }
        }
        let startup_page = match settings.startup_view {
            StartupView::Home => Page::Home,
            StartupView::Albums => Page::Albums,
            StartupView::Songs => Page::Songs,
        };
        let random_sort_seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos() as u64);
        let mut playback = Playback::default();
        playback.volume = settings.volume;
        let playback = Arc::new(Mutex::new(playback));
        #[cfg(target_os = "linux")]
        let mpris = crate::features::playback::mpris::MprisBridge::new(Arc::clone(&playback));
        let mut app = Self {
            workbench: WorkbenchState::new(startup_page),
            library: LibraryFeature::new(random_sort_seed),
            playback,
            lyrics: LyricsFeature::default(),
            settings,
            preferences,
            discord_presence: DiscordPresence::default(),
            lastfm_service: LastFmService::default(),
            lastfm_settings,
            last_observed_track: None,
            toasts: Toasts::default(),
            artwork_cache: ArtworkCache::new(),
            tray: None,
            applied_logo: None,
            #[cfg(target_os = "linux")]
            smb_auth: saved_samba
                .as_ref()
                .map(|samba| crate::features::library::smb::SmbAuth {
                    username: samba.username.clone(),
                    password: samba.password.clone(),
                    workgroup: samba.workgroup.clone(),
                }),
            #[cfg(target_os = "linux")]
            mpris,
        };
        for error in [settings_error, lastfm_error].into_iter().flatten() {
            app.toasts.push(error);
        }
        if let Some(path) = saved_path {
            #[cfg(target_os = "linux")]
            if crate::features::library::is_smb_path(&path) {
                let Some(auth) = app.smb_auth.clone() else {
                    app.toasts
                        .push("Reconnect to your SMB share in Preferences");
                    return app;
                };
                app.playback().set_smb_auth(auth);
            }
            app.start_scan(path);
        }
        app
    }

    /// Lock the shared playback state, recovering it if another thread panicked while holding it.
    pub(crate) fn playback(&self) -> MutexGuard<'_, Playback> {
        self.playback.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Start playback from the tray unless a track is already playing.
    pub(crate) fn tray_play(&self) {
        let mut playback = self.playback();
        if !playback.is_playing() {
            playback.play_pause(&self.library);
        }
    }

    /// Pause playback from the tray unless it is already paused or stopped.
    pub(crate) fn tray_pause(&self) {
        let mut playback = self.playback();
        if playback.is_playing() {
            playback.toggle_pause();
        }
    }

    /// Publish the current library to the MPRIS media controls.
    #[cfg(target_os = "linux")]
    pub(super) fn sync_mpris(&self) {
        self.mpris.set_library(
            Arc::clone(&self.library.model.tracks),
            Arc::clone(&self.library.model.track_album),
            &self.library.model.albums,
        );
    }

    pub(crate) fn choose_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Choose your music folder")
            .pick_folder()
        {
            self.settings.music_path = Some(path.clone());
            let settings_error = self
                .settings
                .save()
                .err()
                .map(|error| format!("Could not save settings: {error}"));
            self.start_scan(path);
            self.artwork_cache.clear();
            self.workbench.show_album_details = false;
            if let Some(error) = settings_error {
                self.toasts.push(error);
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn connect_smb_share(&mut self) {
        let url = self.preferences.smb_url_draft.trim().to_owned();
        let result = (|| -> anyhow::Result<()> {
            let auth = crate::features::library::smb::SmbAuth {
                username: self.preferences.smb_username.trim().to_owned(),
                password: secrecy::SecretString::from(self.preferences.smb_password.clone()),
                workgroup: self.preferences.smb_workgroup.trim().to_owned(),
            };
            crate::features::library::smb::validate_url(&url)
                .context("Could not connect to SMB share")?;
            let samba_settings = crate::platform::persistence::settings::SambaSettings {
                share_url: url.clone(),
                username: auth.username.clone(),
                password: auth.password.clone(),
                workgroup: auth.workgroup.clone(),
            };
            crate::platform::persistence::settings::save_samba_settings(&samba_settings)
                .context("Could not save SMB settings")?;
            let path = PathBuf::from(url);
            self.settings.music_path = Some(path.clone());
            self.settings
                .save()
                .context("Could not save SMB location")?;
            self.playback().set_smb_auth(auth.clone());
            self.smb_auth = Some(auth);
            self.start_scan(path);
            Ok(())
        })();
        if let Err(error) = result {
            self.toasts.push(format!("{error:#}"));
        }
    }

    pub(crate) fn save_settings(&mut self) {
        if let Err(error) = self.settings.save() {
            self.toasts
                .push(format!("Could not save settings: {error}"));
        }
    }

    /// Persist the Discord application ID to the credential store, then save preferences.
    pub(crate) fn save_discord_application_id(&mut self) {
        if let Err(error) = self.settings.save_discord_application_id() {
            self.toasts
                .push(format!("Could not save Discord application ID: {error}"));
        }
        self.save_settings();
    }

    fn save_album_state<'a>(&mut self, keys: impl IntoIterator<Item = &'a str>) {
        if let Err(error) = self.settings.save_album_state(keys) {
            self.toasts
                .push(format!("Could not save album state: {error}"));
        }
    }

    fn save_lastfm_settings(&mut self) {
        if let Err(error) =
            crate::platform::persistence::settings::save_lastfm_settings(&self.lastfm_settings)
        {
            self.toasts
                .push(format!("Could not save Last.fm settings: {error}"));
        }
    }

    pub(crate) fn save_lastfm_settings_from_preferences(&mut self) {
        let api_key = self.preferences.lastfm_api_key_draft.trim().to_owned();
        let api_secret = self.preferences.lastfm_api_secret_draft.trim().to_owned();
        if api_key != self.lastfm_settings.api_key.expose_secret()
            || api_secret != self.lastfm_settings.api_secret.expose_secret()
        {
            self.lastfm_settings.session_key = None;
            self.lastfm_settings.username = None;
            self.lastfm_service.disconnect();
        }
        self.lastfm_settings.api_key = secrecy::SecretString::from(api_key);
        self.lastfm_settings.api_secret = secrecy::SecretString::from(api_secret);
        self.save_lastfm_settings();
    }

    pub(crate) fn disconnect_lastfm(&mut self) {
        self.lastfm_settings.session_key = None;
        self.lastfm_settings.username = None;
        self.lastfm_service.disconnect();
        self.save_lastfm_settings();
    }

    /// Track the window size in memory; it is persisted once when the application exits.
    pub(super) fn observe_window_size(&mut self, ctx: &egui::Context) {
        let size = ctx.input(|input| input.viewport_rect().size());
        if size.x.is_finite() && size.y.is_finite() && size.x >= 760.0 && size.y >= 520.0 {
            self.settings.window_size = Some([size.x.round(), size.y.round()]);
        }
    }

    pub(super) fn record_new_albums(&mut self) {
        let now = unix_time_seconds();
        let mut added = Vec::new();
        for album in &self.library.model.albums {
            if !self.settings.album_added.contains_key(&album.key) {
                self.settings.album_added.insert(album.key.clone(), now);
                added.push(album.key.clone());
            }
        }
        if !added.is_empty() {
            self.save_album_state(added.iter().map(String::as_str));
        }
    }

    pub(super) fn observe_playback_track(&mut self, playback: &Playback) {
        if self.last_observed_track == playback.current {
            return;
        }
        self.last_observed_track = playback.current;
        let Some(track_index) = playback.current else {
            return;
        };
        let Some(album_index) = self
            .library
            .model
            .track_album
            .get(track_index)
            .copied()
            .flatten()
        else {
            return;
        };
        let Some(album) = self.library.model.albums.get(album_index.get()) else {
            return;
        };
        let key = album.key.clone();
        *self
            .settings
            .album_play_counts
            .entry(key.clone())
            .or_default() += 1;
        self.settings
            .album_last_played
            .insert(key.clone(), unix_time_seconds());
        self.save_album_state([key.as_str()]);
    }
}
