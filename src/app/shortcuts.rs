use super::application::ViperApp;
use crate::{shared::ui::controls::SEARCH_FIELD_ID, workbench::Page};
use eframe::egui::{self, Key, Modifiers};
use std::time::Duration;

/// Seconds skipped by Ctrl+Left and Ctrl+Right.
const SEEK_STEP: Duration = Duration::from_secs(5);
/// Volume percentage changed by Ctrl+Up and Ctrl+Down.
const VOLUME_STEP: u8 = 5;

impl ViperApp {
    /// Handle global keyboard shortcuts before any panel consumes input.
    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let consume = |modifiers, key| ctx.input_mut(|input| input.consume_key(modifiers, key));
        let workbench = &mut self.workbench;
        let popup_open = egui::Popup::is_any_open(ctx);
        if !workbench.show_preferences
            && (popup_open
                || workbench.show_queue
                || workbench.show_shortcuts
                || workbench.show_album_details)
            && consume(Modifiers::NONE, Key::Escape)
        {
            if popup_open {
                egui::Popup::close_all(ctx);
            } else if workbench.show_shortcuts {
                workbench.show_shortcuts = false;
            } else if workbench.show_queue {
                workbench.show_queue = false;
            } else {
                workbench.show_album_details = false;
            }
        }
        if workbench.show_preferences || ctx.egui_wants_keyboard_input() {
            return;
        }
        if !workbench.show_shortcuts {
            if consume(Modifiers::NONE, Key::F) {
                if workbench.page != Page::Songs {
                    workbench.page = Page::Albums;
                    workbench.show_album_details = false;
                }
                // The typed character arrives as a separate text event; drop it so the newly focused field doesn't receive it.
                ctx.input_mut(|input| {
                    input.events.retain(|event| {
                        !matches!(event, egui::Event::Text(text) if matches!(text.as_str(), "f" | "F"))
                    });
                });
                ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(SEARCH_FIELD_ID)));
            }
            if consume(Modifiers::NONE, Key::Q) {
                workbench.show_queue = !workbench.show_queue;
            }
            if consume(Modifiers::NONE, Key::F5) {
                self.playback()
                    .play_random_track(&self.library.model.tracks);
            }
            self.handle_navigation(ctx);
        }
        if consume(Modifiers::NONE, Key::Space) {
            self.playback().play_pause(&self.library);
        }
        for (key, forward) in [(Key::ArrowRight, true), (Key::ArrowLeft, false)] {
            if consume(Modifiers::COMMAND, key) {
                let mut playback = self.playback();
                let position = playback.position();
                let target = if forward {
                    position + SEEK_STEP
                } else {
                    position.saturating_sub(SEEK_STEP)
                };
                playback.seek(&self.library.model.tracks, target);
            }
        }
        for (key, louder) in [(Key::ArrowUp, true), (Key::ArrowDown, false)] {
            if consume(Modifiers::COMMAND, key) {
                self.settings.volume = if louder {
                    self.settings.volume.saturating_add(VOLUME_STEP).min(100)
                } else {
                    self.settings.volume.saturating_sub(VOLUME_STEP)
                };
                self.playback().set_volume(self.settings.volume);
                self.save_settings();
            }
        }
    }

    /// Move the selection with the arrow keys and activate it with Enter on the current page.
    fn handle_navigation(&mut self, ctx: &egui::Context) {
        let pressed = |key| ctx.input_mut(|input| input.consume_key(Modifiers::NONE, key));
        let columns = self.library.view.grid_columns.max(1) as isize;
        let delta = [
            (Key::ArrowUp, -1),
            (Key::ArrowDown, 1),
            (Key::PageUp, -10),
            (Key::PageDown, 10),
            (Key::Home, isize::MIN),
            (Key::End, isize::MAX),
        ]
        .into_iter()
        .find_map(|(key, delta)| pressed(key).then_some(delta));
        let horizontal = [(Key::ArrowLeft, -1), (Key::ArrowRight, 1)]
            .into_iter()
            .find_map(|(key, delta)| pressed(key).then_some(delta));
        let enter = pressed(Key::Enter);
        let view = &mut self.library.view;
        match self.workbench.page {
            Page::Home => {}
            Page::Albums if self.workbench.show_album_details => {
                let Some(album) = view
                    .selected_album
                    .and_then(|index| self.library.model.albums.get(index))
                else {
                    return;
                };
                if let Some(delta) = delta {
                    view.selected_track = step(&album.tracks, view.selected_track, delta);
                    view.scroll_to_selection = true;
                }
                if enter {
                    if let Some(track) = view.selected_track {
                        self.playback
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .play_in_context(&album.tracks, &self.library.model.tracks, track);
                    }
                }
            }
            Page::Albums => {
                let delta = delta
                    .map(|delta| match delta {
                        -1 | 1 => delta * columns,
                        other => other,
                    })
                    .or(horizontal);
                if let Some(delta) = delta {
                    let albums = self.library.filtered_album_indices(&self.settings);
                    let view = &mut self.library.view;
                    view.highlighted_album = step(&albums, view.highlighted_album, delta);
                    view.scroll_to_selection = true;
                }
                if enter {
                    if let Some(album) = self.library.view.highlighted_album {
                        self.workbench.open_album(&mut self.library.view, album);
                    }
                }
            }
            Page::Songs => {
                let songs = self.library.filtered_song_indices();
                let view = &mut self.library.view;
                if let Some(delta) = delta {
                    view.selected_song = step(&songs, view.selected_song, delta);
                    view.scroll_to_selection = true;
                }
                if enter {
                    if let Some(track) = view.selected_song {
                        self.playback
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .play_in_context(&songs, &self.library.model.tracks, track);
                    }
                }
            }
        }
    }
}

/// Move `delta` items from `current` within `items`, clamping at either end.
///
/// With no current selection, any movement selects the first item.
fn step(items: &[usize], current: Option<usize>, delta: isize) -> Option<usize> {
    let last = items.len().checked_sub(1)?;
    let Some(position) = current.and_then(|item| items.iter().position(|&other| other == item))
    else {
        return items.first().copied();
    };
    let target = position.saturating_add_signed(delta).min(last);
    items.get(target).copied()
}
