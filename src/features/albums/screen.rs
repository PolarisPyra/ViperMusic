use super::{grid, list, toolbar, track_list};
use crate::{
    app::ViperApp,
    features::{
        library::{is_smb_path, Album, Library},
        tag_editor::TagEditor,
    },
    platform::{desktop, persistence::settings::AlbumLayout},
    shared::ui::{
        controls::{menu_item, sz},
        icons::{icon, IconName},
        theme::{self, Palette},
    },
};
use gpui::{
    div, img, linear_color_stop, linear_gradient, prelude::*, relative, AnyElement, Context, Div,
    Hsla, Stateful, WeakEntity, Window,
};
use gpui_component::menu::{ContextMenuExt, PopupMenu};
use std::{
    borrow::Cow,
    sync::OnceLock,
    time::{Duration, Instant},
};

/// Time between redraws while placeholders shimmer.
const SHIMMER_FRAME: Duration = Duration::from_millis(33);

/// Draw the albums page: the toolbar, the album grid or list, and the selected album's tracks.
///
/// # Arguments
/// * `app` - Application holding the library.
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
    let panel_width = track_list::panel_width(app, cx);
    let width = (crate::workbench::content_width(app, window) - panel_width).max(0.0);
    let toolbar = toolbar::render(app, window, cx);
    let albums = app.library.filtered_album_indices(&app.settings);
    let body = body(app, &albums, width, cx);
    let tracks = track_list::render(app, window, cx);
    div()
        .id("albums-page")
        .flex()
        .size_full()
        .bg(colors.canvas)
        .on_drag_move(track_list::resize_listener(cx))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(toolbar)
                .child(body),
        )
        .child(tracks)
        .into_any_element()
}

/// The album grid or list, or a placeholder when there is nothing to show.
fn body(
    app: &mut ViperApp,
    albums: &std::sync::Arc<Vec<usize>>,
    width: f32,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let layout = app.settings.album_layout;
    let content = if albums.is_empty() {
        empty(app, width, cx)
    } else {
        // Keyboard navigation asks for the highlighted album to be scrolled into view.
        let scroll_to = (!app.workbench.show_album_details
            && std::mem::take(&mut app.library.view.scroll_to_selection))
        .then(|| {
            let highlighted = app.library.view.highlighted_album?;
            albums.iter().position(|&index| index == highlighted)
        })
        .flatten();
        match layout {
            AlbumLayout::Grid => grid::render(app, albums, width, scroll_to, cx),
            AlbumLayout::List => {
                app.library.view.grid_columns = 1;
                list::render(app, albums, scroll_to, cx)
            }
        }
    };
    div()
        .id("album-panel-background")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .when(layout == AlbumLayout::List, |body| {
            body.child(list::header(app, cx))
        })
        .child(content)
        .on_click(cx.listener(|app, _, _, cx| {
            app.library.view.highlighted_album = None;
            cx.notify();
        }))
        .into_any_element()
}

/// Skeleton cards while the first scan runs, otherwise a message explaining the empty page.
fn empty(app: &ViperApp, width: f32, cx: &mut Context<ViperApp>) -> AnyElement {
    let colors = theme::colors(cx);
    let library = &app.library;
    if library.scanning && library.model.albums.is_empty() && library.view.search.trim().is_empty()
    {
        return match app.settings.album_layout {
            AlbumLayout::Grid => grid::skeleton(app, width, colors),
            AlbumLayout::List => list::skeleton(app, colors),
        };
    }
    let message = if library.model.albums.is_empty() {
        "Your albums will appear here"
    } else if library.scanning && library.view.search.trim().is_empty() {
        "Scanning your music folder…"
    } else {
        "No albums match your search"
    };
    div()
        .flex()
        .justify_center()
        .pt(sz(70.0))
        .text_size(sz(18.0))
        .text_color(colors.subtle)
        .child(message)
        .into_any_element()
}

/// The text read to assistive technology for an album card or row.
pub(super) fn album_description(album: &Album) -> String {
    format!(
        "{} by {}, {} tracks",
        album.title,
        album.artist,
        album.tracks.len()
    )
}

/// Return the shared file format of an album's tracks, `MIXED`, or `OTHER`.
pub(super) fn album_format_label(library: &Library, album: &Album) -> Cow<'static, str> {
    let mut format: Option<&str> = None;
    for track in album
        .tracks
        .iter()
        .filter_map(|&index| library.tracks.get(index))
    {
        let extension = track
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("other");
        match format {
            Some(first) if !first.eq_ignore_ascii_case(extension) => return Cow::Borrowed("MIXED"),
            Some(_) => {}
            None => format = Some(extension),
        }
    }
    format.map_or(Cow::Borrowed("OTHER"), |extension| {
        Cow::Owned(extension.to_ascii_uppercase())
    })
}

/// Rows of covers outside the view that are loaded ahead, on either side.
pub(super) const PREFETCH_ROWS: usize = 2;

/// Start loading covers without drawing them, so they are ready when they are scrolled to.
///
/// Call it after the covers in view are asked for, so those are decoded first.
///
/// # Arguments
/// * `app` - Application holding the library and the cover cache.
/// * `albums` - Albums whose covers to load.
pub(super) fn prefetch_covers(app: &mut ViperApp, albums: &[usize]) {
    for &index in albums {
        if let Some(bytes) = app
            .library
            .model
            .albums
            .get(index)
            .and_then(|album| album.art.clone())
        {
            app.artwork_cache.texture(&app.repaint, index, &bytes);
        }
    }
}

/// An album's cover at `size`, fading in once decoded, with its format badge when enabled.
///
/// # Arguments
/// * `app` - Application holding the artwork cache.
/// * `index` - Album to draw the cover of.
/// * `size` - Edge length at 100% zoom.
/// * `radius` - Corner radius of the placeholder.
/// * `colors` - Active theme palette.
///
/// # Returns
/// The cover element.
pub(super) fn album_cover(
    app: &mut ViperApp,
    index: usize,
    size: f32,
    radius: f32,
    colors: Palette,
) -> Div {
    let library = &app.library.model;
    let Some(album) = library.albums.get(index) else {
        return div().size(sz(size));
    };
    let badge = app
        .settings
        .album_format_badges
        .then(|| album_format_label(library, album));
    let texture = album
        .art
        .clone()
        .and_then(|bytes| app.artwork_cache.texture(&app.repaint, index, &bytes));
    let cover = div().relative().flex_none().size(sz(size));
    let cover = match texture {
        Some(texture) => {
            let opacity = app.artwork_cache.texture_opacity(&app.repaint, index);
            cover
                .when(opacity < 1.0, |cover| {
                    cover.child(shimmer(app, radius, colors).absolute().inset_0())
                })
                .child(
                    img(texture)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .opacity(opacity),
                )
        }
        // The same shimmer the cover fades in over, so it appears without a jump.
        None if app.cover_pending(index) => cover.child(shimmer(app, radius, colors).size_full()),
        None => cover
            .flex()
            .items_center()
            .justify_center()
            .rounded(sz(radius))
            .bg(colors.surface)
            .child(icon(
                IconName::Disc,
                (size * 0.24).clamp(20.0, 32.0),
                colors.muted,
            )),
    };
    cover.children(badge.map(|label| {
        div()
            .absolute()
            .left(sz(5.0))
            .bottom(sz(5.0))
            .px(sz(7.0))
            .py(sz(4.0))
            .rounded(sz(3.0))
            .bg(colors.surface)
            .text_size(sz(11.0))
            .text_color(colors.text)
            .child(label.into_owned())
    }))
}

/// A pulsing placeholder with a highlight sweeping across it, shown while artwork loads.
///
/// # Arguments
/// * `app` - Application, to keep redrawing while the placeholder is visible.
/// * `radius` - Corner radius.
/// * `colors` - Active theme palette.
///
/// # Returns
/// The placeholder element; give it a size.
pub(super) fn shimmer(app: &ViperApp, radius: f32, colors: Palette) -> Div {
    static START: OnceLock<Instant> = OnceLock::new();
    const SHIMMER_PERIOD: f32 = 1.5;
    const BAND: f32 = 0.72;
    let time = START.get_or_init(Instant::now).elapsed().as_secs_f32();
    app.repaint.request_repaint_after(SHIMMER_FRAME);
    let pulse = (time * 1.8).sin() * 0.5 + 0.5;
    let base = mix(colors.surface, colors.hover, pulse * 0.35);
    let phase = (time / SHIMMER_PERIOD).rem_euclid(1.0);
    let eased = phase * phase * (3.0 - 2.0 * phase);
    let left = 1.0 - (1.0 + BAND) * eased;
    let highlight = colors.hover.opacity(0.24);
    let clear = highlight.opacity(0.0);
    div()
        .relative()
        .overflow_hidden()
        .rounded(sz(radius))
        .bg(base)
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(relative(left))
                .w(relative(BAND * 0.5))
                .bg(linear_gradient(
                    90.0,
                    linear_color_stop(clear, 0.0),
                    linear_color_stop(highlight, 1.0),
                )),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(relative(left + BAND * 0.5))
                .w(relative(BAND * 0.5))
                .bg(linear_gradient(
                    90.0,
                    linear_color_stop(highlight, 0.0),
                    linear_color_stop(clear, 1.0),
                )),
        )
}

/// Blend from `from` to `to` by `amount`.
fn mix(from: Hsla, to: Hsla, amount: f32) -> Hsla {
    let (from, to) = (from.to_rgb(), to.to_rgb());
    let lerp = |a: f32, b: f32| a + (b - a) * amount;
    gpui::Rgba {
        r: lerp(from.r, to.r),
        g: lerp(from.g, to.g),
        b: lerp(from.b, to.b),
        a: lerp(from.a, to.a),
    }
    .into()
}

/// Select an album and show its tracks.
pub(super) fn open_album(app: &mut ViperApp, index: usize) {
    app.library.view.select_album(index);
    app.workbench.show_album_details = true;
}

/// Play an album from its first track.
pub(super) fn play_album(app: &mut ViperApp, index: usize) {
    if let Some(album) = app.library.model.albums.get(index).cloned() {
        app.playback().play_album(&album, &app.library.model.tracks);
    }
}

/// Make `element` open and play an album when clicked, with its context menu on right-click.
///
/// # Arguments
/// * `element` - Album card or row.
/// * `index` - Album the element shows.
/// * `cx` - Application entity context.
///
/// # Returns
/// The interactive element.
pub(super) fn album_interactions(
    element: Stateful<Div>,
    index: usize,
    cx: &mut Context<ViperApp>,
) -> impl IntoElement {
    let weak = cx.weak_entity();
    element
        .on_click(cx.listener(move |app, event: &gpui::ClickEvent, _, cx| {
            if event.click_count() >= 2 {
                play_album(app, index);
            } else {
                open_album(app, index);
            }
            cx.stop_propagation();
            cx.notify();
        }))
        .context_menu(move |menu, _, cx| album_menu(menu, &weak, index, cx))
}

/// The album context menu: playing, queueing, opening the folder and editing tags.
fn album_menu(
    menu: PopupMenu,
    weak: &WeakEntity<ViperApp>,
    index: usize,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let Some(app) = weak.upgrade() else {
        return menu;
    };
    let library = &app.read(cx).library.model;
    let Some(album) = library.albums.get(index) else {
        return menu;
    };
    let folder = album
        .tracks
        .first()
        .and_then(|&track| library.tracks.get(track))
        .and_then(|track| track.path.parent())
        .filter(|folder| !is_smb_path(folder))
        .map(std::path::Path::to_path_buf);
    // Tags can only be written to files on this computer.
    let editable = folder.is_some();
    menu.item(menu_item("Play album", weak, move |app, _, _| {
        play_album(app, index)
    }))
    .item(menu_item("Play next", weak, move |app, _, _| {
        if let Some(album) = app.library.model.albums.get(index) {
            app.playback().play_next(&album.tracks);
        }
    }))
    .item(menu_item("Add album to queue", weak, move |app, _, _| {
        if let Some(album) = app.library.model.albums.get(index) {
            app.playback().enqueue(&album.tracks);
        }
    }))
    .item(
        menu_item("Open in file browser", weak, move |app, _, _| {
            if let Some(folder) = &folder {
                if let Err(error) = desktop::open_folder(folder) {
                    app.toasts.push(format!(
                        "Could not open album folder {}: {error}",
                        folder.display()
                    ));
                }
            }
        })
        .disabled(!editable),
    )
    .item(
        menu_item("Edit metadata", weak, move |app, _, _| {
            app.workbench.tag_editor = TagEditor::new(&app.library, index);
        })
        .disabled(!editable),
    )
}
