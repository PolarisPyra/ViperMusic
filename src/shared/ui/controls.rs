use super::theme::{self, Palette};
use eframe::egui;

/// egui ID of the library search field, used to focus it from the keyboard.
pub(crate) const SEARCH_FIELD_ID: &str = "library-search-field";

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
    let colors = theme::colors(ui.ctx());
    let text_color = if enabled { colors.text } else { colors.muted };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(12.0),
        text_color,
    );
    let size = egui::vec2(galley.size().x + 24.0, (galley.size().y + 12.0).max(36.0));
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    describe_button(&response, label);
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
    ui.painter()
        .galley(rect.center() - galley.size() * 0.5, galley, text_color);
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
    let colors = theme::colors(ui.ctx());
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

/// Describe a custom-painted button to assistive technology such as screen readers.
///
/// # Arguments
///
/// * `response` - Response of the painted control.
/// * `label` - Spoken name of the control.
pub(crate) fn describe_button(response: &egui::Response, label: &str) {
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            response.sense.senses_click(),
            label,
        )
    });
}

/// Draw a context or dropdown menu entry that closes the menu when clicked.
///
/// # Arguments
///
/// * `ui` - Menu UI.
/// * `enabled` - Whether the entry can be clicked.
/// * `label` - Entry text.
///
/// # Returns
///
/// Whether the entry was clicked.
pub(crate) fn menu_entry(ui: &mut egui::Ui, enabled: bool, label: &str) -> bool {
    let colors = theme::colors(ui.ctx());
    let clicked = ui
        .add_enabled(
            enabled,
            egui::Button::selectable(false, egui::RichText::new(label).color(colors.text)),
        )
        .clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// Frame shared by the application's modal dialogs.
///
/// # Arguments
///
/// * `colors` - Active theme palette.
///
/// # Returns
///
/// A rounded, bordered panel frame.
pub(crate) fn modal_frame(colors: Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(colors.panel)
        .stroke(egui::Stroke::new(1.0_f32, colors.border))
        .corner_radius(14.0)
        .inner_margin(egui::Margin::symmetric(18, 18))
}

/// Paint a dialog close button.
///
/// # Arguments
///
/// * `ui` - The UI that should contain the button.
/// * `tooltip` - Hover text describing what the button closes.
///
/// # Returns
///
/// Whether the button was clicked.
pub(crate) fn close_button(ui: &mut egui::Ui, tooltip: &str) -> bool {
    let colors = theme::colors(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
    describe_button(&response, tooltip);
    if response.hovered() {
        ui.painter().rect_filled(rect, 8.0, colors.hover);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::X,
        theme::icon_font(16.0),
        if response.hovered() {
            colors.text
        } else {
            colors.muted
        },
    );
    response.on_hover_text(tooltip).clicked()
}

/// Draw the library search box; `F` focuses it from anywhere.
///
/// # Arguments
///
/// * `ui` - The UI that should contain the field.
/// * `query` - Search text shared by the album and song views.
pub(crate) fn search_field(ui: &mut egui::Ui, query: &mut String) {
    let colors = theme::colors(ui.ctx());
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
                    // Frame stroke adds one pixel on each side; keep the inner height at 30 px.
                    ui.set_min_height(30.0);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(18.0, 20.0), egui::Sense::hover());
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            egui_phosphor::regular::MAGNIFYING_GLASS,
                            theme::icon_font(18.0),
                            colors.text,
                        );
                        ui.add(
                            egui::TextEdit::singleline(query)
                                .id(egui::Id::new(SEARCH_FIELD_ID))
                                .text_color(colors.text)
                                .hint_text(egui::RichText::new("Search").color(colors.text))
                                .desired_width(122.0)
                                .frame(egui::Frame::NONE),
                        )
                        .on_hover_text("Search (F)");
                        ui.add_space(4.0);
                        egui::Frame::new()
                            .fill(colors.canvas)
                            .corner_radius(4.0)
                            .inner_margin(egui::Margin::symmetric(5, 2))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("F").size(10.0).color(colors.muted));
                            });
                    });
                });
        },
    );
}
