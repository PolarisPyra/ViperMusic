use super::application::ViperApp;
use crate::{
    platform::updates::UpdateAction, shared::action::AppAction, workbench::WorkbenchContext,
};
use eframe::egui;
use std::{sync::Arc, time::Duration};

impl Default for ViperApp {
    fn default() -> Self {
        Self::new()
    }
}

impl ViperApp {
    /// Hand the library to the playback driver, and report playback to toasts, Last.fm, and
    /// Discord. The driver thread advances tracks, so this needs no repaints of its own.
    fn update_playback(&mut self) {
        let playback_handle = Arc::clone(&self.playback);
        let mut playback = playback_handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        playback.sync_library(&self.library.model.tracks, self.settings.gapless_playback);
        if let Some(error) = playback.error.take() {
            self.toasts.push(error);
        }
        self.observe_playback_track(&playback);
        if self.lastfm_service.poll(&mut self.lastfm_settings) {
            if let Err(error) =
                crate::platform::persistence::settings::save_lastfm_settings(&self.lastfm_settings)
            {
                self.toasts
                    .push(format!("Could not save Last.fm session: {error}"));
            }
        }
        let current = playback
            .current
            .and_then(|index| Some((index, self.library.model.tracks.get(index)?)));
        self.lastfm_service.update_playback(
            self.settings.lastfm_scrobbling_enabled,
            &self.lastfm_settings,
            current,
            playback.position(),
        );
        self.discord_presence.update(
            self.settings
                .discord_rich_presence_enabled
                .then_some(self.settings.discord_application_id.as_deref())
                .flatten(),
            current,
            playback.is_paused(),
            playback.position(),
        );
    }

    /// Keep playback, scrobbling, and presence running while the window is closed to the tray.
    pub(crate) fn tick_in_background(&mut self) {
        self.update_playback();
    }

    /// Apply the selected color way to the window and tray icons when it has changed.
    fn sync_logo(&mut self, ctx: &egui::Context) {
        let colorway = self.settings.logo;
        if self.applied_logo == Some(colorway) {
            return;
        }
        #[cfg(target_os = "linux")]
        let changed_while_open = self.applied_logo.is_some();
        self.applied_logo = Some(colorway);
        let logo = colorway.decode();
        ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(Arc::new(
            crate::shared::logo::window_icon(&logo),
        ))));
        if let Some(tray) = &self.tray {
            if let Err(error) = tray.set_logo(&logo) {
                eprintln!("System tray icon could not change: {error}");
            }
        }
        #[cfg(target_os = "linux")]
        match crate::shared::logo::install_launcher_icon(colorway) {
            // HACK: see `install_launcher_icon`. The desktop reads the launcher icon when a window
            // opens, so the window reopens only after the taskbar has loaded the rebuilt application
            // cache. Plasma's taskbar took about half a second, and keeps the old icon if the window
            // reopens before then. Only the latest change reopens the window, so a quick series of
            // picks cannot reopen it while the taskbar is still loading a later one.
            Ok(true) => {
                use std::sync::atomic::Ordering;

                let change = self.logo_changes.fetch_add(1, Ordering::Relaxed) + 1;
                let logo_changes = Arc::clone(&self.logo_changes);
                let reopen_window = Arc::clone(&self.reopen_window);
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    crate::shared::logo::refresh_launcher_cache();
                    if changed_while_open {
                        std::thread::sleep(Duration::from_millis(1500));
                        if logo_changes.load(Ordering::Relaxed) == change {
                            reopen_window.store(true, Ordering::Relaxed);
                            ctx.request_repaint();
                        }
                    }
                });
            }
            Ok(false) => {}
            Err(error) => eprintln!("Launcher icon could not change: {error}"),
        }
    }

    fn poll_artwork(&mut self, ctx: &egui::Context) {
        let mut updated = false;
        for (album_index, artwork) in self.library.background.poll_artwork(ctx) {
            if let Some(album) = self.library.model.albums.get_mut(album_index) {
                album.art = Some(artwork);
                updated = true;
            }
        }
        #[cfg(target_os = "linux")]
        if updated {
            self.sync_mpris();
        }
        #[cfg(not(target_os = "linux"))]
        let _ = updated;
    }

    fn handle_action(&mut self, action: AppAction) {
        match action {
            AppAction::ChooseMusicFolder => self.choose_folder(),
            AppAction::SaveSettings => self.save_settings(),
            AppAction::SaveDiscordApplicationId => self.save_discord_application_id(),
            AppAction::ConnectSmbShare => {
                #[cfg(target_os = "linux")]
                self.connect_smb_share();
            }
            AppAction::SaveLastFmSettings => self.save_lastfm_settings_from_preferences(),
            AppAction::StartLastFmAuth => {
                self.save_lastfm_settings_from_preferences();
                self.lastfm_service.start_auth(&self.lastfm_settings);
            }
            AppAction::FinishLastFmAuth => self.lastfm_service.finish_auth(&self.lastfm_settings),
            AppAction::DisconnectLastFm => self.disconnect_lastfm(),
            AppAction::CheckForUpdates => self.updater.check_now(),
            AppAction::RestartToUpdate => self.restart_requested = true,
        }
    }

    /// Quit so `main` starts the installed update, when one is installed.
    fn restart_to_update(&mut self, ctx: &egui::Context) {
        if self.updater.restart() {
            self.workbench.quit_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for ViperApp {
    // Also runs while the window is minimized or occluded, so playback keeps advancing.
    // Input is stale then, so anything reading it belongs in `ui`.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.poll_scan() {
            self.artwork_cache.clear();
        }
        self.poll_artwork(ctx);
        self.sync_logo(ctx);
        self.poll_file_watcher(ctx);
        self.artwork_cache.poll(ctx);
        if self.library.scanning {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        self.update_playback();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        self.observe_window_size(ctx);
        crate::shared::ui::theme::apply(ctx, self.settings.theme);
        self.handle_shortcuts(ctx);
        self.artwork_cache.begin_frame();

        self.preferences.update_state = self.updater.state(ctx);
        let playback_handle = Arc::clone(&self.playback);
        let mut playback = playback_handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let actions = crate::workbench::show(
            ui,
            &mut WorkbenchContext {
                state: &mut self.workbench,
                library: &mut self.library,
                playback: &mut playback,
                lyrics: &mut self.lyrics,
                settings: &mut self.settings,
                preferences: &mut self.preferences,
                lastfm_service: &mut self.lastfm_service,
                lastfm_settings: &mut self.lastfm_settings,
                artwork_cache: &mut self.artwork_cache,
                toasts: &mut self.toasts,
            },
        );
        drop(playback);
        self.artwork_cache.finish_frame(ctx);
        for action in actions {
            self.handle_action(action);
        }
        let zoom = ctx.zoom_factor();
        if (zoom - self.settings.ui_zoom).abs() > f32::EPSILON
            && !ctx.input(|input| input.pointer.any_down())
        {
            self.settings.ui_zoom = zoom;
            self.save_settings();
        }
        self.toasts.show(ctx);
        match self.updater.show_card(ctx) {
            Some(UpdateAction::Restart) => self.restart_requested = true,
            Some(UpdateAction::Download) => {
                ctx.open_url(egui::OpenUrl::new_tab(crate::platform::updates::RELEASES_URL));
            }
            None => {}
        }
        if std::mem::take(&mut self.restart_requested) {
            self.restart_to_update(ctx);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Err(error) = self.settings.save() {
            eprintln!("Could not save window size: {error}");
        }
    }
}

impl Drop for ViperApp {
    fn drop(&mut self) {
        self.library.background.cancel_all();
    }
}
