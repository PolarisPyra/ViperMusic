use super::database;
use rusqlite::{params, types::Type, Connection, OptionalExtension};
use secrecy::{ExposeSecret, SecretString};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
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
    /// Open the list of all songs.
    Songs,
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

fn keyring_entry(service: &str, name: &str) -> io::Result<keyring::Entry> {
    keyring::Entry::new(service, name).map_err(io::Error::other)
}

/// Read a credential, migrating a non-empty legacy SQLite value when the keyring has none.
fn load_secret(entry: &keyring::Entry, legacy_value: Option<&str>) -> io::Result<Option<String>> {
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => {
            let Some(value) = legacy_value.filter(|value| !value.is_empty()) else {
                return Ok(None);
            };
            entry.set_password(value).map_err(io::Error::other)?;
            Ok(Some(value.to_owned()))
        }
        Err(error) => Err(io::Error::other(error)),
    }
}

/// Store a credential, deleting it when the value is absent or empty.
fn save_secret(entry: &keyring::Entry, value: Option<&str>) -> io::Result<()> {
    match value.filter(|value| !value.is_empty()) {
        Some(value) => entry.set_password(value).map_err(io::Error::other),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(io::Error::other(error)),
        },
    }
}

/// Overwrite plaintext credentials in SQLite and flush them from the WAL and free pages.
fn scrub(
    connection: &Connection,
    update: &str,
    parameters: impl rusqlite::Params,
    vacuum: bool,
) -> io::Result<()> {
    connection
        .pragma_update(None, "secure_delete", "ON")
        .map_err(database::database_error)?;
    connection
        .execute(update, parameters)
        .map_err(database::database_error)?;
    connection
        .execute_batch(if vacuum {
            "PRAGMA wal_checkpoint(TRUNCATE); VACUUM;"
        } else {
            "PRAGMA wal_checkpoint(TRUNCATE);"
        })
        .map_err(database::database_error)
}

/// Load saved SMB connection details, migrating any legacy password to the OS credential store.
///
/// # Returns
/// The saved connection, or `None` when no connection is configured.
///
/// # Errors
/// Returns an I/O error if the database or credential store cannot be read or migrated.
pub fn load_samba_settings() -> io::Result<Option<SambaSettings>> {
    let connection = database::open()?;
    let saved = connection
        .query_row(
            "SELECT share_url, username, password, workgroup
             FROM samba_settings WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(database::database_error)?;
    let Some((share_url, username, legacy_password, workgroup)) = saved else {
        return Ok(None);
    };
    let password = load_secret(
        &keyring_entry(SAMBA_KEYRING_SERVICE, "password")?,
        Some(&legacy_password),
    )?
    .unwrap_or_default();
    if !legacy_password.is_empty() {
        scrub(
            &connection,
            "UPDATE samba_settings SET password = '' WHERE id = 1",
            [],
            true,
        )?;
    }
    Ok(Some(SambaSettings {
        share_url,
        username,
        password: SecretString::from(password),
        workgroup,
    }))
}

/// Persist SMB connection details, storing the password in the OS credential store.
///
/// # Arguments
/// * `settings` - Share URL and credentials to save.
///
/// # Errors
/// Returns an I/O error if the database or credential store cannot be updated.
pub fn save_samba_settings(settings: &SambaSettings) -> io::Result<()> {
    save_secret(
        &keyring_entry(SAMBA_KEYRING_SERVICE, "password")?,
        Some(settings.password.expose_secret()),
    )?;
    scrub(
        &database::open()?,
        "INSERT INTO samba_settings (id, share_url, username, password, workgroup)
         VALUES (1, ?1, ?2, '', ?3)
         ON CONFLICT(id) DO UPDATE SET
            share_url=excluded.share_url,
            username=excluded.username,
            password='',
            workgroup=excluded.workgroup",
        params![settings.share_url, settings.username, settings.workgroup],
        false,
    )
}

/// Load Last.fm credentials from the operating system's credential store.
///
/// Legacy plaintext SQLite credentials are migrated to the keyring on first load.
///
/// # Returns
/// The saved credentials and session, or defaults when none are configured.
///
/// # Errors
/// Returns an I/O error if the database or credential store cannot be read or migrated.
pub fn load_lastfm_settings() -> io::Result<LastFmSettings> {
    let connection = database::open()?;
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
        .map_err(database::database_error)?;
    let Some((api_key, api_secret, session_key, username)) = legacy else {
        return Ok(LastFmSettings::default());
    };

    let load = |name, legacy: Option<&str>| {
        load_secret(&keyring_entry(LASTFM_KEYRING_SERVICE, name)?, legacy)
    };
    let settings = LastFmSettings {
        api_key: load("api-key", Some(&api_key))?.unwrap_or_default().into(),
        api_secret: load("api-secret", Some(&api_secret))?
            .unwrap_or_default()
            .into(),
        session_key: load("session-key", session_key.as_deref())?.map(SecretString::from),
        username,
    };
    if !api_key.is_empty() || !api_secret.is_empty() || session_key.is_some() {
        scrub(
            &connection,
            "UPDATE lastfm_settings SET api_key = '', api_secret = '', session_key = NULL
             WHERE id = 1",
            [],
            true,
        )?;
    }
    Ok(settings)
}

/// Persist Last.fm API credentials and the authorized session in the OS keyring.
///
/// # Arguments
/// * `settings` - Credentials, session, and account name to save.
///
/// # Errors
/// Returns an I/O error if the database or credential store cannot be updated.
pub fn save_lastfm_settings(settings: &LastFmSettings) -> io::Result<()> {
    for (name, value) in [
        ("api-key", Some(settings.api_key.expose_secret())),
        ("api-secret", Some(settings.api_secret.expose_secret())),
        (
            "session-key",
            settings
                .session_key
                .as_ref()
                .map(ExposeSecret::expose_secret),
        ),
    ] {
        save_secret(&keyring_entry(LASTFM_KEYRING_SERVICE, name)?, value)?;
    }
    scrub(
        &database::open()?,
        "INSERT INTO lastfm_settings (id, api_key, api_secret, session_key, username)
         VALUES (1, '', '', NULL, ?1)
         ON CONFLICT(id) DO UPDATE SET
            api_key='', api_secret='', session_key=NULL, username=excluded.username",
        [&settings.username],
        false,
    )
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
    /// Relative widths for the Title, Artist, and Album columns in the songs list.
    pub song_list_column_widths: [f32; 3],
    /// Whether album artwork displays audio format badges.
    pub album_format_badges: bool,
    /// Whether the album lyrics panel opens when an album track panel is shown.
    pub lyrics_panel_open: bool,
    /// Interface zoom factor, where 1.0 is the default size.
    pub ui_zoom: f32,
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
            song_list_column_widths: [0.42, 0.29, 0.29],
            album_format_badges: true,
            lyrics_panel_open: false,
            ui_zoom: 1.0,
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
        let connection = database::open()?;
        let stored = connection
            .query_row(
                "SELECT music_path, window_width, window_height, startup_view,
                        left_panel_width, left_panel_hidden, right_panel_width, volume,
                        album_sort, sort_ascending, discord_application_id, album_layout,
                        compact_sidebar, theme, lyrics_panel_open, discord_rich_presence_enabled,
                        gapless_playback, album_list_column_widths, album_format_badges,
                        lastfm_scrobbling_enabled, ui_zoom, song_list_column_widths
                 FROM app_settings WHERE id = 1",
                [],
                |row| {
                    let window_width: Option<f64> = row.get(1)?;
                    let window_height: Option<f64> = row.get(2)?;
                    Ok(Self {
                        music_path: row.get::<_, Option<String>>(0)?.map(PathBuf::from),
                        window_size: window_width
                            .zip(window_height)
                            .map(|(width, height)| [width as f32, height as f32]),
                        startup_view: json_column(row, 3)?,
                        left_panel_width: row.get(4)?,
                        left_panel_hidden: row.get(5)?,
                        right_panel_width: row.get(6)?,
                        volume: row.get(7)?,
                        album_sort: json_column(row, 8)?,
                        sort_ascending: row.get(9)?,
                        discord_application_id: row.get(10)?,
                        album_layout: json_column(row, 11)?,
                        compact_sidebar: row.get(12)?,
                        theme: json_column(row, 13)?,
                        lyrics_panel_open: row.get(14)?,
                        discord_rich_presence_enabled: row.get(15)?,
                        gapless_playback: row.get(16)?,
                        album_list_column_widths: json_column(row, 17)?,
                        album_format_badges: row.get(18)?,
                        lastfm_scrobbling_enabled: row.get(19)?,
                        ui_zoom: row.get(20)?,
                        song_list_column_widths: json_column(row, 21)?,
                        ..Self::default()
                    })
                },
            )
            .optional()
            .map_err(database::database_error)?;
        let first_run = stored.is_none();
        let mut settings = match stored {
            Some(settings) => settings,
            None => load_legacy_settings()?.unwrap_or_default(),
        };

        let legacy_discord_id = settings.discord_application_id.take();
        settings.discord_application_id = load_secret(
            &keyring_entry(DISCORD_KEYRING_SERVICE, "application-id")?,
            legacy_discord_id.as_deref(),
        )?;
        if legacy_discord_id.is_some() {
            scrub(
                &connection,
                "UPDATE app_settings SET discord_application_id = NULL WHERE id = 1",
                [],
                true,
            )?;
        }

        let mut statement = connection
            .prepare(
                "SELECT album_key, favorite, rating, play_count, last_played, added
                 FROM album_state",
            )
            .map_err(database::database_error)?;
        let mut rows = statement.query([]).map_err(database::database_error)?;
        while let Some(row) = rows.next().map_err(database::database_error)? {
            settings
                .insert_album_state(row)
                .map_err(database::database_error)?;
        }

        if first_run {
            settings.save()?;
            settings.save_discord_application_id()?;
            let keys: Vec<_> = settings.album_keys().cloned().collect();
            settings.save_album_state(keys.iter().map(String::as_str))?;
            for path in legacy_settings_paths()? {
                let _ = fs::remove_file(path);
            }
        }
        Ok(settings)
    }

    fn insert_album_state(&mut self, row: &rusqlite::Row<'_>) -> rusqlite::Result<()> {
        let key: String = row.get(0)?;
        let timestamp = |column| {
            row.get::<_, Option<i64>>(column)
                .map(|value| value.and_then(|value| u64::try_from(value).ok()))
        };
        if row.get::<_, bool>(1)? {
            self.favorite_albums.insert(key.clone());
        }
        if let Some(value) = row.get::<_, Option<u8>>(2)? {
            self.album_ratings.insert(key.clone(), value);
        }
        if let Some(value) = timestamp(3)? {
            self.album_play_counts.insert(key.clone(), value);
        }
        if let Some(value) = timestamp(4)? {
            self.album_last_played.insert(key.clone(), value);
        }
        if let Some(value) = timestamp(5)? {
            self.album_added.insert(key, value);
        }
        Ok(())
    }

    /// Save application preferences, excluding per-album state and credentials.
    ///
    /// # Errors
    /// Returns an I/O error if encoding or the database write fails.
    pub fn save(&self) -> io::Result<()> {
        let (window_width, window_height) =
            self.window_size.map_or((None, None), |[width, height]| {
                (Some(f64::from(width)), Some(f64::from(height)))
            });
        database::open()?
            .execute(
                "INSERT INTO app_settings (
                    id, music_path, window_width, window_height, startup_view,
                    left_panel_width, left_panel_hidden, right_panel_width, volume,
                    album_sort, sort_ascending, discord_application_id, album_layout,
                    compact_sidebar, theme, lyrics_panel_open, discord_rich_presence_enabled,
                    gapless_playback, album_list_column_widths, album_format_badges,
                    lastfm_scrobbling_enabled, ui_zoom, song_list_column_widths
                 ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11, ?12, ?13, ?14,
                           ?15, ?16, ?17, ?18, ?19, ?20, ?21)
                 ON CONFLICT(id) DO UPDATE SET
                    music_path=excluded.music_path, window_width=excluded.window_width,
                    window_height=excluded.window_height, startup_view=excluded.startup_view,
                    left_panel_width=excluded.left_panel_width,
                    left_panel_hidden=excluded.left_panel_hidden,
                    right_panel_width=excluded.right_panel_width, volume=excluded.volume,
                    album_sort=excluded.album_sort, sort_ascending=excluded.sort_ascending,
                    discord_application_id=NULL, album_layout=excluded.album_layout,
                    compact_sidebar=excluded.compact_sidebar, theme=excluded.theme,
                    lyrics_panel_open=excluded.lyrics_panel_open,
                    discord_rich_presence_enabled=excluded.discord_rich_presence_enabled,
                    gapless_playback=excluded.gapless_playback,
                    album_list_column_widths=excluded.album_list_column_widths,
                    album_format_badges=excluded.album_format_badges,
                    lastfm_scrobbling_enabled=excluded.lastfm_scrobbling_enabled,
                    ui_zoom=excluded.ui_zoom,
                    song_list_column_widths=excluded.song_list_column_widths",
                params![
                    self.music_path
                        .as_deref()
                        .map(|path| path.to_string_lossy()),
                    window_width,
                    window_height,
                    encode_setting(&self.startup_view)?,
                    self.left_panel_width,
                    self.left_panel_hidden,
                    self.right_panel_width,
                    self.volume,
                    encode_setting(&self.album_sort)?,
                    self.sort_ascending,
                    encode_setting(&self.album_layout)?,
                    self.compact_sidebar,
                    encode_setting(&self.theme)?,
                    self.lyrics_panel_open,
                    self.discord_rich_presence_enabled,
                    self.gapless_playback,
                    encode_setting(&self.album_list_column_widths)?,
                    self.album_format_badges,
                    self.lastfm_scrobbling_enabled,
                    self.ui_zoom,
                    encode_setting(&self.song_list_column_widths)?,
                ],
            )
            .map(drop)
            .map_err(database::database_error)
    }

    /// Persist the Discord application ID in the OS credential store.
    ///
    /// # Errors
    /// Returns an I/O error if the credential store cannot be updated.
    pub fn save_discord_application_id(&self) -> io::Result<()> {
        save_secret(
            &keyring_entry(DISCORD_KEYRING_SERVICE, "application-id")?,
            self.discord_application_id.as_deref(),
        )
    }

    /// Persist favorite, rating, and playback history for the given album keys.
    ///
    /// Albums without any stored state are removed from the database.
    ///
    /// # Arguments
    /// * `keys` - Stable album keys whose in-memory state should be written.
    ///
    /// # Errors
    /// Returns an I/O error if the database transaction fails.
    pub fn save_album_state<'a>(&self, keys: impl IntoIterator<Item = &'a str>) -> io::Result<()> {
        let mut connection = database::open()?;
        let transaction = connection.transaction().map_err(database::database_error)?;
        {
            let mut upsert = transaction
                .prepare(
                    "INSERT INTO album_state
                        (album_key, favorite, rating, play_count, last_played, added)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(album_key) DO UPDATE SET
                        favorite=excluded.favorite, rating=excluded.rating,
                        play_count=excluded.play_count, last_played=excluded.last_played,
                        added=excluded.added",
                )
                .map_err(database::database_error)?;
            let mut delete = transaction
                .prepare("DELETE FROM album_state WHERE album_key = ?1")
                .map_err(database::database_error)?;
            for key in keys {
                let favorite = self.favorite_albums.contains(key);
                let rating = self.album_ratings.get(key).copied();
                let play_count = self.album_play_counts.get(key).copied();
                let last_played = self.album_last_played.get(key).copied();
                let added = self.album_added.get(key).copied();
                let result = if favorite
                    || rating.is_some()
                    || play_count.is_some()
                    || last_played.is_some()
                    || added.is_some()
                {
                    upsert.execute(params![
                        key,
                        favorite,
                        rating,
                        play_count.map(sqlite_integer),
                        last_played.map(sqlite_integer),
                        added.map(sqlite_integer),
                    ])
                } else {
                    delete.execute([key])
                };
                result.map_err(database::database_error)?;
            }
        }
        transaction.commit().map_err(database::database_error)
    }

    fn album_keys(&self) -> impl Iterator<Item = &String> {
        self.favorite_albums
            .iter()
            .chain(self.album_ratings.keys())
            .chain(self.album_play_counts.keys())
            .chain(self.album_last_played.keys())
            .chain(self.album_added.keys())
    }

    /// Return the path to the application SQLite database.
    ///
    /// # Returns
    /// The configured database path.
    ///
    /// # Errors
    /// Returns an I/O error if the configuration directory is unavailable.
    pub fn file_path() -> io::Result<PathBuf> {
        database::database_path()
    }
}

fn encode_setting<T: Serialize>(value: &T) -> io::Result<String> {
    serde_json::to_string(value).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

/// Decode a JSON-encoded setting stored in a text column.
fn json_column<T: DeserializeOwned>(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<T> {
    let value: String = row.get(column)?;
    serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
    })
}

fn sqlite_integer(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn legacy_settings_paths() -> io::Result<[PathBuf; 6]> {
    let database = database::database_path()?;
    let previous_database = database::previous_database_path()?;
    let legacy_database = database::legacy_database_path()?;
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
