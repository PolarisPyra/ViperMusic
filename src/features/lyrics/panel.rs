use super::{Lyrics, LyricsFeature, LyricsResult};
use crate::{
    features::{library::Library, playback::Playback},
    shared::ui::theme,
};
use eframe::egui;

const MIN_LYRICS_HEIGHT: f32 = 96.0;
const DEFAULT_LYRICS_HEIGHT: f32 = 220.0;
const TOGGLE_ROW_HEIGHT: f32 = 30.0;
const RESIZE_HANDLE_HEIGHT: f32 = 28.0;
const MIN_TRACK_LIST_HEIGHT: f32 = 100.0;
const RESIZE_ACTIVE_ID: &str = "album-tracklist-lyrics-resizing";

pub(crate) fn maximum_content_height(available_height: f32) -> f32 {
    (available_height - MIN_TRACK_LIST_HEIGHT - TOGGLE_ROW_HEIGHT - RESIZE_HANDLE_HEIGHT)
        .max(MIN_LYRICS_HEIGHT)
}

pub(crate) fn is_resizing(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(egui::Id::new(RESIZE_ACTIVE_ID)))
        .unwrap_or(false)
}

pub(crate) fn reserved_height(
    ctx: &egui::Context,
    persisted_open: bool,
    available_height: f32,
) -> f32 {
    let open_id = egui::Id::new("album-tracklist-lyrics-open");
    let height_id = egui::Id::new("album-tracklist-lyrics-height");
    let user_resized_id = egui::Id::new("album-tracklist-lyrics-user-resized");
    let open = ctx
        .data(|data| data.get_temp::<bool>(open_id))
        .unwrap_or(persisted_open);
    if !open {
        return 0.0;
    }
    let user_resized = ctx
        .data(|data| data.get_temp::<bool>(user_resized_id))
        .unwrap_or(false);
    let preferred_height = if user_resized {
        ctx.data(|data| data.get_temp::<f32>(height_id))
            .unwrap_or(DEFAULT_LYRICS_HEIGHT)
    } else {
        DEFAULT_LYRICS_HEIGHT
    };
    let max_content_height = maximum_content_height(available_height);
    preferred_height.clamp(MIN_LYRICS_HEIGHT, max_content_height)
        + TOGGLE_ROW_HEIGHT
        + RESIZE_HANDLE_HEIGHT
}

pub(crate) fn show(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    lyrics: &mut LyricsFeature,
    library: &Library,
    playback: &Playback,
    persisted_open: &mut bool,
    max_content_height: f32,
) {
    let current_track = playback.current;
    let colors = theme::colors(ctx);
    let open_id = egui::Id::new("album-tracklist-lyrics-open");
    let height_id = egui::Id::new("album-tracklist-lyrics-height");
    let user_resized_id = egui::Id::new("album-tracklist-lyrics-user-resized");
    let mut open = ctx
        .data(|data| data.get_temp::<bool>(open_id))
        .unwrap_or(*persisted_open);
    let mut height = ctx
        .data(|data| data.get_temp::<f32>(height_id))
        .unwrap_or(DEFAULT_LYRICS_HEIGHT);
    let mut user_resized = ctx
        .data(|data| data.get_temp::<bool>(user_resized_id))
        .unwrap_or(false);
    let progress = ctx.animate_bool_with_time(open_id, open, 0.24);
    height = height.clamp(MIN_LYRICS_HEIGHT, max_content_height);
    let available_height = ui.available_height();
    let content_height = (available_height - TOGGLE_ROW_HEIGHT - RESIZE_HANDLE_HEIGHT).max(0.0);
    let visible_height = content_height * progress;
    let visible_handle_height = RESIZE_HANDLE_HEIGHT * progress;
    let collapsed_spacer_height = (available_height - TOGGLE_ROW_HEIGHT).max(0.0);
    let spacer_height = collapsed_spacer_height * (1.0 - progress);
    ui.add_space(spacer_height);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), TOGGLE_ROW_HEIGHT),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| draw_lyrics_toggle(ui, &mut open),
    );

    if progress > 0.0 {
        let (resize_rect, resize_response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), visible_handle_height),
            egui::Sense::hover(),
        );
        let drag_start_height_id = height_id.with("drag-start");
        let drag_start_pointer_id = height_id.with("drag-start-pointer-y");
        let (pressed, down, released, pointer_pos) = ctx.input(|input| {
            (
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.button_down(egui::PointerButton::Primary),
                input.pointer.button_released(egui::PointerButton::Primary),
                input.pointer.interact_pos(),
            )
        });
        let mut resizing = is_resizing(ctx);
        if pressed && open {
            if let Some(pointer_pos) =
                pointer_pos.filter(|position| resize_rect.contains(*position))
            {
                ctx.data_mut(|data| {
                    data.insert_temp(drag_start_height_id, height);
                    data.insert_temp(drag_start_pointer_id, pointer_pos.y);
                    data.insert_temp(egui::Id::new(RESIZE_ACTIVE_ID), true);
                });
                resizing = true;
            }
        }
        if resizing && (down || released) {
            if let Some(pointer_pos) = pointer_pos {
                let drag_start_height = ctx
                    .data(|data| data.get_temp::<f32>(drag_start_height_id))
                    .unwrap_or(height);
                let drag_start_pointer_y = ctx
                    .data(|data| data.get_temp::<f32>(drag_start_pointer_id))
                    .unwrap_or(pointer_pos.y);
                height = (drag_start_height - (pointer_pos.y - drag_start_pointer_y))
                    .clamp(MIN_LYRICS_HEIGHT, max_content_height);
                user_resized = true;
            }
        }
        if released {
            resizing = false;
            ctx.data_mut(|data| data.insert_temp(egui::Id::new(RESIZE_ACTIVE_ID), false));
        }
        let is_resizing = resize_response.hovered() || resizing;
        if is_resizing {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        let handle_color = if resizing {
            colors.accent
        } else if resize_response.hovered() {
            colors.muted
        } else {
            colors.border
        };
        let separator_center = resize_rect.center() + egui::vec2(0.0, 4.0);
        let handle_center = resize_rect.center() - egui::vec2(0.0, 4.0);
        ui.painter().line_segment(
            [
                egui::pos2(resize_rect.left(), separator_center.y),
                egui::pos2(resize_rect.right(), separator_center.y),
            ],
            egui::Stroke::new(1.0_f32, colors.border),
        );
        if is_resizing {
            ui.painter().line_segment(
                [
                    handle_center - egui::vec2(10.0, 0.0),
                    handle_center + egui::vec2(10.0, 0.0),
                ],
                egui::Stroke::new(if resizing { 2.0_f32 } else { 1.0_f32 }, handle_color),
            );
        }
        let lyrics_result = current_track
            .and_then(|index| library.tracks.get(index))
            .map(|track| lyrics.request(ctx, track));
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), visible_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.label(
                    egui::RichText::new("LYRICS")
                        .size(10.0)
                        .strong()
                        .color(colors.subtle),
                );
                ui.add_space(8.0);
                draw_lyrics_content(ui, lyrics_result, playback.position(), current_track);
            },
        );
    }
    ctx.data_mut(|data| {
        data.insert_temp(open_id, open);
        data.insert_temp(height_id, height);
        data.insert_temp(user_resized_id, user_resized);
    });
    *persisted_open = open;
}

fn draw_lyrics_content(
    ui: &mut egui::Ui,
    result: Option<LyricsResult<'_>>,
    position: std::time::Duration,
    current_track: Option<usize>,
) {
    let colors = theme::colors(ui.ctx());
    match result {
        Some(LyricsResult::Found(lyrics)) => draw_lyrics_lines(ui, lyrics, position, current_track),
        Some(LyricsResult::Loading) => {
            ui.label(
                egui::RichText::new("Fetching lyrics…")
                    .size(12.0)
                    .color(colors.muted),
            );
        }
        Some(LyricsResult::Unavailable) => {
            ui.label(
                egui::RichText::new("Lyrics aren't available for this track.")
                    .size(12.0)
                    .color(colors.muted),
            );
        }
        Some(LyricsResult::Failed(error)) => {
            ui.label(
                egui::RichText::new("Couldn't fetch lyrics.")
                    .size(12.0)
                    .color(colors.muted),
            )
            .on_hover_text(error);
        }
        None => {
            ui.label(
                egui::RichText::new("Play a track to see its lyrics.")
                    .size(12.0)
                    .color(colors.muted),
            );
        }
    }
}

fn draw_lyrics_lines(
    ui: &mut egui::Ui,
    lyrics: &Lyrics,
    position: std::time::Duration,
    current_track: Option<usize>,
) {
    let colors = theme::colors(ui.ctx());
    const ROW_HEIGHT: f32 = 20.0;
    let follow_line_id = egui::Id::new("album-tracklist-lyrics-follow-line");
    let active_line = lyrics.synced.then(|| {
        lyrics
            .lines
            .partition_point(|line| line.timestamp.is_some_and(|time| time <= position))
            .saturating_sub(1)
    });
    let scroll_height = ui.available_height();
    let last_followed = ui
        .ctx()
        .data(|data| data.get_temp::<(Option<usize>, Option<usize>)>(follow_line_id));
    let lyrics_rect = ui.available_rect_before_wrap();
    let pointer_over_lyrics = ui.ctx().input(|input| {
        input
            .pointer
            .hover_pos()
            .is_some_and(|position| lyrics_rect.contains(position))
    });
    let should_follow = active_line.is_some()
        && last_followed != Some((current_track, active_line))
        && !pointer_over_lyrics
        && !is_resizing(ui.ctx());
    ui.ctx()
        .data_mut(|data| data.insert_temp(follow_line_id, (current_track, active_line)));
    egui::ScrollArea::vertical()
        .id_salt("album-tracklist-lyrics-scroll")
        .auto_shrink([false, false])
        .max_height(scroll_height)
        .show_rows(ui, ROW_HEIGHT, lyrics.lines.len(), |ui, visible_rows| {
            if should_follow {
                if let Some(active_line) = active_line {
                    let row_step = ROW_HEIGHT + ui.spacing().item_spacing.y;
                    let content_top = ui.max_rect().top() - visible_rows.start as f32 * row_step;
                    let active_top = content_top + active_line as f32 * row_step;
                    let active_rect = egui::Rect::from_min_size(
                        egui::pos2(ui.max_rect().left(), active_top),
                        egui::vec2(ui.available_width(), ROW_HEIGHT),
                    );
                    let viewport = ui.clip_rect();
                    if active_rect.top() < viewport.top()
                        || active_rect.bottom() > viewport.bottom()
                    {
                        ui.scroll_to_rect_animation(
                            active_rect,
                            None,
                            egui::style::ScrollAnimation::new(
                                420.0,
                                egui::emath::Rangef::new(0.24, 0.48),
                            ),
                        );
                    }
                }
            }
            for index in visible_rows {
                let Some(line) = lyrics.lines.get(index) else {
                    continue;
                };
                let active = active_line == Some(index);
                let text = egui::RichText::new(&line.text)
                    .size(12.0)
                    .color(if active { colors.accent } else { colors.muted })
                    .strong();
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), ROW_HEIGHT),
                    egui::Sense::hover(),
                );
                if active {
                    ui.painter().rect_filled(rect, 3.0, colors.selected);
                }
                ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect.shrink2(egui::vec2(4.0, 0.0)))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                )
                .add(egui::Label::new(text).truncate());
            }
        });
}

fn draw_lyrics_toggle(ui: &mut egui::Ui, open: &mut bool) {
    let colors = theme::colors(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::click());
    draw_action_hover(ui, rect, response.hovered());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if *open {
            egui_phosphor::regular::CARET_DOWN
        } else {
            egui_phosphor::regular::CARET_UP
        },
        theme::icon_font(20.0),
        if response.hovered() {
            colors.text
        } else {
            colors.muted
        },
    );
    let text_response = ui.add(
        egui::Label::new(
            egui::RichText::new("View Lyrics")
                .size(11.0)
                .color(colors.muted),
        )
        .sense(egui::Sense::click()),
    );
    if response.on_hover_text("View Lyrics").clicked() || text_response.clicked() {
        *open = !*open;
    }
}

fn draw_action_hover(ui: &egui::Ui, rect: egui::Rect, hovered: bool) {
    let colors = theme::colors(ui.ctx());
    if hovered {
        ui.painter().rect_filled(rect, 7.0, colors.hover);
    }
}
