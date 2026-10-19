use super::screen::{
    album_format_label, describe_album, open_album, paint_cover_with_opacity, paint_format_badge,
    play_album, show_album_context_menu, show_shimmer_rect, AlbumScreen,
};
use crate::{
    platform::persistence::settings::Settings,
    shared::ui::{
        column_resize::{column_boundaries, show_column_handles},
        theme,
        toasts::Toasts,
    },
};
use eframe::egui;

const ARTWORK_COLUMN_WIDTH: f32 = 82.0;
const COLUMN_CONTENT_INSET: f32 = 12.0;

pub(super) fn show_header(
    ui: &mut egui::Ui,
    width: f32,
    settings: &mut Settings,
    toasts: &mut Toasts,
) {
    let colors = theme::colors(ui.ctx());
    const HEADER_HEIGHT: f32 = 32.0;
    let header = ui
        .allocate_exact_size(egui::vec2(width, HEADER_HEIGHT), egui::Sense::hover())
        .0;
    let start = header.left() + 8.0 + ARTWORK_COLUMN_WIDTH;
    let end = header.right() - 8.0;
    if show_column_handles(
        ui,
        header,
        start,
        end,
        &mut settings.album_list_column_widths,
        "album-list-column-resize",
    ) {
        if let Err(save_error) = settings.save() {
            toasts.push(format!(
                "Could not save album list column widths: {save_error}"
            ));
        }
    }
    let positions = column_boundaries(start, end, settings.album_list_column_widths);
    for (x, label) in [
        (positions[0] + COLUMN_CONTENT_INSET, "Title"),
        (positions[1] + COLUMN_CONTENT_INSET, "Artist"),
        (positions[2] + COLUMN_CONTENT_INSET, "Date"),
    ] {
        ui.painter().text(
            egui::pos2(x, header.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.0),
            colors.muted,
        );
    }
    ui.painter().line_segment(
        [header.left_bottom(), header.right_bottom()],
        egui::Stroke::new(1.0_f32, colors.border),
    );
}

pub(super) fn show_album_list(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    screen: &mut AlbumScreen<'_>,
    albums: &[usize],
    viewport: egui::Rect,
    width: f32,
    scroll_to: Option<usize>,
) {
    let colors = theme::colors(ui.ctx());
    const ROW_HEIGHT: f32 = 72.0;
    const ROW_GAP: f32 = 1.0;
    let row_step = ROW_HEIGHT + ROW_GAP;
    let rows = albums.len();
    let content = ui.max_rect();
    let rows_top = content.top();
    ui.set_height(rows as f32 * row_step);
    if let Some(position) = scroll_to {
        ui.scroll_to_rect(
            egui::Rect::from_min_size(
                egui::pos2(content.left(), rows_top + position as f32 * row_step),
                egui::vec2(width, ROW_HEIGHT),
            ),
            Some(egui::Align::Center),
        );
    }
    let first = ((viewport.min.y.max(0.0) / row_step).floor() as usize).min(rows);
    let last = ((viewport.max.y.max(0.0) / row_step).ceil() as usize + 1).min(rows);
    let row_left = content.left() + 8.0;
    let row_width = (width - 16.0).max(0.0);
    let columns = column_boundaries(
        row_left + ARTWORK_COLUMN_WIDTH,
        row_left + row_width - 8.0,
        screen.settings.album_list_column_widths,
    );
    let title_x = columns[0] + COLUMN_CONTENT_INSET;
    let artist_x = columns[1] + COLUMN_CONTENT_INSET;
    let date_x = columns[2] + COLUMN_CONTENT_INSET;
    let title_width = (columns[1] - title_x - 12.0).max(0.0);
    let artist_width = (columns[2] - artist_x - 12.0).max(0.0);
    let date_width = (columns[3] - date_x - 8.0).max(0.0);

    for (row, &album_index) in albums.iter().enumerate().take(last).skip(first) {
        let rect = egui::Rect::from_min_size(
            egui::pos2(row_left, rows_top + row as f32 * row_step),
            egui::vec2(row_width, ROW_HEIGHT),
        );
        let selected = screen.library.view.highlighted_album == Some(album_index);
        let response = ui.interact(
            rect,
            ui.id().with(("album-list-row", album_index)),
            egui::Sense::click(),
        );
        if let Some(album) = screen.library.model.albums.get(album_index) {
            describe_album(&response, album, selected);
        }
        if selected || response.hovered() {
            ui.painter().rect_filled(
                rect.shrink2(egui::vec2(0.0, 2.0)),
                0.0,
                if selected {
                    colors.selected
                } else {
                    colors.hover
                },
            );
        }

        let cover = screen.album_texture(ctx, album_index);
        let cover_opacity = screen.album_texture_opacity(ctx, album_index);
        let Some(album) = screen.library.model.albums.get(album_index) else {
            continue;
        };
        let cover_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 10.0, rect.center().y - 28.0),
            egui::vec2(56.0, 56.0),
        );
        if let Some(cover) = &cover {
            if cover_opacity < 1.0 {
                show_shimmer_rect(ui, cover_rect, 6.0);
            }
            paint_cover_with_opacity(ui, cover, cover_rect, cover_opacity);
        } else {
            ui.painter().rect_filled(cover_rect, 6.0, colors.surface);
            ui.painter().text(
                cover_rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::DISC,
                theme::icon_font(20.0),
                colors.muted,
            );
        }
        if screen.settings.album_format_badges {
            paint_format_badge(
                ui,
                cover_rect,
                album_format_label(&screen.library.model, album),
            );
        }

        let year = album
            .tracks
            .iter()
            .filter_map(|index| screen.library.model.tracks.get(*index)?.release_year)
            .min()
            .map_or_else(|| "—".to_owned(), |year| year.to_string());

        for (x, column_width, text, color) in [
            (title_x, title_width, album.title.as_str(), colors.text),
            (artist_x, artist_width, album.artist.as_str(), colors.muted),
            (date_x, date_width, year.as_str(), colors.muted),
        ] {
            let column_rect = egui::Rect::from_min_size(
                egui::pos2(x, rect.top()),
                egui::vec2(column_width, ROW_HEIGHT),
            );
            ui.painter().with_clip_rect(column_rect).text(
                egui::pos2(x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                text,
                egui::FontId::proportional(13.0),
                color,
            );
        }
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            egui::Stroke::new(1.0_f32, colors.border),
        );
        if response.double_clicked() {
            play_album(screen, album_index);
        } else if response.clicked() {
            open_album(ctx, screen, album_index);
        }
        show_album_context_menu(response, screen, album_index);
    }
}

pub(super) fn show_skeleton_list(ctx: &egui::Context, ui: &mut egui::Ui) {
    for _ in 0..6 {
        let (row, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 72.0), egui::Sense::hover());
        show_shimmer_rect(
            ui,
            egui::Rect::from_min_size(row.min + egui::vec2(10.0, 8.0), egui::vec2(56.0, 56.0)),
            6.0,
        );
        ui.add_space(1.0);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}
