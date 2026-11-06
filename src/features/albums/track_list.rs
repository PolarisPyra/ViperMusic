use crate::{
    app::ViperApp,
    features::{library::Album, lyrics, track_menu},
    shared::ui::{
        controls::{describe_button, label, listen, sz, tooltip, unzoomed, DragPreview},
        icons::{icon, inherited_icon, IconName},
        theme::{self, Palette},
    },
};
use gpui::{
    div, img, prelude::*, px, uniform_list, AnyElement, App, Context, CursorStyle, DragMoveEvent,
    FontWeight, MouseButton, MouseDownEvent, ScrollStrategy, Window,
};
use gpui_component::{menu::ContextMenuExt, scroll::ScrollableElement};
use std::ops::Range;

const MIN_PANEL_WIDTH: f32 = 300.0;
const MAX_PANEL_WIDTH: f32 = 520.0;
const HANDLE_WIDTH: f32 = 6.0;
const TRACK_ROW_HEIGHT: f32 = 50.0;
const MIN_TRACK_LIST_HEIGHT: f32 = 100.0;
/// Height of the panel above the track list: padding, heading, album summary, and separator.
const SUMMARY_HEIGHT: f32 = 36.0 + 152.0;

/// Marker dragged by the panel's left edge to resize it.
pub(super) struct TrackPanelDrag;

/// Width of the track panel at 100% zoom, following its edge while it is dragged.
///
/// Saves the new width once a resize has ended.
///
/// # Arguments
/// * `app` - Application holding the panel width.
/// * `cx` - Application entity context.
///
/// # Returns
/// The panel width.
pub(super) fn panel_width(app: &mut ViperApp, cx: &Context<ViperApp>) -> f32 {
    match app.ui.track_panel_drag {
        Some(width) if !cx.has_active_drag() => {
            app.ui.track_panel_drag = None;
            app.settings.right_panel_width = width;
            app.save_settings();
            width
        }
        Some(width) => width,
        None => app
            .settings
            .right_panel_width
            .clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH),
    }
}

/// Handler that resizes the track panel while its edge is dragged; attach it to the page.
///
/// # Arguments
/// * `cx` - Application entity context.
///
/// # Returns
/// A handler for `on_drag_move` on an element whose right edge is the panel's.
pub(super) fn resize_listener(
    cx: &Context<ViperApp>,
) -> impl Fn(&DragMoveEvent<TrackPanelDrag>, &mut Window, &mut App) + 'static {
    listen(
        cx,
        |app, event: &DragMoveEvent<TrackPanelDrag>, window, _| {
            let width = unzoomed(event.bounds.right() - event.event.position.x, window);
            app.ui.track_panel_drag = Some(width.clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH));
        },
    )
}

/// Draw the panel beside the albums with the selected album's summary, tracks and lyrics.
///
/// # Arguments
/// * `app` - Application holding the library and playback.
/// * `window` - Window being drawn.
/// * `cx` - Application entity context.
///
/// # Returns
/// The panel element.
pub(super) fn render(
    app: &mut ViperApp,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let colors = theme::colors(cx);
    let width = panel_width(app, cx);
    let dragging = app.ui.track_panel_drag.is_some();
    let album_index = app
        .library
        .view
        .selected_album
        .filter(|&index| app.library.model.albums.get(index).is_some());
    let content = match album_index {
        Some(index) => album_content(app, index, window, cx),
        None => empty(colors),
    };
    div()
        .relative()
        .flex()
        .flex_col()
        .flex_none()
        .w(sz(width))
        .h_full()
        .px(sz(20.0))
        .py(sz(18.0))
        .bg(colors.panel)
        .border_l_1()
        .border_color(colors.border)
        .child(content)
        .child(
            div()
                .id("album-tracks-resize-handle")
                .group("album-tracks-resize-handle")
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(sz(HANDLE_WIDTH))
                .flex()
                .items_center()
                .justify_center()
                .cursor(CursorStyle::ResizeLeftRight)
                .on_drag(TrackPanelDrag, |_, _, _, cx| cx.new(|_| DragPreview(None)))
                .child(
                    div()
                        .h(sz(20.0))
                        .w(if dragging { px(2.0) } else { px(1.0) })
                        .when(dragging, |line| line.bg(colors.accent))
                        .when(!dragging, |line| {
                            line.group_hover("album-tracks-resize-handle", |style| {
                                style.bg(colors.muted)
                            })
                        }),
                ),
        )
        .into_any_element()
}

/// The panel's contents while no album is selected.
fn empty(colors: Palette) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .child(heading(colors))
        .child(div().mt(sz(12.0)).h(px(1.0)).bg(colors.border))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .pt(sz(46.0))
                .gap(sz(18.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(sz(136.0))
                        .rounded(sz(8.0))
                        .bg(colors.surface)
                        .child(icon(IconName::Disc, 40.0, colors.muted)),
                )
                .child(label("No album loaded", 16.0, colors.text)),
        )
        .into_any_element()
}

fn heading(colors: Palette) -> AnyElement {
    label("ALBUM TRACKS", 10.0, colors.subtle)
        .font_weight(FontWeight::BOLD)
        .into_any_element()
}

/// The heading, summary, tracks and lyrics of the album at `index`.
fn album_content(
    app: &mut ViperApp,
    index: usize,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let colors = theme::colors(cx);
    let library = &app.library.model;
    let album = &library.albums[index];
    let release_year = album
        .tracks
        .iter()
        .filter_map(|&track| library.tracks.get(track)?.release_year)
        .min()
        .map_or_else(|| "—".to_owned(), |year| year.to_string());
    let header = div()
        .flex()
        .flex_col()
        .gap(sz(2.0))
        .child(heading(colors))
        .child(
            div()
                .flex()
                .gap(sz(6.0))
                .text_size(sz(12.0))
                .child(
                    div()
                        .text_color(colors.muted)
                        .child(format!("{} songs", album.tracks.len())),
                )
                .child(div().text_color(colors.subtle).child("·"))
                .child(div().text_color(colors.muted).child(release_year)),
        );
    let summary = summary(app, index, colors, cx);
    let tracks = tracks(app, index, cx);
    let available = crate::workbench::content_height(window) - SUMMARY_HEIGHT;
    let lyrics = lyrics::panel::render(app, available - MIN_TRACK_LIST_HEIGHT, cx);
    div()
        .flex()
        .flex_col()
        .size_full()
        .child(header)
        .child(div().h(sz(15.0)))
        .child(summary)
        .child(div().mt(sz(15.0)).mb(sz(7.0)).h(px(1.0)).bg(colors.border))
        .child(tracks)
        .child(lyrics)
        .into_any_element()
}

/// The album's cover, title, artist, star rating and favorite toggle.
fn summary(
    app: &mut ViperApp,
    index: usize,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let texture = app.library.model.albums[index]
        .art
        .clone()
        .and_then(|bytes| app.artwork_cache.texture(&app.repaint, index, &bytes));
    let album = &app.library.model.albums[index];
    div()
        .flex()
        .items_start()
        .gap(sz(12.0))
        .children(texture.map(|texture| img(texture).flex_none().size(sz(80.0)).rounded(sz(4.0))))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .pt(sz(12.0))
                .gap(sz(2.0))
                .child(label(album.title.clone(), 18.0, colors.text).truncate())
                .child(label(album.artist.clone(), 13.0, colors.muted).truncate())
                .child(div().mt(sz(3.0)).child(preferences(app, album, colors, cx))),
        )
        .into_any_element()
}

/// The star rating and favorite toggle for `album`.
fn preferences(
    app: &ViperApp,
    album: &Album,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let key = album.key.clone();
    let rating = app
        .settings
        .album_ratings
        .get(&key)
        .copied()
        .unwrap_or_default();
    let favorite = app.settings.favorite_albums.contains(&key);
    let stars = (1..=5)
        .map(|value| {
            let description = if value == rating {
                "Clear rating".to_owned()
            } else {
                format!("Rate {value} out of 5")
            };
            let key = key.clone();
            describe_button(
                div().id(("album-rating", usize::from(value))),
                description.clone(),
            )
            .flex()
            .items_center()
            .justify_center()
            .w(sz(22.0))
            .h(sz(24.0))
            .cursor_pointer()
            .tooltip(tooltip(description))
            .child(icon(
                IconName::Star,
                18.0,
                if value <= rating {
                    colors.warning
                } else {
                    colors.subtle
                },
            ))
            .on_click(cx.listener(move |app, _, _, cx| {
                if value == rating {
                    app.settings.album_ratings.remove(&key);
                } else {
                    app.settings.album_ratings.insert(key.clone(), value);
                }
                album_state_changed(app, &key);
                cx.notify();
            }))
        })
        .collect::<Vec<_>>();
    let description = if favorite {
        "Remove from favorites"
    } else {
        "Add to favorites"
    };
    let heart = describe_button(div().id("album-favorite"), description)
        .flex()
        .items_center()
        .justify_center()
        .ml(sz(6.0))
        .w(sz(30.0))
        .h(sz(24.0))
        .rounded(sz(7.0))
        .cursor_pointer()
        .tooltip(tooltip(description))
        .text_color(if favorite {
            colors.danger
        } else {
            colors.muted
        })
        .hover(|style| {
            let style = style.bg(colors.hover);
            if favorite {
                style
            } else {
                style.text_color(colors.text)
            }
        })
        .child(inherited_icon(IconName::Heart, 20.0))
        .on_click(cx.listener(move |app, _, _, cx| {
            if favorite {
                app.settings.favorite_albums.remove(&key);
            } else {
                app.settings.favorite_albums.insert(key.clone());
            }
            album_state_changed(app, &key);
            cx.notify();
        }));
    div()
        .flex()
        .items_center()
        .h(sz(24.0))
        .gap(px(1.0))
        .children(stars)
        .child(heart)
        .into_any_element()
}

/// Re-sort the albums and save the state of the album with `key`.
fn album_state_changed(app: &mut ViperApp, key: &str) {
    app.library.view.album_sort_dirty = true;
    app.save_album_state([key]);
}

/// The album's tracks, drawing only the rows in view.
fn tracks(app: &mut ViperApp, album_index: usize, cx: &mut Context<ViperApp>) -> AnyElement {
    let scroll = app.ui.album_tracks_scroll.clone();
    let album = &app.library.model.albums[album_index];
    let count = album.tracks.len();
    // Keyboard navigation and "Go to album" ask for the selected track to be scrolled into view.
    if app.workbench.show_album_details && app.library.view.scroll_to_selection {
        let position = app
            .library
            .view
            .selected_track
            .and_then(|track| album.tracks.iter().position(|&index| index == track));
        app.library.view.scroll_to_selection = false;
        if let Some(position) = position {
            scroll.scroll_to_item(position, ScrollStrategy::Center);
        }
    }
    div()
        .relative()
        .flex_1()
        .min_h(sz(MIN_TRACK_LIST_HEIGHT))
        .child(
            uniform_list(
                "album-tracks",
                count,
                cx.processor(move |app, range: Range<usize>, _, cx| {
                    let colors = theme::colors(cx);
                    range
                        .filter_map(|position| {
                            let track = *app
                                .library
                                .model
                                .albums
                                .get(album_index)?
                                .tracks
                                .get(position)?;
                            Some(track_row(app, album_index, position, track, colors, cx))
                        })
                        .collect()
                }),
            )
            .track_scroll(&scroll)
            .pt(sz(12.0))
            .pb(sz(18.0))
            // Keeps rows clear of the scrollbar.
            .pr(sz(12.0))
            .size_full(),
        )
        .vertical_scrollbar(&scroll)
        .into_any_element()
}

/// Draw one track; click selects it and double-click plays the album from it.
fn track_row(
    app: &ViperApp,
    album_index: usize,
    position: usize,
    track_index: usize,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let Some(track) = app.library.model.tracks.get(track_index) else {
        return div().h(sz(TRACK_ROW_HEIGHT)).into_any_element();
    };
    let (playing, is_playing) = {
        let playback = app.playback();
        (playback.current == Some(track_index), playback.is_playing())
    };
    let selected = app.library.view.selected_track == Some(track_index);
    let leading = if playing {
        icon(
            if is_playing {
                IconName::Pause
            } else {
                IconName::Play
            },
            18.0,
            colors.accent,
        )
        .into_any_element()
    } else {
        label(format!("{:02}", position + 1), 11.0, colors.subtle).into_any_element()
    };
    let duration = track.duration_ms.map(format_duration);
    let weak = cx.weak_entity();
    let row = div()
        .id(("album-track", track_index))
        .role(gpui::Role::ListItem)
        .aria_label(track.title.clone())
        .flex()
        .items_center()
        .w_full()
        .h(sz(TRACK_ROW_HEIGHT))
        .rounded(sz(3.0))
        .cursor_pointer()
        .when(selected, |row| row.bg(colors.selected))
        .when(!selected, |row| row.hover(|style| style.bg(colors.hover)))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .ml(sz(18.0))
                .size(sz(20.0))
                .child(leading),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .ml(sz(12.0))
                .mr(sz(10.0))
                .child(
                    label(
                        track.title.clone(),
                        12.0,
                        if selected { colors.accent } else { colors.text },
                    )
                    .truncate(),
                )
                .child(label(track.artist.clone(), 10.0, colors.muted).truncate()),
        )
        .children(duration.map(|duration| {
            label(duration, 10.0, colors.subtle)
                .flex_none()
                .mr(sz(10.0))
        }))
        .on_mouse_down(
            MouseButton::Right,
            listen(cx, move |app, _: &MouseDownEvent, _, _| {
                app.library.view.selected_track = Some(track_index);
            }),
        )
        .on_click(cx.listener(move |app, event: &gpui::ClickEvent, _, cx| {
            app.library.view.selected_track = Some(track_index);
            if event.click_count() >= 2 {
                let library = &app.library.model;
                if let Some(album) = library.albums.get(album_index) {
                    app.playback()
                        .play_in_context(&album.tracks, &library.tracks, track_index);
                }
            }
            cx.notify();
        }))
        .context_menu(move |menu, _, cx| track_menu::build(menu, &weak, track_index, false, cx));
    row.into_any_element()
}

fn format_duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
