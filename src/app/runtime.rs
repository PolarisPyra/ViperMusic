use super::application::ViperApp;
use crate::{
    features::playback::RepeatMode, shared::action::AppAction, workbench::WorkbenchContext,
};
use eframe::egui;
use std::{sync::Arc, time::Duration};

impl Default for ViperApp {
    fn default() -> Self {
        Self::new()
    }
}

impl ViperApp {
    /// Advance playback, schedule gapless transitions, and request timely repaints.
    fn update_playback(&mut self, ctx: &egui::Context) {
        let playback_handle = Arc::clone(&self.playback);
        let mut playback = playback_handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tracks = &self.library.model.tracks;
        if playback.has_playback_work() {
            if playback.is_playing() {
                playback.advance_if_finished(tracks);
            }
            let gapless = self.settings.gapless_playback && playback.repeat_mode != RepeatMode::One;
            playback.update_gapless(tracks, gapless);
            if playback.has_playback_work() {
                ctx.request_repaint_after(if playback.gapless_preparation_pending() {
                    Duration::from_millis(16)
                } else if gapless && playback.has_next() {
                    Duration::from_millis(100)
                } else {
                    Duration::from_millis(400)
                });
            }
        }
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
    pub(crate) fn tick_in_background(&mut self, ctx: &egui::Context) {
        self.update_playback(ctx);
    }

    /// Apply the selected color way to the window and tray icons when it has changed.
    fn sync_logo(&mut self, ctx: &egui::Context) {
        let colorway = self.settings.logo;
        if self.applied_logo == Some(colorway) {
            return;
        }
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
        // Also runs at startup, so a reinstall that reset the launcher icon gets the choice back.
        #[cfg(target_os = "linux")]
        if let Err(error) = crate::shared::logo::install_launcher_icon(colorway) {
            eprintln!("Launcher icon could not change: {error}");
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
        self.update_playback(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        self.observe_window_size(ctx);
        crate::shared::ui::theme::apply(ctx, self.settings.theme);
        self.handle_shortcuts(ctx);
        self.artwork_cache.begin_frame();

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
