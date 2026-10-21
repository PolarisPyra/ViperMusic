use crate::{
    features::{library::state::LibraryFeature, playback::Playback},
    shared::ui::{controls::close_button, theme},
    workbench::WorkbenchState,
};
use eframe::egui;

const PANEL_WIDTH: f32 = 340.0;

/// Draw the playback queue as a side panel that stays open while browsing.
///
/// Rows can be dragged to reorder them or removed individually.
///
/// # Arguments
/// * `ctx` - egui context for the frame.
/// * `workbench` - Holds whether the panel is open.
/// * `playback` - Queue to display and edit.
/// * `library` - Tracks referenced by the queue.
pub(in crate::workbench) fn show(
    ui: &mut egui::Ui,
    workbench: &mut WorkbenchState,
    playback: &mut Playback,
    library: &LibraryFeature,
) {
    let ctx = &ui.ctx().clone();
    if !workbench.show_queue {
        return;
    }
    let colors = theme::colors(ctx);
    let mut close_requested = false;
    let mut clear_requested = false;
    let mut remove = None;
    let mut move_item = None;
    egui::Panel::right("queue-panel")
        .resizable(false)
        .exact_size(PANEL_WIDTH)
        .frame(
            egui::Frame::new()
                .fill(colors.panel)
                .stroke(egui::Stroke::new(1.0_f32, colors.border))
                .inner_margin(egui::Margin::symmetric(16, 16)),
        )
        .show(ui, |ui| {
            let queue = playback.queue();
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Up Next")
                            .size(20.0)
                            .strong()
                            .color(colors.text),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{} queued song{}",
                            queue.len(),
                            if queue.len() == 1 { "" } else { "s" }
                        ))
                        .size(12.0)
                        .color(colors.muted),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close_requested = close_button(ui, "Close queue (Q)");
                    if !queue.is_empty()
                        && ui
                            .add(egui::Button::new(
                                egui::RichText::new("Clear").size(12.0).color(colors.text),
                            ))
                            .on_hover_text("Remove every queued song")
                            .clicked()
                    {
                        clear_requested = true;
                    }
                });
            });
            ui.add_space(12.0);
            if let Some(current) = playback
                .current
                .and_then(|index| library.model.tracks.get(index))
            {
                egui::Frame::new()
                    .fill(colors.surface)
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            egui::RichText::new("NOW PLAYING")
                                .size(10.0)
                                .strong()
                                .color(colors.accent),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&current.title)
                                    .size(13.0)
                                    .strong()
                                    .color(colors.text),
                            )
                            .truncate(),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&current.artist)
                                    .size(11.0)
                                    .color(colors.muted),
                            )
                            .truncate(),
                        );
                    });
                ui.add_space(12.0);
            }
            if queue.is_empty() {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("The queue is empty")
                            .size(15.0)
                            .strong()
                            .color(colors.text),
                    );
                    ui.label(
                        egui::RichText::new("Right-click a song or album to add it.")
                            .size(12.0)
                            .color(colors.muted),
                    );
                });
                return;
            }
            ui.label(
                egui::RichText::new("Drag songs to reorder")
                    .size(11.0)
                    .color(colors.subtle),
            );
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .id_salt("queue-panel-list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for (index, &track_index) in queue.iter().enumerate() {
                        let item =
                            ui.dnd_drag_source(egui::Id::new(("queue-item", index)), index, |ui| {
                                queue_row(ui, library, index, track_index)
                            });
                        if item.inner {
                            remove = Some(index);
                        }
                        let response = item.response;
                        if response.dnd_hover_payload::<usize>().is_some() {
                            let y = if ui
                                .ctx()
                                .pointer_hover_pos()
                                .is_some_and(|pointer| pointer.y > response.rect.center().y)
                            {
                                response.rect.bottom() + 3.0
                            } else {
                                response.rect.top() - 3.0
                            };
                            ui.painter().hline(
                                response.rect.x_range(),
                                y,
                                egui::Stroke::new(2.0_f32, colors.accent),
                            );
                        }
                        if let Some(from) = response.dnd_release_payload::<usize>() {
                            let below = ui
                                .ctx()
                                .pointer_interact_pos()
                                .is_some_and(|pointer| pointer.y > response.rect.center().y);
                            let target = if below { index + 1 } else { index };
                            // Removing the dragged row shifts later targets up by one.
                            let to = if *from < target { target - 1 } else { target };
                            move_item = Some((*from, to));
                        }
                    }
                });
        });
    if let Some((from, to)) = move_item {
        playback.move_queued(from, to);
    }
    if let Some(index) = remove {
        playback.remove_queued(index);
    }
    if clear_requested {
        playback.clear_queue();
    }
    if close_requested {
        workbench.show_queue = false;
    }
}

/// Draw one queued song.
///
/// # Returns
/// Whether its remove button was clicked.
fn queue_row(
    ui: &mut egui::Ui,
    library: &LibraryFeature,
    index: usize,
    track_index: usize,
) -> bool {
    let colors = theme::colors(ui.ctx());
    let mut removed = false;
    egui::Frame::new()
        .fill(colors.surface)
        .stroke(egui::Stroke::new(1.0_f32, colors.border))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::DOTS_SIX_VERTICAL)
                        .font(theme::icon_font(16.0))
                        .color(colors.subtle),
                );
                ui.label(
                    egui::RichText::new(format!("{:02}", index + 1))
                        .size(11.0)
                        .color(colors.subtle),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let remove = ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(egui_phosphor::regular::X)
                                    .font(theme::icon_font(16.0))
                                    .color(colors.muted),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Remove from queue");
                    remove.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            format!("Remove queued song {}", index + 1),
                        )
                    });
                    removed = remove.clicked();
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| match library
                        .model
                        .tracks
                        .get(track_index)
                    {
                        Some(track) => {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&track.title)
                                        .size(12.0)
                                        .strong()
                                        .color(colors.text),
                                )
                                .truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&track.artist)
                                        .size(10.0)
                                        .color(colors.muted),
                                )
                                .truncate(),
                            );
                        }
                        None => {
                            ui.label("Unavailable track");
                        }
                    });
                });
            });
        });
    removed
}
