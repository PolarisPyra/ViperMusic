//! The Rip CD page: the drive and its disc, the disc's tags to check before ripping, and the rip's
//! progress and result.
//!
//! Drawing only reads the ripper and edits the draft tags; everything else the user asks for is
//! collected as a [`Command`] and applied after the frame is drawn, so no state changes under the
//! code drawing it.
use super::{
    drive::{Drive, DriveStatus},
    rip,
    state::{Cover, Lookup, Phase, Rip, RipperFeature, Session, TrackProgress},
};
use crate::{
    features::library::{is_smb_path, state::LibraryFeature},
    platform::{desktop, persistence::settings::Settings},
    shared::ui::{
        controls::{action_button, connection_text_edit, describe_button},
        theme::{self, icon_font, Palette},
        toasts::Toasts,
    },
    workbench::WorkbenchState,
};
use eframe::egui;
use egui_phosphor::regular as icons;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// Side of the album cover beside the tags.
const COVER_SIZE: f32 = 168.0;
/// Height of a row in the track list.
const ROW_HEIGHT: f32 = 34.0;

pub(crate) struct RipperScreen<'a> {
    pub(crate) ripper: &'a mut RipperFeature,
    pub(crate) workbench: &'a mut WorkbenchState,
    pub(crate) library: &'a mut LibraryFeature,
    pub(crate) settings: &'a Settings,
    pub(crate) toasts: &'a mut Toasts,
}

/// What the user asked for while the page was drawn.
enum Command {
    SelectDrive(Drive),
    ToggleTray,
    ReadAgain,
    SearchAgain,
    ChooseRelease(usize),
    ChooseDestination,
    Rip(PathBuf),
    Cancel,
    RetryFailed,
    EditAgain,
    NextDisc,
    OpenFolder(PathBuf),
    ShowAlbum(usize),
}

pub(crate) fn show(ui: &mut egui::Ui, screen: RipperScreen<'_>) {
    let ctx = &ui.ctx().clone();
    let colors = theme::colors(ctx);
    screen.ripper.watch_drives(ctx);
    let destination = destination(screen.ripper, screen.settings);
    let waiting_for = screen.waiting_for();
    let mut command = None;
    let paranoia_missing = screen.ripper.needs_paranoia();
    let ripper = &mut *screen.ripper;
    if let Phase::Ready(session) = &ripper.phase {
        // The rip button stays in view however long the track list is.
        egui::Panel::bottom("ripper-actions")
            .frame(
                egui::Frame::new()
                    .fill(colors.panel)
                    .inner_margin(egui::Margin::symmetric(24, 12)),
            )
            .show(ui, |ui| {
                rip_footer(
                    ui,
                    colors,
                    session,
                    ReadyFooter {
                        destination: destination.as_deref(),
                        library_root: screen.settings.music_path.as_deref(),
                        eject_when_done: &mut ripper.eject_when_done,
                        paranoia_missing,
                    },
                    &mut command,
                );
            });
    }
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(colors.canvas)
                .inner_margin(egui::Margin::symmetric(24, 0)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("ripper-content")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(24.0);
                    header(ui, colors, screen.ripper, &mut command);
                    ui.add_space(20.0);
                    match &mut screen.ripper.phase {
                        Phase::Waiting => waiting(ui, colors, waiting_for, &mut command),
                        Phase::Reading(_) => busy_card(
                            ui,
                            colors,
                            "Reading the disc",
                            "Reading its tracks and looking it up on MusicBrainz.",
                        ),
                        Phase::Failed(message) => {
                            card(ui, colors, |ui| {
                                card_title(ui, colors, icons::WARNING, "Couldn't read this disc");
                                card_text(ui, colors, message);
                                ui.add_space(14.0);
                                ui.horizontal(|ui| {
                                    if primary_button(ui, colors, "Try again", true).clicked() {
                                        command = Some(Command::ReadAgain);
                                    }
                                    if action_button(ui, "Eject", true).clicked() {
                                        command = Some(Command::ToggleTray);
                                    }
                                });
                            });
                        }
                        Phase::Ready(session) => ready(ui, colors, session, &mut command),
                        Phase::Ripping(rip) => ripping(ui, colors, rip, false, &mut command),
                        Phase::Cancelling(rip) => ripping(ui, colors, rip, true, &mut command),
                        Phase::Done(rip) => {
                            done(ui, colors, rip, screen.library, &mut command);
                        }
                    }
                    ui.add_space(24.0);
                });
        });
    if let Some(command) = command {
        apply(command, ctx, screen);
    }
}

impl RipperScreen<'_> {
    fn waiting_for(&self) -> WaitingFor {
        match (&self.ripper.drives, self.ripper.status()) {
            (None, _) => WaitingFor::FirstProbe,
            (Some(drives), _) if drives.is_empty() => WaitingFor::NoDrive,
            (Some(_), status) => WaitingFor::Disc(status.cloned()),
        }
    }
}

fn apply(command: Command, ctx: &egui::Context, screen: RipperScreen<'_>) {
    let ripper = screen.ripper;
    match command {
        Command::SelectDrive(drive) => ripper.select_drive(drive),
        Command::ToggleTray => ripper.toggle_tray(screen.toasts),
        Command::ReadAgain => ripper.read_disc(),
        Command::SearchAgain => {
            if let Phase::Ready(session) = &mut ripper.phase {
                session.search_again(ctx);
            }
        }
        Command::ChooseRelease(index) => {
            if let Phase::Ready(session) = &mut ripper.phase {
                session.choose_release(ctx, index);
            }
        }
        Command::ChooseDestination => {
            let mut dialog = rfd::FileDialog::new().set_title("Choose where ripped albums go");
            if let Some(current) = destination(ripper, screen.settings) {
                dialog = dialog.set_directory(current);
            }
            if let Some(folder) = dialog.pick_folder() {
                ripper.destination = Some(folder);
            }
        }
        Command::Rip(root) => ripper.start_rip(ctx, &root),
        Command::Cancel => ripper.cancel(),
        Command::RetryFailed => ripper.retry_failed(ctx),
        Command::EditAgain => ripper.edit_again(),
        Command::NextDisc => ripper.next_disc(screen.toasts),
        Command::OpenFolder(folder) => {
            if let Err(error) = desktop::open_folder(&folder) {
                screen
                    .toasts
                    .push(format!("Could not open the folder: {error}"));
            }
        }
        Command::ShowAlbum(index) => {
            screen.workbench.open_album(&mut screen.library.view, index);
        }
    }
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

fn header(
    ui: &mut egui::Ui,
    colors: Palette,
    ripper: &RipperFeature,
    command: &mut Option<Command>,
) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Rip a CD")
                    .size(30.0)
                    .strong()
                    .color(colors.text),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Copy an audio CD into your library as FLAC, tagged from MusicBrainz.",
                )
                .size(14.0)
                .color(colors.muted),
            );
        });
        let Some(drives) = ripper.drives.as_deref().filter(|drives| !drives.is_empty()) else {
            return;
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tray_open = ripper.status() == Some(&DriveStatus::TrayOpen);
            let (icon, label) = if tray_open {
                (icons::ARROW_LINE_DOWN, "Close tray")
            } else {
                (icons::EJECT, "Eject")
            };
            let enabled = !ripper.busy();
            if icon_button(ui, colors, icon, label, enabled).clicked() {
                *command = Some(Command::ToggleTray);
            }
            let selected = ripper.selected.as_ref();
            let selected_name = selected.map_or("No drive", Drive::name);
            ui.add_enabled_ui(enabled && drives.len() > 1, |ui| {
                egui::ComboBox::from_id_salt("ripper-drive")
                    .selected_text(selected_name)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for (drive, _) in drives {
                            if ui
                                .selectable_label(Some(drive) == selected, drive.name())
                                .clicked()
                            {
                                *command = Some(Command::SelectDrive(drive.clone()));
                            }
                        }
                    })
                    .response
                    .on_hover_text("The drive to rip from")
                    .on_disabled_hover_text(if enabled {
                        "This is the only drive"
                    } else {
                        "Wait for the rip to finish to change drives"
                    });
            });
        });
    });
}

/// What the page waits for before there is a disc to read.
enum WaitingFor {
    FirstProbe,
    NoDrive,
    Disc(Option<DriveStatus>),
}

fn waiting(ui: &mut egui::Ui, colors: Palette, state: WaitingFor, command: &mut Option<Command>) {
    match state {
        WaitingFor::FirstProbe | WaitingFor::Disc(None | Some(DriveStatus::DiscPresent)) => {
            busy_card(ui, colors, "Looking for a disc", "Checking your CD drives.");
        }
        WaitingFor::Disc(Some(DriveStatus::NotReady)) => busy_card(
            ui,
            colors,
            "The drive is spinning up",
            "The disc will be read as soon as the drive is ready.",
        ),
        WaitingFor::NoDrive => card(ui, colors, |ui| {
            card_title(ui, colors, icons::DISC, "No CD drive found");
            card_text(
                ui,
                colors,
                "Connect a CD or DVD drive. Viper Music finds it on its own.",
            );
        }),
        WaitingFor::Disc(Some(DriveStatus::NoDisc)) => card(ui, colors, |ui| {
            card_title(ui, colors, icons::DISC, "Insert an audio CD");
            card_text(
                ui,
                colors,
                "Viper Music reads it as soon as it is in and looks it up on MusicBrainz.",
            );
        }),
        WaitingFor::Disc(Some(DriveStatus::TrayOpen)) => card(ui, colors, |ui| {
            card_title(ui, colors, icons::DISC, "The tray is open");
            card_text(ui, colors, "Put an audio CD in and close the tray.");
            ui.add_space(14.0);
            if primary_button(ui, colors, "Close tray", true).clicked() {
                *command = Some(Command::ToggleTray);
            }
        }),
        WaitingFor::Disc(Some(DriveStatus::Inaccessible(reason))) => card(ui, colors, |ui| {
            card_title(ui, colors, icons::WARNING, "Can't use this drive");
            card_text(ui, colors, &reason);
            if cfg!(target_os = "linux") {
                card_text(
                    ui,
                    colors,
                    "On most systems your user needs to be in the optical or cdrom group.",
                );
            }
        }),
    }
}

struct ReadyFooter<'a> {
    destination: Option<&'a Path>,
    library_root: Option<&'a Path>,
    eject_when_done: &'a mut bool,
    paranoia_missing: bool,
}

fn ready(ui: &mut egui::Ui, colors: Palette, session: &mut Session, command: &mut Option<Command>) {
    ui.horizontal_top(|ui| {
        cover(ui, colors, &session.cover);
        ui.add_space(20.0);
        ui.vertical(|ui| {
            lookup_notice(ui, colors, session, command);
            ui.add_space(10.0);
            album_fields(ui, colors, session);
        });
    });
    ui.add_space(20.0);
    track_list(ui, colors, session);
}

fn cover(ui: &mut egui::Ui, colors: Palette, cover: &Cover) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(COVER_SIZE, COVER_SIZE), egui::Sense::hover());
    let painter = ui.painter();
    match cover {
        Cover::Ready { texture, .. } => {
            egui::Image::new((texture.id(), rect.size()))
                .corner_radius(10.0)
                .paint_at(ui, rect);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Image, true, "Album cover")
            });
        }
        Cover::Loading { .. } => {
            painter.rect_filled(rect, 10.0, colors.surface);
            egui::Spinner::new()
                .size(22.0)
                .color(colors.muted)
                .paint_at(
                    ui,
                    egui::Rect::from_center_size(rect.center(), egui::vec2(22.0, 22.0)),
                );
        }
        Cover::None | Cover::Unavailable { .. } => {
            painter.rect_filled(rect, 10.0, colors.surface);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                icons::VINYL_RECORD,
                icon_font(56.0),
                colors.subtle,
            );
            response.on_hover_text(if matches!(cover, Cover::None) {
                "No release chosen, so there is no cover"
            } else {
                "The Cover Art Archive has no cover for this release"
            });
        }
    }
}

fn lookup_notice(
    ui: &mut egui::Ui,
    colors: Palette,
    session: &Session,
    command: &mut Option<Command>,
) {
    match &session.lookup {
        Lookup::Searching(_) => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0).color(colors.muted));
                ui.label(
                    egui::RichText::new("Looking up the disc on MusicBrainz…")
                        .size(13.0)
                        .color(colors.muted),
                );
            });
        }
        Lookup::Matched { releases, chosen } if releases.len() > 1 => {
            ui.horizontal(|ui| {
                notice(
                    ui,
                    colors,
                    icons::CHECK_CIRCLE,
                    colors.accent,
                    &format!("{} releases have this disc. Pick yours:", releases.len()),
                );
            });
            ui.add_space(4.0);
            egui::ComboBox::from_id_salt("ripper-release")
                .selected_text(releases[*chosen].label())
                .width(ui.available_width().min(520.0))
                .show_ui(ui, |ui| {
                    for (index, release) in releases.iter().enumerate() {
                        if ui
                            .selectable_label(index == *chosen, release.label())
                            .clicked()
                        {
                            *command = Some(Command::ChooseRelease(index));
                        }
                    }
                })
                .response
                .on_hover_text("Releases differ in year, country, and sometimes track titles");
        }
        Lookup::Matched { .. } => notice(
            ui,
            colors,
            icons::CHECK_CIRCLE,
            colors.accent,
            "Found on MusicBrainz. Check the tags before ripping.",
        ),
        Lookup::NotFound => {
            notice(
                ui,
                colors,
                icons::INFO,
                colors.muted,
                "MusicBrainz doesn't know this disc. Fill in the tags yourself.",
            );
            ui.hyperlink_to(
                egui::RichText::new("Add it to MusicBrainz, so its tags fill in next time")
                    .size(13.0)
                    .color(colors.accent),
                &session.submission_url,
            );
        }
        Lookup::Failed(error) => {
            notice(ui, colors, icons::WARNING, colors.warning, error);
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                if ui
                    .link(
                        egui::RichText::new("Try again")
                            .size(13.0)
                            .color(colors.accent),
                    )
                    .clicked()
                {
                    *command = Some(Command::SearchAgain);
                }
            });
        }
    }
}

fn notice(ui: &mut egui::Ui, colors: Palette, icon: &str, tint: egui::Color32, text: &str) {
    ui.horizontal_top(|ui| {
        ui.label(egui::RichText::new(icon).font(icon_font(15.0)).color(tint));
        // Wrap long messages rather than widen the page.
        ui.add(egui::Label::new(egui::RichText::new(text).size(13.0).color(colors.text)).wrap());
    });
}

fn album_fields(ui: &mut egui::Ui, colors: Palette, session: &mut Session) {
    let draft = &mut session.draft;
    let width = ui.available_width().min(520.0);
    egui::Grid::new("ripper-album")
        .num_columns(2)
        .spacing(egui::vec2(12.0, 8.0))
        .show(ui, |ui| {
            field_label(ui, colors, "Album");
            ui.scope(|ui| {
                ui.set_width(width - 100.0);
                connection_text_edit(ui, &mut draft.album, "Album title", false);
            });
            ui.end_row();

            field_label(ui, colors, "Album artist");
            let mut artist = draft.artist.clone();
            ui.scope(|ui| {
                ui.set_width(width - 100.0);
                if connection_text_edit(ui, &mut artist, "Artist", false).changed() {
                    draft.set_artist(artist);
                }
            })
            .response
            .on_hover_text("Track artists that match it change with it");
            ui.end_row();

            field_label(ui, colors, "Year");
            ui.horizontal(|ui| {
                ui.scope(|ui| {
                    ui.set_width(72.0);
                    connection_text_edit(ui, &mut draft.year, "Year", false);
                });
                ui.add_space(16.0);
                field_label(ui, colors, "Disc");
                ui.add(
                    egui::DragValue::new(&mut draft.disc_number).range(1..=draft.disc_count.max(1)),
                );
                ui.label(egui::RichText::new("of").size(13.0).color(colors.muted));
                ui.add(egui::DragValue::new(&mut draft.disc_count).range(1..=99));
            });
            ui.end_row();
        });
    if draft.disc_number > draft.disc_count {
        draft.disc_number = draft.disc_count;
    }
}

fn field_label(ui: &mut egui::Ui, colors: Palette, text: &str) {
    ui.label(egui::RichText::new(text).size(13.0).color(colors.muted));
}

fn track_list(ui: &mut egui::Ui, colors: Palette, session: &mut Session) {
    let various = session.draft.various_artists();
    let all = session.draft.tracks.iter().all(|track| track.include);
    let none = !session.draft.tracks.iter().any(|track| track.include);
    egui::Frame::new()
        .fill(colors.surface)
        .stroke(egui::Stroke::new(1.0, colors.border))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // In line with the rows' checkboxes, which sit inside the rows' margin.
                ui.add_space(4.0);
                let mut every = all;
                let response = ui
                    .add(egui::Checkbox::new(&mut every, "").indeterminate(!all && !none))
                    .on_hover_text(if all {
                        "Choose no tracks"
                    } else {
                        "Choose every track"
                    });
                describe_button(&response, "Choose every track");
                if response.clicked() {
                    for track in &mut session.draft.tracks {
                        track.include = !all;
                    }
                }
                ui.label(
                    egui::RichText::new(format!("{} tracks", session.tracks.len()))
                        .size(13.0)
                        .strong()
                        .color(colors.text),
                );
            });
            ui.add_space(4.0);
            let length_width = 52.0;
            for (track, draft) in session.tracks.iter().zip(&mut session.draft.tracks) {
                egui::Frame::new()
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(4, 0))
                    .show(ui, |ui| {
                        ui.set_height(ROW_HEIGHT);
                        ui.horizontal_centered(|ui| {
                            ui.checkbox(&mut draft.include, "")
                                .on_hover_text("Rip this track");
                            ui.add_sized(
                                egui::vec2(24.0, ROW_HEIGHT),
                                egui::Label::new(
                                    egui::RichText::new(format!("{:>2}", track.number))
                                        .size(12.0)
                                        .monospace()
                                        .color(colors.muted),
                                ),
                            );
                            let text_width = ui.available_width() - length_width - 12.0;
                            let title_width = if various {
                                text_width * 0.6
                            } else {
                                text_width
                            };
                            ui.scope(|ui| {
                                ui.set_width(title_width);
                                connection_text_edit(ui, &mut draft.title, "Title", false);
                            });
                            if various {
                                ui.scope(|ui| {
                                    ui.set_width(text_width - title_width - 8.0);
                                    connection_text_edit(ui, &mut draft.artist, "Artist", false);
                                });
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(clock(track.duration()))
                                            .size(12.0)
                                            .monospace()
                                            .color(colors.muted),
                                    );
                                },
                            );
                        });
                    });
            }
        });
}

fn rip_footer(
    ui: &mut egui::Ui,
    colors: Palette,
    session: &Session,
    footer: ReadyFooter<'_>,
    command: &mut Option<Command>,
) {
    let chosen = session
        .draft
        .tracks
        .iter()
        .filter(|track| track.include)
        .count();
    let blocked = if footer.paranoia_missing {
        Some("Install cdparanoia to rip from this drive")
    } else if chosen == 0 {
        Some("Choose at least one track")
    } else if footer.destination.is_none() {
        Some("Choose where the album goes")
    } else {
        None
    };
    ui.horizontal(|ui| {
        let label = match chosen {
            1 => "Rip 1 track".to_owned(),
            count => format!("Rip {count} tracks"),
        };
        if primary_button(ui, colors, &label, blocked.is_none()).clicked() {
            if let Some(root) = footer.destination {
                *command = Some(Command::Rip(root.to_path_buf()));
            }
        }
        ui.add_space(8.0);
        match blocked {
            Some(reason) => {
                let tint = if footer.paranoia_missing {
                    colors.danger
                } else {
                    colors.muted
                };
                ui.label(egui::RichText::new(reason).size(13.0).color(tint));
            }
            None => {
                ui.label(
                    egui::RichText::new(format!(
                        "{} · FLAC, tagged, with the cover",
                        clock(session.duration())
                    ))
                    .size(13.0)
                    .color(colors.muted),
                );
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.checkbox(footer.eject_when_done, "Eject when done")
                .on_hover_text("Eject the disc once every track is ripped");
        });
    });
    ui.add_space(6.0);
    let (path, tint) = match footer.destination {
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
    let outside_library = match (footer.destination, footer.library_root) {
        (Some(destination), Some(root)) => !destination.starts_with(root),
        (Some(_), None) => true,
        (None, _) => false,
    };
    ui.horizontal(|ui| {
        field_label(ui, colors, "Saves to");
        // Leave room for the link after a long path.
        let width = (ui.available_width() - 64.0).max(0.0);
        ui.allocate_ui(egui::vec2(width, 20.0), |ui| {
            ui.add(egui::Label::new(egui::RichText::new(path).size(13.0).color(tint)).truncate());
        });
        if ui
            .link(
                egui::RichText::new("Change…")
                    .size(13.0)
                    .color(colors.accent),
            )
            .clicked()
        {
            *command = Some(Command::ChooseDestination);
        }
    });
    if outside_library {
        notice(
            ui,
            colors,
            icons::INFO,
            colors.muted,
            "This folder is outside your library, so the album won't appear in Viper Music \
             until you add it.",
        );
    }
}

fn ripping(
    ui: &mut egui::Ui,
    colors: Palette,
    rip: &Rip,
    cancelling: bool,
    command: &mut Option<Command>,
) {
    rip_header(ui, colors, rip);
    ui.add_space(16.0);
    card(ui, colors, |ui| {
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
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Ripping · {:.0}%", progress * 100.0))
                    .size(15.0)
                    .strong()
                    .color(colors.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if cancelling { "Stopping…" } else { "Cancel" };
                if action_button(ui, label, !cancelling)
                    .on_hover_text("Stop ripping; tracks already ripped are kept")
                    .clicked()
                {
                    *command = Some(Command::Cancel);
                }
            });
        });
        ui.add_space(8.0);
        ui.add(
            egui::ProgressBar::new(progress)
                .fill(colors.accent)
                .desired_height(8.0)
                .corner_radius(4.0),
        );
        ui.add_space(6.0);
        ui.label(egui::RichText::new(detail).size(12.0).color(colors.muted));
    });
    ui.add_space(16.0);
    progress_rows(ui, colors, rip);
}

fn done(
    ui: &mut egui::Ui,
    colors: Palette,
    rip: &Rip,
    library: &LibraryFeature,
    command: &mut Option<Command>,
) {
    rip_header(ui, colors, rip);
    ui.add_space(16.0);
    let ripped = rip.count(|row| *row == TrackProgress::Done);
    let failed = rip.count(|row| matches!(row, TrackProgress::Failed(_)));
    let draft = &rip.session.draft;
    let album =
        library.model.albums.iter().position(|album| {
            album.title == draft.album.trim() && album.artist == draft.artist.trim()
        });
    card(ui, colors, |ui| {
        if failed == 0 {
            card_title(ui, colors, icons::CHECK_CIRCLE, "Ripped");
            card_text(
                ui,
                colors,
                &format!(
                    "{ripped} {} saved to {}.",
                    if ripped == 1 { "track" } else { "tracks" },
                    rip.folder.display()
                ),
            );
        } else {
            card_title(
                ui,
                colors,
                icons::WARNING,
                &format!("{failed} of {} tracks could not be read", ripped + failed),
            );
            card_text(
                ui,
                colors,
                "Scratches and dirt cause most read errors. Clean the disc and rip the failed \
                 tracks again; the tracks that worked are kept.",
            );
        }
        if rip.ejected {
            card_text(
                ui,
                colors,
                "The disc was ejected. Insert another to rip it.",
            );
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if failed > 0 && !rip.ejected {
                if primary_button(ui, colors, "Rip failed tracks again", true).clicked() {
                    *command = Some(Command::RetryFailed);
                }
            } else if let Some(index) = album {
                if primary_button(ui, colors, "Show in Albums", true).clicked() {
                    *command = Some(Command::ShowAlbum(index));
                }
            }
            if action_button(ui, "Open folder", true).clicked() {
                *command = Some(Command::OpenFolder(rip.folder.clone()));
            }
            if !rip.ejected && action_button(ui, "Edit tags and rip again", true).clicked() {
                *command = Some(Command::EditAgain);
            }
            if action_button(ui, "Rip another CD", true).clicked() {
                *command = Some(Command::NextDisc);
            }
        });
    });
    ui.add_space(16.0);
    progress_rows(ui, colors, rip);
}

/// The album being ripped: its cover, title, and artist.
fn rip_header(ui: &mut egui::Ui, colors: Palette, rip: &Rip) {
    let draft = &rip.session.draft;
    ui.horizontal(|ui| {
        if let Cover::Ready { texture, .. } = &rip.session.cover {
            ui.add(egui::Image::new((texture.id(), egui::vec2(72.0, 72.0))).corner_radius(8.0));
            ui.add_space(12.0);
        }
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(draft.album.trim())
                    .size(19.0)
                    .strong()
                    .color(colors.text),
            );
            ui.label(
                egui::RichText::new(draft.artist.trim())
                    .size(14.0)
                    .color(colors.muted),
            );
        });
    });
}

fn progress_rows(ui: &mut egui::Ui, colors: Palette, rip: &Rip) {
    egui::Frame::new()
        .fill(colors.surface)
        .stroke(egui::Stroke::new(1.0, colors.border))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for ((track, draft), row) in rip
                .session
                .tracks
                .iter()
                .zip(&rip.session.draft.tracks)
                .zip(&rip.rows)
            {
                let (icon, tint, status) = match row {
                    TrackProgress::Skipped => (icons::MINUS, colors.subtle, "Skipped".to_owned()),
                    TrackProgress::Queued => (icons::CLOCK, colors.subtle, "Waiting".to_owned()),
                    TrackProgress::Reading { read } => (
                        icons::DISC,
                        colors.accent,
                        format!("{:.0}%", *read as f64 * 100.0 / track.bytes() as f64),
                    ),
                    TrackProgress::Done => (icons::CHECK, colors.accent, "Ripped".to_owned()),
                    TrackProgress::Failed(_) => {
                        (icons::WARNING, colors.danger, "Failed".to_owned())
                    }
                };
                let response = ui
                    .horizontal(|ui| {
                        ui.set_height(28.0);
                        ui.label(egui::RichText::new(icon).font(icon_font(15.0)).color(tint));
                        ui.label(
                            egui::RichText::new(format!("{:>2}", track.number))
                                .size(12.0)
                                .monospace()
                                .color(colors.muted),
                        );
                        let title_color = if *row == TrackProgress::Skipped {
                            colors.subtle
                        } else {
                            colors.text
                        };
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(draft.title.trim())
                                    .size(13.0)
                                    .color(title_color),
                            )
                            .truncate(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(status).size(12.0).color(tint));
                            if let TrackProgress::Reading { read } = row {
                                ui.add(
                                    egui::ProgressBar::new(
                                        (*read as f64 / track.bytes() as f64) as f32,
                                    )
                                    .fill(colors.accent)
                                    .desired_width(120.0)
                                    .desired_height(4.0),
                                );
                            }
                        });
                    })
                    .response;
                if let TrackProgress::Failed(error) = row {
                    response.on_hover_text(error);
                }
            }
        });
}

fn card(ui: &mut egui::Ui, colors: Palette, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(colors.surface)
        .stroke(egui::Stroke::new(1.0, colors.border))
        .corner_radius(12.0)
        // The same inset as the track list's rows, so stacked cards and lists line up inside.
        .inner_margin(egui::Margin::symmetric(16, 20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui);
        });
}

fn busy_card(ui: &mut egui::Ui, colors: Palette, title: &str, text: &str) {
    card(ui, colors, |ui| {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(18.0).color(colors.accent));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(title)
                    .size(19.0)
                    .strong()
                    .color(colors.text),
            );
        });
        ui.add_space(5.0);
        card_text(ui, colors, text);
    });
}

fn card_title(ui: &mut egui::Ui, colors: Palette, icon: &str, title: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(icon)
                .font(icon_font(20.0))
                .color(colors.accent),
        );
        ui.label(
            egui::RichText::new(title)
                .size(19.0)
                .strong()
                .color(colors.text),
        );
    });
    ui.add_space(5.0);
}

fn card_text(ui: &mut egui::Ui, colors: Palette, text: &str) {
    ui.label(egui::RichText::new(text).size(13.0).color(colors.muted));
}

/// The page's main action, filled with the accent color.
fn primary_button(
    ui: &mut egui::Ui,
    colors: Palette,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let (fill, text) = if enabled {
        (colors.accent, colors.on_accent)
    } else {
        (colors.surface, colors.muted)
    };
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(16.0, 8.0);
        ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(label).size(13.0).strong().color(text))
                .fill(fill)
                .corner_radius(7.0)
                .min_size(egui::vec2(0.0, 36.0)),
        )
    })
    .inner
}

fn icon_button(
    ui: &mut egui::Ui,
    colors: Palette,
    icon: &str,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), sense);
    describe_button(&response, label);
    if enabled && response.hovered() {
        ui.painter().rect_filled(rect, 8.0, colors.hover);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        icon_font(18.0),
        if enabled { colors.text } else { colors.subtle },
    );
    response.on_hover_text(label)
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
