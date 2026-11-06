//! The tag editor dialog: an album's tags and its tracks', and a MusicBrainz search to fill them
//! in.
use super::state::{AlbumFields, Job, TagEditor, TrackFields};
use crate::{
    app::ViperApp,
    shared::ui::{
        controls::{
            action_button, backdrop, click, close_button, field_label, label, menu_item,
            modal_frame, notice, primary_button, sz, text_field, tooltip, unzoomed,
        },
        icons::IconName,
        text_input::FieldOptions,
        theme::{self, Palette},
    },
};
use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, MouseButton, Window};
use gpui_component::{button::Button, menu::DropdownMenu, spinner::Spinner, Disableable, Sizable};

/// Prefix of the keys of the dialog's text fields.
const FIELD_PREFIX: &str = "tag-editor-";
const MAX_WIDTH: f32 = 760.0;
/// Space between the dialog and the window's edges.
const MARGIN: f32 = 24.0;
/// Padding inside the dialog frame, on both sides together.
const FRAME_PADDING: f32 = 36.0;
/// Width of the disc and track number fields.
const NUMBER_WIDTH: f32 = 44.0;
/// Space between a track's fields.
const FIELD_GAP: f32 = 8.0;
/// Reaches the album tag a field edits.
type AlbumField = fn(&mut AlbumFields) -> &mut String;
/// Reaches the track tag a field edits.
type TrackField = fn(&mut TrackFields) -> &mut String;

/// Draw the tag editor while it is open, and apply its finished searches and saves.
///
/// # Arguments
/// * `app` - Application holding the open editor.
/// * `window` - Window being drawn.
/// * `cx` - Application entity context.
///
/// # Returns
/// The dialog over a backdrop, or `None` while the editor is closed.
pub(crate) fn render(
    app: &mut ViperApp,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> Option<AnyElement> {
    let editor = app.workbench.tag_editor.as_mut()?;
    let fetching = matches!(editor.job, Job::Fetching(_));
    if editor.poll(&mut app.library, &mut app.settings, &mut app.toasts) {
        close(app);
        return None;
    }
    // A chosen release replaced the tags, so the fields are created again with them.
    if fetching && matches!(app.workbench.tag_editor.as_ref()?.job, Job::Idle) {
        app.inputs.remove_prefixed(FIELD_PREFIX);
    }
    let colors = theme::colors(cx);
    let viewport = window.viewport_size();
    let (screen_width, screen_height) = (
        unzoomed(viewport.width, window),
        unzoomed(viewport.height, window),
    );
    let margin = MARGIN.min(screen_width * 0.05).min(screen_height * 0.05);
    let width = (screen_width - margin * 2.0).clamp(0.0, MAX_WIDTH);
    let content_width = (width - FRAME_PADDING).max(0.0);
    let editor = app.workbench.tag_editor.as_ref()?;
    let busy = editor.busy();
    let header = header(editor.tracks.len(), colors, cx);
    let lookup = lookup(editor, content_width, colors, cx);
    let footer = footer(editor, colors, cx);
    let album = album_fields(app, content_width, busy, window, cx);
    let tracks = track_fields(app, content_width, busy, window, cx);
    Some(
        backdrop("tag-editor-backdrop", colors)
            .on_click(click(cx, |app, _, _| request_close(app)))
            .child(
                modal_frame(colors)
                    .w(sz(width))
                    .max_h(sz((screen_height - margin * 2.0).max(0.0)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(header)
                    .child(div().mt(sz(14.0)).child(lookup))
                    .child(div().my(sz(12.0)).h(px(1.0)).bg(colors.border))
                    .child(album)
                    .child(
                        div()
                            .id("tag-editor-tracks")
                            .flex()
                            .flex_col()
                            .min_h_0()
                            .mt(sz(14.0))
                            .overflow_y_scroll()
                            .child(tracks),
                    )
                    .child(div().mt(sz(12.0)).child(footer)),
            )
            .into_any_element(),
    )
}

/// Close the editor unless it is saving, since closing mid-save would leave the library showing
/// the old tags.
///
/// # Arguments
/// * `app` - Application holding the open editor.
pub(crate) fn request_close(app: &mut ViperApp) {
    if !app
        .workbench
        .tag_editor
        .as_ref()
        .is_some_and(|editor| matches!(editor.job, Job::Saving(_)))
    {
        close(app);
    }
}

fn close(app: &mut ViperApp) {
    app.workbench.tag_editor = None;
    app.inputs.remove_prefixed(FIELD_PREFIX);
}

fn header(tracks: usize, colors: Palette, cx: &Context<ViperApp>) -> AnyElement {
    div()
        .flex()
        .items_start()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .child(label("Edit metadata", 23.0, colors.text).font_weight(FontWeight::BOLD))
                .child(label(
                    format!(
                        "Change the tags saved in this album's {tracks} files, by hand or from MusicBrainz"
                    ),
                    12.0,
                    colors.muted,
                )),
        )
        .child(close_button(
            "tag-editor-close",
            "Close without saving",
            colors,
            click(cx, |app, _, _| request_close(app)),
        ))
        .into_any_element()
}

/// The MusicBrainz search button, the releases it found, and the last problem.
fn lookup(
    editor: &TagEditor,
    content_width: f32,
    colors: Palette,
    cx: &Context<ViperApp>,
) -> AnyElement {
    let busy = editor.busy();
    let searchable = !busy && !editor.album.title.trim().is_empty();
    let waiting = match editor.job {
        Job::Searching(_) => Some("Searching MusicBrainz…"),
        Job::Fetching(_) => Some("Getting the release's tracks…"),
        Job::Idle | Job::Saving(_) => None,
    };
    let search = action_button(
        "tag-editor-search",
        "Look up on MusicBrainz",
        searchable,
        colors,
        click(cx, |app, _, _| {
            if let Some(editor) = app.workbench.tag_editor.as_mut() {
                editor.search(&app.repaint);
            }
        }),
    )
    .tooltip(tooltip(
        "Search MusicBrainz for releases with this album title and artist",
    ));
    let releases = (!editor.releases.is_empty()).then(|| {
        let selected = editor
            .chosen
            .and_then(|index| editor.releases.get(index))
            .map_or(
                "Choose a release to fill in the tags".to_owned(),
                |release| release.label.clone(),
            );
        let labels = editor
            .releases
            .iter()
            .map(|release| release.label.clone())
            .collect::<Vec<_>>();
        let chosen = editor.chosen;
        let weak = cx.weak_entity();
        let width = content_width.min(560.0);
        div()
            .id("tag-editor-releases")
            .mt(sz(8.0))
            .tooltip(tooltip(
                "Releases differ in year, country, and sometimes track titles",
            ))
            .child(
                Button::new("tag-editor-release")
                    .label(selected)
                    .outline()
                    .small()
                    .w(sz(width))
                    .disabled(busy)
                    .dropdown_menu(move |mut menu, _, _| {
                        menu = menu.min_w(px(width));
                        for (index, text) in labels.iter().enumerate() {
                            menu = menu.item(
                                menu_item(text.clone(), &weak, move |app, _, _| {
                                    if let Some(editor) = app.workbench.tag_editor.as_mut() {
                                        editor.choose(&app.repaint, index);
                                    }
                                })
                                .checked(chosen == Some(index)),
                            );
                        }
                        menu
                    }),
            )
    });
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap(sz(8.0))
                .child(search)
                .children(waiting.map(|waiting| {
                    div()
                        .flex()
                        .items_center()
                        .gap(sz(6.0))
                        .child(Spinner::new().small().color(colors.muted))
                        .child(label(waiting, 13.0, colors.muted))
                })),
        )
        .children(releases)
        .children(editor.message.clone().map(|message| {
            div().mt(sz(8.0)).child(notice(
                colors,
                IconName::TriangleAlert,
                colors.warning,
                message,
            ))
        }))
        .into_any_element()
}

/// The album title, album artist and year fields.
fn album_fields(
    app: &mut ViperApp,
    content_width: f32,
    busy: bool,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let colors = theme::colors(cx);
    let width = content_width.min(560.0) - 100.0;
    let fields: [(&str, &str, f32, AlbumField); 3] = [
        ("Album", "Album title", width, |album| &mut album.title),
        ("Album artist", "Artist", width, |album| &mut album.artist),
        ("Year", "Year", 72.0, |album| &mut album.year),
    ];
    let rows = fields.map(|(name, placeholder, field_width, field)| {
        let value = app
            .workbench
            .tag_editor
            .as_mut()
            .map(|editor| field(&mut editor.album).clone())
            .unwrap_or_default();
        let input = text_field(
            app,
            format!("{FIELD_PREFIX}album-{name}"),
            &value,
            FieldOptions {
                placeholder,
                masked: false,
            },
            window,
            cx,
            move |app, text| {
                if let Some(editor) = app.workbench.tag_editor.as_mut() {
                    text.clone_into(field(&mut editor.album));
                }
            },
        )
        .disabled(busy);
        div()
            .flex()
            .items_center()
            .gap(sz(12.0))
            .child(field_label(colors, name).w(sz(88.0)))
            .child(div().w(sz(field_width)).child(input))
    });
    div()
        .flex()
        .flex_col()
        .gap(sz(8.0))
        .children(rows)
        .into_any_element()
}

/// The disc, number, title and artist fields of every track.
fn track_fields(
    app: &mut ViperApp,
    content_width: f32,
    busy: bool,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let colors = theme::colors(cx);
    let text_width = (content_width - NUMBER_WIDTH * 2.0 - FIELD_GAP * 3.0).max(0.0);
    let title_width = text_width * 0.55;
    let widths = [
        NUMBER_WIDTH,
        NUMBER_WIDTH,
        title_width,
        text_width - title_width,
    ];
    let columns: [(&str, &str, TrackField); 4] = [
        ("Disc", "Disc", |fields| &mut fields.disc),
        ("Track", "#", |fields| &mut fields.number),
        ("Title", "Title", |fields| &mut fields.title),
        ("Artist", "Artist", |fields| &mut fields.artist),
    ];
    let header = div().flex().gap(sz(FIELD_GAP)).mb(sz(4.0)).children(
        widths
            .into_iter()
            .zip(columns)
            .map(|(width, (name, _, _))| field_label(colors, name).w(sz(width))),
    );
    let count = app
        .workbench
        .tag_editor
        .as_ref()
        .map_or(0, |editor| editor.tracks.len());
    let rows = (0..count)
        .map(|track| {
            let inputs = widths
                .into_iter()
                .zip(columns)
                .map(|(width, (name, placeholder, field))| {
                    let value = app
                        .workbench
                        .tag_editor
                        .as_mut()
                        .and_then(|editor| editor.tracks.get_mut(track))
                        .map(|draft| field(&mut draft.fields).clone())
                        .unwrap_or_default();
                    let input = text_field(
                        app,
                        format!("{FIELD_PREFIX}track-{track}-{name}"),
                        &value,
                        FieldOptions {
                            placeholder,
                            masked: false,
                        },
                        window,
                        cx,
                        move |app, text| {
                            if let Some(draft) = app
                                .workbench
                                .tag_editor
                                .as_mut()
                                .and_then(|editor| editor.tracks.get_mut(track))
                            {
                                text.clone_into(field(&mut draft.fields));
                            }
                        },
                    )
                    .disabled(busy);
                    div().flex_none().w(sz(width)).child(input)
                })
                .collect::<Vec<_>>();
            div().flex().gap(sz(FIELD_GAP)).mb(sz(2.0)).children(inputs)
        })
        .collect::<Vec<_>>();
    div()
        .flex()
        .flex_col()
        .child(header)
        .children(rows)
        .into_any_element()
}

/// Cancel and Save, with a note while the tags are written.
fn footer(editor: &TagEditor, colors: Palette, cx: &Context<ViperApp>) -> AnyElement {
    let saving = matches!(editor.job, Job::Saving(_));
    div()
        .flex()
        .items_center()
        .justify_end()
        .gap(sz(8.0))
        .h(sz(36.0))
        .when(saving, |footer| {
            footer
                .child(Spinner::new().small().color(colors.muted))
                .child(label("Saving the tags…", 13.0, colors.muted))
        })
        .child(action_button(
            "tag-editor-cancel",
            "Cancel",
            !saving,
            colors,
            click(cx, |app, _, _| request_close(app)),
        ))
        .child(primary_button(
            "tag-editor-save",
            "Save",
            !editor.busy(),
            colors,
            click(cx, |app, _, _| {
                if let Some(editor) = app.workbench.tag_editor.as_mut() {
                    if editor.save(&app.repaint) {
                        close(app);
                    }
                }
            }),
        ))
        .into_any_element()
}
