use super::application::ViperApp;
use crate::{shared::ui::controls::SEARCH_FIELD_ID, workbench::Page};
use gpui::{Context, KeyDownEvent, Window};
use gpui_component::WindowExt;
use std::time::Duration;

/// Seconds skipped by Ctrl+Left and Ctrl+Right.
const SEEK_STEP: Duration = Duration::from_secs(5);
/// Volume percentage changed by Ctrl+Up and Ctrl+Down.
const VOLUME_STEP: u8 = 5;
/// Zoom range and step for Ctrl + and Ctrl −.
const ZOOM_RANGE: (f32, f32) = (0.75, 2.0);
const ZOOM_STEP: f32 = 0.1;

impl ViperApp {
    /// Handle a global keyboard shortcut that no focused control took.
    ///
    /// # Arguments
    /// * `event` - Key press.
    /// * `window` - Window the key was pressed in.
    /// * `cx` - Application entity context.
    ///
    /// # Returns
    /// Whether the key was a shortcut, so it should go no further.
    pub(crate) fn handle_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let keystroke = &event.keystroke;
        let key = keystroke.key.as_str();
        let plain = !keystroke.modifiers.modified();
        let command = keystroke.modifiers.secondary(cx) && !keystroke.modifiers.alt;
        if plain && key == "escape" && self.workbench.dialog_open() {
            if self.workbench.tag_editor.is_some() {
                crate::features::tag_editor::request_close(self);
            } else {
                self.workbench.show_preferences = false;
            }
            return true;
        }
        let workbench = &mut self.workbench;
        if !workbench.dialog_open()
            && plain
            && key == "escape"
            && (workbench.show_queue || workbench.show_shortcuts || workbench.show_album_details)
        {
            if workbench.show_shortcuts {
                workbench.show_shortcuts = false;
            } else if workbench.show_queue {
                workbench.show_queue = false;
            } else {
                workbench.show_album_details = false;
            }
            return true;
        }
        if workbench.dialog_open() || window.has_focused_input(cx) {
            return false;
        }
        if command {
            return self.handle_command_shortcut(key);
        }
        if !plain {
            return false;
        }
        if !self.workbench.show_shortcuts {
            match key {
                "f" => {
                    let workbench = &mut self.workbench;
                    if workbench.page != Page::Songs {
                        workbench.page = Page::Albums;
                        workbench.show_album_details = false;
                    }
                    // The field may not exist until the page is drawn, so the draw focuses it.
                    self.ui.focus_search = true;
                    self.inputs.focus(SEARCH_FIELD_ID, window, cx);
                    return true;
                }
                "q" => {
                    self.workbench.show_queue = !self.workbench.show_queue;
                    return true;
                }
                "f5" => {
                    self.playback()
                        .play_random_track(&self.library.model.tracks);
                    return true;
                }
                _ => {}
            }
            if self.handle_navigation(key) {
                return true;
            }
        }
        if key == "space" {
            self.playback().play_pause(&self.library);
            return true;
        }
        false
    }

    /// Handle Ctrl shortcuts: seeking, volume, and zoom.
    fn handle_command_shortcut(&mut self, key: &str) -> bool {
        match key {
            "right" | "left" => {
                let mut playback = self.playback();
                let position = playback.position();
                let target = if key == "right" {
                    position + SEEK_STEP
                } else {
                    position.saturating_sub(SEEK_STEP)
                };
                playback.seek(&self.library.model.tracks, target);
            }
            "up" | "down" => {
                self.settings.volume = if key == "up" {
                    self.settings.volume.saturating_add(VOLUME_STEP).min(100)
                } else {
                    self.settings.volume.saturating_sub(VOLUME_STEP)
                };
                self.playback().set_volume(self.settings.volume);
                self.save_settings();
            }
            "=" | "+" | "-" | "0" => {
                let zoom = match key {
                    "0" => 1.0,
                    "-" => self.settings.ui_zoom - ZOOM_STEP,
                    _ => self.settings.ui_zoom + ZOOM_STEP,
                };
                self.settings.ui_zoom =
                    ((zoom * 100.0).round() / 100.0).clamp(ZOOM_RANGE.0, ZOOM_RANGE.1);
                self.save_settings();
            }
            _ => return false,
        }
        true
    }

    /// Move the selection with the arrow keys and activate it with Enter on the current page.
    fn handle_navigation(&mut self, key: &str) -> bool {
        let columns = self.library.view.grid_columns.max(1) as isize;
        let delta = match key {
            "up" => Some(-1),
            "down" => Some(1),
            "pageup" => Some(-10),
            "pagedown" => Some(10),
            "home" => Some(isize::MIN),
            "end" => Some(isize::MAX),
            _ => None,
        };
        let horizontal = match key {
            "left" => Some(-1),
            "right" => Some(1),
            _ => None,
        };
        let enter = key == "enter";
        if delta.is_none() && horizontal.is_none() && !enter {
            return false;
        }
        let view = &mut self.library.view;
        match self.workbench.page {
            Page::Home | Page::Ripper => return false,
            Page::Albums if self.workbench.show_album_details => {
                let Some(album) = view
                    .selected_album
                    .and_then(|index| self.library.model.albums.get(index))
                else {
                    return false;
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
        true
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
