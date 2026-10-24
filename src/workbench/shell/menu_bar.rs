use crate::{
    features::{library::state::LibraryFeature, playback::Playback},
    shared::{
        action::AppAction,
        ui::{
            controls::{close_button, menu_entry, modal_frame},
            theme::{self, Palette},
        },
    },
    workbench::{Page, WorkbenchState},
};
use eframe::egui;

pub(in crate::workbench) fn show(
    ui: &mut egui::Ui,
    workbench: &mut WorkbenchState,
    library: &LibraryFeature,
    playback: &mut Playback,
) -> Option<AppAction> {
    let ctx = &ui.ctx().clone();
    let colors = theme::colors(ctx);
    let mut action = None;
    // Menu popups read the context style, so apply menu visuals for this panel only.
    let original_style = ctx.global_style();
    let mut menu_style = (*original_style).clone();
    menu_style.spacing.menu_spacing = 6.0;
    style_menu_visuals(&mut menu_style.visuals, colors);
    ctx.set_global_style(menu_style);

    egui::Panel::top("app-menu-bar")
        .exact_size(34.0)
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
        .show(ui, |ui| {
            let menu_size = ui.available_size();
            ui.allocate_ui_with_layout(
                menu_size,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(
                        egui::RichText::new(egui_phosphor::regular::DISC)
                            .font(theme::icon_font(18.0))
                            .color(colors.muted),
                    );
                    ui.add_space(4.0);
                    ui.scope(|ui| {
                        // Keep the menu triggers visually quiet; popup styling is
                        // applied inside each menu closure to match album sort.
                        theme::style_dropdown(ui);

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
                                    ui.close();
                                }
                                ui.separator();
                                if menu_item(ui, "Quit").clicked() {
                                    workbench.quit_requested = true;
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    ui.close();
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
                                        ui.close();
                                        action = Some(AppAction::ChooseMusicFolder);
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
                                    ui.close();
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
                                if menu_entry(ui, can_play, play_label) {
                                    playback.play_pause(library);
                                }
                                ui.separator();
                                if menu_entry(
                                    ui,
                                    has_current_track || playback.has_history(),
                                    "Previous Track",
                                ) {
                                    playback.previous(&library.model.tracks);
                                }
                                if menu_entry(ui, playback.has_next(), "Next Track") {
                                    playback.skip_next(&library.model.tracks);
                                }
                                if menu_entry(ui, has_current_track, "Stop") {
                                    playback.stop();
                                }
                            },
                        );
                        ui.menu_button(
                            egui::RichText::new("View").size(12.0).color(colors.muted),
                            |ui| {
                                style_menu_popup(ui);
                                for page in Page::ALL {
                                    if ui
                                        .selectable_label(workbench.page == page, page.label())
                                        .clicked()
                                    {
                                        workbench.page = page;
                                        ui.close();
                                    }
                                }
                            },
                        );
                        ui.menu_button(
                            egui::RichText::new("Help").size(12.0).color(colors.muted),
                            |ui| {
                                style_menu_popup(ui);
                                if menu_item(ui, "Keyboard Shortcuts").clicked() {
                                    workbench.show_shortcuts = true;
                                    ui.close();
                                }
                            },
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        version_chip(ui, colors);
                    });
                },
            );
        });

    ctx.set_global_style(original_style);
    show_shortcuts_popup(ctx, workbench);
    action
}

fn show_shortcuts_popup(ctx: &egui::Context, workbench: &mut WorkbenchState) {
    if !workbench.show_shortcuts {
        return;
    }

    let colors = theme::colors(ctx);
    let mut close_requested = false;
    let response = egui::Modal::new(egui::Id::new("keyboard-shortcuts-modal"))
        .frame(modal_frame(colors))
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
                    close_requested = close_button(ui, "Close keyboard shortcuts");
                });
            });
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            let shortcuts = [
                ("F", "Focus search"),
                ("Arrows", "Move the selection"),
                ("Enter", "Open album or play the selected song"),
                ("Space", "Play or pause"),
                ("Ctrl ← / →", "Seek 5 seconds"),
                ("Ctrl ↑ / ↓", "Change volume"),
                ("Ctrl + / − / 0", "Zoom in, out, or reset"),
                ("F5", "Play a random song"),
                ("Q", "Show or hide queue"),
                ("Esc", "Close panels and album details"),
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

pub(crate) fn style_menu_popup(ui: &mut egui::Ui) {
    theme::style_menu_contents(ui, 220.0);
}

fn style_menu_visuals(visuals: &mut egui::Visuals, colors: Palette) {
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

/// The running build's version as a small pill at the right end of the menu bar.
fn version_chip(ui: &mut egui::Ui, colors: Palette) {
    let galley = ui.painter().layout_no_wrap(
        concat!("v", env!("CARGO_PKG_VERSION")).to_owned(),
        egui::FontId::proportional(11.0),
        colors.muted,
    );
    let chip_size = galley.size() + egui::vec2(16.0, 4.0);
    let (rect, _) = ui.allocate_exact_size(chip_size, egui::Sense::hover());
    let radius = rect.height() / 2.0;
    ui.painter()
        .rect_filled(rect, radius, colors.text.gamma_multiply(0.06));
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, colors.border),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() * 0.5, galley, colors.muted);
}

fn menu_item(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let colors = theme::colors(ui.ctx());
    ui.selectable_label(false, egui::RichText::new(label).color(colors.text))
}
