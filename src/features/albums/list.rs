use super::screen::{album_cover, album_description, album_interactions, prefetch_covers, shimmer};
use crate::{
    app::ViperApp,
    shared::ui::{
        column_resize::{column_fractions, column_handle, drag_divider, ColumnDrag},
        controls::{listen, sz, unzoomed},
        theme::{self, Palette},
    },
};
use gpui::{
    div, prelude::*, px, relative, uniform_list, AnyElement, Context, Div, DragMoveEvent, Hsla,
    ScrollStrategy, SharedString,
};
use gpui_component::scroll::ScrollableElement;
use std::{ops::Range, sync::Arc};

/// Name of the album list's columns for divider drags.
const TABLE: &str = "album-list";
const ARTWORK_COLUMN_WIDTH: f32 = 82.0;
const COLUMN_CONTENT_INSET: f32 = 12.0;
/// Space between a row and the list's edges.
const ROW_INSET: f32 = 8.0;
const ROW_HEIGHT: f32 = 72.0;
const ROW_GAP: f32 = 1.0;
/// Rows outside the view whose covers are loaded ahead, on either side.
const PREFETCH_ROWS: usize = 6;

/// Draw the list's column titles, whose dividers can be dragged to resize the columns.
///
/// # Arguments
/// * `app` - Application holding the column widths.
/// * `cx` - Application entity context.
///
/// # Returns
/// The header element.
pub(super) fn header(app: &ViperApp, cx: &mut Context<ViperApp>) -> AnyElement {
    let colors = theme::colors(cx);
    let fractions = column_fractions(app.settings.album_list_column_widths);
    let dragging = app
        .ui
        .column_drag
        .filter(|(table, _)| *table == TABLE)
        .map(|(_, boundary)| boundary);
    let titles = ["Title", "Artist", "Date"]
        .into_iter()
        .zip(fractions)
        .map(|(title, fraction)| {
            column(fraction)
                .text_size(sz(12.0))
                .text_color(colors.muted)
                .child(title)
        });
    let span = div()
        .id("album-list-columns")
        .relative()
        .flex()
        .flex_1()
        .h_full()
        .children(titles)
        .child(column_handle(
            TABLE,
            1,
            fractions[0],
            dragging == Some(1),
            colors,
        ))
        .child(column_handle(
            TABLE,
            2,
            fractions[0] + fractions[1],
            dragging == Some(2),
            colors,
        ))
        .on_drag_move(listen(
            cx,
            |app, event: &DragMoveEvent<ColumnDrag>, window, cx| {
                let drag = event.drag(cx);
                if drag.table != TABLE {
                    return;
                }
                let boundary = drag.boundary;
                let x = unzoomed(event.event.position.x - event.bounds.left(), window);
                let width = unzoomed(event.bounds.size.width, window);
                drag_divider(
                    &mut app.settings.album_list_column_widths,
                    boundary,
                    x,
                    width,
                );
                app.ui.column_drag = Some((TABLE, boundary));
            },
        ));
    div()
        .flex()
        .flex_none()
        .h(sz(32.0))
        .px(sz(ROW_INSET))
        .border_b_1()
        .border_color(colors.border)
        .child(div().flex_none().w(sz(ARTWORK_COLUMN_WIDTH)))
        .child(span)
        .into_any_element()
}

/// One flexible column taking `fraction` of the row's span.
fn column(fraction: f32) -> Div {
    div()
        .flex()
        .items_center()
        .flex_none()
        .w(relative(fraction))
        .h_full()
        .pl(sz(COLUMN_CONTENT_INSET))
        .pr(sz(12.0))
        .overflow_hidden()
}

/// Draw albums as rows of a table, drawing only the rows in view.
///
/// # Arguments
/// * `app` - Application holding the library.
/// * `albums` - Albums to show, in order.
/// * `scroll_to` - Position in `albums` to scroll into view.
/// * `cx` - Application entity context.
///
/// # Returns
/// The list element.
pub(super) fn render(
    app: &mut ViperApp,
    albums: &Arc<Vec<usize>>,
    scroll_to: Option<usize>,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let scroll = app.ui.albums_scroll.clone();
    if let Some(position) = scroll_to {
        scroll.scroll_to_item(position, ScrollStrategy::Center);
    }
    let albums = Arc::clone(albums);
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(
            uniform_list(
                "album-list-scroll",
                albums.len(),
                cx.processor(move |app, range: Range<usize>, _, cx| {
                    let colors = theme::colors(cx);
                    let rows = range
                        .clone()
                        .map(|row| row_element(app, albums[row], colors, cx))
                        .collect::<Vec<_>>();
                    let above = range.start.saturating_sub(PREFETCH_ROWS);
                    let below = (range.end + PREFETCH_ROWS).min(albums.len());
                    prefetch_covers(app, &albums[above..range.start]);
                    prefetch_covers(app, &albums[range.end..below]);
                    rows
                }),
            )
            .track_scroll(&scroll)
            .px(sz(ROW_INSET))
            .size_full(),
        )
        .vertical_scrollbar(&scroll)
        .into_any_element()
}

/// Draw one album row: cover, title, artist, and release year.
fn row_element(
    app: &mut ViperApp,
    index: usize,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let cover = album_cover(app, index, 56.0, 6.0, colors);
    let library = &app.library.model;
    let Some(album) = library.albums.get(index) else {
        return div().h(sz(ROW_HEIGHT + ROW_GAP)).into_any_element();
    };
    let selected = app.library.view.highlighted_album == Some(index);
    let year = album
        .tracks
        .iter()
        .filter_map(|&track| library.tracks.get(track)?.release_year)
        .min()
        .map_or_else(|| "—".to_owned(), |year| year.to_string());
    let fractions = column_fractions(app.settings.album_list_column_widths);
    let cell = |fraction: f32, text: SharedString, color: Hsla| {
        column(fraction)
            .text_size(sz(13.0))
            .text_color(color)
            .child(div().truncate().child(text))
    };
    let row = div()
        .id(("album-list-row", index))
        .role(gpui::Role::ListItem)
        .aria_label(album_description(album))
        .flex()
        .items_center()
        .w_full()
        .h(sz(ROW_HEIGHT))
        .my(px(2.0))
        .border_b_1()
        .border_color(colors.border)
        .cursor_pointer()
        .when(selected, |row| row.bg(colors.selected))
        .when(!selected, |row| row.hover(|style| style.bg(colors.hover)))
        .child(
            div()
                .flex_none()
                .w(sz(ARTWORK_COLUMN_WIDTH))
                .pl(sz(10.0))
                .child(cover),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .h_full()
                .child(cell(fractions[0], album.title.clone().into(), colors.text))
                .child(cell(
                    fractions[1],
                    album.artist.clone().into(),
                    colors.muted,
                ))
                .child(cell(fractions[2], year.into(), colors.muted)),
        );
    div()
        .h(sz(ROW_HEIGHT + ROW_GAP))
        .child(album_interactions(row, index, cx))
        .into_any_element()
}

/// Placeholder rows shown while the first scan runs.
pub(super) fn skeleton(app: &ViperApp, colors: Palette) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .children((0..6).map(|_| {
            div()
                .h(sz(ROW_HEIGHT + ROW_GAP))
                .pl(sz(10.0))
                .pt(sz(8.0))
                .child(shimmer(app, 6.0, colors).size(sz(56.0)))
        }))
        .into_any_element()
}
