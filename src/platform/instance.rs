//! Lets a second launch ask the running instance to show its window.

#[cfg(unix)]
use std::{
    io,
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
};

/// Socket the running instance listens on, private to the user's runtime directory.
#[cfg(unix)]
fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("viper-music.sock")
}

/// Ask the running instance to show its window.
///
/// # Errors
/// Returns an I/O error if no instance is listening.
#[cfg(unix)]
pub fn show_running() -> io::Result<()> {
    UnixStream::connect(socket_path()).map(drop)
}

/// Ask the running instance to show its window.
///
/// # Errors
/// Never fails; other launches are not forwarded on this platform.
#[cfg(not(unix))]
pub fn show_running() -> std::io::Result<()> {
    Ok(())
}

/// Call `on_launch` from a background thread each time another launch connects.
///
/// Only call this while holding the single-instance lock, since it replaces any stale socket.
///
/// # Arguments
/// * `on_launch` - Called once per later launch.
#[cfg(unix)]
pub(crate) fn listen(on_launch: impl Fn() + Send + 'static) {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("Cannot listen for other launches: {error}");
            return;
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if stream.is_ok() {
                on_launch();
            }
        }
    });
}

/// Call `on_launch` from a background thread each time another launch connects.
///
/// # Arguments
/// * `on_launch` - Unused; other launches are not forwarded on this platform.
#[cfg(not(unix))]
pub(crate) fn listen(_on_launch: impl Fn() + Send + 'static) {}
