use crate::{
    app::ViperApp,
    features::track_menu,
    shared::ui::{
        column_resize::{column_fractions, column_handle, drag_divider, ColumnDrag},
        controls::{label, listen, search_field, sz, unzoomed},
        icons::{icon, IconName},
        theme::{self, Palette},
    },
};
use gpui::{
    div, prelude::*, px, relative, uniform_list, AnyElement, Context, Div, DragMoveEvent, Hsla,
    MouseButton, MouseDownEvent, ScrollStrategy, SharedString, Window,
};
use gpui_component::{menu::ContextMenuExt, scroll::ScrollableElement};
use std::{ops::Range, sync::Arc};

/// Name of the song list's columns for divider drags.
const TABLE: &str = "song-list";
const ROW_HEIGHT: f32 = 36.0;
const HEADER_HEIGHT: f32 = 32.0;
const NUMBER_WIDTH: f32 = 56.0;
const DURATION_WIDTH: f32 = 64.0;
/// Gap between a flexible column's divider and the text drawn after it.
const COLUMN_TEXT_INSET: f32 = 8.0;

/// Draw every library track as a searchable, virtualized list.
///
/// Tracks without album tags appear here even though no album groups them.
///
/// # Arguments
/// * `app` - Application holding the library and playback.
/// * `window` - Window being drawn.
/// * `cx` - Application entity context.
///
/// # Returns
/// The page element.
pub(crate) fn render(
    app: &mut ViperApp,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let colors = theme::colors(cx);
    let songs = app.library.filtered_song_indices();
    let count = format!(
        "{} song{}",
        songs.len(),
        if songs.len() == 1 { "" } else { "s" }
    );
    let toolbar = div()
        .flex()
        .flex_none()
        .items_center()
        .h(sz(48.0))
        .px(sz(20.0))
        .py(sz(8.0))
        .border_b_1()
        .border_color(colors.border)
        .child(label(count, 13.0, colors.muted))
        .child(div().flex_1())
        .child(search_field(app, window, cx));
    let body = if songs.is_empty() {
        div()
            .flex()
            .justify_center()
            .pt(sz(70.0))
            .child(label(
                if app.library.model.tracks.is_empty() {
                    "Your songs will appear here"
                } else {
                    "No songs match your search"
                },
                18.0,
                colors.subtle,
            ))
            .into_any_element()
    } else {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px(sz(12.0))
            .child(header(app, colors, cx))
            .child(list(app, &songs, cx))
            .into_any_element()
    };
    div()
        .flex()
        .flex_col()
        .size_full()
        .bg(colors.canvas)
        .child(toolbar)
        .child(body)
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
        .pl(sz(COLUMN_TEXT_INSET))
        .pr(sz(12.0))
        .overflow_hidden()
}

/// The fixed track number column.
fn number_column() -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .w(sz(NUMBER_WIDTH))
        .h_full()
        .pl(sz(8.0))
}

/// The fixed duration column.
fn duration_column() -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_end()
        .w(sz(DURATION_WIDTH))
        .h_full()
        .pr(sz(8.0))
}

/// Draw the list's column titles, whose dividers can be dragged to resize the columns.
fn header(app: &ViperApp, colors: Palette, cx: &mut Context<ViperApp>) -> AnyElement {
    let fractions = column_fractions(app.settings.song_list_column_widths);
    let dragging = app
        .ui
        .column_drag
        .filter(|(table, _)| *table == TABLE)
        .map(|(_, boundary)| boundary);
    let title = |text: &'static str| label(text, 12.0, colors.muted);
    let titles = ["Title", "Artist", "Album"]
        .into_iter()
        .zip(fractions)
        .map(|(text, fraction)| column(fraction).child(title(text)));
    let span = div()
        .id("song-list-columns")
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
                    &mut app.settings.song_list_column_widths,
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
        .h(sz(HEADER_HEIGHT))
        .border_b_1()
        .border_color(colors.border)
        .child(number_column().child(title("#")))
        .child(span)
        .child(duration_column().justify_start().child(title("Time")))
        .into_any_element()
}

/// The songs, drawing only the rows in view.
fn list(app: &mut ViperApp, songs: &Arc<Vec<usize>>, cx: &mut Context<ViperApp>) -> AnyElement {
    let scroll = app.ui.songs_scroll.clone();
    if std::mem::take(&mut app.library.view.scroll_to_selection) {
        if let Some(position) = app
            .library
            .view
            .selected_song
            .and_then(|track| songs.iter().position(|&index| index == track))
        {
            scroll.scroll_to_item(position, ScrollStrategy::Center);
        }
    }
    let songs = Arc::clone(songs);
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(
            uniform_list(
                "songs",
                songs.len(),
                cx.processor(move |app, range: Range<usize>, _, cx| {
                    let colors = theme::colors(cx);
                    range
                        .map(|position| row(app, &songs, position, colors, cx))
                        .collect()
                }),
            )
            .track_scroll(&scroll)
            .size_full(),
        )
        .vertical_scrollbar(&scroll)
        .into_any_element()
}

/// Draw the song at `position`; click selects it and double-click plays from it.
fn row(
    app: &ViperApp,
    songs: &Arc<Vec<usize>>,
    position: usize,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let track_index = songs[position];
    let Some(track) = app.library.model.tracks.get(track_index) else {
        return div().h(sz(ROW_HEIGHT)).into_any_element();
    };
    let (playing, is_playing) = {
        let playback = app.playback();
        (playback.current == Some(track_index), playback.is_playing())
    };
    let selected = app.library.view.selected_song == Some(track_index);
    let number = if playing {
        icon(
            if is_playing {
                IconName::Volume2
            } else {
                IconName::Pause
            },
            15.0,
            colors.accent,
        )
        .into_any_element()
    } else {
        label((position + 1).to_string(), 11.0, colors.subtle).into_any_element()
    };
    let fractions = column_fractions(app.settings.song_list_column_widths);
    let cell = |fraction: f32, text: SharedString, color: Hsla| {
        column(fraction).child(label(text, 13.0, color).truncate())
    };
    let duration = track.duration_ms.map(|duration| {
        let seconds = duration / 1_000;
        label(
            format!("{}:{:02}", seconds / 60, seconds % 60),
            12.0,
            colors.subtle,
        )
    });
    let weak = cx.weak_entity();
    let songs = Arc::clone(songs);
    div()
        .id(("song", track_index))
        .role(gpui::Role::ListItem)
        .aria_label(format!("{} by {}", track.title, track.artist))
        .flex()
        .w_full()
        .h(sz(ROW_HEIGHT))
        .rounded(px(3.0))
        .cursor_pointer()
        .when(selected, |row| row.bg(colors.selected))
        .when(!selected, |row| row.hover(|style| style.bg(colors.hover)))
        .child(number_column().child(number))
        .child(
            div()
                .flex()
                .flex_1()
                .h_full()
                .child(cell(
                    fractions[0],
                    track.title.clone().into(),
                    if playing { colors.accent } else { colors.text },
                ))
                .child(cell(
                    fractions[1],
                    track.artist.clone().into(),
                    colors.muted,
                ))
                .child(cell(fractions[2], track.album.clone().into(), colors.muted)),
        )
        .child(duration_column().children(duration))
        .on_mouse_down(
            MouseButton::Right,
            listen(cx, move |app, _: &MouseDownEvent, _, _| {
                app.library.view.selected_song = Some(track_index);
            }),
        )
        .on_click(cx.listener(move |app, event: &gpui::ClickEvent, _, cx| {
            app.library.view.selected_song = Some(track_index);
            if event.click_count() >= 2 {
                app.playback()
                    .play_in_context(&songs, &app.library.model.tracks, track_index);
            }
            cx.notify();
        }))
        .context_menu(move |menu, _, cx| track_menu::build(menu, &weak, track_index, true, cx))
        .into_any_element()
}
