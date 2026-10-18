use eframe::egui;

/// Paint a compact button using the application's preference style.
///
/// # Arguments
///
/// * `ui` - The UI that should contain the button.
/// * `label` - Text displayed inside the button.
/// * `enabled` - Whether the button can be clicked.
///
/// # Returns
///
/// The button response, including whether it was clicked.
pub(crate) fn action_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let font = egui::FontId::proportional(12.0);
    let text_color = if enabled {
        crate::shared::ui::theme::colors(ui.ctx()).text
    } else {
        crate::shared::ui::theme::colors(ui.ctx()).muted
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, text_color);
    action_button_with_dimensions(ui, label, enabled, galley.size().x + 24.0, 36.0)
}

fn action_button_with_dimensions(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    width: f32,
    height: f32,
) -> egui::Response {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
    let font = egui::FontId::proportional(12.0);
    let text_color = if enabled { colors.text } else { colors.muted };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), text_color);
    let size = egui::vec2(
        width.max(galley.size().x + 24.0),
        height.max(galley.size().y + 12.0),
    );
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let fill = if !enabled {
        colors.panel
    } else if response.is_pointer_button_down_on() {
        colors.selected
    } else if response.hovered() {
        colors.hover
    } else {
        colors.surface
    };
    ui.painter().rect(
        rect,
        7.0,
        fill,
        egui::Stroke::new(1.0_f32, colors.border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        text_color,
    );
    response
}

/// Add a single-line text field using the application's preference style.
///
/// # Arguments
///
/// * `ui` - The UI that should contain the field.
/// * `value` - The editable text value.
/// * `hint` - Placeholder text shown while the value is empty.
/// * `password` - Whether the field should obscure its contents.
///
/// # Returns
///
/// The text edit response, including focus and change state.
pub(crate) fn connection_text_edit(
    ui: &mut egui::Ui,
    value: &mut String,
    hint: &str,
    password: bool,
) -> egui::Response {
    let colors = crate::shared::ui::theme::colors(ui.ctx());
    ui.scope(|ui| {
        let input_fill = colors.input;
        let input_border = colors.border;
        let visuals = &mut ui.visuals_mut().widgets;
        visuals.inactive.bg_fill = input_fill;
        visuals.inactive.bg_stroke = egui::Stroke::new(1.0_f32, input_border);
        visuals.hovered.bg_fill = colors.surface;
        visuals.hovered.bg_stroke = egui::Stroke::new(1.0_f32, colors.border);
        visuals.active.bg_fill = input_fill;
        visuals.active.bg_stroke = egui::Stroke::new(1.0_f32, colors.accent);
        let mut edit = egui::TextEdit::singleline(value)
            .hint_text(egui::RichText::new(hint).color(colors.muted))
            .text_color(colors.text)
            .background_color(input_fill)
            .margin(egui::vec2(8.0, 6.0))
            .desired_width(ui.available_width());
        if password {
            edit = edit.password(true);
        }
        ui.add(edit)
    })
    .inner
}
