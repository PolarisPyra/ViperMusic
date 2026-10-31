use super::application::ViperApp;
use crate::features::library::background::WatcherEvent;
use eframe::egui;

impl ViperApp {
    pub(super) fn poll_file_watcher(&mut self, ctx: &egui::Context) {
        let events = self.library.background.poll_file_watcher(
            self.settings.music_path.as_deref(),
            self.library.scanning,
            ctx,
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
