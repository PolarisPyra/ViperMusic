use crate::{
    features::library::state::LibraryViewState,
    platform::persistence::settings::{AlbumLayout, AlbumSort, Settings},
    shared::ui::theme,
};
use eframe::egui;

pub(crate) fn show(
    ctx: &egui::Context,
    settings: &mut Settings,
    library_view: &mut LibraryViewState,
    error: &mut Option<String>,
) {
    let colors = theme::colors(ctx);
    egui::TopBottomPanel::top("view-toolbar")
        .exact_height(48.0)
        .frame(
            egui::Frame::new()
                .fill(colors.canvas)
                .inner_margin(egui::Margin::symmetric(20, 8)),
        )
        .show(ctx, |ui| {
            crate::shared::ui::theme::style_dropdown(ui);
            ui.spacing_mut().menu_margin = egui::Margin::same(8);
            ui.spacing_mut().interact_size.y = 32.0;
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let active = settings.album_sort;
                let mut changed = None;
                let popup_id = ui.make_persistent_id("album-sort-popup");
                let (sort_rect, sort_response) =
                    ui.allocate_exact_size(egui::vec2(152.0, 32.0), egui::Sense::click());
                paint_sort_dropdown(ui, sort_rect, &sort_response, active.label(), popup_id);
                if sort_response.clicked() {
                    ui.memory_mut(|memory| memory.toggle_popup(popup_id));
                }
                let popup_anchor = egui::Response {
                    rect: sort_response.rect.translate(egui::vec2(0.0, 6.0)),
                    ..sort_response.clone()
                };
                egui::popup_below_widget(
                    ui,
                    popup_id,
                    &popup_anchor,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |popup_ui| {
                        crate::shared::ui::theme::style_menu_contents(popup_ui, sort_rect.width());
                        for sort in AlbumSort::ALL {
                            if popup_ui
                                .selectable_label(
                                    active == sort,
                                    egui::RichText::new(sort.label()).color(colors.text),
                                )
                                .clicked()
                            {
                                changed = Some(sort);
                                popup_ui.memory_mut(|memory| memory.close_popup());
                            }
                        }
                    },
                );
                if let Some(sort) = changed {
                    if settings.album_sort != sort {
                        settings.album_sort = sort;
                        if sort == AlbumSort::Random {
                            library_view.random_sort_seed =
                                library_view.random_sort_seed.wrapping_add(1);
                        }
                        library_view.album_sort_dirty = true;
                        save_settings(settings, error);
                    }
                }

                let sort_direction = view_button(
                    ui,
                    false,
                    if settings.sort_ascending {
                        egui_phosphor::regular::SORT_ASCENDING
                    } else {
                        egui_phosphor::regular::SORT_DESCENDING
                    },
                    if settings.sort_ascending {
                        "Ascending"
                    } else {
                        "Descending"
                    },
                );
                if sort_direction {
                    settings.sort_ascending = !settings.sort_ascending;
                    library_view.album_sort_dirty = true;
                    save_settings(settings, error);
                }

                let mut layout_changed = None;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (layout, icon, label) in [
                        (
                            AlbumLayout::List,
                            egui_phosphor::regular::LIST_BULLETS,
                            "List view",
                        ),
                        (
                            AlbumLayout::Grid,
                            egui_phosphor::regular::GRID_FOUR,
                            "Grid view",
                        ),
                    ] {
                        if view_button(ui, settings.album_layout == layout, icon, label) {
                            layout_changed = Some(layout);
                        }
                    }
                    ui.allocate_ui_with_layout(
                        egui::vec2(220.0, 32.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            egui::Frame::new()
                                .fill(colors.panel)
                                .stroke(egui::Stroke::new(1.0_f32, colors.border))
                                .corner_radius(3.0)
                                .inner_margin(egui::Margin::symmetric(10, 0))
                                .show(ui, |ui| {
                                    // Frame stroke adds one pixel on each side;
                                    // keep the inner height at 30 px for a 32 px control.
                                    ui.set_min_height(30.0);
                                    ui.with_layout(
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::vec2(18.0, 20.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().text(
                                                rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                egui_phosphor::regular::MAGNIFYING_GLASS,
                                                theme::icon_font(18.0),
                                                colors.text,
                                            );
                                            let search = ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut library_view.search,
                                                )
                                                .id(egui::Id::new("library-search-field"))
                                                .text_color(colors.text)
                                                .hint_text(
                                                    egui::RichText::new("Search")
                                                        .color(colors.text),
                                                )
                                                .desired_width(122.0)
                                                .frame(false),
                                            );
                                            search.on_hover_text("Search (F)");
                                            ui.add_space(4.0);
                                            egui::Frame::new()
                                                .fill(colors.canvas)
                                                .corner_radius(4.0)
                                                .inner_margin(egui::Margin::symmetric(5, 2))
                                                .show(ui, |ui| {
                                                    ui.label(
                                                        egui::RichText::new("F")
                                                            .size(10.0)
                                                            .color(colors.muted),
                                                    );
                                                });
                                        },
                                    );
                                });
                        },
                    );
                });
                if let Some(layout) = layout_changed {
                    settings.album_layout = layout;
                    save_settings(settings, error);
                }
            });
        });
}

fn save_settings(settings: &Settings, error: &mut Option<String>) {
    if let Err(save_error) = settings.save() {
        *error = Some(format!("Could not save settings: {save_error}"));
    }
}

fn paint_sort_dropdown(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    label: &str,
    popup_id: egui::Id,
) {
    let colors = theme::colors(ui.ctx());
    let open = ui.memory(|memory| memory.is_popup_open(popup_id));
    let fill = if open || response.hovered() {
        colors.hover
    } else {
        colors.panel
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0_f32, colors.border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        colors.text,
    );
    ui.painter().text(
        egui::pos2(rect.right() - 14.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::CARET_DOWN,
        theme::icon_font(15.0),
        colors.muted,
    );
}

fn view_button(ui: &mut egui::Ui, selected: bool, icon: &str, label: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(34.0, 32.0), egui::Sense::click());
    paint_icon_button(ui, rect, &response, icon, selected);
    response.on_hover_text(label).clicked()
}

fn paint_icon_button(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    icon: &str,
    selected: bool,
) {
    let colors = theme::colors(ui.ctx());
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            6.0,
            if selected {
                colors.selected
            } else {
                colors.hover
            },
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        theme::icon_font(18.0),
        if selected || response.hovered() {
            colors.text
        } else {
            colors.muted
        },
    );
}
