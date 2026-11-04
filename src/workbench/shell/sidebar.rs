use crate::{
    platform::persistence::settings::{Settings, StartupView},
    shared::ui::{controls::menu_entry, theme},
    workbench::{Page, WorkbenchState},
};
use eframe::egui;
use std::borrow::Cow;

const COMPACT_WIDTH: f32 = 56.0;
const COMPACT_THRESHOLD: f32 = 140.0;
const MIN_EXPANDED_WIDTH: f32 = 170.0;
const MAX_WIDTH: f32 = 360.0;

#[derive(Clone, Copy)]
struct ResizeState {
    compact: bool,
    width: f32,
    drag_start: Option<DragStart>,
}

#[derive(Clone, Copy)]
struct DragStart {
    pointer_x: f32,
    width: f32,
}

pub(in crate::workbench) fn show(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    workbench: &mut WorkbenchState,
    rip_progress: Option<f32>,
) -> Option<String> {
    let ctx = &ui.ctx().clone();
    let colors = theme::colors(ctx);
    let mut error = None;
    let panel_id = egui::Id::new("library-sidebar");
    let resize_state_id = panel_id.with("mode");
    let preferred = || ResizeState {
        compact: settings.compact_sidebar,
        width: if settings.compact_sidebar {
            COMPACT_WIDTH
        } else {
            settings
                .left_panel_width
                .clamp(MIN_EXPANDED_WIDTH, MAX_WIDTH)
        },
        drag_start: None,
    };
    let mut resize_state = ctx
        .data(|data| data.get_temp::<ResizeState>(resize_state_id))
        .unwrap_or_else(preferred);

    // Preference changes reset the width; drag changes retain the same panel and drag handle.
    if resize_state.compact != settings.compact_sidebar {
        ctx.data_mut(|data| data.remove::<egui::containers::panel::PanelState>(panel_id));
        resize_state = preferred();
    }
    let panel_width = resize_state.width.clamp(COMPACT_WIDTH, MAX_WIDTH);
    egui::Panel::left(panel_id)
        .resizable(false)
        .exact_size(panel_width)
        .frame(
            egui::Frame::new()
                .fill(colors.sidebar)
                .inner_margin(egui::Margin::symmetric(8, 10)),
        )
        .show(ui, |ui| {
            // Switch the nav layout with the live width so labels do not clip while resizing.
            let compact = resize_state.width < COMPACT_THRESHOLD;
            for page in Page::ALL {
                if settings.hidden_pages.contains(&page) {
                    continue;
                }
                if let Some(save_error) =
                    nav_item(ui, page, compact, workbench, settings, rip_progress)
                {
                    error = Some(save_error);
                }
            }
            let content_rect = ui.max_rect();
            let handle_rect = egui::Rect::from_min_max(
                egui::pos2(content_rect.right() - 6.0, content_rect.top()),
                content_rect.right_bottom(),
            );
            let resize = ui.interact(
                handle_rect,
                panel_id.with("resize-handle"),
                egui::Sense::drag(),
            );
            if resize.hovered() || resize.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                ui.painter().line_segment(
                    [
                        handle_rect.right_center() - egui::vec2(0.0, 10.0),
                        handle_rect.right_center() + egui::vec2(0.0, 10.0),
                    ],
                    egui::Stroke::new(
                        if resize.dragged() { 2.0_f32 } else { 1.0_f32 },
                        if resize.dragged() {
                            colors.accent
                        } else {
                            colors.muted
                        },
                    ),
                );
            }
            if resize.drag_started() {
                if let Some(pointer) = resize.interact_pointer_pos() {
                    resize_state.drag_start = Some(DragStart {
                        pointer_x: pointer.x,
                        width: panel_width,
                    });
                }
            }
            if resize.dragged() || resize.drag_stopped() {
                if let (Some(drag_start), Some(pointer)) =
                    (resize_state.drag_start, resize.interact_pointer_pos())
                {
                    resize_state.width = (drag_start.width + pointer.x - drag_start.pointer_x)
                        .clamp(COMPACT_WIDTH, MAX_WIDTH);
                }
                ui.ctx().request_repaint();
            }
            if resize.drag_stopped() {
                let compact = resize_state.width < COMPACT_THRESHOLD;
                resize_state.width = if compact {
                    COMPACT_WIDTH
                } else {
                    resize_state.width.clamp(MIN_EXPANDED_WIDTH, MAX_WIDTH)
                };
                settings.compact_sidebar = compact;
                if !compact {
                    settings.left_panel_width = resize_state.width;
                }
                resize_state.compact = compact;
                resize_state.drag_start = None;
                if let Err(save_error) = settings.save() {
                    error = Some(format!("Could not save settings: {save_error}"));
                }
            }
        });
    ctx.data_mut(|data| data.insert_temp(resize_state_id, resize_state));

    // If album details were closed and panel was hidden, show it again
    if settings.left_panel_hidden && !workbench.show_album_details {
        settings.left_panel_hidden = false;
        if let Err(save_error) = settings.save() {
            error = Some(format!("Could not save settings: {save_error}"));
        }
    }
    error
}

/// Draw a sidebar navigation entry with a context menu for choosing the startup page and hiding
/// the entry, and the rip's progress under Rip CD while a CD is ripped.
///
/// # Returns
/// A message describing a settings save failure, if any.
fn nav_item(
    ui: &mut egui::Ui,
    page: Page,
    compact: bool,
    workbench: &mut WorkbenchState,
    settings: &mut Settings,
    rip_progress: Option<f32>,
) -> Option<String> {
    let label = page.label();
    let (icon, startup_view) = match page {
        Page::Home => (
            egui_phosphor::regular::HOUSE_SIMPLE,
            Some(StartupView::Home),
        ),
        Page::Albums => (egui_phosphor::regular::DISC, Some(StartupView::Albums)),
        Page::Songs => (
            egui_phosphor::regular::MUSIC_NOTES,
            Some(StartupView::Songs),
        ),
        Page::Ripper => (egui_phosphor::regular::VINYL_RECORD, None),
    };
    let mut error = None;
    let colors = theme::colors(ui.ctx());
    let selected = workbench.page == page;
    let size = egui::vec2(ui.available_width(), 40.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let fill = if selected {
        colors.selected
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 3.0, fill);
    let icon_tint = if selected { colors.text } else { colors.subtle };
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let icon_center = if compact {
        rect.center()
    } else {
        rect.min + egui::vec2(22.0, rect.height() * 0.5)
    };
    ui.painter().text(
        icon_center,
        egui::Align2::CENTER_CENTER,
        icon,
        theme::icon_font(18.0),
        icon_tint,
    );
    let response = if compact {
        response.on_hover_text(label)
    } else {
        ui.painter().text(
            rect.min + egui::vec2(43.0, rect.height() * 0.5),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.0),
            colors.text,
        );
        response
    };
    if let (Page::Ripper, Some(progress)) = (page, rip_progress) {
        // A thin bar along the entry's bottom edge, so a rip can be followed from any page.
        let track = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 8.0, rect.bottom() - 4.0),
            egui::pos2(rect.right() - 8.0, rect.bottom() - 2.0),
        );
        ui.painter().rect_filled(track, 1.0, colors.border);
        let mut filled = track;
        filled.set_width(track.width() * progress.clamp(0.0, 1.0));
        ui.painter().rect_filled(filled, 1.0, colors.accent);
    }
    let response = match (page, rip_progress) {
        (Page::Ripper, Some(progress)) => {
            response.on_hover_text(format!("Ripping a CD: {:.0}%", progress * 100.0))
        }
        _ => response,
    };
    if response.clicked_by(egui::PointerButton::Primary) {
        workbench.page = page;
    }
    let mut changed = false;
    response.context_menu(|ui| {
        crate::workbench::style_menu_popup(ui);
        if let Some(startup_view) = startup_view {
            let current = settings.startup_view == startup_view;
            let text = if current {
                Cow::Borrowed("Current startup view")
            } else {
                Cow::Owned(format!("Set {label} as startup view"))
            };
            if menu_entry(ui, !current, &text) {
                settings.startup_view = startup_view;
                changed = true;
            }
        }
        if menu_entry(ui, settings.can_hide_page(page), "Hide from sidebar") {
            settings.hidden_pages.insert(page);
            changed = true;
        }
    });
    if changed {
        if let Err(save_error) = settings.save() {
            error = Some(format!("Could not save settings: {save_error}"));
        }
    }
    error
}
