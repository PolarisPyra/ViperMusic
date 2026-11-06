//! The Rip CD page: the drive and its disc, the disc's tags to check before ripping, and the rip's
//! progress and result.
use super::{
    drive::{Drive, DriveStatus},
    rip,
    state::{Cover, Lookup, Phase, Rip, RipperFeature, Session, TrackProgress},
};
use crate::{
    app::ViperApp,
    features::library::is_smb_path,
    platform::{desktop, persistence::settings::Settings},
    shared::ui::{
        controls::{
            action_button, click, describe_button, field_label, icon_button, label, listen,
            menu_item, notice, primary_button, sz, text_field, tooltip,
        },
        icons::{icon, IconName},
        text_input::FieldOptions,
        theme::{self, Palette},
    },
};
use gpui::{
    div, img, prelude::*, px, relative, AnyElement, ClickEvent, Context, Div, ElementId,
    FontWeight, SharedString, Stateful, Window,
};
use gpui_component::{
    button::Button, checkbox::Checkbox, input::Input, menu::DropdownMenu, spinner::Spinner,
    Disableable, Sizable,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// Side of the album cover beside the tags.
const COVER_SIZE: f32 = 168.0;
/// Height of a row in the track list.
const ROW_HEIGHT: f32 = 34.0;
/// Widest the release list and the album's tag fields grow.
const FIELDS_WIDTH: f32 = 520.0;
/// Width of the labels before the album's tag fields.
const LABEL_WIDTH: f32 = 88.0;

/// Draw the Rip CD page, watching the drives while it is shown.
///
/// # Arguments
/// * `app` - Application holding the ripper.
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
    app.ripper.watch_drives(&app.repaint);
    let destination = destination(&app.ripper, &app.settings);
    let header = header(&app.ripper, colors, cx);
    let body = match &app.ripper.phase {
        Phase::Waiting => waiting(waiting_for(&app.ripper), colors, cx),
        Phase::Reading(_) => busy_card(
            colors,
            "Reading the disc",
            "Reading its tracks and looking it up on MusicBrainz.",
        ),
        Phase::Failed(message) => card(colors)
            .child(card_title(
                colors,
                IconName::TriangleAlert,
                "Couldn't read this disc",
            ))
            .child(card_text(colors, message.clone()))
            .child(
                div()
                    .flex()
                    .gap(sz(8.0))
                    .mt(sz(14.0))
                    .child(primary_button(
                        "ripper-read-again",
                        "Try again",
                        true,
                        colors,
                        click(cx, |app, _, _| app.ripper.read_disc(&app.repaint)),
                    ))
                    .child(action_button(
                        "ripper-eject-failed",
                        "Eject",
                        true,
                        colors,
                        toggle_tray(cx),
                    )),
            ),
        Phase::Ready(_) => ready(app, window, colors, cx),
        Phase::Ripping(rip) => ripping(rip, false, colors, cx),
        Phase::Cancelling(rip) => ripping(rip, true, colors, cx),
        Phase::Done(rip) => {
            let draft = &rip.session.draft;
            let album = app.library.model.albums.iter().position(|album| {
                album.title == draft.album.trim() && album.artist == draft.artist.trim()
            });
            done(rip, album, colors, cx)
        }
    };
    // The rip button stays in view however long the track list is.
    let footer = match &app.ripper.phase {
        Phase::Ready(session) => Some(rip_footer(
            &app.ripper,
            session,
            destination,
            app.settings.music_path.as_deref(),
            colors,
            cx,
        )),
        _ => None,
    };
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(colors.canvas)
        .child(
            div()
                .id("ripper-content")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(sz(24.0))
                .py(sz(24.0))
                .child(header)
                .child(div().mt(sz(20.0)).child(body)),
        )
        .children(footer)
        .into_any_element()
}

/// Where albums are ripped to: the folder the user chose, else the library's folder when it is
/// on this computer, else `~/Music`.
fn destination(ripper: &RipperFeature, settings: &Settings) -> Option<PathBuf> {
    ripper
        .destination
        .clone()
        .or_else(|| {
            settings
                .music_path
                .clone()
                .filter(|path| !is_smb_path(path))
        })
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Music")))
}

/// Ask for the folder albums are ripped into.
fn choose_destination(cx: &mut Context<ViperApp>) {
    ViperApp::pick_folder(cx, |app, folder| app.ripper.destination = Some(folder));
}

/// A click handler that opens or closes the drive's tray.
fn toggle_tray(cx: &Context<ViperApp>) -> impl Fn(&ClickEvent, &mut Window, &mut gpui::App) {
    click(cx, |app, _, _| app.ripper.toggle_tray(&mut app.toasts))
}

fn header(ripper: &RipperFeature, colors: Palette, cx: &Context<ViperApp>) -> Div {
    let titles = div()
        .flex_1()
        .min_w_0()
        .child(label("Rip a CD", 30.0, colors.text).font_weight(FontWeight::BOLD))
        .child(
            label(
                "Copy an audio CD into your library as FLAC, tagged from MusicBrainz.",
                14.0,
                colors.muted,
            )
            .mt(sz(6.0)),
        );
    let row = div().flex().items_center().gap(sz(12.0)).child(titles);
    let Some(drives) = ripper.drives.as_deref().filter(|drives| !drives.is_empty()) else {
        return row;
    };
    let enabled = !ripper.busy();
    let selectable = enabled && drives.len() > 1;
    let selected = ripper.selected.clone();
    let names = drives
        .iter()
        .map(|(drive, _)| drive.clone())
        .collect::<Vec<Drive>>();
    let weak = cx.weak_entity();
    let drive_menu = Button::new("ripper-drive")
        .label(selected.as_ref().map_or("No drive", Drive::name).to_owned())
        .outline()
        .small()
        .w(sz(220.0))
        .disabled(!selectable)
        .dropdown_menu(move |mut menu, _, _| {
            menu = menu.min_w(px(220.0));
            for drive in &names {
                let chosen = drive.clone();
                menu = menu.item(
                    menu_item(drive.name().to_owned(), &weak, move |app, _, _| {
                        app.ripper.select_drive(chosen.clone());
                    })
                    .checked(Some(drive) == selected.as_ref()),
                );
            }
            menu
        });
    let hint = if selectable {
        "The drive to rip from"
    } else if enabled {
        "This is the only drive"
    } else {
        "Wait for the rip to finish to change drives"
    };
    let (tray_icon, tray_label) = if ripper.status() == Some(&DriveStatus::TrayOpen) {
        (IconName::ArrowDownToLine, "Close tray")
    } else {
        (IconName::Eject, "Eject")
    };
    let tray = if enabled {
        icon_button(
            "ripper-tray",
            tray_icon,
            tray_label,
            colors,
            toggle_tray(cx),
        )
        .into_any_element()
    } else {
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(sz(32.0))
            .child(icon(tray_icon, 16.0, colors.subtle))
            .into_any_element()
    };
    row.child(
        div()
            .id("ripper-drive-hint")
            .tooltip(tooltip(hint))
            .child(drive_menu),
    )
    .child(tray)
}

/// What the page waits for before there is a disc to read.
enum WaitingFor {
    FirstProbe,
    NoDrive,
    Disc(Option<DriveStatus>),
}

fn waiting_for(ripper: &RipperFeature) -> WaitingFor {
    match (&ripper.drives, ripper.status()) {
        (None, _) => WaitingFor::FirstProbe,
        (Some(drives), _) if drives.is_empty() => WaitingFor::NoDrive,
        (Some(_), status) => WaitingFor::Disc(status.cloned()),
    }
}

fn waiting(state: WaitingFor, colors: Palette, cx: &Context<ViperApp>) -> Div {
    match state {
        WaitingFor::FirstProbe | WaitingFor::Disc(None | Some(DriveStatus::DiscPresent)) => {
            busy_card(colors, "Looking for a disc", "Checking your CD drives.")
        }
        WaitingFor::Disc(Some(DriveStatus::NotReady)) => busy_card(
            colors,
            "The drive is spinning up",
            "The disc will be read as soon as the drive is ready.",
        ),
        WaitingFor::NoDrive => card(colors)
            .child(card_title(colors, IconName::Disc, "No CD drive found"))
            .child(card_text(
                colors,
                "Connect a CD or DVD drive. Viper Music finds it on its own.",
            )),
        WaitingFor::Disc(Some(DriveStatus::NoDisc)) => card(colors)
            .child(card_title(colors, IconName::Disc, "Insert an audio CD"))
            .child(card_text(
                colors,
                "Viper Music reads it as soon as it is in and looks it up on MusicBrainz.",
            )),
        WaitingFor::Disc(Some(DriveStatus::TrayOpen)) => card(colors)
            .child(card_title(colors, IconName::Disc, "The tray is open"))
            .child(card_text(colors, "Put an audio CD in and close the tray."))
            .child(div().flex().mt(sz(14.0)).child(primary_button(
                "ripper-close-tray",
                "Close tray",
                true,
                colors,
                toggle_tray(cx),
            ))),
        WaitingFor::Disc(Some(DriveStatus::Inaccessible(reason))) => card(colors)
            .child(card_title(
                colors,
                IconName::TriangleAlert,
                "Can't use this drive",
            ))
            .child(card_text(colors, reason))
            .when(cfg!(target_os = "linux"), |card| {
                card.child(card_text(
                    colors,
                    "On most systems your user needs to be in the optical or cdrom group.",
                ))
            }),
    }
}

fn ready(
    app: &mut ViperApp,
    window: &mut Window,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> Div {
    let Phase::Ready(session) = &app.ripper.phase else {
        return div();
    };
    let cover = cover(&session.cover, colors);
    let lookup = lookup_notice(session, colors, cx);
    let fields = album_fields(app, window, colors, cx);
    let tracks = track_list(app, window, colors, cx);
    div()
        .child(
            div().flex().items_start().gap(sz(20.0)).child(cover).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(sz(10.0))
                    .child(lookup)
                    .child(fields),
            ),
        )
        .child(div().mt(sz(20.0)).child(tracks))
}

/// Edit the draft of the disc being read, if one is open.
fn edit_session(app: &mut ViperApp, edit: impl FnOnce(&mut Session)) {
    if let Phase::Ready(session) = &mut app.ripper.phase {
        edit(session);
    }
}

/// A text field for one of the draft tags, kept showing the draft when it changes elsewhere,
/// such as when another release is chosen.
fn draft_field(
    app: &mut ViperApp,
    key: String,
    value: &str,
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
    edit: impl Fn(&mut Session, &str) + 'static,
) -> Input {
    let input = text_field(
        app,
        key.clone(),
        value,
        FieldOptions {
            placeholder,
            masked: false,
        },
        window,
        cx,
        move |app, text| edit_session(app, |session| edit(session, text)),
    );
    app.inputs.sync(&key, value, window, cx);
    input
}

fn cover(cover: &Cover, colors: Palette) -> AnyElement {
    let frame = div()
        .id("ripper-cover")
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(sz(COVER_SIZE))
        .rounded(sz(10.0))
        .bg(colors.surface);
    match cover {
        Cover::Ready { texture, .. } => img(texture.clone())
            .flex_none()
            .size(sz(COVER_SIZE))
            .rounded(sz(10.0))
            .into_any_element(),
        Cover::Loading { .. } => frame
            .child(Spinner::new().color(colors.muted))
            .into_any_element(),
        Cover::None | Cover::Unavailable { .. } => frame
            .tooltip(tooltip(if matches!(cover, Cover::None) {
                "No release chosen, so there is no cover"
            } else {
                "The Cover Art Archive has no cover for this release"
            }))
            .child(icon(IconName::Disc3, 56.0, colors.subtle))
            .into_any_element(),
    }
}

/// A line of accent text that runs `on_click`.
fn link(
    id: impl Into<ElementId>,
    text: &'static str,
    colors: Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> Stateful<Div> {
    describe_button(div().id(id), text)
        .cursor_pointer()
        .text_size(sz(13.0))
        .text_color(colors.accent)
        .hover(|style| style.underline())
        .on_click(on_click)
        .child(text)
}

fn lookup_notice(session: &Session, colors: Palette, cx: &Context<ViperApp>) -> Div {
    match &session.lookup {
        Lookup::Searching(_) => div()
            .flex()
            .items_center()
            .gap(sz(8.0))
            .child(Spinner::new().small().color(colors.muted))
            .child(label(
                "Looking up the disc on MusicBrainz…",
                13.0,
                colors.muted,
            )),
        Lookup::Matched { releases, chosen } if releases.len() > 1 => {
            let labels = releases
                .iter()
                .map(|release| SharedString::from(release.label()))
                .collect::<Vec<_>>();
            let chosen = *chosen;
            let weak = cx.weak_entity();
            div()
                .flex()
                .flex_col()
                .gap(sz(4.0))
                .child(notice(
                    colors,
                    IconName::CircleCheck,
                    colors.accent,
                    format!("{} releases have this disc. Pick yours:", releases.len()),
                ))
                .child(
                    div()
                        .id("ripper-releases")
                        .max_w(sz(FIELDS_WIDTH))
                        .tooltip(tooltip(
                            "Releases differ in year, country, and sometimes track titles",
                        ))
                        .child(
                            Button::new("ripper-release")
                                .label(labels[chosen].clone())
                                .outline()
                                .small()
                                .w_full()
                                .dropdown_menu(move |mut menu, _, _| {
                                    menu = menu.min_w(px(FIELDS_WIDTH));
                                    for (index, text) in labels.iter().enumerate() {
                                        menu = menu.item(
                                            menu_item(text.clone(), &weak, move |app, _, _| {
                                                let repaint = app.repaint.clone();
                                                edit_session(app, |session| {
                                                    session.choose_release(&repaint, index);
                                                });
                                            })
                                            .checked(index == chosen),
                                        );
                                    }
                                    menu
                                }),
                        ),
                )
        }
        Lookup::Matched { .. } => notice(
            colors,
            IconName::CircleCheck,
            colors.accent,
            "Found on MusicBrainz. Check the tags before ripping.",
        ),
        Lookup::NotFound => {
            let url = session.submission_url.clone();
            div()
                .flex()
                .flex_col()
                .gap(sz(4.0))
                .child(notice(
                    colors,
                    IconName::Info,
                    colors.muted,
                    "MusicBrainz doesn't know this disc. Fill in the tags yourself.",
                ))
                .child(link(
                    "ripper-submit",
                    "Add it to MusicBrainz, so its tags fill in next time",
                    colors,
                    move |_, _, cx| cx.open_url(&url),
                ))
        }
        Lookup::Failed(error) => div()
            .flex()
            .flex_col()
            .gap(sz(4.0))
            .child(notice(
                colors,
                IconName::TriangleAlert,
                colors.warning,
                error.clone(),
            ))
            .child(div().pl(sz(22.0)).flex().child(link(
                "ripper-search-again",
                "Try again",
                colors,
                click(cx, |app, _, _| {
                    let repaint = app.repaint.clone();
                    edit_session(app, |session| session.search_again(&repaint));
                }),
            ))),
    }
}

fn album_fields(
    app: &mut ViperApp,
    window: &mut Window,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> Div {
    let Phase::Ready(session) = &app.ripper.phase else {
        return div();
    };
    let draft = &session.draft;
    let [album, artist, year, disc, discs] = [
        draft.album.clone(),
        draft.artist.clone(),
        draft.year.clone(),
        draft.disc_number.to_string(),
        draft.disc_count.to_string(),
    ];
    let album = draft_field(
        app,
        "ripper-album".to_owned(),
        &album,
        "Album title",
        window,
        cx,
        |session, text| session.draft.album = text.to_owned(),
    );
    let artist = draft_field(
        app,
        "ripper-artist".to_owned(),
        &artist,
        "Artist",
        window,
        cx,
        |session, text| session.draft.set_artist(text.to_owned()),
    );
    let year = draft_field(
        app,
        "ripper-year".to_owned(),
        &year,
        "Year",
        window,
        cx,
        |session, text| session.draft.year = text.to_owned(),
    );
    let disc = draft_field(
        app,
        "ripper-disc".to_owned(),
        &disc,
        "1",
        window,
        cx,
        |session, text| {
            if let Ok(number) = text.trim().parse::<u32>() {
                session.draft.disc_number = number.clamp(1, session.draft.disc_count.max(1));
            }
        },
    );
    let discs = draft_field(
        app,
        "ripper-discs".to_owned(),
        &discs,
        "1",
        window,
        cx,
        |session, text| {
            if let Ok(count) = text.trim().parse::<u32>() {
                let draft = &mut session.draft;
                draft.disc_count = count.clamp(1, 99);
                draft.disc_number = draft.disc_number.min(draft.disc_count);
            }
        },
    );
    let row = |name: &'static str| {
        div()
            .flex()
            .items_center()
            .gap(sz(12.0))
            .child(field_label(colors, name).flex_none().w(sz(LABEL_WIDTH)))
    };
    let wide = |input: Input| {
        div()
            .flex_1()
            .min_w_0()
            .max_w(sz(FIELDS_WIDTH - 100.0))
            .child(input)
    };
    div()
        .flex()
        .flex_col()
        .gap(sz(8.0))
        .child(row("Album").child(wide(album)))
        .child(
            row("Album artist").child(
                div()
                    .id("ripper-artist-hint")
                    .flex_1()
                    .min_w_0()
                    .max_w(sz(FIELDS_WIDTH - 100.0))
                    .tooltip(tooltip("Track artists that match it change with it"))
                    .child(artist),
            ),
        )
        .child(
            row("Year")
                .child(div().flex_none().w(sz(72.0)).child(year))
                .child(field_label(colors, "Disc").ml(sz(4.0)))
                .child(div().flex_none().w(sz(48.0)).child(disc))
                .child(label("of", 13.0, colors.muted))
                .child(div().flex_none().w(sz(48.0)).child(discs)),
        )
}

fn track_list(
    app: &mut ViperApp,
    window: &mut Window,
    colors: Palette,
    cx: &mut Context<ViperApp>,
) -> Div {
    let Phase::Ready(session) = &app.ripper.phase else {
        return div();
    };
    let various = session.draft.various_artists();
    let all = session.draft.tracks.iter().all(|track| track.include);
    let count = session.tracks.len();
    let tracks = session
        .tracks
        .iter()
        .zip(&session.draft.tracks)
        .map(|(track, draft)| {
            (
                track.number,
                track.duration(),
                draft.include,
                draft.title.clone(),
                draft.artist.clone(),
            )
        })
        .collect::<Vec<_>>();
    let rows = tracks
        .into_iter()
        .enumerate()
        .map(|(index, (number, length, include, title, artist))| {
            let title = draft_field(
                app,
                format!("ripper-track-{index}-title"),
                &title,
                "Title",
                window,
                cx,
                move |session, text| {
                    if let Some(track) = session.draft.tracks.get_mut(index) {
                        track.title = text.to_owned();
                    }
                },
            );
            let artist = various.then(|| {
                draft_field(
                    app,
                    format!("ripper-track-{index}-artist"),
                    &artist,
                    "Artist",
                    window,
                    cx,
                    move |session, text| {
                        if let Some(track) = session.draft.tracks.get_mut(index) {
                            track.artist = text.to_owned();
                        }
                    },
                )
            });
            div()
                .flex()
                .items_center()
                .gap(sz(8.0))
                .h(sz(ROW_HEIGHT))
                .px(sz(4.0))
                .child(
                    div()
                        .id(("ripper-include-hint", index))
                        .tooltip(tooltip("Rip this track"))
                        .child(
                            Checkbox::new(("ripper-include", index))
                                .checked(include)
                                .on_click(listen(cx, move |app, checked: &bool, _, _| {
                                    edit_session(app, |session| {
                                        if let Some(track) = session.draft.tracks.get_mut(index) {
                                            track.include = *checked;
                                        }
                                    });
                                })),
                        ),
                )
                .child(
                    label(format!("{number:>2}"), 12.0, colors.muted)
                        .font_family("monospace")
                        .flex_none()
                        .w(sz(24.0)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .gap(sz(8.0))
                        .child(div().flex_1().min_w_0().child(title))
                        .children(artist.map(|artist| div().w(relative(0.4)).child(artist))),
                )
                .child(
                    label(clock(length), 12.0, colors.muted)
                        .font_family("monospace")
                        .flex_none(),
                )
        })
        .collect::<Vec<_>>();
    div()
        .bg(colors.surface)
        .border_1()
        .border_color(colors.border)
        .rounded(sz(12.0))
        .px(sz(12.0))
        .py(sz(10.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(sz(8.0))
                .px(sz(4.0))
                .mb(sz(4.0))
                .child(
                    div()
                        .id("ripper-include-all-hint")
                        .tooltip(tooltip(if all {
                            "Choose no tracks"
                        } else {
                            "Choose every track"
                        }))
                        .child(
                            Checkbox::new("ripper-include-all")
                                .checked(all)
                                .on_click(listen(cx, |app, checked: &bool, _, _| {
                                    edit_session(app, |session| {
                                        for track in &mut session.draft.tracks {
                                            track.include = *checked;
                                        }
                                    });
                                })),
                        ),
                )
                .child(
                    label(format!("{count} tracks"), 13.0, colors.text)
                        .font_weight(FontWeight::BOLD),
                ),
        )
        .children(rows)
}

fn rip_footer(
    ripper: &RipperFeature,
    session: &Session,
    destination: Option<PathBuf>,
    library_root: Option<&Path>,
    colors: Palette,
    cx: &Context<ViperApp>,
) -> Div {
    let paranoia_missing = ripper.needs_paranoia();
    let chosen = session
        .draft
        .tracks
        .iter()
        .filter(|track| track.include)
        .count();
    let blocked = if paranoia_missing {
        Some("Install cdparanoia to rip from this drive")
    } else if chosen == 0 {
        Some("Choose at least one track")
    } else if destination.is_none() {
        Some("Choose where the album goes")
    } else {
        None
    };
    let rip_label = match chosen {
        1 => "Rip 1 track".to_owned(),
        count => format!("Rip {count} tracks"),
    };
    let summary = match blocked {
        Some(reason) => {
            let tint = if paranoia_missing {
                colors.danger
            } else {
                colors.muted
            };
            label(reason, 13.0, tint)
        }
        None => label(
            format!(
                "{} · FLAC, tagged, with the cover",
                clock(session.duration())
            ),
            13.0,
            colors.muted,
        ),
    };
    let (path, tint) = match &destination {
        Some(root) => (
            rip::album_folder(
                root,
                session.draft.artist.trim(),
                session.draft.album.trim(),
            )
            .display()
            .to_string(),
            colors.text,
        ),
        None => ("No folder chosen".to_owned(), colors.warning),
    };
    let outside_library = match (&destination, library_root) {
        (Some(destination), Some(root)) => !destination.starts_with(root),
        (Some(_), None) => true,
        (None, _) => false,
    };
    let root = destination.clone();
    div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(sz(6.0))
        .bg(colors.panel)
        .border_t_1()
        .border_color(colors.border)
        .px(sz(24.0))
        .py(sz(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(sz(8.0))
                .child(primary_button(
                    "ripper-rip",
                    rip_label,
                    blocked.is_none(),
                    colors,
                    click(cx, move |app, _, _| {
                        if let Some(root) = &root {
                            app.ripper.start_rip(&app.repaint, root);
                        }
                    }),
                ))
                .child(summary.flex_1().min_w_0())
                .child(
                    div()
                        .id("ripper-eject-hint")
                        .tooltip(tooltip("Eject the disc once every track is ripped"))
                        .child(
                            Checkbox::new("ripper-eject-when-done")
                                .label("Eject when done")
                                .checked(ripper.eject_when_done)
                                .on_click(listen(cx, |app, checked: &bool, _, _| {
                                    app.ripper.eject_when_done = *checked;
                                })),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(sz(8.0))
                .child(field_label(colors, "Saves to").flex_none())
                .child(label(path, 13.0, tint).min_w_0().truncate())
                .child(
                    link(
                        "ripper-destination",
                        "Change…",
                        colors,
                        click(cx, |_, _, cx| choose_destination(cx)),
                    )
                    .flex_none(),
                ),
        )
        .when(outside_library, |footer| {
            footer.child(notice(
                colors,
                IconName::Info,
                colors.muted,
                "This folder is outside your library, so the album won't appear in Viper Music \
                 until you add it.",
            ))
        })
}

/// A progress bar filled to `fraction`.
fn bar(fraction: f32, height: f32, colors: Palette) -> Div {
    div()
        .h(sz(height))
        .rounded(sz(height / 2.0))
        .bg(colors.border)
        .child(
            div()
                .h_full()
                .w(relative(fraction.clamp(0.0, 1.0)))
                .rounded(sz(height / 2.0))
                .bg(colors.accent),
        )
}

fn ripping(rip: &Rip, cancelling: bool, colors: Palette, cx: &Context<ViperApp>) -> Div {
    let done = rip.count(|row| *row == TrackProgress::Done);
    let total = rip.count(|row| *row != TrackProgress::Skipped);
    let progress = rip.progress();
    let detail = if cancelling {
        "Stopping after the track being read…".to_owned()
    } else {
        match rip.remaining() {
            Some(left) => format!("{done} of {total} tracks · {}", left_text(left)),
            None => format!("{done} of {total} tracks"),
        }
    };
    div()
        .child(rip_header(rip, colors))
        .child(
            card(colors)
                .mt(sz(16.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            label(
                                format!("Ripping · {:.0}%", progress * 100.0),
                                15.0,
                                colors.text,
                            )
                            .flex_1()
                            .font_weight(FontWeight::BOLD),
                        )
                        .child(
                            action_button(
                                "ripper-cancel",
                                if cancelling { "Stopping…" } else { "Cancel" },
                                !cancelling,
                                colors,
                                click(cx, |app, _, _| app.ripper.cancel()),
                            )
                            .tooltip(tooltip("Stop ripping; tracks already ripped are kept")),
                        ),
                )
                .child(bar(progress, 8.0, colors).mt(sz(8.0)))
                .child(label(detail, 12.0, colors.muted).mt(sz(6.0))),
        )
        .child(progress_rows(rip, colors).mt(sz(16.0)))
}

fn done(rip: &Rip, album: Option<usize>, colors: Palette, cx: &Context<ViperApp>) -> Div {
    let ripped = rip.count(|row| *row == TrackProgress::Done);
    let failed = rip.count(|row| matches!(row, TrackProgress::Failed(_)));
    let summary = if failed == 0 {
        card(colors)
            .child(card_title(colors, IconName::CircleCheck, "Ripped"))
            .child(card_text(
                colors,
                format!(
                    "{ripped} {} saved to {}.",
                    if ripped == 1 { "track" } else { "tracks" },
                    rip.folder.display()
                ),
            ))
    } else {
        card(colors)
            .child(card_title(
                colors,
                IconName::TriangleAlert,
                format!("{failed} of {} tracks could not be read", ripped + failed),
            ))
            .child(card_text(
                colors,
                "Scratches and dirt cause most read errors. Clean the disc and rip the failed \
                 tracks again; the tracks that worked are kept.",
            ))
    };
    let main = if failed > 0 && !rip.ejected {
        Some(primary_button(
            "ripper-retry",
            "Rip failed tracks again",
            true,
            colors,
            click(cx, |app, _, _| {
                let repaint = app.repaint.clone();
                app.ripper.retry_failed(&repaint);
            }),
        ))
    } else {
        album.map(|index| {
            primary_button(
                "ripper-show-album",
                "Show in Albums",
                true,
                colors,
                click(cx, move |app, _, _| {
                    app.workbench.open_album(&mut app.library.view, index);
                }),
            )
        })
    };
    let folder = rip.folder.clone();
    let buttons = div()
        .flex()
        .flex_wrap()
        .gap(sz(8.0))
        .mt(sz(14.0))
        .children(main)
        .child(action_button(
            "ripper-open-folder",
            "Open folder",
            true,
            colors,
            click(cx, move |app, _, _| {
                if let Err(error) = desktop::open_folder(&folder) {
                    app.toasts
                        .push(format!("Could not open the folder: {error}"));
                }
            }),
        ))
        .when(!rip.ejected, |buttons| {
            buttons.child(action_button(
                "ripper-edit-again",
                "Edit tags and rip again",
                true,
                colors,
                click(cx, |app, _, _| app.ripper.edit_again()),
            ))
        })
        .child(action_button(
            "ripper-next-disc",
            "Rip another CD",
            true,
            colors,
            click(cx, |app, _, _| app.ripper.next_disc(&mut app.toasts)),
        ));
    div()
        .child(rip_header(rip, colors))
        .child(
            summary
                .mt(sz(16.0))
                .when(rip.ejected, |card| {
                    card.child(card_text(
                        colors,
                        "The disc was ejected. Insert another to rip it.",
                    ))
                })
                .child(buttons),
        )
        .child(progress_rows(rip, colors).mt(sz(16.0)))
}

/// The album being ripped: its cover, title, and artist.
fn rip_header(rip: &Rip, colors: Palette) -> Div {
    let draft = &rip.session.draft;
    let cover = match &rip.session.cover {
        Cover::Ready { texture, .. } => Some(
            img(texture.clone())
                .flex_none()
                .size(sz(72.0))
                .rounded(sz(8.0)),
        ),
        _ => None,
    };
    div()
        .flex()
        .items_center()
        .gap(sz(12.0))
        .children(cover)
        .child(
            div()
                .min_w_0()
                .child(
                    label(draft.album.trim().to_owned(), 19.0, colors.text)
                        .font_weight(FontWeight::BOLD),
                )
                .child(label(draft.artist.trim().to_owned(), 14.0, colors.muted)),
        )
}

fn progress_rows(rip: &Rip, colors: Palette) -> Div {
    let rows = rip
        .session
        .tracks
        .iter()
        .zip(&rip.session.draft.tracks)
        .zip(&rip.rows)
        .enumerate()
        .map(|(index, ((track, draft), row))| {
            let fraction = |read: u64| read as f64 / track.bytes() as f64;
            let (name, tint, status) = match row {
                TrackProgress::Skipped => (IconName::Minus, colors.subtle, "Skipped".to_owned()),
                TrackProgress::Queued => (IconName::Clock, colors.subtle, "Waiting".to_owned()),
                TrackProgress::Reading { read } => (
                    IconName::Disc,
                    colors.accent,
                    format!("{:.0}%", fraction(*read) * 100.0),
                ),
                TrackProgress::Done => (IconName::Check, colors.accent, "Ripped".to_owned()),
                TrackProgress::Failed(_) => {
                    (IconName::TriangleAlert, colors.danger, "Failed".to_owned())
                }
            };
            let title_color = if *row == TrackProgress::Skipped {
                colors.subtle
            } else {
                colors.text
            };
            let reading = match row {
                TrackProgress::Reading { read } => Some(
                    bar(fraction(*read) as f32, 4.0, colors)
                        .flex_none()
                        .w(sz(120.0)),
                ),
                _ => None,
            };
            let error = match row {
                TrackProgress::Failed(error) => Some(error.clone()),
                _ => None,
            };
            div()
                .id(("ripper-progress", index))
                .flex()
                .items_center()
                .gap(sz(8.0))
                .h(sz(28.0))
                .child(icon(name, 15.0, tint))
                .child(
                    label(format!("{:>2}", track.number), 12.0, colors.muted)
                        .font_family("monospace")
                        .flex_none(),
                )
                .child(
                    label(draft.title.trim().to_owned(), 13.0, title_color)
                        .flex_1()
                        .min_w_0()
                        .truncate(),
                )
                .children(reading)
                .child(label(status, 12.0, tint).flex_none())
                .when_some(error, |row, error| row.tooltip(tooltip(error)))
        })
        .collect::<Vec<_>>();
    div()
        .bg(colors.surface)
        .border_1()
        .border_color(colors.border)
        .rounded(sz(12.0))
        .px(sz(16.0))
        .py(sz(10.0))
        .children(rows)
}

fn card(colors: Palette) -> Div {
    div()
        .bg(colors.surface)
        .border_1()
        .border_color(colors.border)
        .rounded(sz(12.0))
        // The same inset as the track list's rows, so stacked cards and lists line up inside.
        .px(sz(16.0))
        .py(sz(20.0))
}

fn busy_card(colors: Palette, title: &'static str, text: &'static str) -> Div {
    card(colors)
        .child(
            div()
                .flex()
                .items_center()
                .gap(sz(8.0))
                .mb(sz(5.0))
                .child(Spinner::new().color(colors.accent))
                .child(label(title, 19.0, colors.text).font_weight(FontWeight::BOLD)),
        )
        .child(card_text(colors, text))
}

fn card_title(colors: Palette, name: IconName, title: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .gap(sz(8.0))
        .mb(sz(5.0))
        .child(icon(name, 20.0, colors.accent))
        .child(label(title, 19.0, colors.text).font_weight(FontWeight::BOLD))
}

fn card_text(colors: Palette, text: impl Into<SharedString>) -> Div {
    label(text, 13.0, colors.muted)
}

/// `m:ss`, or `h:mm:ss` from an hour.
fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

fn left_text(left: Duration) -> String {
    match left.as_secs() {
        0..60 => "less than a minute left".to_owned(),
        60..120 => "about a minute left".to_owned(),
        seconds => format!("about {} minutes left", seconds.div_ceil(60)),
    }
}
