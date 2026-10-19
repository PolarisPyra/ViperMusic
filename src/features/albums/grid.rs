use super::screen::{
    album_format_label, describe_album, open_album, paint_cover_with_opacity, paint_format_badge,
    play_album, show_album_context_menu, show_shimmer_rect, AlbumScreen,
};
use crate::shared::ui::theme;
use eframe::egui;

pub(super) fn show_album_rows(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    screen: &mut AlbumScreen<'_>,
    albums: &[usize],
    viewport: egui::Rect,
    width: f32,
    scroll_to: Option<usize>,
) {
    const TOP_PADDING: f32 = 16.0;
    const ROW_GAP: f32 = 10.0;
    let (columns, card_width, art_size, gap, left) = grid_metrics(width);
    screen.library.view.grid_columns = columns;
    let row_height = art_size + 81.0;
    let row_step = row_height + ROW_GAP;
    let rows = albums.len().div_ceil(columns);
    let content = ui.max_rect();
    ui.set_height(TOP_PADDING + rows as f32 * row_step);
    if let Some(position) = scroll_to {
        let top = content.top() + TOP_PADDING + (position / columns) as f32 * row_step;
        ui.scroll_to_rect(
            egui::Rect::from_min_size(
                egui::pos2(content.left(), top),
                egui::vec2(width, row_height),
            ),
            Some(egui::Align::Center),
        );
    }
    let first = (((viewport.min.y - TOP_PADDING).max(0.0) / row_step).floor() as usize).min(rows);
    let last =
        ((((viewport.max.y - TOP_PADDING).max(0.0) / row_step).ceil() as usize) + 1).min(rows);
    for row in first..last {
        let row_rect = egui::Rect::from_min_size(
            egui::pos2(
                content.left(),
                content.top() + TOP_PADDING + row as f32 * row_step,
            ),
            egui::vec2(ui.available_width(), row_height),
        );
        let mut row_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Min)),
        );
        row_ui.add_space(left);
        row_ui.spacing_mut().item_spacing.x = gap;
        let start = row * columns;
        for &album_index in albums.iter().skip(start).take(columns) {
            show_album_card(
                ctx,
                &mut row_ui,
                screen,
                album_index,
                card_width,
                art_size,
                row_height,
            );
        }
    }
}

fn show_album_card(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    screen: &mut AlbumScreen<'_>,
    index: usize,
    card_width: f32,
    art_size: f32,
    card_height: f32,
) {
    let colors = theme::colors(ui.ctx());
    if screen.library.model.albums.get(index).is_none() {
        return;
    }
    let cover = screen.album_texture(ctx, index);
    let cover_opacity = screen.album_texture_opacity(ctx, index);
    let album = &screen.library.model.albums[index];
    let selected = screen.library.view.highlighted_album == Some(index);
    let response = ui.allocate_ui_with_layout(
        egui::vec2(card_width, card_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            egui::Frame::new()
                .fill(if selected {
                    colors.selected
                } else {
                    egui::Color32::TRANSPARENT
                })
                .inner_margin(egui::Margin::same(6))
                .corner_radius(3)
                .show(ui, |ui| {
                    let art = ui.allocate_space(egui::vec2(art_size, art_size)).1;
                    if ui.is_rect_visible(art) {
                        if let Some(cover) = &cover {
                            if cover_opacity < 1.0 {
                                show_shimmer_rect(ui, art, 8.0);
                            }
                            paint_cover_with_opacity(ui, cover, art, cover_opacity);
                        } else {
                            ui.painter().rect_filled(art, 8.0, colors.surface);
                            ui.painter().text(
                                art.center(),
                                egui::Align2::CENTER_CENTER,
                                egui_phosphor::regular::DISC,
                                theme::icon_font(32.0),
                                colors.muted,
                            );
                        }
                        if screen.settings.album_format_badges {
                            paint_format_badge(
                                ui,
                                art,
                                album_format_label(&screen.library.model, album),
                            );
                        }
                    }
                    ui.add_space(9.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&album.title)
                                .color(colors.text)
                                .size(14.0),
                        )
                        .truncate(),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "{} · {} tracks",
                                album.artist,
                                album.tracks.len()
                            ))
                            .color(colors.muted)
                            .size(12.0),
                        )
                        .truncate(),
                    );
                });
        },
    );
    let response = response.response.interact(egui::Sense::click());
    describe_album(&response, album, selected);
    if response.double_clicked() {
        play_album(screen, index);
    } else if response.clicked() {
        open_album(ctx, screen, index);
    }
    show_album_context_menu(response, screen, index);
}

pub(super) fn show_skeleton_albums(ctx: &egui::Context, ui: &mut egui::Ui, width: f32) {
    let (columns, card_width, art_size, gap, left) = grid_metrics(width);
    let card_height = art_size + 81.0;
    ui.add_space(16.0);
    for _ in 0..3 {
        ui.horizontal(|ui| {
            ui.add_space(left);
            ui.spacing_mut().item_spacing.x = gap;
            for _ in 0..columns {
                ui.allocate_ui_with_layout(
                    egui::vec2(card_width, card_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::same(6))
                            .corner_radius(10.0)
                            .show(ui, |ui| {
                                let art = ui.allocate_space(egui::vec2(art_size, art_size)).1;
                                show_shimmer_rect(ui, art, 8.0);
                            });
                    },
                );
            }
        });
        ui.add_space(10.0);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

fn grid_metrics(width: f32) -> (usize, f32, f32, f32, f32) {
    const LEFT_PADDING: f32 = 12.0;
    const RIGHT_PADDING: f32 = 18.0;
    const MIN_GAP: f32 = 12.0;
    const CARD_WIDTH: f32 = 166.0;
    const MIN_COLUMNS: usize = 3;
    let area = (width - LEFT_PADDING - RIGHT_PADDING).max(0.0);
    let natural_columns = ((area + MIN_GAP) / (CARD_WIDTH + MIN_GAP)).floor() as usize;
    let columns = natural_columns.max(MIN_COLUMNS);
    let card_width = if natural_columns < MIN_COLUMNS {
        ((area - (columns - 1) as f32 * MIN_GAP) / columns as f32).max(48.0)
    } else {
        CARD_WIDTH
    };
    let art_size = (card_width - 12.0).max(36.0);
    let gap = ((area - columns as f32 * card_width) / (columns - 1) as f32).max(MIN_GAP);
    (columns, card_width, art_size, gap, LEFT_PADDING)
}
