use crate::{
    features::{library::state::LibraryFeature, playback::Playback},
    workbench::{Page, WorkbenchState},
};
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuBarAction {
    ChooseMusicFolder,
}

pub(in crate::workbench) fn show(
    ctx: &egui::Context,
    workbench: &mut WorkbenchState,
    library: &LibraryFeature,
    playback: &mut Playback,
) -> Option<MenuBarAction> {
    let colors = crate::shared::ui::theme::colors(ctx);
    let mut action = None;
    let original_style = ctx.style().as_ref().clone();
    let mut menu_style = original_style.clone();
    menu_style.spacing.menu_spacing = 6.0;
    style_menu_visuals(&mut menu_style.visuals, colors);
    ctx.set_style(menu_style);

    egui::TopBottomPanel::top("app-menu-bar")
        .exact_height(34.0)
        .frame(
            egui::Frame::new()
                .fill(colors.canvas)
                .inner_margin(egui::Margin {
                    left: 8,
                    right: 18,
                    top: 2,
                    bottom: 2,
                }),
        )
        .show(ctx, |ui| {
            let menu_size = ui.available_size();
            ui.allocate_ui_with_layout(
                menu_size,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(
                        egui::RichText::new(egui_phosphor::regular::DISC)
                            .font(egui::FontId::new(
                                18.0,
                                egui::FontFamily::Name("phosphor".into()),
                            ))
                            .color(colors.muted),
                    );
                    ui.add_space(4.0);
                    ui.scope(|ui| {
                        // Keep the menu triggers visually quiet; popup styling is
                        // applied inside each menu closure to match album sort.
                        crate::shared::ui::theme::style_dropdown(ui);

                        let visuals = &mut ui.visuals_mut().widgets;
                        visuals.inactive.bg_fill = egui::Color32::TRANSPARENT;
                        visuals.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                        visuals.inactive.bg_stroke = egui::Stroke::NONE;
                        visuals.hovered.bg_fill = colors.hover;
                        visuals.hovered.weak_bg_fill = colors.hover;
                        visuals.hovered.bg_stroke = egui::Stroke::NONE;
                        visuals.hovered.corner_radius = egui::CornerRadius::same(5);
                        visuals.active.bg_fill = colors.hover;
                        visuals.active.weak_bg_fill = colors.hover;
                        visuals.active.bg_stroke = egui::Stroke::NONE;
                        visuals.open.bg_fill = colors.hover;
                        visuals.open.weak_bg_fill = colors.hover;
                        visuals.open.bg_stroke = egui::Stroke::NONE;
                        visuals.open.corner_radius = egui::CornerRadius::same(5);
                        visuals.inactive.corner_radius = egui::CornerRadius::same(5);
                        visuals.active.corner_radius = egui::CornerRadius::same(5);
                        ui.spacing_mut().button_padding = egui::vec2(8.0, 2.0);
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.menu_button(
                            egui::RichText::new("File").size(12.0).color(colors.muted),
                            |ui| {
                                style_menu_popup(ui);
                                if menu_item(ui, "Preferences").clicked() {
                                    workbench.show_preferences = true;
                                    ui.close_menu();
                                }
                                ui.separator();
                                if menu_item(ui, "Quit").clicked() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    ui.close_menu();
                                }
                            },
                        );
                        ui.menu_button(
                            egui::RichText::new("Library")
                                .size(12.0)
                                .color(colors.muted),
                            |ui| {
                                style_menu_popup(ui);
                                ui.menu_button("Add Library", |ui| {
                                    style_menu_popup(ui);
                                    if menu_item(ui, "Choose Music Folder").clicked() {
                                        ui.close_menu();
                                        action = Some(MenuBarAction::ChooseMusicFolder);
                                    }
                                });
                            },
                        );
                        ui.menu_button(
                            egui::RichText::new("Playback")
                                .size(12.0)
                                .color(colors.muted),
                            |ui| {
                                style_menu_popup(ui);
                                if menu_item(ui, "Show Queue").clicked() {
                                    workbench.show_queue = true;
                                    ui.close_menu();
                                }
                                let has_current_track = playback.current.is_some();
                                let can_play = has_current_track
                                    || library.view.selected_album.is_some()
                                    || playback.has_next()
                                    || (playback.shuffle_enabled
                                        && !library.model.tracks.is_empty());
                                let play_label = if has_current_track && playback.is_paused() {
                                    "Resume"
                                } else if has_current_track {
                                    "Pause"
                                } else {
                                    "Play Selected Album"
                                };
                                if ui
                                    .add_enabled(
                                        can_play,
                                        egui::SelectableLabel::new(
                                            false,
                                            egui::RichText::new(play_label).color(colors.text),
                                        ),
                                    )
                                    .clicked()
                                {
                                    if has_current_track {
                                        playback.toggle_pause();
                                    } else if let Some(album_index) = library.view.selected_album {
                                        if let Some(album) = library.model.albums.get(album_index) {
                                            if let Some(track_index) = library
                                                .view
                                                .selected_track
                                                .filter(|index| album.tracks.contains(index))
                                            {
                                                playback.play_track_in_album(
                                                    album,
                                                    &library.model.tracks,
                                                    track_index,
                                                );
                                            } else {
                                                playback.play_album(album, &library.model.tracks);
                                            }
                                        }
                                    } else if playback.has_next() {
                                        playback.skip_next(&library.model.tracks);
                                    } else if playback.shuffle_enabled {
                                        playback.play_random_track(&library.model.tracks);
                                    }
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui
                                    .add_enabled(
                                        has_current_track || playback.has_history(),
                                        egui::SelectableLabel::new(
                                            false,
                                            egui::RichText::new("Previous Track")
                                                .color(colors.text),
                                        ),
                                    )
                                    .clicked()
                                {
                                    playback.previous(&library.model.tracks);
                                    ui.close_menu();
                                }
                                if ui
                                    .add_enabled(
                                        playback.has_next(),
                                        egui::SelectableLabel::new(
                                            false,
                                            egui::RichText::new("Next Track").color(colors.text),
                                        ),
                                    )
                                    .clicked()
                                {
                                    playback.skip_next(&library.model.tracks);
                                    ui.close_menu();
                                }
                                if ui
                                    .add_enabled(
                                        has_current_track,
                                        egui::SelectableLabel::new(
                                            false,
                                            egui::RichText::new("Stop").color(colors.text),
                                        ),
                                    )
                                    .clicked()
                                {
                                    playback.stop();
                                    ui.close_menu();
                                }
                            },
                        );
                        ui.menu_button(
                            egui::RichText::new("View").size(12.0).color(colors.muted),
                            |ui| {
                                style_menu_popup(ui);
                                if ui
                                    .selectable_label(workbench.page == Page::Home, "Home")
                                    .clicked()
                                {
                                    workbench.page = Page::Home;
                                    ui.close_menu();
                                }
                                if ui
                                    .selectable_label(workbench.page == Page::Albums, "Albums")
                                    .clicked()
                                {
                                    workbench.page = Page::Albums;
                                    ui.close_menu();
                                }
                            },
                        );
                        ui.menu_button(
                            egui::RichText::new("Help").size(12.0).color(colors.muted),
                            |ui| {
                                style_menu_popup(ui);
                                if menu_item(ui, "Keyboard Shortcuts").clicked() {
                                    workbench.show_shortcuts = true;
                                    ui.close_menu();
                                }
                            },
                        );
                    });
                },
            );
        });

    ctx.set_style(original_style);
    show_queue_popup(ctx, workbench, playback, library);
    show_shortcuts_popup(ctx, workbench);
    action
}

fn show_shortcuts_popup(ctx: &egui::Context, workbench: &mut WorkbenchState) {
    if !workbench.show_shortcuts {
        return;
    }

    let colors = crate::shared::ui::theme::colors(ctx);
    let mut close_requested = false;
    let response = egui::Modal::new(egui::Id::new("keyboard-shortcuts-modal"))
        .frame(
            egui::Frame::new()
                .fill(colors.panel)
                .stroke(egui::Stroke::new(1.0_f32, colors.border))
                .corner_radius(14.0)
                .inner_margin(egui::Margin::symmetric(18, 18)),
        )
        .backdrop_color(colors.backdrop)
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.set_max_width(420.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Keyboard Shortcuts")
                        .size(22.0)
                        .strong()
                        .color(colors.text),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 8.0, colors.hover);
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        egui_phosphor::regular::X,
                        egui::FontId::new(16.0, egui::FontFamily::Name("phosphor".into())),
                        if response.hovered() {
                            colors.text
                        } else {
                            colors.muted
                        },
                    );
                    if response.on_hover_text("Close keyboard shortcuts").clicked() {
                        close_requested = true;
                    }
                });
            });
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            let shortcuts = [
                ("F", "Focus search"),
                ("F5", "Play a random song"),
                ("Q", "Show or hide queue"),
                ("Space", "Play or pause"),
                ("Esc", "Close album details"),
            ];
            for (shortcut, description) in shortcuts {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(description).color(colors.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::Frame::new()
                            .fill(colors.canvas)
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(7, 3))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(shortcut).size(11.0).color(colors.muted),
                                );
                            });
                    });
                });
                ui.add_space(5.0);
            }
        });

    if close_requested || response.should_close() {
        workbench.show_shortcuts = false;
    }
}

fn show_queue_popup(
    ctx: &egui::Context,
    workbench: &mut WorkbenchState,
    playback: &mut Playback,
    library: &LibraryFeature,
) {
    if !workbench.show_queue {
        return;
    }

    let colors = crate::shared::ui::theme::colors(ctx);
    let screen_size = ctx.screen_rect().size();
    let margin = 24.0_f32.min(screen_size.x * 0.05).min(screen_size.y * 0.05);
    let panel_size = egui::vec2(
        (screen_size.x - margin * 2.0).min(560.0).max(0.0),
        (screen_size.y - margin * 2.0).min(620.0).max(0.0),
    );
    let content_size = (panel_size - egui::vec2(36.0, 36.0)).max(egui::Vec2::ZERO);
    let mut close_requested = false;
    let mut clear_requested = false;

    let response = egui::Modal::new(egui::Id::new("playback-queue-modal"))
        .frame(
            egui::Frame::new()
                .fill(colors.panel)
                .stroke(egui::Stroke::new(1.0_f32, colors.border))
                .corner_radius(14.0)
                .inner_margin(egui::Margin::symmetric(18, 18)),
        )
        .backdrop_color(colors.backdrop)
        .show(ctx, |ui| {
            ui.set_min_width(content_size.x);
            ui.set_max_width(content_size.x);
            if playback.queue().len() > 8 {
                ui.set_min_height(content_size.y);
            }
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Up Next")
                            .size(23.0)
                            .strong()
                            .color(colors.text),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{} queued song{}",
                            playback.queue().len(),
                            if playback.queue().len() == 1 { "" } else { "s" }
                        ))
                        .size(12.0)
                        .color(colors.muted),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 8.0, colors.hover);
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        egui_phosphor::regular::X,
                        egui::FontId::new(16.0, egui::FontFamily::Name("phosphor".into())),
                        if response.hovered() {
                            colors.text
                        } else {
                            colors.muted
                        },
                    );
                    if response.on_hover_text("Close queue").clicked() {
                        close_requested = true;
                    }
                });
            });
            ui.add_space(16.0);
            ui.separator();
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if let Some(current) = playback
                    .current
                    .and_then(|index| library.model.tracks.get(index))
                {
                    egui::Frame::new()
                        .fill(colors.surface)
                        .stroke(egui::Stroke::new(1.0_f32, colors.border))
                        .corner_radius(9.0)
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new("NOW PLAYING")
                                        .size(10.0)
                                        .strong()
                                        .color(colors.accent),
                                );
                                ui.add_space(3.0);
                                ui.label(
                                    egui::RichText::new(&current.title)
                                        .size(13.0)
                                        .strong()
                                        .color(colors.text),
                                );
                                ui.label(
                                    egui::RichText::new(&current.artist)
                                        .size(11.0)
                                        .color(colors.muted),
                                );
                            });
                        });
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let clear_button =
                        egui::Button::new(egui::RichText::new("Clear all").size(12.0).color(
                            if playback.queue().is_empty() {
                                colors.subtle
                            } else {
                                colors.text
                            },
                        ))
                        .min_size(egui::vec2(112.0, 38.0))
                        .corner_radius(9.0)
                        .fill(colors.surface)
                        .stroke(egui::Stroke::new(1.0_f32, colors.border));
                    if ui
                        .add_enabled(!playback.queue().is_empty(), clear_button)
                        .clicked()
                    {
                        clear_requested = true;
                    }
                });
            });
            ui.add_space(12.0);

            let mut remove = None;
            let mut move_item = None;
            if playback.queue().is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("The queue is empty")
                            .size(15.0)
                            .strong()
                            .color(colors.text),
                    );
                    ui.label(
                        egui::RichText::new("Right-click a song to add it to Up Next.")
                            .size(12.0)
                            .color(colors.muted),
                    );
                });
            } else {
                let queue_len = playback.queue().len();
                let mut show_items = |ui: &mut egui::Ui| {
                    ui.spacing_mut().item_spacing.y = 7.0;
                    for (index, track_index) in playback.queue().iter().copied().enumerate() {
                        egui::Frame::new()
                            .fill(colors.surface)
                            .stroke(egui::Stroke::new(1.0_f32, colors.border))
                            .corner_radius(8.0)
                            .inner_margin(egui::Margin::symmetric(10, 7))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{:02}", index + 1))
                                            .size(11.0)
                                            .color(colors.subtle),
                                    );
                                    ui.vertical(|ui| {
                                        if let Some(track) = library.model.tracks.get(track_index) {
                                            ui.label(
                                                egui::RichText::new(&track.title)
                                                    .size(12.0)
                                                    .strong()
                                                    .color(colors.text),
                                            );
                                            ui.label(
                                                egui::RichText::new(&track.artist)
                                                    .size(10.0)
                                                    .color(colors.muted),
                                            );
                                        } else {
                                            ui.label("Unavailable track");
                                        }
                                    });
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if queue_action_button(
                                                ui,
                                                "remove-queue-item",
                                                index,
                                                egui_phosphor::regular::X,
                                                "Remove from queue",
                                                true,
                                            )
                                            .clicked()
                                            {
                                                remove = Some(index);
                                            }
                                            if queue_action_button(
                                                ui,
                                                "move-queue-item-down",
                                                index,
                                                egui_phosphor::regular::ARROW_DOWN,
                                                "Move down",
                                                index + 1 < queue_len,
                                            )
                                            .clicked()
                                            {
                                                move_item = Some((index, index + 1));
                                            }
                                            if queue_action_button(
                                                ui,
                                                "move-queue-item-up",
                                                index,
                                                egui_phosphor::regular::ARROW_UP,
                                                "Move up",
                                                index > 0,
                                            )
                                            .clicked()
                                            {
                                                move_item = Some((index, index - 1));
                                            }
                                        },
                                    );
                                });
                            });
                    }
                };
                if queue_len <= 8 {
                    show_items(ui);
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("playback-queue-list")
                        .max_height(420.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| show_items(ui));
                }
                drop(show_items);
            }

            if let Some((from, to)) = move_item {
                playback.move_queued(from, to, &library.model.tracks);
            }
            if let Some(index) = remove {
                playback.remove_queued(index, &library.model.tracks);
            }
        });

    if close_requested || response.should_close() {
        workbench.show_queue = false;
    }
    if clear_requested {
        playback.clear_queue();
    }
}

fn queue_action_button(
    ui: &mut egui::Ui,
    salt: &'static str,
    index: usize,
    icon: &str,
    tooltip: &'static str,
    enabled: bool,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
    let response = ui.interact(
        rect,
        ui.id().with((salt, index)),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let colors = crate::shared::ui::theme::colors(ui.ctx());
    if response.hovered() && enabled {
        ui.painter().rect_filled(rect, 7.0, colors.hover);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::new(20.0, egui::FontFamily::Name("phosphor".into())),
        if response.hovered() && enabled {
            colors.text
        } else {
            colors.muted
        },
    );
    response.on_hover_text(tooltip)
}

pub(crate) fn style_menu_popup(ui: &mut egui::Ui) {
    crate::shared::ui::theme::style_menu_contents(ui, 220.0);
}

fn style_menu_visuals(visuals: &mut egui::Visuals, colors: crate::shared::ui::theme::Palette) {
    let fill = colors.panel;
    let selected = colors.selected;
    let border = egui::Stroke::new(1.0_f32, colors.border);

    visuals.window_fill = fill;
    visuals.window_stroke = border;
    visuals.menu_corner_radius = egui::CornerRadius::same(8);
    visuals.selection.bg_fill = selected;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, colors.text);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = border;
    }
    visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.bg_fill = colors.hover;
    visuals.widgets.hovered.weak_bg_fill = colors.hover;
    visuals.widgets.open.bg_fill = colors.hover;
    visuals.widgets.open.weak_bg_fill = colors.hover;
    visuals.widgets.open.bg_stroke = egui::Stroke::NONE;
}

fn menu_item(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
    ui.selectable_label(false, egui::RichText::new(label).color(colors.text))
}
