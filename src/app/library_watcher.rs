use super::application::ViperApp;
use crate::features::library::background::WatcherEvent;

impl ViperApp {
    pub(super) fn poll_file_watcher(&mut self) {
        let events = self.library.background.poll_file_watcher(
            self.settings.music_path.as_deref(),
            self.library.scanning,
            &self.repaint,
        );
        for event in events {
            match event {
                WatcherEvent::Error(error) => self.toasts.push(error),
                WatcherEvent::Updated(discovered, removed_paths) => {
                    let update = self
                        .library
                        .apply_watcher_update(discovered, &removed_paths);
                    self.playback()
                        .remap_tracks(&update.track_remap, &self.library.model.tracks);
                    // The playing track moving to another index is not a new play.
                    self.last_observed_track = self
                        .last_observed_track
                        .and_then(|index| update.track_remap.get(index).copied().flatten());
                    #[cfg(target_os = "linux")]
                    self.sync_mpris();
                    if update.selected_album_cleared {
                        self.workbench.show_album_details = false;
                    }
                    if update.added_tracks > 0 || update.removed_any {
                        self.record_new_albums();
                    }
                }
            }
        }
    }
}
