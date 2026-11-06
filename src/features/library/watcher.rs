//! Recursive local filesystem watching for library changes.
use super::is_audio_path;
use crate::shared::repaint::Repaint;
use notify::{
    event::{ModifyKind, RenameMode},
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
};
use thiserror::Error;

/// Watches a local music folder and requests UI repaints for filesystem events.
pub struct FileWatcher {
    // Keeping the watcher alive keeps the recursive OS watch registered.
    _watcher: RecommendedWatcher,
    events: Receiver<notify::Result<Event>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
/// A relevant file or directory change detected by [`FileWatcher`].
pub enum FileChange {
    /// A file or directory was created or changed.
    Upsert(PathBuf),
    /// A file or directory was removed.
    Remove(PathBuf),
}

/// Error returned when a filesystem watcher reports an operating-system failure.
#[derive(Debug, Error)]
pub enum FileWatcherError {
    /// The platform watcher reported an error while polling the event queue.
    #[error("filesystem watcher failed: {0}")]
    Notify(#[from] notify::Error),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ChangeKind {
    Upsert,
    Remove,
    Modify,
}

#[derive(Clone, Copy)]
enum EventChangeKind {
    Create,
    Remove,
    RenameBoth,
    RenameFrom,
    RenameTo,
    RenameAmbiguous,
    Modify,
}

impl FileWatcher {
    /// Start recursively watching a local music folder.
    ///
    /// # Arguments
    /// * `root` - Folder to watch.
    /// * `repaint` - Wakes the application on events.
    ///
    /// # Returns
    /// An active watcher.
    ///
    /// # Errors
    /// Returns a notify error if the watcher cannot be created or registered.
    pub(crate) fn new(root: &Path, repaint: Repaint) -> notify::Result<Self> {
        let (sender, events) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
            repaint.request_repaint();
        })?;
        watcher.watch(root, RecursiveMode::Recursive)?;
        Ok(Self {
            _watcher: watcher,
            events,
        })
    }

    /// Collect queued filesystem events and collapse duplicate paths.
    ///
    /// # Returns
    /// Relevant file changes since the previous poll.
    ///
    /// # Errors
    /// Returns [`FileWatcherError`] if the operating system reports a watcher failure.
    pub fn poll(&self) -> Result<Vec<FileChange>, FileWatcherError> {
        let mut changed = HashSet::new();
        loop {
            match self.events.try_recv() {
                Ok(Ok(event)) => {
                    let kind = match event.kind {
                        EventKind::Create(_) => EventChangeKind::Create,
                        EventKind::Remove(_) => EventChangeKind::Remove,
                        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                            EventChangeKind::RenameFrom
                        }
                        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                            EventChangeKind::RenameBoth
                        }
                        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                            EventChangeKind::RenameTo
                        }
                        EventKind::Modify(ModifyKind::Name(
                            RenameMode::Any | RenameMode::Other,
                        ))
                        | EventKind::Modify(ModifyKind::Any | ModifyKind::Other) => {
                            EventChangeKind::RenameAmbiguous
                        }
                        EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Metadata(_)) => {
                            EventChangeKind::Modify
                        }
                        EventKind::Any | EventKind::Access(_) | EventKind::Other => continue,
                    };
                    for (index, path) in event.paths.into_iter().enumerate() {
                        let path_kind = match kind {
                            EventChangeKind::Create | EventChangeKind::RenameTo => {
                                ChangeKind::Upsert
                            }
                            EventChangeKind::Remove | EventChangeKind::RenameFrom => {
                                ChangeKind::Remove
                            }
                            EventChangeKind::RenameBoth if index == 0 => ChangeKind::Remove,
                            EventChangeKind::RenameBoth => ChangeKind::Upsert,
                            EventChangeKind::RenameAmbiguous if path.exists() => ChangeKind::Upsert,
                            EventChangeKind::RenameAmbiguous => ChangeKind::Remove,
                            EventChangeKind::Modify => ChangeKind::Modify,
                        };
                        let relevant = match path_kind {
                            ChangeKind::Upsert => is_audio_path(&path) || path.is_dir(),
                            ChangeKind::Remove => true,
                            ChangeKind::Modify => is_audio_path(&path),
                        };
                        if relevant {
                            changed.insert((path_kind, path));
                        }
                    }
                }
                Ok(Err(error)) => return Err(FileWatcherError::from(error)),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        Ok(changed
            .into_iter()
            .map(|(kind, path)| match kind {
                ChangeKind::Upsert => FileChange::Upsert(path),
                ChangeKind::Remove => FileChange::Remove(path),
                ChangeKind::Modify => FileChange::Upsert(path),
            })
            .collect())
    }
}
