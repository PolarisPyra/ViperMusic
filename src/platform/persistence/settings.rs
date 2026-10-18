use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, ErrorKind},
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Page shown when the application starts.
pub enum StartupView {
    /// Open the home screen.
    #[default]
    Home,
    /// Open the album browser.
    Albums,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Sort order for the album browser.
pub enum AlbumSort {
    /// Sort by album artist.
    AlbumArtist,
    /// Sort by the stable library identifier.
    Id,
    /// Sort by album artist metadata.
    Artist,
    /// Sort by total album duration.
    Duration,
    /// Sort by play count, highest first.
    MostPlayed,
    /// Sort by album name.
    #[default]
    Name,
    /// Shuffle the album order.
    Random,
    /// Sort by user rating.
    Rating,
    /// Sort by when the album was added.
    RecentlyAdded,
    /// Sort by most recent playback.
    RecentlyPlayed,
    /// Sort by number of tracks.
    SongCount,
    /// Place favorite albums first.
    Favorited,
    /// Sort by release year.
    ReleaseYear,
}

impl AlbumSort {
    /// Every album sorting option shown by the application.
    pub const ALL: [Self; 13] = [
        Self::AlbumArtist,
        Self::Id,
        Self::Artist,
        Self::Duration,
        Self::MostPlayed,
        Self::Name,
        Self::Random,
        Self::Rating,
        Self::RecentlyAdded,
        Self::RecentlyPlayed,
        Self::SongCount,
        Self::Favorited,
        Self::ReleaseYear,
    ];

    /// Return the display label for this sorting option.
    pub fn label(self) -> &'static str {
        match self {
            Self::AlbumArtist => "Album Artist",
            Self::Id => "ID",
            Self::Artist => "Artist",
            Self::Duration => "Duration",
            Self::MostPlayed => "Most played",
            Self::Name => "Name",
            Self::Random => "Random",
            Self::Rating => "Rating",
            Self::RecentlyAdded => "Recently added",
            Self::RecentlyPlayed => "Recently played",
            Self::SongCount => "Song count",
            Self::Favorited => "Favorited",
            Self::ReleaseYear => "Release year",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Presentation style for the album browser.
pub enum AlbumLayout {
    /// Show albums as a cover grid.
    #[default]
    Grid,
    /// Show albums as a text list.
    List,
}

#[derive(Clone, Debug, Default)]
/// Saved SMB share location and authentication details.
pub struct SambaSettings {
    /// SMB URL identifying the server and share.
    pub share_url: String,
    /// Username used to authenticate to the share.
    pub username: String,
    /// Redacted password used to authenticate to the share and stored in the OS credential store.
    pub password: SecretString,
    /// SMB workgroup or domain.
    pub workgroup: String,
}

/// Load saved SMB connection details, migrating any legacy password to the OS credential store.
///
/// # Returns
/// The saved connection, or `None` when no connection is configured.
///
/// # Errors
/// Returns an I/O error if the database cannot be read or migrated.
pub fn load_samba_settings() -> io::Result<Option<SambaSettings>> {
    use rusqlite::OptionalExtension;

    let connection = super::database::open()?;
    let saved = connection
        .query_row(
            "SELECT share_url, username, password, workgroup
             FROM samba_settings WHERE id = 1",
            [],
            |row| {
                Ok(SambaSettings {
                    share_url: row.get(0)?,
                    username: row.get(1)?,
                    password: SecretString::from(row.get::<_, String>(2)?),
                    workgroup: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(super::database::database_error)?;
    let Some(mut saved) = saved else {
        return Ok(None);
    };

    let legacy_password = saved.password.expose_secret().to_owned();
    let entry = samba_keyring_entry()?;
    match entry.get_password() {
        Ok(password) => saved.password = SecretString::from(password),
        Err(keyring::Error::NoEntry) => {
            if !legacy_password.is_empty() {
                entry
                    .set_password(&legacy_password)
                    .map_err(|error| io::Error::new(ErrorKind::Other, error.to_string()))?;
                saved.password = SecretString::from(legacy_password.clone());
            }
        }
        Err(error) => return Err(io::Error::new(ErrorKind::Other, error.to_string())),
    }

    if !legacy_password.is_empty() {
        connection
            .pragma_update(None, "secure_delete", "ON")
            .map_err(super::database::database_error)?;
        connection
            .execute("UPDATE samba_settings SET password = '' WHERE id = 1", [])
            .map_err(super::database::database_error)?;
        connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
            .map_err(super::database::database_error)?;
    }
    Ok(Some(saved))
}

/// Persist SMB connection details, storing the password in the OS credential store.
///
/// # Arguments
/// * `settings` - Share URL and credentials to save.
///
/// # Errors
/// Returns an I/O error if the database cannot be updated.
pub fn save_samba_settings(settings: &SambaSettings) -> io::Result<()> {
    let entry = samba_keyring_entry()?;
    if settings.password.expose_secret().is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(io::Error::new(ErrorKind::Other, error.to_string())),
        }
    } else {
        entry
            .set_password(settings.password.expose_secret())
            .map_err(|error| io::Error::new(ErrorKind::Other, error.to_string()))?;
    }

    let connection = super::database::open()?;
    connection
        .pragma_update(None, "secure_delete", "ON")
        .map_err(super::database::database_error)?;
    connection
        .execute(
            "INSERT INTO samba_settings (id, share_url, username, password, workgroup)
             VALUES (1, ?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                share_url=excluded.share_url,
                username=excluded.username,
                password=excluded.password,
                workgroup=excluded.workgroup",
            rusqlite::params![
                settings.share_url,
                settings.username,
                "",
                settings.workgroup,
            ],
        )
        .map_err(super::database::database_error)?;
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(super::database::database_error)?;
    Ok(())
}

#[derive(Clone, Debug, Default)]
/// Last.fm application credentials and authenticated session.
pub struct LastFmSettings {
    /// Last.fm API key registered by the user.
    pub api_key: SecretString,
    /// Last.fm API shared secret.
    pub api_secret: SecretString,
    /// Session key returned after account authorization.
    pub session_key: Option<SecretString>,
    /// Last.fm account name returned after authorization.
    pub username: Option<String>,
}

const LASTFM_KEYRING_SERVICE: &str = "viper-music-lastfm";
const SAMBA_KEYRING_SERVICE: &str = "viper-music-samba";
const DISCORD_KEYRING_SERVICE: &str = "viper-music-discord";

fn samba_keyring_entry() -> io::Result<keyring::Entry> {
    keyring::Entry::new(SAMBA_KEYRING_SERVICE, "password")
        .map_err(|error| io::Error::new(ErrorKind::Other, error.to_string()))
}

fn discord_keyring_entry() -> io::Result<keyring::Entry> {
    keyring::Entry::new(DISCORD_KEYRING_SERVICE, "application-id")
        .map_err(|error| io::Error::new(ErrorKind::Other, error.to_string()))
}

fn load_discord_application_id(legacy_value: Option<String>) -> io::Result<Option<String>> {
    let entry = discord_keyring_entry()?;
    let application_id = match entry.get_password() {
        Ok(value) => Some(value),
        Err(keyring::Error::NoEntry) => {
            if let Some(value) = legacy_value.as_deref().filter(|value| !value.is_empty()) {
                entry
                    .set_password(value)
                    .map_err(|error| io::Error::new(ErrorKind::Other, error.to_string()))?;
                Some(value.to_owned())
            } else {
                None
            }
        }
        Err(error) => return Err(io::Error::new(ErrorKind::Other, error.to_string())),
    };

    if legacy_value.is_some() {
        let connection = super::database::open()?;
        connection
            .pragma_update(None, "secure_delete", "ON")
            .map_err(super::database::database_error)?;
        connection
            .execute(
                "UPDATE app_settings SET discord_application_id = NULL WHERE id = 1",
                [],
            )
            .map_err(super::database::database_error)?;
        connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
            .map_err(super::database::database_error)?;
    }

    Ok(application_id)
}

fn save_discord_application_id(application_id: Option<&str>) -> io::Result<()> {
    let entry = discord_keyring_entry()?;
    match application_id.filter(|value| !value.is_empty()) {
        Some(value) => entry
            .set_password(value)
            .map_err(|error| io::Error::new(ErrorKind::Other, error.to_string())),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(io::Error::new(ErrorKind::Other, error.to_string())),
        },
    }
}

fn lastfm_keyring_entry(name: &str) -> io::Result<keyring::Entry> {
    keyring::Entry::new(LASTFM_KEYRING_SERVICE, name)
        .map_err(|error| io::Error::new(ErrorKind::Other, error.to_string()))
}

fn load_lastfm_secret(name: &str, legacy_value: Option<String>) -> io::Result<Option<String>> {
    let entry = lastfm_keyring_entry(name)?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => {
            let Some(value) = legacy_value.filter(|value| !value.is_empty()) else {
                return Ok(None);
            };
            entry
                .set_password(&value)
                .map_err(|error| io::Error::new(ErrorKind::Other, error.to_string()))?;
            Ok(Some(value))
        }
        Err(error) => Err(io::Error::new(ErrorKind::Other, error.to_string())),
    }
}

/// Load Last.fm credentials from the operating system's credential store.
///
/// Legacy plaintext SQLite credentials are migrated to the keyring on first load.
pub fn load_lastfm_settings() -> io::Result<LastFmSettings> {
    use rusqlite::OptionalExtension;

    let connection = super::database::open()?;
    let legacy = connection
        .query_row(
            "SELECT api_key, api_secret, session_key, username
             FROM lastfm_settings WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(super::database::database_error)?;
    let Some(legacy) = legacy else {
        return Ok(LastFmSettings::default());
    };

    let needs_migration = !legacy.0.is_empty() || !legacy.1.is_empty() || legacy.2.is_some();
    let api_key = load_lastfm_secret("api-key", Some(legacy.0))?.unwrap_or_default();
    let api_secret = load_lastfm_secret("api-secret", Some(legacy.1))?.unwrap_or_default();
    let session_key = load_lastfm_secret("session-key", legacy.2)?;

    if needs_migration {
        connection
            .pragma_update(None, "secure_delete", "ON")
            .map_err(super::database::database_error)?;
        connection
            .execute(
                "UPDATE lastfm_settings SET api_key = '', api_secret = '', session_key = NULL
                 WHERE id = 1",
                [],
            )
            .map_err(super::database::database_error)?;
        connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
            .map_err(super::database::database_error)?;
    }

    Ok(LastFmSettings {
        api_key: SecretString::from(api_key),
        api_secret: SecretString::from(api_secret),
        session_key: session_key.map(SecretString::from),
        username: legacy.3,
    })
}

/// Persist Last.fm API credentials and the authorized session in the OS keyring.
pub fn save_lastfm_settings(settings: &LastFmSettings) -> io::Result<()> {
    lastfm_keyring_entry("api-key")?
        .set_password(settings.api_key.expose_secret())
        .map_err(|error| io::Error::new(ErrorKind::Other, error.to_string()))?;
    lastfm_keyring_entry("api-secret")?
        .set_password(settings.api_secret.expose_secret())
        .map_err(|error| io::Error::new(ErrorKind::Other, error.to_string()))?;
    let session_entry = lastfm_keyring_entry("session-key")?;
    if let Some(session_key) = settings.session_key.as_ref() {
        session_entry
            .set_password(session_key.expose_secret())
            .map_err(|error| io::Error::new(ErrorKind::Other, error.to_string()))?;
    } else if let Err(error) = session_entry.delete_credential() {
        if !matches!(error, keyring::Error::NoEntry) {
            return Err(io::Error::new(ErrorKind::Other, error.to_string()));
        }
    }

    let connection = super::database::open()?;
    connection
        .pragma_update(None, "secure_delete", "ON")
        .map_err(super::database::database_error)?;
    connection
        .execute(
            "INSERT INTO lastfm_settings (id, api_key, api_secret, session_key, username)
             VALUES (1, ?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                api_key=excluded.api_key,
                api_secret=excluded.api_secret,
                session_key=excluded.session_key,
                username=excluded.username",
            rusqlite::params!["", "", Option::<String>::None, settings.username,],
        )
        .map_err(super::database::database_error)?;
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(super::database::database_error)?;
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
/// Persisted application preferences and library state.
pub struct Settings {
    /// Selected local folder or SMB share URL.
    pub music_path: Option<PathBuf>,
    /// Last saved application window size in logical pixels.
    pub window_size: Option<[f32; 2]>,
    /// Page opened when the application starts.
    pub startup_view: StartupView,
    /// Current left sidebar width in logical pixels.
    pub left_panel_width: f32,
    /// Whether the left sidebar is hidden.
    pub left_panel_hidden: bool,
    /// Whether sidebar entries display icons without their labels.
    pub compact_sidebar: bool,
    /// Selected application color theme.
    pub theme: crate::shared::ui::theme::ThemeId,
    /// Current right panel width in logical pixels.
    pub right_panel_width: f32,
    /// Playback volume as an integer percentage from 0 to 100.
    pub volume: u8,
    /// Selected album sort order.
    pub album_sort: AlbumSort,
    /// Whether album sorting is ascending.
    pub sort_ascending: bool,
    /// Stable keys of albums marked as favorites.
    pub favorite_albums: BTreeSet<String>,
    /// User ratings keyed by stable album key.
    pub album_ratings: BTreeMap<String, u8>,
    /// Playback counts keyed by stable album key.
    pub album_play_counts: BTreeMap<String, u64>,
    /// Unix timestamps of each album's most recent playback.
    pub album_last_played: BTreeMap<String, u64>,
    /// Unix timestamps recording when each album was added.
    pub album_added: BTreeMap<String, u64>,
    /// Optional Discord Rich Presence application ID.
    pub discord_application_id: Option<String>,
    /// Whether Discord Rich Presence updates are enabled.
    pub discord_rich_presence_enabled: bool,
    /// Whether Last.fm scrobbling is enabled.
    pub lastfm_scrobbling_enabled: bool,
    /// Whether to queue decoders for seamless transitions between tracks.
    pub gapless_playback: bool,
    /// Selected album presentation layout.
    pub album_layout: AlbumLayout,
    /// Relative widths for the Title, Artist, and Date columns in album list view.
    pub album_list_column_widths: [f32; 3],
    /// Whether album artwork displays audio format badges.
    pub album_format_badges: bool,
    /// Whether the album lyrics panel opens when an album track panel is shown.
    pub lyrics_panel_open: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            music_path: None,
            window_size: None,
            startup_view: StartupView::Home,
            left_panel_width: 240.0,
            left_panel_hidden: false,
            compact_sidebar: false,
            theme: Default::default(),
            right_panel_width: 320.0,
            volume: 70,
            album_sort: AlbumSort::Name,
            sort_ascending: true,
            favorite_albums: BTreeSet::new(),
            album_ratings: BTreeMap::new(),
            album_play_counts: BTreeMap::new(),
            album_last_played: BTreeMap::new(),
            album_added: BTreeMap::new(),
            discord_application_id: None,
            discord_rich_presence_enabled: true,
            lastfm_scrobbling_enabled: false,
            gapless_playback: false,
            album_layout: AlbumLayout::Grid,
            album_list_column_widths: [0.50, 0.38, 0.12],
            album_format_badges: true,
            lyrics_panel_open: false,
        }
    }
}

impl Settings {
    /// Load settings from SQLite, migrating legacy settings when necessary.
    ///
    /// # Returns
    /// The stored settings, or defaults when no settings exist.
    ///
    /// # Errors
    /// Returns an I/O error when settings cannot be decoded or persisted.
    pub fn load() -> io::Result<Self> {
        use rusqlite::OptionalExtension;

        let connection = super::database::open()?;
        let stored = connection
            .query_row(
                "SELECT music_path, window_width, window_height, startup_view,
                        left_panel_width, left_panel_hidden, right_panel_width, volume,
                album_sort, sort_ascending, discord_application_id, album_layout, compact_sidebar, theme,
                        lyrics_panel_open, discord_rich_presence_enabled,
                        gapless_playback, album_list_column_widths, album_format_badges,
                        lastfm_scrobbling_enabled
                 FROM app_settings WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<f64>>(1)?,
                        row.get::<_, Option<f64>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, f32>(4)?,
                        row.get::<_, bool>(5)?,
                        row.get::<_, f32>(6)?,
                        row.get::<_, u8>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, bool>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, bool>(12)?,
                        row.get::<_, String>(13)?,
                        row.get::<_, bool>(14)?,
                        row.get::<_, bool>(15)?,
                        row.get::<_, bool>(16)?,
                        row.get::<_, String>(17)?,
                        row.get::<_, bool>(18)?,
                        row.get::<_, bool>(19)?,
                    ))
                },
            )
            .optional()
            .map_err(super::database::database_error)?;

        let mut settings = if let Some((
            music_path,
            window_width,
            window_height,
            startup_view,
            left_panel_width,
            left_panel_hidden,
            right_panel_width,
            volume,
            album_sort,
            sort_ascending,
            discord_application_id,
            album_layout,
            compact_sidebar,
            theme,
            lyrics_panel_open,
            discord_rich_presence_enabled,
            gapless_playback,
            album_list_column_widths,
            album_format_badges,
            lastfm_scrobbling_enabled,
        )) = stored
        {
            Self {
                music_path: music_path.map(PathBuf::from),
                window_size: window_width
                    .zip(window_height)
                    .map(|(width, height)| [width as f32, height as f32]),
                startup_view: decode_setting(&startup_view)?,
                left_panel_width,
                left_panel_hidden,
                compact_sidebar,
                theme: decode_setting(&theme)?,
                right_panel_width,
                volume,
                album_sort: decode_setting(&album_sort)?,
                sort_ascending,
                discord_application_id,
                album_layout: decode_setting(&album_layout)?,
                lyrics_panel_open,
                discord_rich_presence_enabled,
                gapless_playback,
                album_list_column_widths: decode_setting(&album_list_column_widths)?,
                album_format_badges,
                lastfm_scrobbling_enabled,
                ..Self::default()
            }
        } else {
            load_legacy_settings()?.unwrap_or_default()
        };

        settings.discord_application_id =
            load_discord_application_id(settings.discord_application_id.take())?;

        {
            let mut statement = connection
                .prepare("SELECT album_key, favorite, rating, play_count, last_played, added FROM album_state")
                .map_err(super::database::database_error)?;
            let mut rows = statement
                .query([])
                .map_err(super::database::database_error)?;
            while let Some(row) = rows.next().map_err(super::database::database_error)? {
                let key: String = row.get(0).map_err(super::database::database_error)?;
                if row
                    .get::<_, bool>(1)
                    .map_err(super::database::database_error)?
                {
                    settings.favorite_albums.insert(key.clone());
                }
                if let Some(value) = row
                    .get::<_, Option<u8>>(2)
                    .map_err(super::database::database_error)?
                {
                    settings.album_ratings.insert(key.clone(), value);
                }
                if let Some(value) = row
                    .get::<_, Option<i64>>(3)
                    .map_err(super::database::database_error)?
                    .and_then(|value| u64::try_from(value).ok())
                {
                    settings.album_play_counts.insert(key.clone(), value);
                }
                if let Some(value) = row
                    .get::<_, Option<i64>>(4)
                    .map_err(super::database::database_error)?
                    .and_then(|value| u64::try_from(value).ok())
                {
                    settings.album_last_played.insert(key.clone(), value);
                }
                if let Some(value) = row
                    .get::<_, Option<i64>>(5)
                    .map_err(super::database::database_error)?
                    .and_then(|value| u64::try_from(value).ok())
                {
                    settings.album_added.insert(key, value);
                }
            }
        }

        if !connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM app_settings WHERE id = 1)",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(super::database::database_error)?
        {
            settings.save()?;
            for path in legacy_settings_paths()? {
                let _ = fs::remove_file(path);
            }
        }
        Ok(settings)
    }

    /// Save settings and album state in one database transaction.
    ///
    /// # Errors
    /// Returns an I/O error if encoding or database writes fail.
    pub fn save(&self) -> io::Result<()> {
        save_discord_application_id(self.discord_application_id.as_deref())?;
        let mut connection = super::database::open()?;
        let transaction = connection
            .transaction()
            .map_err(super::database::database_error)?;
        let (window_width, window_height) =
            self.window_size.map_or((None, None), |[width, height]| {
                (Some(f64::from(width)), Some(f64::from(height)))
            });
        transaction.execute(
            "INSERT INTO app_settings (
                id, music_path, window_width, window_height, startup_view,
                left_panel_width, left_panel_hidden, right_panel_width, volume,
                        album_sort, sort_ascending, discord_application_id, album_layout, compact_sidebar, theme,
                lyrics_panel_open, discord_rich_presence_enabled,
                     gapless_playback, album_list_column_widths, album_format_badges,
                     lastfm_scrobbling_enabled
                 ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
             ON CONFLICT(id) DO UPDATE SET
                music_path=excluded.music_path, window_width=excluded.window_width,
                window_height=excluded.window_height, startup_view=excluded.startup_view,
                left_panel_width=excluded.left_panel_width, left_panel_hidden=excluded.left_panel_hidden,
                right_panel_width=excluded.right_panel_width, volume=excluded.volume,
                album_sort=excluded.album_sort, sort_ascending=excluded.sort_ascending,
                discord_application_id=NULL,
                album_layout=excluded.album_layout,
                compact_sidebar=excluded.compact_sidebar,
                theme=excluded.theme,
                lyrics_panel_open=excluded.lyrics_panel_open,
                discord_rich_presence_enabled=excluded.discord_rich_presence_enabled,
                gapless_playback=excluded.gapless_playback,
                album_list_column_widths=excluded.album_list_column_widths,
                album_format_badges=excluded.album_format_badges,
                lastfm_scrobbling_enabled=excluded.lastfm_scrobbling_enabled",
            rusqlite::params![
                self.music_path.as_ref().map(|path| path.to_string_lossy().into_owned()),
                window_width,
                window_height,
                encode_setting(&self.startup_view)?,
                self.left_panel_width,
                self.left_panel_hidden,
                self.right_panel_width,
                self.volume,
                encode_setting(&self.album_sort)?,
                self.sort_ascending,
                Option::<String>::None,
                encode_setting(&self.album_layout)?,
                self.compact_sidebar,
                encode_setting(&self.theme)?,
                self.lyrics_panel_open,
                self.discord_rich_presence_enabled,
                self.gapless_playback,
                encode_setting(&self.album_list_column_widths)?,
                self.album_format_badges,
                self.lastfm_scrobbling_enabled,
            ],
        ).map_err(super::database::database_error)?;

        transaction
            .execute("DELETE FROM album_state", [])
            .map_err(super::database::database_error)?;
        let keys: BTreeSet<_> = self
            .favorite_albums
            .iter()
            .chain(self.album_ratings.keys())
            .chain(self.album_play_counts.keys())
            .chain(self.album_last_played.keys())
            .chain(self.album_added.keys())
            .collect();
        for key in keys {
            transaction.execute(
                "INSERT INTO album_state (album_key, favorite, rating, play_count, last_played, added)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    key,
                    self.favorite_albums.contains(key),
                    self.album_ratings.get(key).copied(),
                    self.album_play_counts.get(key).copied().map(sqlite_integer),
                    self.album_last_played.get(key).copied().map(sqlite_integer),
                    self.album_added.get(key).copied().map(sqlite_integer),
                ],
            ).map_err(super::database::database_error)?;
        }
        transaction
            .commit()
            .map_err(super::database::database_error)
    }

    /// Return the path to the application SQLite database.
    ///
    /// # Returns
    /// The configured database path.
    ///
    /// # Errors
    /// Returns an I/O error if the configuration directory is unavailable.
    pub fn file_path() -> io::Result<PathBuf> {
        super::database::database_path()
    }
}

fn encode_setting<T: Serialize>(value: &T) -> io::Result<String> {
    serde_json::to_string(value).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

fn decode_setting<T: for<'de> Deserialize<'de>>(value: &str) -> io::Result<T> {
    serde_json::from_str(value).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

fn sqlite_integer(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn legacy_settings_paths() -> io::Result<[PathBuf; 6]> {
    let database = super::database::database_path()?;
    let previous_database = super::database::previous_database_path()?;
    let legacy_database = super::database::legacy_database_path()?;
    Ok([
        database.with_file_name("settings.json"),
        database.with_file_name("musicplayer.json"),
        previous_database.with_file_name("settings.json"),
        previous_database.with_file_name("musicplayer.json"),
        legacy_database.with_file_name("settings.json"),
        legacy_database.with_file_name("musicplayer.json"),
    ])
}

fn load_legacy_settings() -> io::Result<Option<Settings>> {
    for path in legacy_settings_paths()? {
        match fs::read(path) {
            Ok(bytes) => {
                return serde_json::from_slice(&bytes)
                    .map(Some)
                    .map_err(|error| io::Error::new(ErrorKind::InvalidData, error));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}
