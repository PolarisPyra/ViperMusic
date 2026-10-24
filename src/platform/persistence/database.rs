use base64::Engine;
use rusqlite::{Connection, Transaction};
use std::{env, fs, io, path::PathBuf, sync::Mutex, time::Duration};

/// Schema migrations applied before the legacy SMB password step, keyed by target version.
const EARLY_MIGRATIONS: [(i64, &str); 7] = [
    (1, include_str!("migrations/0001_initial.sql")),
    (
        2,
        include_str!("migrations/0002_discord_application_id.sql"),
    ),
    (3, include_str!("migrations/0003_album_layout.sql")),
    (4, include_str!("migrations/0004_samba_settings.sql")),
    (5, include_str!("migrations/0005_smb_cache.sql")),
    (6, include_str!("migrations/0006_compact_sidebar.sql")),
    (7, include_str!("migrations/0007_theme.sql")),
];

/// Schema migrations applied after the legacy SMB password step, keyed by target version.
const LATE_MIGRATIONS: [(i64, &str); 7] = [
    (10, include_str!("migrations/0010_lyrics_panel_open.sql")),
    (
        11,
        include_str!("migrations/0011_discord_rich_presence_enabled.sql"),
    ),
    (
        12,
        include_str!("migrations/0012_audio_presence_controls.sql"),
    ),
    (13, include_str!("migrations/0013_shuffle_scope.sql")),
    (
        14,
        include_str!("migrations/0014_album_list_column_widths.sql"),
    ),
    (15, include_str!("migrations/0015_album_format_badges.sql")),
    (16, include_str!("migrations/0016_lastfm.sql")),
];

/// Column additions applied whenever the column is missing, keyed by table and column.
///
/// Development builds from other branches may have advanced `user_version` past these steps
/// with different schema changes, so their presence is checked directly instead of by version.
const COLUMN_MIGRATIONS: [(&str, &str, &str); 3] = [
    (
        "app_settings",
        "ui_zoom",
        include_str!("migrations/0017_ui_zoom.sql"),
    ),
    (
        "app_settings",
        "song_list_column_widths",
        include_str!("migrations/0018_song_list_column_widths.sql"),
    ),
    (
        "app_settings",
        "logo",
        include_str!("migrations/0019_logo.sql"),
    ),
];

/// Whether the database file has been located, migrated, and switched to WAL in this process.
static INITIALIZED: Mutex<bool> = Mutex::new(false);

pub(crate) fn database_path() -> io::Result<PathBuf> {
    Ok(config_directory()?
        .join("viper-music")
        .join("viper.sqlite3"))
}

pub(crate) fn previous_database_path() -> io::Result<PathBuf> {
    Ok(config_directory()?.join("viper").join("viper.sqlite3"))
}

pub(crate) fn legacy_database_path() -> io::Result<PathBuf> {
    Ok(config_directory()?
        .join("musicplayer")
        .join("musicplayer.sqlite3"))
}

fn config_directory() -> io::Result<PathBuf> {
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"))?;
    Ok(home.join(".config"))
}

fn parent_directory(path: &std::path::Path) -> io::Result<&std::path::Path> {
    path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "database path has no parent directory",
        )
    })
}

/// Open the application database, enabling per-connection pragmas.
///
/// The first call in a process also migrates legacy locations, applies pending schema migrations,
/// and enables WAL; later calls only open a connection.
///
/// # Returns
/// A ready SQLite connection.
///
/// # Errors
/// Returns an I/O error if the database cannot be opened or migrated.
pub(crate) fn open() -> io::Result<Connection> {
    let path = database_path()?;
    let mut initialized = INITIALIZED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !*initialized {
        prepare_database_file(&path)?;
    }
    let mut connection = Connection::open(&path).map_err(database_error)?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(database_error)?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(database_error)?;
    if !*initialized {
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(database_error)?;
        migrate(&mut connection)?;
        *initialized = true;
    }
    Ok(connection)
}

/// Move or copy databases from earlier application names to the current location.
fn prepare_database_file(path: &std::path::Path) -> io::Result<()> {
    let current_directory = parent_directory(path)?;
    let previous_path = previous_database_path()?;
    let previous_directory = parent_directory(&previous_path)?;
    if !current_directory.exists() && previous_directory.exists() {
        fs::rename(previous_directory, current_directory)?;
    }
    fs::create_dir_all(current_directory)?;
    if path.exists() {
        return Ok(());
    }
    let legacy_path = legacy_database_path()?;
    let source_path = [previous_path, legacy_path]
        .into_iter()
        .find(|candidate| candidate.exists());
    if let Some(source_path) = source_path {
        Connection::open(source_path)
            .and_then(|legacy| legacy.backup("main", path, None))
            .map_err(database_error)?;
    }
    Ok(())
}

fn migrate(connection: &mut Connection) -> io::Result<()> {
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(database_error)?;
    for (target, schema) in EARLY_MIGRATIONS {
        if version < target {
            apply(connection, target, |transaction| {
                transaction.execute_batch(schema)
            })?;
        }
    }
    // Version 8 stored the SMB password outside SQLite; earlier schemas already have the column.
    if version == 8 {
        let legacy_password = load_legacy_smb_password()?;
        apply(connection, 9, |transaction| {
            transaction.execute_batch(include_str!("migrations/0009_smb_password_database.sql"))?;
            if let Some(password) = &legacy_password {
                transaction.execute(
                    "UPDATE samba_settings SET password = ?1 WHERE id = 1",
                    [password],
                )?;
            }
            Ok(())
        })?;
        remove_legacy_smb_password();
    } else if version < 8 {
        apply(connection, 9, |_| Ok(()))?;
    }
    for (target, schema) in LATE_MIGRATIONS {
        if version < target {
            apply(connection, target, |transaction| {
                transaction.execute_batch(schema)
            })?;
        }
    }
    for (table, column, schema) in COLUMN_MIGRATIONS {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
                [table, column],
                |row| row.get(0),
            )
            .map_err(database_error)?;
        if !exists {
            connection.execute_batch(schema).map_err(database_error)?;
        }
    }
    Ok(())
}

/// Run one migration step and record its schema version atomically.
fn apply(
    connection: &mut Connection,
    version: i64,
    step: impl FnOnce(&Transaction<'_>) -> rusqlite::Result<()>,
) -> io::Result<()> {
    let transaction = connection.transaction().map_err(database_error)?;
    step(&transaction).map_err(database_error)?;
    transaction
        .pragma_update(None, "user_version", version)
        .map_err(database_error)?;
    transaction.commit().map_err(database_error)
}

fn env_file_path() -> io::Result<PathBuf> {
    Ok(parent_directory(&database_path()?)?.join(".env"))
}

fn load_legacy_smb_password() -> io::Result<Option<String>> {
    if let Some(password) = env::var_os("VIPER_SMB_PASSWORD") {
        return password.into_string().map(Some).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "SMB password environment variable is not valid UTF-8",
            )
        });
    }
    let env_path = env_file_path()?;
    if env_path.exists() {
        dotenvy::from_path(&env_path)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    }
    let Some(encoded) = env::var_os("VIPER_SMB_PASSWORD_B64") else {
        return Ok(None);
    };
    let encoded = encoded.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "SMB password is not valid UTF-8",
        )
    })?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn remove_legacy_smb_password() {
    if let Ok(path) = env_file_path() {
        let _ = fs::remove_file(path);
    }
    env::remove_var("VIPER_SMB_PASSWORD");
    env::remove_var("VIPER_SMB_PASSWORD_B64");
}

pub(crate) fn database_error(error: rusqlite::Error) -> io::Error {
    io::Error::other(error)
}
