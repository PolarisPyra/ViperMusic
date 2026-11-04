//! The tag editor dialog: an album's tags and its tracks', and a MusicBrainz search to fill them
//! in.
//!
//! Drawing only edits the draft tags; everything else the user asks for is collected as a
//! [`Command`] and applied after the dialog is drawn.
use super::state::{Job, TagEditor};
use crate::{
    features::library::state::LibraryFeature,
    platform::persistence::settings::Settings,
    shared::ui::{
        controls::{
            action_button, close_button, connection_text_edit, field_label, modal_frame, notice,
            primary_button,
        },
        theme::{self, Palette},
        toasts::Toasts,
    },
    workbench::WorkbenchState,
};
use eframe::egui;
use egui_phosphor::regular as icons;

/// Height kept below the track list for the dialog's buttons.
const FOOTER_HEIGHT: f32 = 48.0;
/// Width of the disc and track number fields.
const NUMBER_WIDTH: f32 = 44.0;
/// Space between a track's fields.
const FIELD_GAP: f32 = 8.0;

/// What the user asked for while the dialog was drawn.
enum Command {
    Search,
    Choose(usize),
    Save,
    Close,
}

/// Draw the tag editor while it is open, and apply its finished searches and saves.
///
/// # Arguments
/// * `ctx` - egui context for the frame.
/// * `workbench` - Holds the open editor; cleared when it closes.
/// * `library` - Shows the saved tags.
/// * `settings` - Keeps a renamed album's rating and history.
/// * `toasts` - Receives save errors.
pub(crate) fn show(
    ctx: &egui::Context,
    workbench: &mut WorkbenchState,
    library: &mut LibraryFeature,
    settings: &mut Settings,
    toasts: &mut Toasts,
) {
    let Some(editor) = workbench.tag_editor.as_mut() else {
        return;
    };
    if editor.poll(library, settings, toasts) {
        workbench.tag_editor = None;
        return;
    }
    let colors = theme::colors(ctx);
    let screen_size = ctx.content_rect().size();
    let margin = 24.0_f32.min(screen_size.x * 0.05).min(screen_size.y * 0.05);
    let modal_size = egui::vec2(
        (screen_size.x - margin * 2.0).clamp(0.0, 760.0),
        (screen_size.y - margin * 2.0).max(0.0),
    );
    let content_size = (modal_size - egui::vec2(36.0, 36.0)).max(egui::Vec2::ZERO);
    let mut command = None;

    let response = egui::Modal::new(egui::Id::new("tag-editor-modal"))
        .frame(modal_frame(colors))
        .backdrop_color(colors.backdrop)
        .show(ctx, |ui| {
            // As wide as preferences, but only as tall as the album needs.
            ui.set_width(content_size.x);
            ui.set_max_height(content_size.y);
            header(ui, colors, editor.tracks.len(), &mut command);
            ui.add_space(14.0);
            lookup(ui, colors, editor, &mut command);
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);
            let busy = editor.busy();
            ui.add_enabled_ui(!busy, |ui| {
                album_fields(ui, colors, editor);
                ui.add_space(14.0);
                egui::ScrollArea::vertical()
                    .id_salt("tag-editor-tracks")
                    .auto_shrink([false, true])
                    .max_height((ui.available_height() - FOOTER_HEIGHT).max(0.0))
                    .show(ui, |ui| track_fields(ui, colors, editor));
            });
            ui.add_space(12.0);
            footer(ui, colors, editor, &mut command);
        });
    if response.should_close() && command.is_none() {
        command = Some(Command::Close);
    }

    match command {
        Some(Command::Search) => editor.search(ctx),
        Some(Command::Choose(index)) => editor.choose(ctx, index),
        Some(Command::Save) => {
            if editor.save(ctx) {
                workbench.tag_editor = None;
            }
        }
        // Closing mid-save would leave the library showing the old tags.
        Some(Command::Close) if !matches!(editor.job, Job::Saving(_)) => {
            workbench.tag_editor = None;
        }
        Some(Command::Close) | None => {}
    }
}

fn header(ui: &mut egui::Ui, colors: Palette, tracks: usize, command: &mut Option<Command>) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Edit metadata")
                    .size(23.0)
                    .strong()
                    .color(colors.text),
            );
            ui.label(
                egui::RichText::new(format!(
                    "Change the tags saved in this album's {tracks} files, by hand or from MusicBrainz"
                ))
                .size(12.0)
                .color(colors.muted),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if close_button(ui, "Close without saving") {
                *command = Some(Command::Close);
            }
        });
    });
}

fn lookup(ui: &mut egui::Ui, colors: Palette, editor: &TagEditor, command: &mut Option<Command>) {
    let busy = editor.busy();
    ui.horizontal(|ui| {
        let searchable = !busy && !editor.album.title.trim().is_empty();
        if action_button(ui, "Look up on MusicBrainz", searchable)
            .on_hover_text("Search MusicBrainz for releases with this album title and artist")
            .clicked()
        {
            *command = Some(Command::Search);
        }
        let waiting = match editor.job {
            Job::Searching(_) => Some("Searching MusicBrainz…"),
            Job::Fetching(_) => Some("Getting the release's tracks…"),
            Job::Idle | Job::Saving(_) => None,
        };
        if let Some(waiting) = waiting {
            ui.add_space(4.0);
            ui.add(egui::Spinner::new().size(14.0).color(colors.muted));
            ui.label(egui::RichText::new(waiting).size(13.0).color(colors.muted));
        }
    });
    if !editor.releases.is_empty() {
        ui.add_space(8.0);
        ui.add_enabled_ui(!busy, |ui| {
            egui::ComboBox::from_id_salt("tag-editor-release")
                .selected_text(
                    editor
                        .chosen
                        .and_then(|index| editor.releases.get(index))
                        .map_or("Choose a release to fill in the tags", |release| {
                            release.label.as_str()
                        }),
                )
                .width(ui.available_width().min(560.0))
                .show_ui(ui, |ui| {
                    for (index, release) in editor.releases.iter().enumerate() {
                        if ui
                            .selectable_label(editor.chosen == Some(index), &release.label)
                            .clicked()
                        {
                            *command = Some(Command::Choose(index));
                        }
                    }
                })
                .response
                .on_hover_text("Releases differ in year, country, and sometimes track titles");
        });
    }
    if let Some(message) = &editor.message {
        ui.add_space(8.0);
        notice(ui, colors, icons::WARNING, colors.warning, message);
    }
}

fn album_fields(ui: &mut egui::Ui, colors: Palette, editor: &mut TagEditor) {
    let width = ui.available_width().min(560.0) - 100.0;
    let album = &mut editor.album;
    egui::Grid::new("tag-editor-album")
        .num_columns(2)
        .spacing(egui::vec2(12.0, 8.0))
        .show(ui, |ui| {
            for (label, value, hint, field_width) in [
                ("Album", &mut album.title, "Album title", width),
                ("Album artist", &mut album.artist, "Artist", width),
                ("Year", &mut album.year, "Year", 72.0),
            ] {
                field_label(ui, colors, label);
                ui.scope(|ui| {
                    ui.set_width(field_width);
                    connection_text_edit(ui, value, hint, false);
                });
                ui.end_row();
            }
        });
}

fn track_fields(ui: &mut egui::Ui, colors: Palette, editor: &mut TagEditor) {
    let text_width = (ui.available_width() - NUMBER_WIDTH * 2.0 - FIELD_GAP * 3.0).max(0.0);
    let title_width = text_width * 0.55;
    let widths = [
        NUMBER_WIDTH,
        NUMBER_WIDTH,
        title_width,
        text_width - title_width,
    ];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = FIELD_GAP;
        for (width, label) in widths.into_iter().zip(["Disc", "Track", "Title", "Artist"]) {
            ui.allocate_ui_with_layout(
                egui::vec2(width, 18.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_width(width);
                    field_label(ui, colors, label);
                },
            );
        }
    });
    ui.add_space(4.0);
    for track in &mut editor.tracks {
        let fields = &mut track.fields;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = FIELD_GAP;
            for (width, (value, hint)) in widths.into_iter().zip([
                (&mut fields.disc, "Disc"),
                (&mut fields.number, "#"),
                (&mut fields.title, "Title"),
                (&mut fields.artist, "Artist"),
            ]) {
                ui.scope(|ui| {
                    ui.set_width(width);
                    connection_text_edit(ui, value, hint, false);
                });
            }
        });
        ui.add_space(2.0);
    }
}

fn footer(ui: &mut egui::Ui, colors: Palette, editor: &TagEditor, command: &mut Option<Command>) {
    let saving = matches!(editor.job, Job::Saving(_));
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), FOOTER_HEIGHT - 12.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            if primary_button(ui, colors, "Save", !editor.busy()).clicked() {
                *command = Some(Command::Save);
            }
            if action_button(ui, "Cancel", !saving).clicked() {
                *command = Some(Command::Close);
            }
            if saving {
                ui.label(
                    egui::RichText::new("Saving the tags…")
                        .size(13.0)
                        .color(colors.muted),
                );
                ui.add(egui::Spinner::new().size(14.0).color(colors.muted));
            }
        },
    );
}
