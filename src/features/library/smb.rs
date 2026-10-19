//! Direct SMB access through Pavão. Library paths are stored as `smb://` URLs;
//! Credentials use redacted secret wrappers while held in application memory.

use pavao::{SmbClient, SmbCredentials, SmbDirentType, SmbOpenOptions, SmbOptions};
use secrecy::{ExposeSecret, SecretString};
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::UNIX_EPOCH,
};
use thiserror::Error;

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Default)]
pub(crate) struct SmbAuth {
    pub username: String,
    pub password: SecretString,
    pub workgroup: String,
}

/// Failures that can occur while validating or accessing an SMB share.
#[derive(Debug, Error)]
pub(crate) enum SmbAccessError {
    /// The URL does not include a valid server and share.
    #[error("Enter an SMB server and share, for example smb://server/Music")]
    InvalidUrl,
    /// Credentials were embedded in the URL instead of supplied separately.
    #[error("Enter the server and share without embedding credentials")]
    CredentialsInUrl,
    /// An SMB path could not be represented as UTF-8.
    #[error("SMB path is not valid UTF-8")]
    InvalidPathEncoding,
    /// The SMB library rejected a client or remote operation.
    #[error("SMB operation failed: {0}")]
    Native(#[from] pavao::SmbError),
    /// A local staging operation failed.
    #[error("Local SMB staging failed: {0}")]
    Io(#[from] io::Error),
}

struct SharePath {
    server: String,
    share: String,
    path: String,
}

impl SharePath {
    fn parse(url: &str) -> Result<Self, SmbAccessError> {
        let remainder = url
            .strip_prefix("smb://")
            .ok_or(SmbAccessError::InvalidUrl)?;
        let (server, path) = remainder
            .split_once('/')
            .ok_or(SmbAccessError::InvalidUrl)?;
        let (share, path) = path.split_once('/').unwrap_or((path, ""));
        if server.is_empty() || share.is_empty() || server.contains('@') {
            return Err(SmbAccessError::CredentialsInUrl);
        }
        Ok(Self {
            server: server.to_owned(),
            share: format!("/{share}"),
            path: format!("/{path}"),
        })
    }

    fn client(&self, auth: &SmbAuth) -> Result<SmbClient, SmbAccessError> {
        Ok(SmbClient::new(
            SmbCredentials::default()
                .server(format!("smb://{}", self.server))
                .share(&self.share)
                .username(&auth.username)
                .password(auth.password.expose_secret())
                .workgroup(&auth.workgroup),
            SmbOptions::default(),
        )?)
    }
}

pub(crate) struct SmbSession {
    client: SmbClient,
}

impl SmbSession {
    pub(crate) fn new(root: &str, auth: &SmbAuth) -> Result<Self, SmbAccessError> {
        let share = SharePath::parse(root)?;
        Ok(Self {
            client: share.client(auth)?,
        })
    }

    pub(super) fn read_metadata(
        &self,
        path: &Path,
    ) -> Result<super::tag_reader::TrackMetadata, super::tag_reader::ReaderMetadataError> {
        let parsed = SharePath::parse(path.to_str().ok_or_else(|| {
            super::tag_reader::ReaderMetadataError::Failed(
                SmbAccessError::InvalidPathEncoding.to_string(),
            )
        })?)
        .map_err(|error| super::tag_reader::ReaderMetadataError::Failed(error.to_string()))?;
        let remote = self
            .client
            .open_with(&parsed.path, SmbOpenOptions::default().read(true))
            .map_err(|error| super::tag_reader::ReaderMetadataError::Failed(error.to_string()))?;
        super::tag_reader::read_metadata_from_reader(
            super::smb_reader::SmbReader::new(remote),
            path,
        )
    }

    pub(crate) fn stage_file(&self, path: &Path) -> Result<PathBuf, SmbAccessError> {
        let parsed = SharePath::parse(path.to_str().ok_or(SmbAccessError::InvalidPathEncoding)?)?;
        let mut remote = self
            .client
            .open_with(&parsed.path, SmbOpenOptions::default().read(true))?;
        let name = Path::new(&parsed.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("track.audio");
        let temp_dir = std::env::temp_dir().join(format!(
            "viper-smb-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&temp_dir)?;
        let temp = temp_dir.join(name);
        let mut local = File::create(&temp)?;
        if let Err(error) = io::copy(&mut remote, &mut local) {
            let _ = std::fs::remove_file(&temp);
            let _ = std::fs::remove_dir(&temp_dir);
            return Err(SmbAccessError::Io(error));
        }
        Ok(temp)
    }
}

pub(crate) fn validate_url(url: &str) -> Result<(), SmbAccessError> {
    SharePath::parse(url).map(|_| ())
}

pub(crate) struct SmbEntry {
    pub path: PathBuf,
    pub fingerprint: Option<String>,
}

pub(crate) fn scan(
    root: &str,
    auth: &SmbAuth,
    cancel: &AtomicBool,
) -> Result<Vec<SmbEntry>, SmbAccessError> {
    let share = SharePath::parse(root)?;
    let client = share.client(auth)?;
    let mut files = Vec::new();
    let mut directories = vec![share.path.clone()];
    while let Some(directory) = directories.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(Vec::new());
        }
        // Get validation data in the directory listing rather than statting each file.
        // Older servers can still scan without caching if readdirplus is unavailable.
        let entries: Vec<_> = match client.list_dirplus(&directory) {
            Ok(entries) => entries
                .into_iter()
                .map(|entry| {
                    let fingerprint = entry
                        .mtime
                        .duration_since(UNIX_EPOCH)
                        .ok()
                        .filter(|modified| !modified.is_zero())
                        .map(|modified| {
                            format!(
                                "{}:{}:{:?}",
                                entry.size,
                                modified.as_nanos(),
                                entry
                                    .ctime
                                    .duration_since(UNIX_EPOCH)
                                    .ok()
                                    .map(|time| time.as_nanos())
                            )
                        });
                    (entry.name().to_owned(), entry.get_type(), fingerprint)
                })
                .collect(),
            Err(_) => client
                .list_dir(&directory)
                .map_err(SmbAccessError::from)?
                .into_iter()
                .map(|entry| (entry.name().to_owned(), entry.get_type(), None))
                .collect(),
        };
        for (name, kind, fingerprint) in entries {
            let name = name.as_str();
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }
            let path = format!("{}/{}", directory.trim_end_matches('/'), name);
            match kind {
                SmbDirentType::Dir => directories.push(path),
                SmbDirentType::File
                    if crate::features::library::is_audio_path(std::path::Path::new(name)) =>
                {
                    files.push(SmbEntry {
                        path: PathBuf::from(format!(
                            "smb://{}/{}/{}",
                            share.server,
                            share.share.trim_start_matches('/'),
                            path.trim_start_matches('/')
                        )),
                        fingerprint,
                    });
                }
                SmbDirentType::Workgroup
                | SmbDirentType::Server
                | SmbDirentType::FileShare
                | SmbDirentType::PrinterShare
                | SmbDirentType::CommsShare
                | SmbDirentType::IpcShare
                | SmbDirentType::Link
                | SmbDirentType::File => {}
            }
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

pub(crate) fn stage_file(path: &Path, auth: &SmbAuth) -> Result<PathBuf, SmbAccessError> {
    let session = SmbSession::new(
        path.to_str().ok_or(SmbAccessError::InvalidPathEncoding)?,
        auth,
    )?;
    session.stage_file(path)
}
