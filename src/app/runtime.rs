use super::application::ViperApp;
use crate::{
    features::playback::RepeatMode,
    shared::action::AppAction,
    workbench::{Page, WorkbenchContext},
};
use eframe::egui;
use std::{sync::Arc, time::Duration};

impl Default for ViperApp {
    fn default() -> Self {
        Self::new()
    }
}

impl ViperApp {
    /// Handle global keyboard shortcuts before any panel consumes input.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let consume = |key| ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key));
        let workbench = &mut self.workbench;
        let popup_open = ctx.memory(|memory| memory.any_popup_open());
        if !workbench.show_preferences
            && (popup_open
                || workbench.show_queue
                || workbench.show_shortcuts
                || workbench.show_album_details)
            && consume(egui::Key::Escape)
        {
            if popup_open {
                ctx.memory_mut(|memory| memory.close_popup());
            } else if workbench.show_shortcuts {
                workbench.show_shortcuts = false;
            } else if workbench.show_queue {
                workbench.show_queue = false;
            } else {
                workbench.show_album_details = false;
            }
        }
        if workbench.show_preferences || ctx.wants_keyboard_input() {
            return;
        }
        if !workbench.show_shortcuts {
            if consume(egui::Key::F) {
                workbench.page = Page::Albums;
                workbench.show_album_details = false;
                ctx.memory_mut(|memory| {
                    memory.request_focus(egui::Id::new("library-search-field"))
                });
            }
            if consume(egui::Key::Q) {
                workbench.show_queue = !workbench.show_queue;
            }
            if consume(egui::Key::F5) {
                self.playback()
                    .play_random_track(&self.library.model.tracks);
            }
        }
        if consume(egui::Key::Space) {
            self.playback().play_pause(&self.library);
        }
    }

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
        self.observe_playback_track(&playback);
        if self.lastfm_service.poll(&mut self.lastfm_settings) {
            if let Err(error) =
                crate::platform::persistence::settings::save_lastfm_settings(&self.lastfm_settings)
            {
                self.error = Some(format!("Could not save Last.fm session: {error}"));
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.observe_window_size(ctx);
        crate::shared::ui::theme::apply(ctx, self.settings.theme);
        self.handle_shortcuts(ctx);
        if self.poll_scan() {
            self.artwork_cache.clear();
        }
        self.poll_artwork(ctx);
        self.poll_file_watcher(ctx);
        self.artwork_cache.poll(ctx);
        self.artwork_cache.begin_frame();
        if self.library.scanning {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        self.update_playback(ctx);

        let playback_handle = Arc::clone(&self.playback);
        let mut playback = playback_handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let actions = crate::workbench::show(
            ctx,
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
                error: &mut self.error,
            },
        );
        drop(playback);
        self.artwork_cache.finish_frame(ctx);
        for action in actions {
            self.handle_action(action);
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
