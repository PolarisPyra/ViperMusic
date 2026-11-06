use super::{Lyrics, LyricsResult};
use crate::{
    app::ViperApp,
    shared::ui::{
        controls::{describe_button, label, listen, sz, tooltip, unzoomed, DragPreview},
        icons::{inherited_icon, IconName},
        theme::{self, Palette},
    },
    workbench::WorkbenchUi,
};
use gpui::{div, prelude::*, px, AnyElement, Context, CursorStyle, DragMoveEvent, FontWeight};
use gpui_component::scroll::ScrollableElement;

const MIN_LYRICS_HEIGHT: f32 = 96.0;
const DEFAULT_LYRICS_HEIGHT: f32 = 220.0;
const TOGGLE_ROW_HEIGHT: f32 = 30.0;
const RESIZE_HANDLE_HEIGHT: f32 = 28.0;
const LINE_HEIGHT: f32 = 20.0;

/// Marker dragged by the handle above the lyrics to resize them.
struct LyricsDrag;

/// Draw the lyrics toggle and, when open, the resizable lyrics of the playing track.
///
/// # Arguments
/// * `app` - Application holding the lyrics and playback.
/// * `max_height` - Height the panel may take at 100% zoom, toggle row included.
/// * `cx` - Application entity context.
///
/// # Returns
/// The panel element.
pub(crate) fn render(
    app: &mut ViperApp,
    max_height: f32,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let colors = theme::colors(cx);
    if app.ui.lyrics_resizing && !cx.has_active_drag() {
        app.ui.lyrics_resizing = false;
    }
    let open = app.settings.lyrics_panel_open;
    let toggle = toggle(open, colors, cx);
    let panel = div().flex().flex_col().flex_none().child(toggle);
    if !open {
        return panel.into_any_element();
    }
    let max_content =
        (max_height - TOGGLE_ROW_HEIGHT - RESIZE_HANDLE_HEIGHT).max(MIN_LYRICS_HEIGHT);
    let height = app
        .ui
        .lyrics_height
        .unwrap_or(DEFAULT_LYRICS_HEIGHT)
        .clamp(MIN_LYRICS_HEIGHT, max_content);
    let resizing = app.ui.lyrics_resizing;
    let handle = div()
        .id("lyrics-resize-handle")
        .group("lyrics-resize-handle")
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(sz(7.0))
        .h(sz(RESIZE_HANDLE_HEIGHT))
        .cursor(CursorStyle::ResizeUpDown)
        .on_drag(LyricsDrag, |_, _, _, cx| cx.new(|_| DragPreview(None)))
        .child(
            div()
                .w(sz(20.0))
                .h(if resizing { px(2.0) } else { px(1.0) })
                .when(resizing, |line| line.bg(colors.accent))
                .when(!resizing, |line| {
                    line.group_hover("lyrics-resize-handle", |style| style.bg(colors.muted))
                }),
        )
        .child(div().w_full().h(px(1.0)).bg(colors.border));
    let content = content(app, colors, cx);
    panel
        .child(
            div()
                .flex()
                .flex_col()
                .on_drag_move(listen(
                    cx,
                    move |app, event: &DragMoveEvent<LyricsDrag>, window, _| {
                        // The panel's bottom stays put, so the pointer sets the height above it.
                        let height =
                            unzoomed(event.bounds.bottom() - event.event.position.y, window)
                                - RESIZE_HANDLE_HEIGHT * 0.5;
                        app.ui.lyrics_height = Some(height.clamp(MIN_LYRICS_HEIGHT, max_content));
                        app.ui.lyrics_resizing = true;
                    },
                ))
                .child(handle)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .h(sz(height))
                        .child(label("LYRICS", 10.0, colors.subtle).font_weight(FontWeight::BOLD))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_h_0()
                                .mt(sz(8.0))
                                .child(content),
                        ),
                ),
        )
        .into_any_element()
}

/// The "View Lyrics" row that opens and closes the panel.
fn toggle(open: bool, colors: Palette, cx: &mut Context<ViperApp>) -> AnyElement {
    let toggle = cx.listener(|app, _, _, cx| {
        app.settings.lyrics_panel_open = !app.settings.lyrics_panel_open;
        app.save_settings();
        cx.notify();
    });
    describe_button(div().id("lyrics-toggle"), "View Lyrics")
        .flex()
        .flex_none()
        .items_center()
        .justify_end()
        .gap(sz(4.0))
        .h(sz(TOGGLE_ROW_HEIGHT))
        .cursor_pointer()
        .group("lyrics-toggle")
        .tooltip(tooltip("View Lyrics"))
        .child(label("View Lyrics", 11.0, colors.muted))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(sz(30.0))
                .rounded(sz(7.0))
                .text_color(colors.muted)
                .group_hover("lyrics-toggle", |style| {
                    style.bg(colors.hover).text_color(colors.text)
                })
                .child(inherited_icon(
                    if open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronUp
                    },
                    20.0,
                )),
        )
        .on_click(toggle)
        .into_any_element()
}

/// The playing track's lyrics, or a message explaining why there are none.
fn content(app: &mut ViperApp, colors: Palette, cx: &mut Context<ViperApp>) -> AnyElement {
    let (current, position, playing) = {
        let playback = app.playback();
        (playback.current, playback.position(), playback.is_playing())
    };
    let result = current
        .and_then(|index| app.library.model.tracks.get(index))
        .map(|track| app.lyrics.request(&app.repaint, track));
    let message = match result {
        Some(LyricsResult::Found(lyrics)) => {
            let started = lyrics
                .lines
                .partition_point(|line| line.timestamp.is_some_and(|time| time <= position));
            // The player bar redraws only as often as its seek bar moves, so wake for the next line.
            if playing && lyrics.synced {
                if let Some(next) = lyrics.lines.get(started).and_then(|line| line.timestamp) {
                    app.repaint
                        .request_repaint_after(next.saturating_sub(position));
                }
            }
            let active = lyrics.synced.then(|| started.saturating_sub(1));
            return lines(&mut app.ui, lyrics, (current, active), colors, cx);
        }
        Some(LyricsResult::Loading) => "Fetching lyrics…",
        Some(LyricsResult::Unavailable) => "Lyrics aren't available for this track.",
        Some(LyricsResult::Failed(error)) => {
            return div()
                .id("lyrics-failed")
                .tooltip(tooltip(error.to_owned()))
                .child(label("Couldn't fetch lyrics.", 12.0, colors.muted))
                .into_any_element();
        }
        None => "Play a track to see its lyrics.",
    };
    label(message, 12.0, colors.muted).into_any_element()
}

/// The lyric lines, highlighting and following the sung line of synced lyrics.
///
/// `follow` is the playing track and its sung line, to scroll only when either changes.
fn lines(
    ui: &mut WorkbenchUi,
    lyrics: &Lyrics,
    follow: (Option<usize>, Option<usize>),
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let active = follow.1;
    let scroll = ui.lyrics_scroll.clone();
    if let Some(line) = active {
        if ui.lyrics_followed != Some(follow) && !ui.lyrics_hovered && !ui.lyrics_resizing {
            scroll.scroll_to_item(line);
        }
    }
    ui.lyrics_followed = Some(follow);
    let rows = lyrics.lines.iter().enumerate().map(|(index, line)| {
        let active = active == Some(index);
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(sz(LINE_HEIGHT))
            .px(sz(4.0))
            .rounded(sz(3.0))
            .when(active, |row| row.bg(colors.selected))
            .child(
                label(
                    line.text.clone(),
                    12.0,
                    if active { colors.accent } else { colors.muted },
                )
                .font_weight(FontWeight::SEMIBOLD)
                .truncate(),
            )
    });
    div()
        .relative()
        .size_full()
        .child(
            div()
                .id("lyrics-lines")
                .flex()
                .flex_col()
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .on_hover(listen(cx, |app, hovered: &bool, _, _| {
                    app.ui.lyrics_hovered = *hovered;
                }))
                .children(rows),
        )
        .vertical_scrollbar(&scroll)
        .into_any_element()
}
