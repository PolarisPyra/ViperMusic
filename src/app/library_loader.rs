use super::application::ViperApp;
use crate::features::library::state::ScanCompletion;
use std::path::PathBuf;

impl ViperApp {
    pub(super) fn start_scan(&mut self, root: PathBuf) {
        self.playback().stop();
        self.last_observed_track = None;
        self.artwork_cache.clear();

        if let Err(error) = self.library.start_scan(
            root,
            #[cfg(target_os = "linux")]
            self.smb_auth.clone(),
        ) {
            self.toasts.push(error);
        }
        #[cfg(target_os = "linux")]
        self.sync_mpris();
    }

    pub(super) fn poll_scan(&mut self) -> bool {
        let Some(completion) = self.library.poll_scan() else {
            return false;
        };
        #[cfg(target_os = "linux")]
        self.sync_mpris();
        match completion {
            ScanCompletion::Loaded { warning } => {
                if let Some(warning) = warning {
                    self.toasts.push(warning);
                }
                self.record_new_albums();
            }
            ScanCompletion::Failed(error) => self.toasts.push(error),
        }
        true
    }
}
