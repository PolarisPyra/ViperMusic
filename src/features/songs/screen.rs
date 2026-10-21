use crate::{
    features::{
        library::{state::LibraryFeature, Library},
        playback::Playback,
        track_menu::{self, TrackMenuAction},
    },
    platform::persistence::settings::Settings,
    shared::ui::{
        column_resize::{column_boundaries, show_column_handles},
        controls::search_field,
        theme,
        toasts::Toasts,
    },
    workbench::WorkbenchState,
};
use eframe::egui;

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
/// * `ctx` - egui context for the frame.
/// * `workbench` - Page state, used when navigating to an album.
/// * `library` - Loaded library and the shared search and selection state.
/// * `playback` - Playback queue started by double-clicking a song.
/// * `settings` - Persisted song list column widths, updated by header drags.
/// * `toasts` - Receives errors from context menu actions and saving column widths.
pub(crate) fn show(
    ui: &mut egui::Ui,
    workbench: &mut WorkbenchState,
    library: &mut LibraryFeature,
    playback: &mut Playback,
    settings: &mut Settings,
    toasts: &mut Toasts,
) {
    let ctx = &ui.ctx().clone();
    let colors = theme::colors(ctx);
    let songs = library.filtered_song_indices();
    egui::Panel::top("songs-toolbar")
        .exact_size(48.0)
        .frame(
            egui::Frame::new()
                .fill(colors.canvas)
                .inner_margin(egui::Margin::symmetric(20, 8)),
        )
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} song{}",
                        songs.len(),
                        if songs.len() == 1 { "" } else { "s" }
                    ))
                    .size(13.0)
                    .color(colors.muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    search_field(ui, &mut library.view.search);
                });
            });
        });

    let mut navigation = None;
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(colors.canvas)
                .inner_margin(egui::Margin::symmetric(12, 0)),
        )
        .show(ui, |ui| {
            if songs.is_empty() {
                ui.add_space(70.0);
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(if library.model.tracks.is_empty() {
                            "Your songs will appear here"
                        } else {
                            "No songs match your search"
                        })
                        .size(18.0)
                        .color(colors.subtle),
                    );
                });
                return;
            }
            let header = ui
                .allocate_exact_size(
                    egui::vec2(ui.available_width(), HEADER_HEIGHT),
                    egui::Sense::hover(),
                )
                .0;
            let (start, end) = Columns::flexible_span(header);
            if show_column_handles(
                ui,
                header,
                start,
                end,
                &mut settings.song_list_column_widths,
                "song-list-column-resize",
            ) {
                if let Err(save_error) = settings.save() {
                    toasts.push(format!(
                        "Could not save song list column widths: {save_error}"
                    ));
                }
            }
            let columns = Columns::new(header, settings.song_list_column_widths);
            for (x, label) in [
                (columns.number, "#"),
                (columns.title + COLUMN_TEXT_INSET, "Title"),
                (columns.artist + COLUMN_TEXT_INSET, "Artist"),
                (columns.album + COLUMN_TEXT_INSET, "Album"),
                (columns.duration, "Time"),
            ] {
                ui.painter().text(
                    egui::pos2(x, header.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(12.0),
                    colors.muted,
                );
            }
            ui.painter().line_segment(
                [header.left_bottom(), header.right_bottom()],
                egui::Stroke::new(1.0_f32, colors.border),
            );

            let mut scroll = egui::ScrollArea::vertical()
                .id_salt("songs-scroll")
                .auto_shrink([false, false]);
            if std::mem::take(&mut library.view.scroll_to_selection) {
                if let Some(position) = library
                    .view
                    .selected_song
                    .and_then(|track| songs.iter().position(|&index| index == track))
                {
                    let offset =
                        position as f32 * ROW_HEIGHT - (ui.available_height() - ROW_HEIGHT) * 0.5;
                    scroll = scroll.vertical_scroll_offset(offset.max(0.0));
                }
            }
            let mut rows = SongRows {
                library: &library.model,
                selected_song: &mut library.view.selected_song,
                playback,
                toasts,
                songs: &songs,
                column_widths: settings.song_list_column_widths,
            };
            scroll.show_rows(ui, ROW_HEIGHT, songs.len(), |ui, visible| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for position in visible {
                    if let Some(action) = rows.show(ui, position) {
                        navigation = Some(action);
                    }
                }
            });
        });
    if let Some(TrackMenuAction::GoToAlbum(album)) = navigation {
        workbench.open_album(&mut library.view, album);
    }
}

/// Horizontal text positions for each song column.
struct Columns {
    number: f32,
    title: f32,
    artist: f32,
    album: f32,
    duration: f32,
    right: f32,
}

impl Columns {
    /// Span shared by the flexible Title, Artist, and Album columns.
    ///
    /// # Arguments
    /// * `row` - Header or row rectangle the columns sit in.
    ///
    /// # Returns
    /// The left edge after the track number and the left edge of the duration column.
    fn flexible_span(row: egui::Rect) -> (f32, f32) {
        (
            row.left() + 8.0 + NUMBER_WIDTH,
            row.right() - DURATION_WIDTH,
        )
    }

    /// Lay out the song columns across `row` using the persisted column proportions.
    ///
    /// # Arguments
    /// * `row` - Header or row rectangle the columns sit in.
    /// * `proportions` - Relative widths of the Title, Artist, and Album columns.
    fn new(row: egui::Rect, proportions: [f32; 3]) -> Self {
        let (start, end) = Self::flexible_span(row);
        let [title, artist, album, duration] = column_boundaries(start, end, proportions);
        Self {
            number: row.left() + 8.0,
            title,
            artist,
            album,
            duration,
            right: row.right(),
        }
    }
}

/// Inputs shared by every row of the song list.
struct SongRows<'a> {
    library: &'a Library,
    selected_song: &'a mut Option<usize>,
    playback: &'a mut Playback,
    toasts: &'a mut Toasts,
    songs: &'a [usize],
    column_widths: [f32; 3],
}

impl SongRows<'_> {
    /// Draw the song at `position`; click selects it and double-click plays from it.
    ///
    /// # Returns
    /// Navigation requested from the row's context menu.
    fn show(&mut self, ui: &mut egui::Ui, position: usize) -> Option<TrackMenuAction> {
        let SongRows {
            library,
            selected_song,
            playback,
            toasts,
            songs,
            column_widths,
        } = self;
        let track_index = *songs.get(position)?;
        let colors = theme::colors(ui.ctx());
        let track = library.tracks.get(track_index)?;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), ROW_HEIGHT),
            egui::Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                true,
                **selected_song == Some(track_index),
                format!("{} by {}", track.title, track.artist),
            )
        });
        if response.clicked() || response.secondary_clicked() {
            **selected_song = Some(track_index);
        }
        if response.double_clicked() {
            playback.play_in_context(songs, &library.tracks, track_index);
        }
        let mut action = None;
        response.context_menu(|ui| {
            action = track_menu::show(ui, playback, library, track_index, true, toasts);
        });
        if !ui.is_rect_visible(rect) {
            return action;
        }
        let selected = **selected_song == Some(track_index) || response.context_menu_opened();
        let playing = playback.current == Some(track_index);
        if selected || response.hovered() {
            ui.painter().rect_filled(
                rect,
                3.0,
                if selected {
                    colors.selected
                } else {
                    colors.hover
                },
            );
        }
        let columns = Columns::new(rect, *column_widths);
        let center = rect.center().y;
        if playing {
            ui.painter().text(
                egui::pos2(columns.number, center),
                egui::Align2::LEFT_CENTER,
                if playback.is_playing() {
                    egui_phosphor::regular::SPEAKER_HIGH
                } else {
                    egui_phosphor::regular::PAUSE
                },
                theme::icon_font(15.0),
                colors.accent,
            );
        } else {
            ui.painter().text(
                egui::pos2(columns.number, center),
                egui::Align2::LEFT_CENTER,
                (position + 1).to_string(),
                egui::FontId::proportional(11.0),
                colors.subtle,
            );
        }
        let title_color = if playing { colors.accent } else { colors.text };
        for (left, right, text, color) in [
            (
                columns.title,
                columns.artist,
                track.title.as_str(),
                title_color,
            ),
            (
                columns.artist,
                columns.album,
                track.artist.as_str(),
                colors.muted,
            ),
            (
                columns.album,
                columns.duration,
                track.album.as_str(),
                colors.muted,
            ),
        ] {
            let text_x = left + COLUMN_TEXT_INSET;
            let clip = egui::Rect::from_x_y_ranges(text_x..=right - 12.0, rect.y_range());
            ui.painter().with_clip_rect(clip).text(
                egui::pos2(text_x, center),
                egui::Align2::LEFT_CENTER,
                text,
                egui::FontId::proportional(13.0),
                color,
            );
        }
        if let Some(duration) = track.duration_ms {
            let seconds = duration / 1_000;
            ui.painter().text(
                egui::pos2(columns.right - 8.0, center),
                egui::Align2::RIGHT_CENTER,
                format!("{}:{:02}", seconds / 60, seconds % 60),
                egui::FontId::proportional(12.0),
                colors.subtle,
            );
        }
        action
    }
}
