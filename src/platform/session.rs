//! Notices when the desktop session the app started in has ended.

#[cfg(unix)]
use std::{env, fs, os::unix::fs::MetadataExt, path::PathBuf};

/// The display server socket the app connected to at startup.
///
/// Logging out removes the socket, and logging back in creates a new one, so a process that
/// outlives its session, such as one hidden to the tray while the user's services linger, can tell
/// that its display connection is dead.
pub(crate) struct DisplaySocket {
    /// Socket path with its device and inode at startup, or `None` for an unknown display.
    #[cfg(unix)]
    socket: Option<(PathBuf, (u64, u64))>,
}

/// Device and inode of the file at `path`.
#[cfg(unix)]
fn identity(path: &PathBuf) -> Option<(u64, u64)> {
    fs::metadata(path)
        .ok()
        .map(|metadata| (metadata.dev(), metadata.ino()))
}

/// Path of the Wayland socket, or of the local X11 socket when Wayland is not in use.
#[cfg(unix)]
fn socket_path() -> Option<PathBuf> {
    if let Some(display) = env::var_os("WAYLAND_DISPLAY") {
        let display = PathBuf::from(display);
        return if display.is_absolute() {
            Some(display)
        } else {
            env::var_os("XDG_RUNTIME_DIR").map(|runtime| PathBuf::from(runtime).join(display))
        };
    }
    let display = env::var("DISPLAY").ok()?;
    let number = display.strip_prefix(':')?.split('.').next()?;
    Some(PathBuf::from(format!("/tmp/.X11-unix/X{number}")))
}

impl DisplaySocket {
    /// Record the display socket the app is about to connect to.
    ///
    /// # Returns
    /// The socket identity; it never reports the session as ended if the socket cannot be found.
    pub(crate) fn current() -> Self {
        #[cfg(unix)]
        return Self {
            socket: socket_path().and_then(|path| {
                let saved = identity(&path)?;
                Some((path, saved))
            }),
        };
        #[cfg(not(unix))]
        Self {}
    }

    /// Whether the display socket is gone or belongs to a newer session.
    ///
    /// # Returns
    /// `true` once the session the app started in has ended.
    pub(crate) fn is_gone(&self) -> bool {
        #[cfg(unix)]
        return self
            .socket
            .as_ref()
            .is_some_and(|(path, saved)| identity(path) != Some(*saved));
        #[cfg(not(unix))]
        false
    }
}
