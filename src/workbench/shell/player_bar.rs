use crate::{
    features::{
        library::state::LibraryFeature,
        playback::{Playback, RepeatMode},
    },
    platform::persistence::settings::Settings,
    shared::{
        artwork::ArtworkCache,
        ui::{controls::describe_button, theme, toasts::Toasts},
    },
    workbench::WorkbenchState,
};
use eframe::egui;
use egui::emath::GuiRounding;
use std::time::Duration;

const PANEL_HEIGHT: f32 = 112.0;
const TRACK_ART_SIZE: f32 = 68.0;
/// Shortest wait between repaints while playing, about 30 fps.
const MIN_PLAYING_REPAINT: Duration = Duration::from_millis(33);

pub(in crate::workbench) fn show(
    ui: &mut egui::Ui,
    workbench: &mut WorkbenchState,
    playback: &mut Playback,
    library: &mut LibraryFeature,
    settings: &mut Settings,
    artwork_cache: &mut ArtworkCache,
    toasts: &mut Toasts,
) {
    let ctx = &ui.ctx().clone();
    let colors = theme::colors(ctx);
    egui::Panel::bottom("scrubber-controls")
        .exact_size(PANEL_HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(colors.panel)
                .inner_margin(egui::Margin::symmetric(24, 8)),
        )
        .show(ui, |ui| {
            let current = playback.current;
            let previous_enabled = current.is_some() || playback.has_history();
            let next_enabled = playback.has_next();
            let duration = current
                .and_then(|index| library.model.tracks.get(index))
                .and_then(|track| track.duration_ms)
                .map(Duration::from_millis);

            let row_width = ui.available_width();
            let (row_rect, _) =
                ui.allocate_exact_size(egui::vec2(row_width, 72.0), egui::Sense::hover());
            let volume_width = if row_width < 862.0 { 150.0 } else { 176.0 };
            let control_spacing =
                ((row_width * 0.5 - volume_width - 50.0 - 17.0 - 8.0) / 2.0).clamp(54.0, 68.0);
            let controls_width = control_spacing * 4.0 + 34.0;
            let max_info_width = ((row_width - controls_width) * 0.5 - 16.0).max(0.0);
            let info_rect = egui::Rect::from_min_size(
                row_rect.min,
                egui::vec2(
                    (row_width * 0.4).min(440.0).min(max_info_width),
                    row_rect.height(),
                ),
            );
            let mut info_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(info_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            let open_album = show_track_info(ctx, &mut info_ui, library, artwork_cache, current);

            // Anchor the controls to the full bar rect; column sizing otherwise
            // shifts them when the track-info column has no content.
            let controls_rect = egui::Rect::from_center_size(
                row_rect.center(),
                egui::vec2(controls_width, row_rect.height()),
            );
            let shuffle_rect = egui::Rect::from_center_size(
                egui::pos2(
                    row_rect.center().x - control_spacing * 2.0,
                    controls_rect.center().y,
                ),
                egui::vec2(34.0, 38.0),
            );
            let previous_rect = egui::Rect::from_center_size(
                egui::pos2(
                    row_rect.center().x - control_spacing,
                    controls_rect.center().y,
                ),
                egui::vec2(40.0, 44.0),
            );
            let play_rect =
                egui::Rect::from_center_size(controls_rect.center(), egui::vec2(50.0, 50.0));
            let next_rect = egui::Rect::from_center_size(
                egui::pos2(
                    row_rect.center().x + control_spacing,
                    controls_rect.center().y,
                ),
                egui::vec2(40.0, 44.0),
            );
            let repeat_rect = egui::Rect::from_center_size(
                egui::pos2(
                    row_rect.center().x + control_spacing * 2.0,
                    controls_rect.center().y,
                ),
                egui::vec2(34.0, 38.0),
            );
            let shuffle = transport_icon_button(
                ui,
                shuffle_rect,
                "player-shuffle",
                egui_phosphor::regular::SHUFFLE,
                if playback.shuffle_enabled {
                    "Shuffle: On"
                } else {
                    "Shuffle: Off"
                },
                playback.shuffle_enabled,
                true,
            );
            let previous = ui.interact(
                previous_rect,
                egui::Id::new("player-previous"),
                if previous_enabled {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            let can_start = playback.current.is_some()
                || library
                    .view
                    .selected_album
                    .and_then(|index| library.model.albums.get(index))
                    .is_some_and(|album| !album.tracks.is_empty())
                || playback.has_next()
                || (playback.shuffle_enabled && !library.model.tracks.is_empty());
            let play_enabled = playback.current.is_some() || can_start;
            let play = ui.interact(
                play_rect,
                egui::Id::new("player-play-pause"),
                if play_enabled {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            let next = ui.interact(
                next_rect,
                egui::Id::new("player-next"),
                if next_enabled {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            let (repeat_id, repeat_icon, repeat_tooltip) = match playback.repeat_mode {
                RepeatMode::Off => (
                    "player-repeat-off",
                    egui_phosphor::regular::REPEAT,
                    "Repeat: Off",
                ),
                RepeatMode::All => (
                    "player-repeat-all",
                    egui_phosphor::regular::REPEAT,
                    "Repeat: Queue",
                ),
                RepeatMode::One => (
                    "player-repeat-one",
                    egui_phosphor::regular::REPEAT_ONCE,
                    "Repeat: Track",
                ),
            };
            let repeat = transport_icon_button(
                ui,
                repeat_rect,
                repeat_id,
                repeat_icon,
                repeat_tooltip,
                playback.repeat_mode != RepeatMode::Off,
                true,
            );
            ui.painter().text(
                previous_rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::SKIP_BACK,
                theme::icon_font(28.0),
                if previous.hovered() && previous_enabled {
                    colors.text
                } else {
                    colors.muted
                },
            );
            ui.painter().circle_filled(
                play_rect.center(),
                23.0,
                if playback.current.is_some() {
                    colors.accent
                } else {
                    colors.surface
                },
            );
            let icon = if playback.is_playing() {
                egui_phosphor::regular::PAUSE
            } else {
                egui_phosphor::regular::PLAY
            };
            ui.painter().text(
                play_rect.center(),
                egui::Align2::CENTER_CENTER,
                icon,
                theme::icon_font(34.0),
                if playback.current.is_some() {
                    colors.on_accent
                } else {
                    colors.muted
                },
            );
            ui.painter().text(
                next_rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::SKIP_FORWARD,
                theme::icon_font(28.0),
                if next.hovered() && next_enabled {
                    colors.text
                } else {
                    colors.muted
                },
            );
            describe_button(&previous, "Previous track");
            describe_button(&next, "Next track");
            describe_button(
                &play,
                if playback.is_playing() {
                    "Pause"
                } else {
                    "Play"
                },
            );
            let previous = previous.on_hover_text("Previous track");
            let play = play.on_hover_text(if playback.current.is_none() && !play_enabled {
                "Select an album or turn on Shuffle to play"
            } else if playback.is_playing() {
                "Pause"
            } else if playback.current.is_some() {
                "Resume"
            } else {
                "Play"
            });
            let next = next.on_hover_text("Next track");
            if shuffle.clicked() {
                playback.toggle_shuffle(&library.model.tracks);
            }
            if repeat.double_clicked() {
                playback.set_repeat_mode(RepeatMode::One);
            } else if repeat.clicked() {
                playback.toggle_repeat_all();
            }
            if previous.clicked() {
                playback.previous(&library.model.tracks);
            }
            if play.clicked() {
                playback.play_pause(library);
            }
            if next.clicked() {
                playback.skip_next(&library.model.tracks);
            }

            let volume_area = egui::Rect::from_min_size(
                egui::pos2(
                    row_rect.right() - volume_width - 90.0,
                    row_rect.center().y - 15.0,
                ),
                egui::vec2(volume_width, 30.0),
            );
            let queue_rect = egui::Rect::from_center_size(
                egui::pos2(row_rect.right() - 66.0, row_rect.center().y),
                egui::vec2(34.0, 38.0),
            );
            let random_rect = egui::Rect::from_center_size(
                egui::pos2(row_rect.right() - 24.0, row_rect.center().y),
                egui::vec2(34.0, 38.0),
            );
            let random = transport_icon_button(
                ui,
                random_rect,
                "player-random-track",
                egui_phosphor::regular::DICE_FIVE,
                "Play random song",
                false,
                !library.model.tracks.is_empty(),
            );
            if random.clicked() {
                playback.play_random_track(&library.model.tracks);
            }
            let queue = transport_icon_button(
                ui,
                queue_rect,
                "player-queue",
                egui_phosphor::regular::QUEUE,
                "Queue",
                workbench.show_queue,
                true,
            );
            if queue.clicked() {
                workbench.show_queue = !workbench.show_queue;
            }
            if let Some(track) = current.and_then(|index| library.model.tracks.get(index)) {
                show_audio_chips(ui, controls_rect, volume_area, track);
            }

            let volume_icon = egui::Rect::from_min_size(
                volume_area.min + egui::vec2(0.0, 4.0),
                egui::vec2(22.0, 22.0),
            );
            let volume_glyph = match settings.volume {
                0 => egui_phosphor::regular::SPEAKER_X,
                1..=35 => egui_phosphor::regular::SPEAKER_LOW,
                36..=u8::MAX => egui_phosphor::regular::SPEAKER_HIGH,
            };
            ui.painter().text(
                volume_icon.center(),
                egui::Align2::CENTER_CENTER,
                volume_glyph,
                theme::icon_font(22.0),
                colors.muted,
            );
            let volume_track_area = egui::Rect::from_min_max(
                volume_area.min + egui::vec2(30.0, 0.0),
                volume_area.max - egui::vec2(46.0, 0.0),
            );
            let volume_response = ui.interact(
                volume_track_area,
                egui::Id::new("player-volume-slider"),
                egui::Sense::click_and_drag(),
            );
            volume_response.widget_info(|| {
                egui::WidgetInfo::slider(true, f64::from(settings.volume), "Volume")
            });
            let volume_track = egui::Rect::from_center_size(
                volume_track_area.center(),
                egui::vec2(volume_track_area.width(), 3.0),
            );
            ui.painter().rect_filled(volume_track, 2.0, colors.surface);
            let volume_fraction = settings.volume as f32 / 100.0;
            let volume_x = volume_track.left() + volume_track.width() * volume_fraction;
            let volume_thumb_center = egui::pos2(
                volume_x.round_to_pixel_center(ui.pixels_per_point()),
                volume_track.center().y,
            );
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    volume_track.min,
                    egui::pos2(volume_x, volume_track.bottom()),
                ),
                2.0,
                colors.accent,
            );
            ui.painter().rect_filled(
                egui::Rect::from_center_size(volume_thumb_center, egui::vec2(6.0, 12.0)),
                3.0,
                colors.text,
            );
            if volume_response.clicked() || volume_response.dragged() {
                if let Some(pointer) = volume_response.interact_pointer_pos() {
                    let fraction =
                        ((pointer.x - volume_track.left()) / volume_track.width()).clamp(0.0, 1.0);
                    settings.volume = (fraction * 100.0).round() as u8;
                }
            }
            if volume_response.dragged() {
                playback.set_volume(settings.volume);
            }
            if volume_response.clicked() || volume_response.drag_stopped() {
                playback.set_volume(settings.volume);
                if let Err(save_error) = settings.save() {
                    toasts.push(format!("Could not save settings: {save_error}"));
                }
            }

            let max_seconds = duration.map_or(0.0, |duration| duration.as_secs_f64());
            // Seconds until the time label or the knob next changes.
            let mut next_change = 1.0;
            ui.horizontal(|ui| {
                let (position_label, _) =
                    ui.allocate_exact_size(egui::vec2(38.0, 14.0), egui::Sense::hover());
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2((ui.available_width() - 46.0).max(1.0), 14.0),
                    if max_seconds > 0.0 {
                        egui::Sense::click_and_drag()
                    } else {
                        egui::Sense::hover()
                    },
                );
                let track_rect =
                    egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 3.0));
                let seconds_at = |pointer: egui::Pos2| {
                    max_seconds
                        * f64::from(
                            ((pointer.x - track_rect.left()) / track_rect.width()).clamp(0.0, 1.0),
                        )
                };
                // Dragging previews the target time; the decoder seeks once on release.
                let preview_id = response.id.with("seek-preview");
                if response.dragged() {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        ui.data_mut(|data| data.insert_temp(preview_id, seconds_at(pointer)));
                    }
                }
                let preview = ui.data(|data| data.get_temp::<f64>(preview_id));
                if response.drag_stopped() {
                    ui.data_mut(|data| data.remove::<f64>(preview_id));
                    if let Some(target) = preview {
                        playback.seek(&library.model.tracks, Duration::from_secs_f64(target));
                    }
                } else if response.clicked() {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        playback.seek(
                            &library.model.tracks,
                            Duration::from_secs_f64(seconds_at(pointer)),
                        );
                    }
                }
                let shown = preview.map_or_else(|| playback.position(), Duration::from_secs_f64);
                ui.painter().text(
                    egui::pos2(position_label.left(), position_label.center().y),
                    egui::Align2::LEFT_CENTER,
                    format_time(shown),
                    egui::FontId::proportional(11.0),
                    colors.subtle,
                );
                ui.painter().rect_filled(track_rect, 2.0, colors.surface);
                let fraction = if max_seconds > 0.0 {
                    (shown.as_secs_f64() / max_seconds).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                };
                // Repaint when the time label's second or the knob's pixel next changes, not at a fixed
                // rate: a long track moves the knob a pixel only every second or so.
                let position = shown.as_secs_f64();
                next_change = 1.0 - position.fract();
                let pixels = f64::from(track_rect.width() * ui.pixels_per_point());
                if max_seconds > 0.0 && pixels > 0.0 {
                    let seconds_per_pixel = max_seconds / pixels;
                    next_change = next_change.min(seconds_per_pixel - position % seconds_per_pixel);
                }
                // Fill and knob share one pixel-snapped edge so they never drift apart by a pixel.
                let knob_x = (track_rect.left() + track_rect.width() * fraction)
                    .round_to_pixels(ui.pixels_per_point());
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        track_rect.min,
                        egui::pos2(knob_x, track_rect.bottom()),
                    ),
                    2.0,
                    colors.accent,
                );
                if current.is_some() && duration.is_some() {
                    let thumb_center = egui::pos2(knob_x, rect.center().y);
                    ui.painter().rect_filled(
                        egui::Rect::from_center_size(thumb_center, egui::vec2(6.0, 12.0)),
                        3.0,
                        colors.text,
                    );
                }
                response.widget_info(|| {
                    egui::WidgetInfo::slider(
                        max_seconds > 0.0,
                        shown.as_secs_f64(),
                        "Playback position",
                    )
                });
                if max_seconds > 0.0 {
                    if let Some(pointer) = response.hover_pos() {
                        response.on_hover_text_at_pointer(format_time(Duration::from_secs_f64(
                            seconds_at(pointer),
                        )));
                    }
                }

                let (duration_label, _) =
                    ui.allocate_exact_size(egui::vec2(38.0, 14.0), egui::Sense::hover());
                ui.painter().text(
                    egui::pos2(duration_label.left(), duration_label.center().y),
                    egui::Align2::LEFT_CENTER,
                    duration.map(format_time).unwrap_or_default(),
                    egui::FontId::proportional(11.0),
                    colors.subtle,
                );
            });
            ui.painter().text(
                egui::pos2(volume_area.right() - 24.0, volume_area.center().y),
                egui::Align2::CENTER_CENTER,
                format!("{}%", settings.volume),
                egui::FontId::proportional(11.0),
                colors.muted,
            );
            if playback.is_playing() {
                ctx.request_repaint_after(
                    Duration::from_secs_f64(next_change).max(MIN_PLAYING_REPAINT),
                );
            }
            if let Some(album_index) = open_album {
                workbench.open_album(&mut library.view, album_index);
            }
        });
}

fn transport_icon_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: &'static str,
    icon: &str,
    tooltip: &str,
    active: bool,
    enabled: bool,
) -> egui::Response {
    let colors = theme::colors(ui.ctx());
    let response = ui.interact(
        rect,
        egui::Id::new(id),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    describe_button(&response, tooltip);
    let color = if active {
        colors.accent
    } else if response.hovered() && enabled {
        colors.text
    } else {
        colors.muted
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        theme::icon_font(20.0),
        color,
    );
    response.on_hover_text(tooltip)
}

/// Draw the current track's artwork, title, and artist.
///
/// # Returns
/// The album to open when the user clicked the track details.
fn show_track_info(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    library: &LibraryFeature,
    artwork_cache: &mut ArtworkCache,
    current: Option<usize>,
) -> Option<usize> {
    let colors = theme::colors(ui.ctx());
    let index = current?;
    let track = library.model.tracks.get(index)?;
    let album_index = library
        .model
        .track_album
        .get(index)
        .copied()
        .flatten()
        .map(crate::features::library::AlbumIndex::get);
    let texture = album_index.and_then(|album_index| {
        let bytes = library.model.albums.get(album_index)?.art.as_ref()?;
        artwork_cache.texture(ctx, album_index, bytes)
    });
    let start = ui.cursor().min;
    let cover = ui
        .allocate_space(egui::vec2(TRACK_ART_SIZE, TRACK_ART_SIZE))
        .1;
    if let Some(texture) = texture {
        ui.painter().image(
            texture.id(),
            cover,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    ui.add_space(9.0);
    let text = ui.vertical(|ui| {
        ui.add(
            egui::Label::new(
                egui::RichText::new(&track.title)
                    .size(15.0)
                    .strong()
                    .color(colors.text),
            )
            .truncate(),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(&track.artist)
                    .size(13.0)
                    .color(colors.muted),
            )
            .truncate(),
        );
    });
    let album_index = album_index?;
    let area = egui::Rect::from_min_max(start, cover.max.max(text.response.rect.max));
    let response = ui
        .interact(
            area,
            ui.id().with("now-playing-album"),
            egui::Sense::click(),
        )
        .on_hover_text("Go to album");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Go to playing album")
    });
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked().then_some(album_index)
}

fn show_audio_chips(
    ui: &mut egui::Ui,
    controls_rect: egui::Rect,
    volume_area: egui::Rect,
    track: &crate::features::library::Track,
) {
    let colors = theme::colors(ui.ctx());
    let left = controls_rect.right() + 14.0;
    let right = volume_area.left() - 10.0;
    if right <= left {
        return;
    }
    let file_type = track
        .path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("AUDIO")
        .to_ascii_uppercase();
    let audio = track.audio;

    let mut details = vec![file_type.clone()];
    let mut tooltip_details = vec![format!("File type: {file_type}")];
    if let Some(bitrate) = audio.bitrate_kbps {
        details.push(format!("{bitrate} kb/s"));
        tooltip_details.push(format!("Bitrate: {bitrate} kb/s"));
    }
    if let (Some(bit_depth), Some(sample_rate)) = (audio.bit_depth, audio.sample_rate_hz) {
        let sample_rate_khz = sample_rate as f32 / 1000.0;
        let sample_rate_label = if sample_rate % 1000 == 0 {
            format!("{}", sample_rate / 1000)
        } else {
            format!("{sample_rate_khz:.1}")
        };
        details.push(format!("{bit_depth}/{sample_rate_label}"));
        tooltip_details.push(format!(
            "{bit_depth}-bit, {sample_rate_khz:.1} kHz sample rate"
        ));
    } else if let Some(sample_rate) = audio.sample_rate_hz {
        let label = format!("{:.1} kHz", sample_rate as f32 / 1000.0);
        details.push(label.clone());
        tooltip_details.push(format!("Sample rate: {label}"));
    } else if let Some(bit_depth) = audio.bit_depth {
        details.push(format!("{bit_depth}-bit"));
        tooltip_details.push(format!("Bit depth: {bit_depth}-bit"));
    }

    let label = details.join("  •  ");
    let tooltip = tooltip_details.join("\n");
    let font = egui::FontId::proportional(10.0);
    let text_color = colors.muted;
    let galley = ui.painter().layout_no_wrap(label, font, text_color);
    let chip_width = galley.size().x + 16.0;
    let available_width = right - left;
    if chip_width <= available_width {
        let rect = egui::Rect::from_min_size(
            egui::pos2(right - chip_width, controls_rect.center().y - 11.0),
            egui::vec2(chip_width, 22.0),
        );
        ui.painter().rect(
            rect,
            3.0,
            colors.panel,
            egui::Stroke::new(1.0_f32, colors.border),
            egui::StrokeKind::Inside,
        );
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, text_color);
        ui.interact(rect, ui.id().with("audio-info-chip"), egui::Sense::hover())
            .on_hover_text(tooltip);
    }
}

fn format_time(time: Duration) -> String {
    let total = time.as_secs();
    format!("{}:{:02}", total / 60, total % 60)
}
