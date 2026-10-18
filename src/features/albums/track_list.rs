use crate::shared::artwork::ArtworkCache;
use crate::{
    features::{library::state::LibraryFeature, lyrics::LyricsFeature, playback::Playback},
    platform::persistence::settings::Settings,
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
) -> Option<String> {
    let colors = crate::shared::ui::theme::colors(ctx);
    let Some(album_index) = library_feature
        .view
        .selected_album
        .filter(|index| library_feature.model.albums.get(*index).is_some())
    else {
        return show_empty_album_sidebar(ctx, settings);
    };

    let cover = library_feature
        .model
        .albums
        .get(album_index)
        .and_then(|album| {
            let bytes = album.art.as_deref()?;
            artwork_cache.texture(ctx, album_index, bytes)
        });
    let album = &library_feature.model.albums[album_index];
    let album_key = crate::features::library::sorting::album_sort_key(&album.artist, &album.title);
    let width = settings
        .right_panel_width
        .clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
    let persisted_lyrics_open = settings.lyrics_panel_open;
    let mut changed_settings = false;
    let library = &library_feature.model;
    let selected_track = &mut library_feature.view.selected_track;

    let output = egui::SidePanel::right("album-tracklist-side-panel")
        .resizable(true)
        .width_range(MIN_PANEL_WIDTH..=MAX_PANEL_WIDTH)
        .default_width(width)
        .frame(egui::Frame::new().fill(colors.panel).inner_margin(0))
        .show(ctx, |ui| {
            let panel_rect = ui.max_rect();
            draw_track_panel_background(ui);
            let panel_height = panel_rect.height();
            egui::Frame::new()
                .fill(colors.panel)
                .inner_margin(egui::Margin::symmetric(20, 18))
                .show(ui, |ui| {
                    let content_height = (panel_height - 36.0).max(0.0);
                    let content_width = (panel_rect.width() - 40.0).max(0.0);
                    ui.set_width(content_width);
                    ui.set_min_height(content_height);
                    ui.set_max_height(content_height);

                    let release_year = album
                        .tracks
                        .iter()
                        .filter_map(|index| library.tracks.get(*index)?.release_year)
                        .min()
                        .map_or_else(|| "—".to_owned(), |year| year.to_string());
                    draw_track_panel_heading(ui, album.tracks.len(), &release_year);
                    ui.add_space(15.0);
                    draw_album_summary(
                        ui,
                        cover.as_ref(),
                        &album.title,
                        &album.artist,
                        &album_key,
                        settings,
                        &mut changed_settings,
                    );
                    ui.add_space(15.0);
                    ui.separator();
                    ui.add_space(7.0);
                    let max_lyrics_height = show_tracks(
                        ctx,
                        ui,
                        library,
                        playback,
                        selected_track,
                        album_index,
                        settings.lyrics_panel_open,
                    );
                    crate::features::lyrics::panel::show(
                        ctx,
                        ui,
                        lyrics,
                        library,
                        playback.current,
                        playback.position(),
                        &mut settings.lyrics_panel_open,
                        max_lyrics_height,
                    );
                });
        });

    let mut save_error = None;
    if changed_settings {
        library_feature.view.album_sort_dirty = true;
    }
    if changed_settings || settings.lyrics_panel_open != persisted_lyrics_open {
        save_error = save_settings(settings);
    }

    let resize_id = egui::Id::new("album-tracklist-side-panel").with("__resize");
    let resize_response = ctx.read_response(resize_id);
    paint_panel_resize_grabber(ctx, output.response.rect, resize_response.as_ref(), colors);
    let resized = resize_response
        .as_ref()
        .is_some_and(|response| response.drag_stopped_by(egui::PointerButton::Primary));
    if resized {
        settings.right_panel_width = output
            .response
            .rect
            .width()
            .clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
        save_error = save_settings(settings);
    }
    save_error
}

fn show_empty_album_sidebar(
    ctx: &egui::Context,
    settings: &mut crate::platform::persistence::settings::Settings,
) -> Option<String> {
    let colors = crate::shared::ui::theme::colors(ctx);
    let width = settings
        .right_panel_width
        .clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
    let output = egui::SidePanel::right("album-tracklist-side-panel")
        .resizable(true)
        .width_range(MIN_PANEL_WIDTH..=MAX_PANEL_WIDTH)
        .default_width(width)
        .frame(egui::Frame::new().fill(colors.panel).inner_margin(0))
        .show(ctx, |ui| {
            draw_track_panel_background(ui);
            egui::Frame::new()
                .fill(colors.panel)
                .inner_margin(egui::Margin::symmetric(20, 18))
                .show(ui, |ui| {
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
                                egui::FontId::new(40.0, egui::FontFamily::Name("phosphor".into())),
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
                });
        });

    let resize_id = egui::Id::new("album-tracklist-side-panel").with("__resize");
    let resize_response = ctx.read_response(resize_id);
    paint_panel_resize_grabber(ctx, output.response.rect, resize_response.as_ref(), colors);
    if resize_response
        .as_ref()
        .is_some_and(|response| response.drag_stopped_by(egui::PointerButton::Primary))
    {
        settings.right_panel_width = output
            .response
            .rect
            .width()
            .clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
        return save_settings(settings);
    }
    None
}

fn paint_panel_resize_grabber(
    ctx: &egui::Context,
    panel_rect: egui::Rect,
    response: Option<&egui::Response>,
    colors: crate::shared::ui::theme::Palette,
) {
    let hovered = response.is_some_and(egui::Response::hovered);
    let dragged = response.is_some_and(egui::Response::dragged);
    if hovered || dragged {
        ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("album-tracklist-resize-grabber"),
    ));
    let center = egui::pos2(panel_rect.left() + 6.0, panel_rect.center().y);
    let color = if dragged { colors.accent } else { colors.muted };
    if hovered || dragged {
        painter.line_segment(
            [
                center - egui::vec2(0.0, 10.0),
                center + egui::vec2(0.0, 10.0),
            ],
            egui::Stroke::new(if dragged { 2.0_f32 } else { 1.0_f32 }, color),
        );
    }
}

fn draw_track_panel_background(ui: &egui::Ui) {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, colors.panel);
    ui.painter().line_segment(
        [rect.left_top(), rect.left_bottom()],
        egui::Stroke::new(1.0_f32, colors.border),
    );
}

fn draw_track_panel_heading(ui: &mut egui::Ui, track_count: usize, release_year: &str) {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
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

fn draw_album_summary(
    ui: &mut egui::Ui,
    cover: Option<&egui::TextureHandle>,
    title: &str,
    artist: &str,
    key: &str,
    settings: &mut crate::platform::persistence::settings::Settings,
    changed: &mut bool,
) {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
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
                egui::Label::new(egui::RichText::new(title).size(18.0).color(colors.text))
                    .truncate(),
            );
            ui.add(
                egui::Label::new(egui::RichText::new(artist).size(13.0).color(colors.muted))
                    .truncate(),
            );
            ui.add_space(3.0);
            draw_album_preferences(ui, key, settings, changed);
        });
    });
}

fn draw_album_preferences(
    ui: &mut egui::Ui,
    key: &str,
    settings: &mut crate::platform::persistence::settings::Settings,
    changed: &mut bool,
) {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
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
                let filled = value <= rating;
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if filled {
                        egui_phosphor::fill::STAR
                    } else {
                        egui_phosphor::regular::STAR
                    },
                    egui::FontId::new(
                        18.0,
                        egui::FontFamily::Name(
                            if filled { "phosphor-fill" } else { "phosphor" }.into(),
                        ),
                    ),
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
                    *changed = true;
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
            draw_action_hover(ui, heart, response.hovered());
            ui.painter().text(
                heart.center(),
                egui::Align2::CENTER_CENTER,
                if favorite {
                    egui_phosphor::fill::HEART
                } else {
                    egui_phosphor::regular::HEART
                },
                egui::FontId::new(
                    20.0,
                    egui::FontFamily::Name(
                        if favorite {
                            "phosphor-fill"
                        } else {
                            "phosphor"
                        }
                        .into(),
                    ),
                ),
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
                *changed = true;
            }
        },
    );
}

fn draw_action_hover(ui: &egui::Ui, rect: egui::Rect, hovered: bool) {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
    if hovered {
        ui.painter().rect_filled(rect, 7.0, colors.hover);
    }
}

fn save_settings(settings: &Settings) -> Option<String> {
    settings
        .save()
        .err()
        .map(|error| format!("Could not save settings: {error}"))
}

fn paint_cover(ui: &egui::Ui, texture: &egui::TextureHandle, rect: egui::Rect) {
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

fn show_tracks(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    library: &crate::features::library::Library,
    playback: &mut crate::features::playback::Playback,
    selected_track: &mut Option<usize>,
    album_index: usize,
    lyrics_open: bool,
) -> f32 {
    let Some(album) = library.albums.get(album_index) else {
        return 0.0;
    };
    let available_height = ui.available_height();
    let max_lyrics_content_height =
        crate::features::lyrics::panel::maximum_content_height(available_height);
    let lyrics_panel_height =
        crate::features::lyrics::panel::reserved_height(ctx, lyrics_open, available_height);
    let max_track_list_height = (available_height - lyrics_panel_height).max(100.0);
    let track_list_height = if lyrics_open {
        max_track_list_height
    } else {
        ((album.tracks.len() as f32 * 50.0) + 30.0)
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
            egui::ScrollArea::vertical()
                .id_salt("album-popout-tracks")
                .drag_to_scroll(false)
                .enable_scrolling(
                    pointer_inside && !crate::features::lyrics::panel::is_resizing(ctx),
                )
                .max_height(track_list_height)
                .show(ui, |ui| {
                    ui.add_space(12.0);
                    for (position, &track_index) in album.tracks.iter().enumerate() {
                        let Some(track) = library.tracks.get(track_index) else {
                            continue;
                        };
                        draw_track_row(
                            ui,
                            library,
                            playback,
                            selected_track,
                            album_index,
                            track_index,
                            position,
                            track,
                        );
                    }
                    ui.add_space(18.0);
                });
        },
    );
    max_lyrics_content_height
}

fn draw_track_row(
    ui: &mut egui::Ui,
    library: &crate::features::library::Library,
    playback: &mut crate::features::playback::Playback,
    selected_track: &mut Option<usize>,
    album_index: usize,
    track_index: usize,
    position: usize,
    track: &crate::features::library::Track,
) {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 50.0), egui::Sense::click());
    if response.secondary_clicked() {
        *selected_track = Some(track_index);
    }
    response.context_menu(|ui| {
        if ui.button("Add to queue").clicked() {
            playback.enqueue(track_index);
            ui.close_menu();
        }
    });
    let context_menu_open = response.context_menu_opened();
    let highlight = egui::Rect::from_min_max(rect.min, rect.max - egui::vec2(12.0, 0.0));
    let playing = playback.current == Some(track_index);
    let selected = *selected_track == Some(track_index);
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
        egui::FontId::new(
            if playing { 18.0 } else { 11.0 },
            if playing {
                egui::FontFamily::Name("phosphor".into())
            } else {
                egui::FontFamily::Proportional
            },
        ),
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
    if response.clicked() {
        if selected {
            if let Some(album) = library.albums.get(album_index) {
                playback.play_track_in_album(album, &library.tracks, track_index);
            }
        } else {
            *selected_track = Some(track_index);
        }
    }
}

fn format_duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
