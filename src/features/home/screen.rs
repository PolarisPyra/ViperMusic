use crate::{
    app::ViperApp,
    features::library::{Album, AlbumIndex},
    platform::persistence::settings::AlbumSort,
    shared::ui::{
        controls::{
            action_button, click, label, listen, primary_button, sz, tooltip, SEARCH_FIELD_ID,
        },
        icons::{icon, IconName},
        theme::{self, Palette},
    },
    workbench::Page,
};
use gpui::{
    div, img, point, prelude::*, AnyElement, Context, Div, FontWeight, ScrollEvent, ScrollHandle,
    Window,
};
use std::{cmp::Reverse, collections::BTreeMap};

/// Albums shown per Home shelf; "See all" opens the full list in the album browser.
const SHELF_LIMIT: usize = 20;
const CARD_ART: f32 = 144.0;
const CARD_GAP: f32 = 16.0;

/// Draw the Home page: onboarding, what is playing, and shelves of recent and favorite albums.
///
/// # Arguments
/// * `app` - Application holding the library and settings.
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
    let sections = sections(app, window, colors, cx);
    div()
        .id("home-content")
        .size_full()
        .overflow_y_scroll()
        .track_scroll(&app.ui.home_scroll)
        .bg(colors.canvas)
        .px(sz(24.0))
        .py(sz(24.0))
        .child(label("Your library", 30.0, colors.text).font_weight(FontWeight::BOLD))
        .child(
            label(
                "Pick up your current track or browse albums.",
                14.0,
                colors.muted,
            )
            .mt(sz(6.0))
            .mb(sz(24.0)),
        )
        .children(sections)
        .into_any_element()
}

/// The page below its title, depending on what the library holds.
fn sections(
    app: &mut ViperApp,
    window: &mut Window,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> Vec<AnyElement> {
    if app.settings.music_path.is_none() {
        return vec![onboarding(colors, cx)];
    }
    let filtered = app.library.filtered_album_indices(&app.settings);
    if !app.library.view.search.trim().is_empty() && filtered.is_empty() {
        return vec![empty_state(
            "No albums found",
            "Try another search, or clear it to see your library.",
            action_button(
                "home-clear-search",
                "Clear search",
                true,
                colors,
                click(cx, |app, window, cx| {
                    app.library.view.search.clear();
                    app.inputs.set(SEARCH_FIELD_ID, "", window, cx);
                }),
            ),
            colors,
        )];
    }
    if app.library.model.albums.is_empty() {
        let state = if app.library.scanning {
            empty_state(
                "Finding your music",
                "Your albums will show up here as the folder is scanned.",
                browse_button("home-browse-scanning", colors, cx),
                colors,
            )
        } else {
            empty_state(
                "Your library is empty",
                "Choose a folder with supported audio files to get started.",
                action_button(
                    "home-choose-folder",
                    "Choose Music Folder",
                    true,
                    colors,
                    click(cx, |_, _, cx| ViperApp::choose_folder(cx)),
                ),
                colors,
            )
        };
        return vec![state];
    }
    let albums = &app.library.model.albums;
    let recently_added = albums_by_timestamp(&filtered, albums, &app.settings.album_added);
    let recently_played = albums_by_timestamp(&filtered, albums, &app.settings.album_last_played);
    let mut favorites: Vec<_> = filtered
        .iter()
        .copied()
        .filter(|&index| {
            albums
                .get(index)
                .is_some_and(|album| app.settings.favorite_albums.contains(&album.key))
        })
        .collect();
    favorites.sort_by_cached_key(|&index| {
        let album = &albums[index];
        (album.title.to_lowercase(), album.artist.to_lowercase())
    });
    let mut sections = vec![continue_listening(app, colors, cx)];
    if !recently_added.is_empty() {
        sections.push(shelf(
            app,
            0,
            "Recently added",
            &recently_added,
            AlbumSort::RecentlyAdded,
            window,
            cx,
        ));
    }
    if !recently_played.is_empty() {
        sections.push(shelf(
            app,
            1,
            "Recently played",
            &recently_played,
            AlbumSort::RecentlyPlayed,
            window,
            cx,
        ));
    }
    if favorites.is_empty() {
        sections.push(
            div()
                .flex()
                .flex_col()
                .items_start()
                .child(section_heading("Favorites", colors))
                .child(label(
                    "Browse albums and mark a favorite to keep it close at hand.",
                    13.0,
                    colors.muted,
                ))
                .child(
                    div()
                        .mt(sz(10.0))
                        .child(browse_button("home-browse-favorites", colors, cx)),
                )
                .into_any_element(),
        );
    } else {
        sections.push(shelf(
            app,
            2,
            "Favorites",
            &favorites,
            AlbumSort::Favorited,
            window,
            cx,
        ));
    }
    sections
}

/// Albums of `indices` that have a timestamp, newest first.
fn albums_by_timestamp(
    indices: &[usize],
    albums: &[Album],
    timestamps: &BTreeMap<String, u64>,
) -> Vec<usize> {
    let mut albums_with_timestamps: Vec<_> = indices
        .iter()
        .filter_map(|&index| Some((index, *timestamps.get(&albums.get(index)?.key)?)))
        .collect();
    albums_with_timestamps.sort_by_key(|&(_, timestamp)| Reverse(timestamp));
    albums_with_timestamps
        .into_iter()
        .map(|(index, _)| index)
        .collect()
}

/// A button that opens the album browser.
fn browse_button(id: &'static str, colors: Palette, cx: &Context<ViperApp>) -> impl IntoElement {
    action_button(
        id,
        "Browse albums",
        true,
        colors,
        click(cx, |app, _, _| app.workbench.page = Page::Albums),
    )
}

/// The card asking for a music folder before the library has one.
fn onboarding(colors: Palette, cx: &Context<ViperApp>) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(sz(20.0))
        .max_w(sz(720.0))
        .min_h(sz(185.0))
        .p(sz(24.0))
        .rounded(sz(14.0))
        .bg(colors.panel)
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(sz(72.0))
                .rounded(sz(14.0))
                .bg(colors.selected)
                .child(icon(IconName::Music, 34.0, colors.accent)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_start()
                .flex_1()
                .child(label("Start with your music folder", 20.0, colors.text).font_weight(FontWeight::BOLD))
                .child(
                    label(
                        "Choose where your music is stored. Your albums and artwork will appear here.",
                        13.0,
                        colors.muted,
                    )
                    .mt(sz(7.0))
                    .mb(sz(14.0)),
                )
                .child(
                    primary_button(
                        "home-onboarding-choose",
                        "Choose Music Folder",
                        true,
                        colors,
                        click(cx, |_, _, cx| ViperApp::choose_folder(cx)),
                    )
                    .min_w(sz(190.0))
                    .h(sz(38.0)),
                ),
        )
        .into_any_element()
}

/// A card explaining why there is nothing to show, with one action.
fn empty_state(
    title: &'static str,
    description: &'static str,
    action: impl IntoElement,
    colors: Palette,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_start()
        .p(sz(24.0))
        .rounded(sz(12.0))
        .bg(colors.panel)
        .child(label(title, 19.0, colors.text).font_weight(FontWeight::BOLD))
        .child(
            label(description, 13.0, colors.muted)
                .mt(sz(5.0))
                .mb(sz(12.0)),
        )
        .child(action)
        .into_any_element()
}

fn section_heading(title: &'static str, colors: Palette) -> Div {
    label(title, 19.0, colors.text)
        .font_weight(FontWeight::BOLD)
        .mt(sz(5.0))
        .mb(sz(11.0))
}

/// The playing track with a pause button, or a pointer to the album browser.
fn continue_listening(
    app: &mut ViperApp,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let heading = section_heading("Continue listening", colors);
    let (current, paused) = {
        let playback = app.playback();
        (playback.current, playback.is_paused())
    };
    let section = div()
        .flex()
        .flex_col()
        .items_start()
        .mb(sz(25.0))
        .child(heading);
    let Some(track_index) = current else {
        return section
            .child(label(
                "Nothing is playing. Browse albums to start playback.",
                13.0,
                colors.muted,
            ))
            .child(
                div()
                    .mt(sz(8.0))
                    .child(browse_button("home-browse-idle", colors, cx)),
            )
            .into_any_element();
    };
    let model = &app.library.model;
    let Some(track) = model.tracks.get(track_index) else {
        return section.into_any_element();
    };
    let album_index = model
        .track_album
        .get(track_index)
        .copied()
        .flatten()
        .map(AlbumIndex::get);
    let subtitle = format!(
        "{} · {}",
        track.artist,
        album_index
            .and_then(|index| model.albums.get(index))
            .map_or("", |album| album.title.as_str())
    );
    let title = track.title.clone();
    let cover = album_index.and_then(|index| {
        let bytes = app.library.model.albums.get(index)?.art.clone()?;
        app.artwork_cache.texture(&app.repaint, index, &bytes)
    });
    section
        .child(
            div()
                .flex()
                .items_start()
                .gap(sz(8.0))
                .p(sz(16.0))
                .rounded(sz(12.0))
                .bg(colors.panel)
                .children(cover.map(|texture| img(texture).flex_none().size(sz(84.0))))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_start()
                        .child(label(title, 17.0, colors.text).font_weight(FontWeight::BOLD))
                        .child(label(subtitle, 13.0, colors.muted).mb(sz(8.0)))
                        .child(action_button(
                            "home-pause",
                            if paused { "Resume" } else { "Pause" },
                            true,
                            colors,
                            click(cx, |app, _, _| app.playback().toggle_pause()),
                        )),
                ),
        )
        .into_any_element()
}

/// Draw a horizontal shelf of up to [`SHELF_LIMIT`] albums.
///
/// # Arguments
/// * `app` - Application holding the library.
/// * `slot` - Which of the Home shelves this is, for its scroll position.
/// * `title` - Shelf heading.
/// * `indices` - Albums on the shelf, in order.
/// * `see_all` - Album browser sort applied, descending, when the user asks to see every album.
/// * `window` - Window being drawn.
/// * `cx` - Application entity context.
///
/// # Returns
/// The shelf element.
fn shelf(
    app: &mut ViperApp,
    slot: usize,
    title: &'static str,
    indices: &[usize],
    see_all: AlbumSort,
    window: &Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let colors = theme::colors(cx);
    let see_all_link = (indices.len() > SHELF_LIMIT).then(|| {
        div()
            .id(("home-see-all", slot))
            .cursor_pointer()
            .child(label(
                format!("See all {}", indices.len()),
                12.0,
                colors.accent,
            ))
            .on_click(click(cx, move |app, _, _| {
                app.settings.album_sort = see_all;
                app.settings.sort_ascending = false;
                app.library.view.album_sort_dirty = true;
                app.workbench.page = Page::Albums;
                app.workbench.show_album_details = false;
                app.save_settings();
            }))
    });
    let cards = indices[..indices.len().min(SHELF_LIMIT)]
        .iter()
        .map(|&index| card(app, index, colors, cx))
        .collect::<Vec<_>>();
    let scroll = app.ui.home_shelves[slot].clone();
    let line_height = window.line_height();
    div()
        .flex()
        .flex_col()
        .mt(sz(5.0))
        .mb(sz(14.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .mb(sz(11.0))
                .child(label(title, 19.0, colors.text).font_weight(FontWeight::BOLD))
                .children(see_all_link),
        )
        .child(
            div()
                .id(("home-shelf", slot))
                .flex()
                .gap(sz(CARD_GAP))
                // Keep the horizontal scrollbar clear of the album titles.
                .pb(sz(12.0))
                .overflow_x_scroll()
                .restrict_scroll_to_axis()
                .track_scroll(&scroll)
                .on_scroll(listen(cx, move |_, event: &ScrollEvent, _, cx| {
                    scroll_shelf_with_wheel(&scroll, event, line_height, cx);
                }))
                .children(cards),
        )
        .into_any_element()
}

/// Scroll a shelf horizontally with the vertical mouse wheel while the pointer is over it.
///
/// The wheel falls through to the page once the shelf reaches either end.
fn scroll_shelf_with_wheel(
    scroll: &ScrollHandle,
    event: &ScrollEvent,
    line_height: gpui::Pixels,
    cx: &mut Context<ViperApp>,
) {
    let delta = event.delta.pixel_delta(line_height);
    if delta.y == gpui::px(0.0) || delta.x != gpui::px(0.0) {
        return;
    }
    let offset = scroll.offset();
    let x = (offset.x + delta.y).clamp(-scroll.max_offset().x, gpui::px(0.0));
    if x != offset.x {
        scroll.set_offset(point(x, offset.y));
        cx.stop_propagation();
    }
}

/// Draw one album card; click opens the album and double-click plays it.
fn card(
    app: &mut ViperApp,
    index: usize,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let texture = app.library.model.albums.get(index).and_then(|album| {
        let bytes = album.art.clone()?;
        app.artwork_cache.texture(&app.repaint, index, &bytes)
    });
    // A cover on its way leaves the card blank rather than flashing the no-cover icon.
    let pending = app.cover_pending(index);
    let Some(album) = app.library.model.albums.get(index) else {
        return div().into_any_element();
    };
    let art = div()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .size(sz(CARD_ART))
        .rounded(sz(7.0))
        .overflow_hidden()
        .bg(colors.surface)
        .map(|art| match texture {
            Some(texture) => art.child(img(texture).size_full()),
            None if pending => art,
            None => art.child(icon(IconName::Disc, 32.0, colors.muted)),
        })
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(sz(4.0))
                .group_hover("home-card", |style| {
                    style.border_2().border_color(colors.accent)
                }),
        );
    div()
        .id(("home-card", index))
        .group("home-card")
        .role(gpui::Role::Button)
        .aria_label(format!("{} by {}", album.title, album.artist))
        .flex()
        .flex_col()
        .flex_none()
        .w(sz(CARD_ART))
        .cursor_pointer()
        .tooltip(tooltip("Double-click to play"))
        .child(art)
        .child(
            label(album.title.clone(), 13.0, colors.text)
                .mt(sz(9.0))
                .truncate(),
        )
        .child(
            label(album.artist.clone(), 11.0, colors.muted)
                .mt(sz(3.0))
                .truncate(),
        )
        .on_click(cx.listener(move |app, event: &gpui::ClickEvent, _, cx| {
            if event.click_count() >= 2 {
                let library = &app.library.model;
                if let Some(album) = library.albums.get(index) {
                    app.playback().play_album(album, &library.tracks);
                }
            } else {
                app.workbench.open_album(&mut app.library.view, index);
            }
            cx.notify();
        }))
        .into_any_element()
}
