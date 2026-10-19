use super::{
    watcher::{FileChange, FileWatcher},
    DiscoveredTracks, Library, ScanProgress,
};
use eframe::egui;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct LibraryBackgroundTasks {
    pub(crate) scan_receiver: Option<Receiver<Library>>,
    scan_cancel: Option<Arc<AtomicBool>>,
    artwork_receiver: Option<Receiver<(usize, Arc<[u8]>)>>,
    artwork_cancel: Option<Arc<AtomicBool>>,
    pub(crate) file_watcher: Option<FileWatcher>,
    pub(crate) pending_watch_paths: HashSet<PathBuf>,
    pub(crate) watch_debounce_until: Option<Instant>,
    pub(crate) watch_scan_receiver: Option<Receiver<(DiscoveredTracks, Vec<PathBuf>)>>,
    watch_scan_cancel: Option<Arc<AtomicBool>>,
    pub(crate) watcher_root: Option<PathBuf>,
    pub(crate) pending_removed_paths: HashSet<PathBuf>,
}

pub(crate) enum WatcherEvent {
    Updated(DiscoveredTracks, Vec<PathBuf>),
    Error(String),
}

impl LibraryBackgroundTasks {
    pub(crate) fn start_artwork_scan(
        &mut self,
        paths: Vec<(usize, PathBuf)>,
    ) -> Result<(), String> {
        self.cancel_artwork_scan();
        if paths.is_empty() {
            return Ok(());
        }
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        std::thread::Builder::new()
            .name("music-library-artwork".into())
            .spawn(move || {
                load_artwork_paths(&paths, &worker_cancel, &sender);
            })
            .map_err(|error| format!("Could not start album artwork loading: {error}"))?;
        self.artwork_receiver = Some(receiver);
        self.artwork_cancel = Some(cancel);
        Ok(())
    }

    pub(crate) fn poll_artwork(&mut self, ctx: &egui::Context) -> Vec<(usize, Arc<[u8]>)> {
        let mut artwork = Vec::new();
        let mut finished = false;
        if let Some(receiver) = &self.artwork_receiver {
            loop {
                match receiver.try_recv() {
                    Ok(item) => artwork.push(item),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
        }
        if finished {
            self.artwork_receiver = None;
            self.artwork_cancel = None;
        } else if self.artwork_receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
        artwork
    }

    pub(crate) fn poll_file_watcher(
        &mut self,
        root: Option<&Path>,
        scanning: bool,
        ctx: &egui::Context,
    ) -> Vec<WatcherEvent> {
        let mut events = Vec::new();
        let root = root.filter(|root| !super::is_smb_path(root));
        if let Some(root) = root {
            if self.watcher_root.as_deref() != Some(root) {
                self.file_watcher = None;
                match FileWatcher::new(root, ctx.clone()) {
                    Ok(watcher) => {
                        self.file_watcher = Some(watcher);
                        self.watcher_root = Some(root.to_owned());
                    }
                    Err(error) => {
                        self.watcher_root = Some(root.to_owned());
                        events.push(WatcherEvent::Error(format!(
                            "Could not watch music folder: {error}"
                        )));
                    }
                }
            }
        } else {
            self.file_watcher = None;
            self.watcher_root = None;
        }

        if let Some(watcher) = &self.file_watcher {
            match watcher.poll() {
                Ok(paths) if !paths.is_empty() => {
                    for change in paths {
                        match change {
                            FileChange::Upsert(path) => {
                                self.pending_watch_paths.insert(path);
                            }
                            FileChange::Remove(path) => {
                                self.pending_removed_paths.insert(path);
                            }
                        }
                    }
                    self.watch_debounce_until = Some(Instant::now() + Duration::from_millis(800));
                }
                Ok(_) => {}
                Err(error) => {
                    events.push(WatcherEvent::Error(format!(
                        "Music folder watcher error: {error}"
                    )));
                    self.file_watcher = None;
                }
            }
        }

        let finished = if let Some(receiver) = &self.watch_scan_receiver {
            match receiver.try_recv() {
                Ok((discovered, removed)) => {
                    events.push(WatcherEvent::Updated(discovered, removed));
                    true
                }
                Err(TryRecvError::Disconnected) => {
                    events.push(WatcherEvent::Error(
                        "The music folder update scan stopped unexpectedly".into(),
                    ));
                    true
                }
                Err(TryRecvError::Empty) => false,
            }
        } else {
            false
        };
        if finished {
            self.finish_watch_scan();
        }

        if !scanning
            && self.watch_scan_receiver.is_none()
            && (!self.pending_watch_paths.is_empty() || !self.pending_removed_paths.is_empty())
            && self
                .watch_debounce_until
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            let paths = self.pending_watch_paths.drain().collect();
            let removed_paths = self.pending_removed_paths.drain().collect();
            self.watch_debounce_until = None;
            if let Err(error) = self.start_watch_scan(paths, removed_paths) {
                events.push(WatcherEvent::Error(error));
            }
        }

        if !self.pending_watch_paths.is_empty()
            || !self.pending_removed_paths.is_empty()
            || self.watch_scan_receiver.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        events
    }

    pub(crate) fn start_watch_scan(
        &mut self,
        paths: Vec<PathBuf>,
        removed_paths: Vec<PathBuf>,
    ) -> Result<(), String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("music-library-watch-scan".into())
            .spawn(move || {
                let discovered = super::scan_added_tracks(&paths, &worker_cancel);
                let _ = sender.send((discovered, removed_paths));
            })
            .map_err(|error| format!("Could not update music library: {error}"))?;
        self.set_watch_scan(receiver, cancel);
        Ok(())
    }

    pub(crate) fn start_scan(
        &mut self,
        root: PathBuf,
        #[cfg(target_os = "linux")] smb_auth: Option<super::smb::SmbAuth>,
    ) -> Result<Arc<ScanProgress>, String> {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let progress = Arc::new(ScanProgress::default());
        let worker_progress = Arc::clone(&progress);
        self.replace_scan(receiver, cancel);

        if let Err(error) = std::thread::Builder::new()
            .name("music-library-scan".into())
            .spawn(move || {
                #[cfg(target_os = "linux")]
                let library = super::scan_library_with_auth(
                    &root,
                    &worker_cancel,
                    &worker_progress,
                    smb_auth,
                );
                #[cfg(not(target_os = "linux"))]
                let library = super::scan_library(&root, &worker_cancel, &worker_progress);
                let _ = sender.send(library);
            })
        {
            self.finish_scan();
            return Err(format!("Could not start library scan: {error}"));
        }
        Ok(progress)
    }

    pub(crate) fn poll_scan(&mut self) -> Option<Result<Library, String>> {
        let receiver = self.scan_receiver.as_ref()?;
        let result = match receiver.try_recv() {
            Ok(library) => Some(Ok(library)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("The library scan stopped unexpectedly".into()))
            }
        };
        if result.is_some() {
            self.finish_scan();
        }
        result
    }

    pub(crate) fn replace_scan(&mut self, receiver: Receiver<Library>, flag: Arc<AtomicBool>) {
        cancel(&mut self.scan_cancel);
        self.scan_cancel = Some(flag);
        self.scan_receiver = Some(receiver);
    }

    pub(crate) fn set_watch_scan(
        &mut self,
        receiver: Receiver<(DiscoveredTracks, Vec<PathBuf>)>,
        cancel: Arc<AtomicBool>,
    ) {
        self.watch_scan_receiver = Some(receiver);
        self.watch_scan_cancel = Some(cancel);
    }

    pub(crate) fn finish_scan(&mut self) {
        self.scan_receiver = None;
        self.scan_cancel = None;
    }

    pub(crate) fn finish_watch_scan(&mut self) {
        self.watch_scan_receiver = None;
        self.watch_scan_cancel = None;
    }

    pub(crate) fn cancel_all(&mut self) {
        cancel(&mut self.scan_cancel);
        cancel(&mut self.watch_scan_cancel);
        self.cancel_artwork_scan();
    }

    pub(crate) fn clear_watch_state(&mut self) {
        cancel(&mut self.watch_scan_cancel);
        self.watch_scan_receiver = None;
        self.file_watcher = None;
        self.watcher_root = None;
        self.pending_watch_paths.clear();
        self.pending_removed_paths.clear();
        self.watch_debounce_until = None;
    }

    pub(crate) fn cancel_artwork_scan(&mut self) {
        cancel(&mut self.artwork_cancel);
        self.artwork_receiver = None;
    }
}

/// Signal a background worker to stop and forget its cancellation flag.
fn cancel(flag: &mut Option<Arc<AtomicBool>>) {
    if let Some(flag) = flag.take() {
        flag.store(true, Ordering::Relaxed);
    }
}

fn load_artwork_paths(
    paths: &[(usize, PathBuf)],
    cancel: &AtomicBool,
    sender: &Sender<(usize, Arc<[u8]>)>,
) {
    use rayon::prelude::*;

    paths.par_iter().for_each(|(album_index, path)| {
        if cancel.load(Ordering::Relaxed) || super::is_smb_path(path) {
            return;
        }
        if let Some(artwork) = super::scanner::load_track_artwork(path) {
            let _ = sender.send((*album_index, Arc::from(artwork)));
        }
    });
}

impl Drop for LibraryBackgroundTasks {
    fn drop(&mut self) {
        self.cancel_all();
    }
}
