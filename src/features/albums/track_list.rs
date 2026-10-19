use crate::{
    features::{
        library::{state::LibraryFeature, Album, Library},
        lyrics::LyricsFeature,
        playback::Playback,
        track_menu,
    },
    platform::persistence::settings::Settings,
    shared::{
        artwork::ArtworkCache,
        ui::{
            theme::{self, icon_fill_font, icon_font},
            toasts::Toasts,
        },
    },
};
use eframe::egui;

const MIN_PANEL_WIDTH: f32 = 260.0;
const MAX_PANEL_WIDTH: f32 = 520.0;

pub(crate) fn show(
    ctx: &egui::Context,
    library_feature: &mut LibraryFeature,
    playback: &mut Playback,
    lyrics: &mut LyricsFeature,
    settings: &mut Settings,
    artwork_cache: &mut ArtworkCache,
    toasts: &mut Toasts,
) {
    let Some(album_index) = library_feature
        .view
        .selected_album
        .filter(|index| library_feature.model.albums.get(*index).is_some())
    else {
        side_panel(ctx, settings, toasts, |ui, _, _| show_empty_album(ui));
        return;
    };

    let library = &library_feature.model;
    let album = &library.albums[album_index];
    let cover = album
        .art
        .as_ref()
        .and_then(|bytes| artwork_cache.texture(ctx, album_index, bytes));
    let persisted_lyrics_open = settings.lyrics_panel_open;
    let mut album_state_changed = false;
    let selected_track = &mut library_feature.view.selected_track;
    let scroll_to_selection = std::mem::take(&mut library_feature.view.scroll_to_selection);

    side_panel(ctx, settings, toasts, |ui, settings, toasts| {
        let content = ui.max_rect();
        ui.set_width(content.width());
        ui.set_min_height(content.height());
        ui.set_max_height(content.height());

        let release_year = album
            .tracks
            .iter()
            .filter_map(|index| library.tracks.get(*index)?.release_year)
            .min()
            .map_or_else(|| "—".to_owned(), |year| year.to_string());
        draw_track_panel_heading(ui, album.tracks.len(), &release_year);
        ui.add_space(15.0);
        album_state_changed = draw_album_summary(ui, cover.as_ref(), album, settings);
        ui.add_space(15.0);
        ui.separator();
        ui.add_space(7.0);
        let max_lyrics_height = show_tracks(
            ctx,
            ui,
            TrackRows {
                library,
                album,
                playback: &mut *playback,
                selected_track,
                toasts,
            },
            settings.lyrics_panel_open,
            scroll_to_selection,
        );
        crate::features::lyrics::panel::show(
            ctx,
            ui,
            lyrics,
            library,
            playback,
            &mut settings.lyrics_panel_open,
            max_lyrics_height,
        );
    });

    if album_state_changed {
        library_feature.view.album_sort_dirty = true;
        if let Err(error) = settings.save_album_state([album.key.as_str()]) {
            toasts.push(format!("Could not save album state: {error}"));
        }
    }
    if settings.lyrics_panel_open != persisted_lyrics_open {
        save_settings(settings, toasts);
    }
}

/// Draw the resizable album side panel and persist its width after a resize.
fn side_panel(
    ctx: &egui::Context,
    settings: &mut Settings,
    toasts: &mut Toasts,
    add_contents: impl FnOnce(&mut egui::Ui, &mut Settings, &mut Toasts),
) {
    const PANEL_ID: &str = "album-tracklist-side-panel";
    let colors = theme::colors(ctx);
    let output = egui::SidePanel::right(PANEL_ID)
        .resizable(true)
        .width_range(MIN_PANEL_WIDTH..=MAX_PANEL_WIDTH)
        .default_width(
            settings
                .right_panel_width
                .clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH),
        )
        .frame(egui::Frame::new().fill(colors.panel).inner_margin(0))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            ui.painter().line_segment(
                [rect.left_top(), rect.left_bottom()],
                egui::Stroke::new(1.0_f32, colors.border),
            );
            egui::Frame::new()
                .fill(colors.panel)
                .inner_margin(egui::Margin::symmetric(20, 18))
                .show(ui, |ui| add_contents(ui, settings, toasts));
        });

    let resize_response = ctx.read_response(egui::Id::new(PANEL_ID).with("__resize"));
    let hovered = resize_response
        .as_ref()
        .is_some_and(egui::Response::hovered);
    let dragged = resize_response
        .as_ref()
        .is_some_and(egui::Response::dragged);
    if hovered || dragged {
        ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        let center = egui::pos2(
            output.response.rect.left() + 6.0,
            output.response.rect.center().y,
        );
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("album-tracklist-resize-grabber"),
        ))
        .line_segment(
            [
                center - egui::vec2(0.0, 10.0),
                center + egui::vec2(0.0, 10.0),
            ],
            if dragged {
                egui::Stroke::new(2.0_f32, colors.accent)
            } else {
                egui::Stroke::new(1.0_f32, colors.muted)
            },
        );
    }
    if resize_response
        .as_ref()
        .is_some_and(|response| response.drag_stopped_by(egui::PointerButton::Primary))
    {
        settings.right_panel_width = output
            .response
            .rect
            .width()
            .clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
        save_settings(settings, toasts);
    }
}

fn show_empty_album(ui: &mut egui::Ui) {
    let colors = theme::colors(ui.ctx());
    ui.set_min_width(ui.available_width());
    ui.label(
        egui::RichText::new("ALBUM TRACKS")
            .size(10.0)
            .strong()
            .color(colors.subtle),
    );
    ui.add_space(12.0);
    ui.separator();
    ui.allocate_ui_with_layout(
        egui::vec2(
            ui.available_width(),
            (ui.available_height() - 40.0).max(0.0),
        ),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.add_space(46.0);
            let art_rect = ui.allocate_space(egui::vec2(136.0, 136.0)).1;
            ui.painter().rect_filled(art_rect, 8.0, colors.surface);
            ui.painter().text(
                art_rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::DISC,
                icon_font(40.0),
                colors.muted,
            );
            ui.add_space(18.0);
            ui.label(
                egui::RichText::new("No album loaded")
                    .size(16.0)
                    .color(colors.text),
            );
        },
    );
}

fn draw_track_panel_heading(ui: &mut egui::Ui, track_count: usize, release_year: &str) {
    let colors = theme::colors(ui.ctx());
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("ALBUM TRACKS")
                    .size(10.0)
                    .strong()
                    .color(colors.subtle),
            );
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(
                    egui::RichText::new(format!("{track_count} songs"))
                        .size(12.0)
                        .color(colors.muted),
                );
                ui.label(egui::RichText::new("·").size(12.0).color(colors.subtle));
                ui.label(
                    egui::RichText::new(release_year)
                        .size(12.0)
                        .color(colors.muted),
                );
            });
        });
    });
}

/// Draw album artwork, title, rating, and favorite controls.
///
/// # Returns
/// Whether the album's rating or favorite state changed.
fn draw_album_summary(
    ui: &mut egui::Ui,
    cover: Option<&egui::TextureHandle>,
    album: &Album,
    settings: &mut Settings,
) -> bool {
    let colors = theme::colors(ui.ctx());
    ui.horizontal(|ui| {
        if let Some(texture) = cover {
            let rect = ui.allocate_space(egui::vec2(80.0, 80.0)).1;
            paint_cover(ui, texture, rect);
            ui.add_space(12.0);
        }
        ui.vertical(|ui| {
            ui.add_space(12.0);
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&album.title)
                        .size(18.0)
                        .color(colors.text),
                )
                .truncate(),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&album.artist)
                        .size(13.0)
                        .color(colors.muted),
                )
                .truncate(),
            );
            ui.add_space(3.0);
            draw_album_preferences(ui, &album.key, settings)
        })
        .inner
    })
    .inner
}

/// Draw the star rating and favorite toggle for an album.
///
/// # Returns
/// Whether the album's rating or favorite state changed.
fn draw_album_preferences(ui: &mut egui::Ui, key: &str, settings: &mut Settings) -> bool {
    let colors = theme::colors(ui.ctx());
    let mut changed = false;
    let rating = settings.album_ratings.get(key).copied().unwrap_or_default();
    let favorite = settings.favorite_albums.contains(key);
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(width, 24.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let row = ui.max_rect();
            ui.spacing_mut().item_spacing.x = 1.0;
            for value in 1..=5 {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(22.0, 24.0), egui::Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::RadioButton,
                        true,
                        value == rating,
                        format!("Rate {value} out of 5"),
                    )
                });
                let filled = value <= rating;
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if filled {
                        egui_phosphor::fill::STAR
                    } else {
                        egui_phosphor::regular::STAR
                    },
                    if filled {
                        icon_fill_font(18.0)
                    } else {
                        icon_font(18.0)
                    },
                    if filled {
                        colors.warning
                    } else {
                        colors.subtle
                    },
                );
                let tooltip = if value == rating {
                    "Clear rating".to_owned()
                } else {
                    format!("Rate {value} out of 5")
                };
                if response.on_hover_text(tooltip).clicked() {
                    if value == rating {
                        settings.album_ratings.remove(key);
                    } else {
                        settings.album_ratings.insert(key.to_owned(), value);
                    }
                    changed = true;
                }
            }
            let heart = egui::Rect::from_center_size(
                egui::pos2(row.right() - 15.0, row.center().y),
                egui::vec2(30.0, 30.0),
            );
            let response = ui.interact(
                heart,
                ui.id().with("album-favorite-heart"),
                egui::Sense::click(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, favorite, "Favorite")
            });
            if response.hovered() {
                ui.painter().rect_filled(heart, 7.0, colors.hover);
            }
            ui.painter().text(
                heart.center(),
                egui::Align2::CENTER_CENTER,
                if favorite {
                    egui_phosphor::fill::HEART
                } else {
                    egui_phosphor::regular::HEART
                },
                if favorite {
                    icon_fill_font(20.0)
                } else {
                    icon_font(20.0)
                },
                if favorite {
                    colors.danger
                } else if response.hovered() {
                    colors.text
                } else {
                    colors.muted
                },
            );
            if response
                .on_hover_text(if favorite {
                    "Remove from favorites"
                } else {
                    "Add to favorites"
                })
                .clicked()
            {
                if favorite {
                    settings.favorite_albums.remove(key);
                } else {
                    settings.favorite_albums.insert(key.to_owned());
                }
                changed = true;
            }
        },
    );
    changed
}

fn save_settings(settings: &Settings, toasts: &mut Toasts) {
    if let Err(error) = settings.save() {
        toasts.push(format!("Could not save settings: {error}"));
    }
}

fn paint_cover(ui: &egui::Ui, texture: &egui::TextureHandle, rect: egui::Rect) {
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

/// Inputs shared by every row of the album track list.
struct TrackRows<'a> {
    library: &'a Library,
    album: &'a Album,
    playback: &'a mut Playback,
    selected_track: &'a mut Option<usize>,
    toasts: &'a mut Toasts,
}

const TRACK_ROW_HEIGHT: f32 = 50.0;

fn show_tracks(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    mut rows: TrackRows<'_>,
    lyrics_open: bool,
    scroll_to_selection: bool,
) -> f32 {
    let album = rows.album;
    let available_height = ui.available_height();
    let max_lyrics_content_height =
        crate::features::lyrics::panel::maximum_content_height(available_height);
    let lyrics_panel_height =
        crate::features::lyrics::panel::reserved_height(ctx, lyrics_open, available_height);
    let max_track_list_height = (available_height - lyrics_panel_height).max(100.0);
    let track_list_height = if lyrics_open {
        max_track_list_height
    } else {
        ((album.tracks.len() as f32 * TRACK_ROW_HEIGHT) + 30.0)
            .min(max_track_list_height)
            .max(100.0)
    };
    let track_list_rect = egui::Rect::from_min_size(
        ui.available_rect_before_wrap().min,
        egui::vec2(ui.available_width(), track_list_height),
    );
    let pointer_inside = ctx.input(|input| {
        input
            .pointer
            .hover_pos()
            .is_some_and(|position| track_list_rect.contains(position))
    });
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), track_list_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            let mut scroll = egui::ScrollArea::vertical()
                .id_salt("album-popout-tracks")
                .drag_to_scroll(false)
                .enable_scrolling(
                    pointer_inside && !crate::features::lyrics::panel::is_resizing(ctx),
                )
                .max_height(track_list_height);
            if let Some(position) = rows
                .selected_track
                .filter(|_| scroll_to_selection)
                .and_then(|track| album.tracks.iter().position(|&index| index == track))
            {
                let offset = 12.0 + position as f32 * TRACK_ROW_HEIGHT
                    - (track_list_height - TRACK_ROW_HEIGHT) * 0.5;
                scroll = scroll.vertical_scroll_offset(offset.max(0.0));
            }
            scroll.show(ui, |ui| {
                ui.add_space(12.0);
                for (position, &track_index) in album.tracks.iter().enumerate() {
                    rows.show(ui, position, track_index);
                }
                ui.add_space(18.0);
            });
        },
    );
    max_lyrics_content_height
}

impl TrackRows<'_> {
    /// Draw one track; click selects it and double-click plays the album from it.
    fn show(&mut self, ui: &mut egui::Ui, position: usize, track_index: usize) {
        let TrackRows {
            library,
            album,
            playback,
            selected_track,
            toasts,
        } = self;
        let Some(track) = library.tracks.get(track_index) else {
            return;
        };
        let colors = theme::colors(ui.ctx());
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), TRACK_ROW_HEIGHT),
            egui::Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                true,
                **selected_track == Some(track_index),
                &track.title,
            )
        });
        if response.clicked() || response.secondary_clicked() {
            **selected_track = Some(track_index);
        }
        if response.double_clicked() {
            playback.play_in_context(&album.tracks, &library.tracks, track_index);
        }
        response.context_menu(|ui| {
            track_menu::show(ui, playback, library, track_index, false, toasts);
        });
        let context_menu_open = response.context_menu_opened();
        let highlight = egui::Rect::from_min_max(rect.min, rect.max - egui::vec2(12.0, 0.0));
        let playing = playback.current == Some(track_index);
        let selected = **selected_track == Some(track_index);
        let highlighted = selected || context_menu_open;
        if highlighted {
            ui.painter().rect_filled(highlight, 3.0, colors.selected);
        }

        let leading =
            egui::Rect::from_min_size(rect.min + egui::vec2(18.0, 14.0), egui::vec2(20.0, 20.0));
        let (glyph, color) = if playing {
            (
                if playback.is_playing() {
                    egui_phosphor::regular::PAUSE
                } else {
                    egui_phosphor::regular::PLAY
                },
                colors.accent,
            )
        } else {
            ("", colors.subtle)
        };
        ui.painter().text(
            leading.center(),
            egui::Align2::CENTER_CENTER,
            if playing {
                glyph.to_owned()
            } else {
                format!("{:02}", position + 1)
            },
            if playing {
                icon_font(18.0)
            } else {
                egui::FontId::proportional(11.0)
            },
            color,
        );

        let text_rect = egui::Rect::from_min_max(
            rect.min + egui::vec2(50.0, 7.0),
            rect.max - egui::vec2(58.0, 5.0),
        );
        let mut text_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        text_ui.add(
            egui::Label::new(
                egui::RichText::new(&track.title)
                    .size(12.0)
                    .color(if highlighted {
                        colors.accent
                    } else {
                        colors.text
                    }),
            )
            .truncate(),
        );
        text_ui.add(
            egui::Label::new(
                egui::RichText::new(&track.artist)
                    .size(10.0)
                    .color(colors.muted),
            )
            .truncate(),
        );
        if let Some(duration) = track.duration_ms {
            ui.painter().text(
                highlight.right_center() - egui::vec2(10.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                format_duration(duration),
                egui::FontId::proportional(10.0),
                colors.subtle,
            );
        }
    }
}

fn format_duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
