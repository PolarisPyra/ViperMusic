use super::application::ViperApp;
use crate::{
    features::playback::Playback,
    platform::{persistence::settings::Settings, tray::TrayEvent},
    shared::{action::AppAction, repaint::RepaintReceiver},
};
use gpui::{
    px, size, App, AppContext, Bounds, Context, Entity, IntoElement, Render, TitlebarOptions,
    Window, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::Root;
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

/// Smallest window that still fits the sidebar, a page, and the player bar.
const MIN_WINDOW_SIZE: [f32; 2] = [760.0, 520.0];
/// How often the hidden app checks its background work and whether the session has ended.
const HIDDEN_TICK: Duration = Duration::from_millis(50);

/// Saved window size, kept within the minimum, or the default size.
fn window_size(settings: &Settings) -> [f32; 2] {
    settings
        .window_size
        .filter(|[width, height]| width.is_finite() && height.is_finite())
        .map(|[width, height]| {
            [
                width.max(MIN_WINDOW_SIZE[0]),
                height.max(MIN_WINDOW_SIZE[1]),
            ]
        })
        .unwrap_or([1440.0, 900.0])
}

/// Take the volume MPRIS set on `playback` into `settings`, so the slider shows it and it is saved.
///
/// # Returns
/// Whether the volume changed and needs saving.
fn adopt_volume(settings: &mut Settings, playback: &Playback) -> bool {
    std::mem::replace(&mut settings.volume, playback.volume) != playback.volume
}

/// Tray and launch events with repeated Show requests dropped. One Show already brings the window
/// forward, and each one after it would open another window before the first has opened.
fn merge_shows(events: impl IntoIterator<Item = TrayEvent>) -> Vec<TrayEvent> {
    let mut shown = false;
    events
        .into_iter()
        .filter(|&event| event != TrayEvent::Show || !std::mem::replace(&mut shown, true))
        .collect()
}

impl ViperApp {
    /// Open the main window on `app`, which must not have one open.
    ///
    /// # Arguments
    /// * `app` - Application entity drawn in the window.
    /// * `cx` - Application context.
    pub(super) fn open_window(app: &Entity<Self>, cx: &mut App) {
        let (settings_size, colorway) = {
            let app = app.read(cx);
            (window_size(&app.settings), app.settings.logo)
        };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(settings_size[0]), px(settings_size[1])),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Viper Music".into()),
                ..Default::default()
            }),
            app_id: Some("viper".to_owned()),
            window_min_size: Some(size(px(MIN_WINDOW_SIZE[0]), px(MIN_WINDOW_SIZE[1]))),
            window_decorations: Some(WindowDecorations::Server),
            icon: Some(Arc::new(crate::shared::logo::window_icon(
                &colorway.decode(),
            ))),
            ..Default::default()
        };
        let view = app.clone();
        let opened = cx.open_window(options, move |window, cx| {
            let weak = view.downgrade();
            window.on_window_should_close(cx, move |_, cx| {
                weak.update(cx, |app, cx| app.window_should_close(cx))
                    .unwrap_or(true)
            });
            let focus = view.read(cx).focus.clone();
            window.focus(&focus, cx);
            cx.new(|cx| Root::new(view, window, cx))
        });
        match opened {
            Ok(handle) => app.update(cx, |app, _| app.window = Some(handle.into())),
            Err(error) => {
                eprintln!("The window could not open: {error:#}");
                app.update(cx, |app, cx| app.quit(cx));
            }
        }
    }

    /// Decide what closing the window does: hide to the tray, or quit without one.
    fn window_should_close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.workbench.quit_requested || self.tray.is_none() {
            self.quit(cx);
        } else {
            self.window = None;
            self.inputs.clear();
            // Save the window size now, since a logout or the tray's forced exit skips `quit`.
            self.save_settings();
        }
        true
    }

    /// Show the window, opening it again if it is hidden to the tray.
    fn show_window(&mut self, window_active: bool, cx: &mut Context<Self>) {
        match self.window {
            None => {
                let app = cx.entity();
                cx.defer(move |cx| Self::open_window(&app, cx));
            }
            // Wayland ignores focus requests, so bring the window forward by recreating it.
            Some(_) if !window_active => self.reopen_window(cx),
            Some(_) => {}
        }
    }

    /// Replace the window with a new one, such as so the desktop reads a changed icon.
    fn reopen_window(&mut self, cx: &mut Context<Self>) {
        let old = self.window.take();
        self.inputs.clear();
        let app = cx.entity();
        // Open the new window first, so closing the old one never leaves the app without one.
        cx.defer(move |cx| {
            Self::open_window(&app, cx);
            if let Some(old) = old {
                let _ = old.update(cx, |_, window, _| window.remove_window());
            }
        });
    }

    /// Save settings and quit.
    pub(crate) fn quit(&mut self, cx: &mut Context<Self>) {
        self.workbench.quit_requested = true;
        if let Err(error) = self.settings.save() {
            eprintln!("Could not save window size: {error}");
        }
        cx.quit();
    }

    /// Run [`Self::logic`] whenever background work or a delayed redraw wakes the app.
    ///
    /// The loop holds the app, which the window holds only while it is open, so hiding to the tray
    /// keeps playback, the tray, and later launches working until the app quits.
    ///
    /// # Arguments
    /// * `wakes` - Receiver of the app's [`crate::shared::repaint::Repaint`] handle.
    /// * `cx` - Application entity context.
    pub(super) fn start_polling(&mut self, wakes: RepaintReceiver, cx: &mut Context<Self>) {
        // Nothing has woken the loop yet, so poll once to pick up the startup scan.
        self.repaint.request_repaint();
        let this = cx.entity();
        cx.spawn(async move |_, cx| loop {
            let executor = cx.background_executor().clone();
            wakes.wait(|delay| executor.timer(delay)).await;
            let window = this.read_with(cx, |app, _| app.window);
            let window_active = window
                .and_then(|window| {
                    window
                        .update(cx, |_, window, _| window.is_window_active())
                        .ok()
                })
                .unwrap_or(false);
            this.update(cx, |app, cx| app.logic(window_active, cx));
        })
        .detach();
    }

    /// Hand the library to the playback driver and the settings to Last.fm and Discord, and report
    /// playback errors as toasts. The driver, scrobbler, and presence threads keep going while the
    /// window draws no frames, so this needs no repaints of its own.
    fn update_playback(&mut self) {
        let playback_handle = Arc::clone(&self.playback);
        let mut playback = playback_handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        playback.sync_library(&self.library.model.tracks, self.settings.gapless_playback);
        if let Some(error) = playback.error.take() {
            self.toasts.push(error);
        }
        let volume_changed = adopt_volume(&mut self.settings, &playback);
        self.observe_playback_track(&playback);
        drop(playback);
        if volume_changed {
            self.save_settings();
        }
        if self.lastfm_service.poll(&mut self.lastfm_settings) {
            if let Err(error) =
                crate::platform::persistence::settings::save_lastfm_settings(&self.lastfm_settings)
            {
                self.toasts
                    .push(format!("Could not save Last.fm session: {error}"));
            }
        }
        self.lastfm_service.configure(
            self.settings.lastfm_scrobbling_enabled,
            &self.lastfm_settings,
        );
        self.discord_presence.set_options(
            self.settings
                .discord_rich_presence_enabled
                .then_some(self.settings.discord_presence),
        );
    }

    /// Apply the selected color way to the tray and launcher icons when it has changed.
    fn sync_logo(&mut self) {
        let colorway = self.settings.logo;
        if self.applied_logo == Some(colorway) {
            return;
        }
        #[cfg(target_os = "linux")]
        let changed_while_open = self.applied_logo.is_some();
        self.applied_logo = Some(colorway);
        if let Some(tray) = &self.tray {
            if let Err(error) = tray.set_logo(&colorway.decode()) {
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
                let change = self.logo_changes.fetch_add(1, Ordering::Relaxed) + 1;
                let logo_changes = Arc::clone(&self.logo_changes);
                let reopen_window = Arc::clone(&self.reopen_window);
                let repaint = self.repaint.clone();
                std::thread::spawn(move || {
                    crate::shared::logo::refresh_launcher_cache();
                    if changed_while_open {
                        std::thread::sleep(Duration::from_millis(1500));
                        if logo_changes.load(Ordering::Relaxed) == change {
                            reopen_window.store(true, Ordering::Relaxed);
                            repaint.request_repaint();
                        }
                    }
                });
            }
            Ok(false) => {}
            Err(error) => eprintln!("Launcher icon could not change: {error}"),
        }
    }

    fn poll_artwork(&mut self) {
        let mut updated = false;
        for (album_index, artwork) in self.library.background.poll_artwork(&self.repaint) {
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

    /// Carry out a request raised by a view.
    ///
    /// # Arguments
    /// * `action` - Request to handle.
    pub(crate) fn handle_action(&mut self, action: AppAction) {
        match action {
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
            AppAction::RestartToUpdate => self.request_restart(),
        }
    }

    /// Restart into the installed update, once any disc being read or ripped is finished.
    pub(crate) fn request_restart(&mut self) {
        if self.ripper.busy() {
            self.toasts
                .push("Viper Music will restart to update once the disc is done".to_owned());
        }
        self.restart_requested = true;
        self.repaint.request_repaint();
    }

    /// Poll background work, handle tray events, and redraw the window if one is open.
    ///
    /// Runs whether or not the window is open, so playback keeps advancing while it is hidden.
    fn logic(&mut self, window_active: bool, cx: &mut Context<Self>) {
        for event in merge_shows(self.tray_events.try_iter()) {
            match event {
                TrayEvent::Show => self.show_window(window_active, cx),
                TrayEvent::Play => self.tray_play(),
                TrayEvent::Pause => self.tray_pause(),
                TrayEvent::Quit => return self.quit(cx),
            }
        }
        if self.poll_scan() {
            self.artwork_cache.clear();
        }
        self.poll_artwork();
        self.sync_logo();
        self.poll_file_watcher();
        self.artwork_cache.poll();
        self.ripper.poll(&self.repaint, &mut self.toasts);
        for image in self.artwork_cache.take_evicted() {
            cx.drop_image(image, None);
        }
        if self.library.scanning {
            self.repaint
                .request_repaint_after(Duration::from_millis(100));
        }
        self.update_playback();
        // Quitting to update wins over a reopen asked for at the same time. It waits for a rip,
        // whose thread wakes the app when it ends.
        if self.restart_requested && !self.ripper.busy() {
            self.restart_requested = false;
            if self.updater.restart() {
                return self.quit(cx);
            }
        }
        // HACK: see `install_launcher_icon`. The window manager reads the icon only when a window opens.
        if self.reopen_window.swap(false, Ordering::Relaxed) && self.window.is_some() {
            self.reopen_window(cx);
        }
        if self.window.is_some() {
            cx.notify();
        } else {
            // Logging out leaves a hidden instance with a dead display connection, which would
            // block later launches and crash when asked to show its window.
            if self.display.is_gone() {
                return self.quit(cx);
            }
            self.repaint.request_repaint_after(HIDDEN_TICK);
        }
    }
}

impl Render for ViperApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.observe_window_size(window);
        crate::shared::ui::theme::apply(cx, self.settings.theme);
        // Every size is in rems, so the rem size zooms the whole interface.
        window.set_rem_size(px(16.0 * self.settings.ui_zoom));
        self.artwork_cache.begin_frame();
        self.preferences.update_state = self.updater.state();
        crate::workbench::render(self, window, cx)
    }
}

impl Drop for ViperApp {
    fn drop(&mut self) {
        self.library.background.cancel_all();
    }
}

#[cfg(test)]
mod tests {
    use super::{adopt_volume, merge_shows};
    use crate::{
        features::playback::Playback,
        platform::{persistence::settings::Settings, tray::TrayEvent},
    };

    #[test]
    fn volume_set_over_mpris_is_shown_and_saved() {
        let mut settings = Settings::default();
        let mut playback = Playback::default();
        playback.volume = settings.volume;
        assert!(!adopt_volume(&mut settings, &playback));
        playback.volume = settings.volume / 2;
        assert!(adopt_volume(&mut settings, &playback));
        assert_eq!(settings.volume, playback.volume);
        assert!(!adopt_volume(&mut settings, &playback));
    }

    #[test]
    fn two_launches_at_once_open_one_window() {
        let events = [
            TrayEvent::Show,
            TrayEvent::Pause,
            TrayEvent::Show,
            TrayEvent::Play,
        ];
        assert_eq!(
            merge_shows(events),
            [TrayEvent::Show, TrayEvent::Pause, TrayEvent::Play]
        );
    }
}
