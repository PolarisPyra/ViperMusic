use super::screen::{
    album_cover, album_description, album_interactions, prefetch_covers, shimmer, PREFETCH_ROWS,
};
use crate::{
    app::ViperApp,
    shared::ui::{
        controls::sz,
        theme::{self, Palette},
    },
};
use gpui::{div, prelude::*, uniform_list, AnyElement, Context, ScrollStrategy};
use gpui_component::scroll::ScrollableElement;
use std::{ops::Range, sync::Arc};

const TOP_PADDING: f32 = 16.0;
const ROW_GAP: f32 = 10.0;
/// Height of a card below its artwork: padding, gap, and two lines of text.
const CARD_TEXT_HEIGHT: f32 = 81.0;

/// Card sizes for a grid filling `width`.
#[derive(Clone, Copy)]
struct GridMetrics {
    columns: usize,
    card_width: f32,
    art_size: f32,
    gap: f32,
    left: f32,
}

/// Draw albums as a grid of cards, drawing only the rows in view.
///
/// # Arguments
/// * `app` - Application holding the library.
/// * `albums` - Albums to show, in order.
/// * `width` - Width of the grid at 100% zoom.
/// * `scroll_to` - Position in `albums` to scroll into view.
/// * `cx` - Application entity context.
///
/// # Returns
/// The grid element.
pub(super) fn render(
    app: &mut ViperApp,
    albums: &Arc<Vec<usize>>,
    width: f32,
    scroll_to: Option<usize>,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let metrics = grid_metrics(width);
    app.library.view.grid_columns = metrics.columns;
    let rows = albums.len().div_ceil(metrics.columns);
    let scroll = app.ui.albums_scroll.clone();
    if let Some(position) = scroll_to {
        scroll.scroll_to_item(position / metrics.columns, ScrollStrategy::Center);
    }
    let albums = Arc::clone(albums);
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(
            uniform_list(
                "album-grid-scroll",
                rows,
                cx.processor(move |app, range: Range<usize>, _, cx| {
                    let colors = theme::colors(cx);
                    let rows = range
                        .clone()
                        .map(|row| {
                            let start = row * metrics.columns;
                            let end = (start + metrics.columns).min(albums.len());
                            let cards = albums[start..end]
                                .iter()
                                .map(|&index| card(app, index, metrics, colors, cx))
                                .collect::<Vec<_>>();
                            div()
                                .flex()
                                .h(sz(metrics.art_size + CARD_TEXT_HEIGHT + ROW_GAP))
                                .pl(sz(metrics.left))
                                .gap(sz(metrics.gap))
                                .children(cards)
                        })
                        .collect::<Vec<_>>();
                    let cover = |row: usize| (row * metrics.columns).min(albums.len());
                    let above =
                        cover(range.start.saturating_sub(PREFETCH_ROWS))..cover(range.start);
                    let below = cover(range.end)..cover(range.end + PREFETCH_ROWS);
                    prefetch_covers(app, &albums[above]);
                    prefetch_covers(app, &albums[below]);
                    rows
                }),
            )
            .track_scroll(&scroll)
            .pt(sz(TOP_PADDING))
            .size_full(),
        )
        .vertical_scrollbar(&scroll)
        .into_any_element()
}

/// Draw one album card: cover, title, and artist with the track count.
fn card(
    app: &mut ViperApp,
    index: usize,
    metrics: GridMetrics,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let cover = album_cover(app, index, metrics.art_size, 8.0, colors);
    let Some(album) = app.library.model.albums.get(index) else {
        return div().into_any_element();
    };
    let selected = app.library.view.highlighted_album == Some(index);
    let element = div()
        .id(("album-card", index))
        .role(gpui::Role::ListItem)
        .aria_label(album_description(album))
        .flex()
        .flex_col()
        .flex_none()
        .w(sz(metrics.card_width))
        .h(sz(metrics.art_size + CARD_TEXT_HEIGHT))
        .p(sz(6.0))
        .gap(sz(9.0))
        .rounded(sz(3.0))
        .cursor_pointer()
        .when(selected, |card| card.bg(colors.selected))
        .when(!selected, |card| card.hover(|style| style.bg(colors.hover)))
        .child(cover)
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .text_size(sz(14.0))
                        .text_color(colors.text)
                        .truncate()
                        .child(album.title.clone()),
                )
                .child(
                    div()
                        .text_size(sz(12.0))
                        .text_color(colors.muted)
                        .truncate()
                        .child(format!("{} · {} tracks", album.artist, album.tracks.len())),
                ),
        );
    album_interactions(element, index, cx).into_any_element()
}

/// Placeholder cards shown while the first scan runs.
pub(super) fn skeleton(app: &ViperApp, width: f32, colors: Palette) -> AnyElement {
    let metrics = grid_metrics(width);
    let rows = (0..3).map(|_| {
        div()
            .flex()
            .pl(sz(metrics.left))
            .gap(sz(metrics.gap))
            .children((0..metrics.columns).map(|_| {
                div()
                    .flex_none()
                    .w(sz(metrics.card_width))
                    .h(sz(metrics.art_size + CARD_TEXT_HEIGHT))
                    .p(sz(6.0))
                    .child(shimmer(app, 8.0, colors).size(sz(metrics.art_size)))
            }))
    });
    div()
        .flex()
        .flex_col()
        .pt(sz(TOP_PADDING))
        .gap(sz(ROW_GAP))
        .children(rows)
        .into_any_element()
}

fn grid_metrics(width: f32) -> GridMetrics {
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
    GridMetrics {
        columns,
        card_width,
        art_size,
        gap,
        left: LEFT_PADDING,
    }
}
