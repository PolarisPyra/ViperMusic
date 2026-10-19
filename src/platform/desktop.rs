use std::{io, path::Path, process::Command};

/// Open a folder in the desktop's file manager.
///
/// # Arguments
/// * `path` - Local folder to show.
///
/// # Errors
/// Returns an I/O error if the file manager cannot be launched.
pub(crate) fn open_folder(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    command.arg(path).spawn().map(drop)
}
