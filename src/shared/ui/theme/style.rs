use super::colors;
use eframe::egui;
use std::sync::LazyLock;

static ICON_FAMILY: LazyLock<egui::FontFamily> =
    LazyLock::new(|| egui::FontFamily::Name("phosphor".into()));
static ICON_FILL_FAMILY: LazyLock<egui::FontFamily> =
    LazyLock::new(|| egui::FontFamily::Name("phosphor-fill".into()));

/// Return the regular Phosphor icon font at `size` points without allocating a family name.
pub(crate) fn icon_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, ICON_FAMILY.clone())
}

/// Return the filled Phosphor icon font at `size` points without allocating a family name.
pub(crate) fn icon_fill_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, ICON_FILL_FAMILY.clone())
}

pub(crate) fn style_dropdown(ui: &mut egui::Ui) {
    let colors = colors(ui.ctx());
    let border = egui::Stroke::new(1.0_f32, colors.border);
    let visuals = ui.visuals_mut();
    visuals.window_fill = colors.panel;
    visuals.window_stroke = border;
    visuals.menu_corner_radius = egui::CornerRadius::same(6);
    visuals.selection.bg_fill = colors.selected;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, colors.text);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.bg_fill = colors.panel;
        widget.weak_bg_fill = colors.panel;
        widget.bg_stroke = border;
    }
    visuals.widgets.hovered.bg_fill = colors.hover;
    visuals.widgets.hovered.weak_bg_fill = colors.hover;
}

/// Apply the shared spacing and hover treatment used by dropdown menu contents.
///
/// # Arguments
///
/// * `ui` - The dropdown menu content UI to style.
/// * `minimum_width` - Minimum menu width in points, or zero to use its contents.
pub(crate) fn style_menu_contents(ui: &mut egui::Ui, minimum_width: f32) {
    let colors = colors(ui.ctx());
    style_dropdown(ui);
    if minimum_width > 0.0 {
        ui.set_min_width(minimum_width);
    }
    ui.spacing_mut().menu_margin = egui::Margin::same(8);
    ui.spacing_mut().button_padding = egui::vec2(14.0, 5.0);
    ui.spacing_mut().item_spacing.y = 4.0;
    let widgets = &mut ui.visuals_mut().widgets;
    widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    widgets.inactive.bg_stroke = egui::Stroke::NONE;
    widgets.hovered.bg_fill = colors.hover;
    widgets.hovered.weak_bg_fill = colors.hover;
    widgets.active.bg_fill = colors.hover;
    widgets.active.weak_bg_fill = colors.hover;
    widgets.active.bg_stroke = egui::Stroke::NONE;
    widgets.open.bg_fill = colors.hover;
    widgets.open.weak_bg_fill = colors.hover;
    widgets.open.bg_stroke = egui::Stroke::NONE;
}
