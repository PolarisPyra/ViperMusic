use crate::shared::ui::theme;
use eframe::egui;

/// Narrowest width a flexible column may shrink to while dragging.
pub(crate) const MIN_COLUMN_WIDTH: f32 = 64.0;
/// Width of the invisible grab area centered on each column divider.
const HANDLE_GRAB_WIDTH: f32 = 12.0;

/// Split the span between `start` and `end` into one column per proportion.
///
/// # Arguments
/// * `start` - Left edge of the first column.
/// * `end` - Right edge of the last column.
/// * `proportions` - Relative width of each of the three flexible columns.
///
/// # Returns
/// The four x positions: the start, the two dividers, and the end.
pub(crate) fn column_boundaries(start: f32, end: f32, proportions: [f32; 3]) -> [f32; 4] {
    let widths = distribute_column_widths((end - start).max(0.0), proportions);
    [start, start + widths[0], start + widths[0] + widths[1], end]
}

/// Convert proportions into pixel widths, enforcing a minimum width per column.
///
/// Invalid or all-zero proportions fall back to equal widths.
///
/// # Arguments
/// * `total_width` - Pixel width available for the three columns.
/// * `proportions` - Relative weight of each column.
///
/// # Returns
/// The pixel width of each column, summing to `total_width` when it is positive.
pub(crate) fn distribute_column_widths(total_width: f32, proportions: [f32; 3]) -> [f32; 3] {
    if total_width <= 0.0 {
        return [0.0; 3];
    }
    let mut weights = proportions.map(|weight| {
        if weight.is_finite() && weight > 0.0 {
            weight
        } else {
            0.0
        }
    });
    let mut weight_total = weights.iter().sum::<f32>();
    if weight_total <= f32::EPSILON {
        weights = [1.0; 3];
        weight_total = 3.0;
    }

    let mut widths = weights.map(|weight| total_width * weight / weight_total);
    let minimum = MIN_COLUMN_WIDTH.min(total_width / 3.0);
    for index in 0..3 {
        if widths[index] < minimum {
            let deficit = minimum - widths[index];
            let surplus = widths
                .iter()
                .enumerate()
                .filter(|(other, width)| *other != index && **width > minimum)
                .map(|(_, width)| *width - minimum)
                .sum::<f32>();
            if surplus > 0.0 {
                for (other, width) in widths.iter_mut().enumerate() {
                    if other != index && *width > minimum {
                        let available = *width - minimum;
                        *width -= deficit * available / surplus;
                    }
                }
            }
            widths[index] = minimum;
        }
    }
    widths
}

/// Draw the draggable dividers between flexible columns and apply any drag to `proportions`.
///
/// Dividers are drawn in the header row and highlight while hovered or dragged.
///
/// # Arguments
/// * `ui` - Ui that hosts the header row.
/// * `header` - Header rectangle the dividers are drawn in.
/// * `start` - Left edge of the first flexible column.
/// * `end` - Right edge of the last flexible column.
/// * `proportions` - Relative widths of the flexible columns, updated in place while dragging.
/// * `id_salt` - Unique salt that keeps each table's divider interactions separate.
///
/// # Returns
/// `true` when a drag ended this frame, so the caller can persist the new widths.
pub(crate) fn show_column_handles(
    ui: &mut egui::Ui,
    header: egui::Rect,
    start: f32,
    end: f32,
    proportions: &mut [f32; 3],
    id_salt: &'static str,
) -> bool {
    let colors = theme::colors(ui.ctx());
    let total_width = (end - start).max(0.0);
    let mut boundaries = column_boundaries(start, end, *proportions);
    let mut drag_stopped = false;
    for boundary in 1..3 {
        let x = boundaries[boundary];
        let handle = egui::Rect::from_center_size(
            egui::pos2(x, header.center().y),
            egui::vec2(HANDLE_GRAB_WIDTH, header.height()),
        );
        let response = ui
            .interact(
                handle,
                ui.id().with((id_salt, boundary)),
                egui::Sense::drag(),
            )
            .on_hover_text("Drag to resize column");
        let active = response.hovered() || response.dragged();
        if active {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if response.dragged() && total_width > 0.0 {
            let widths = distribute_column_widths(total_width, *proportions);
            let minimum = MIN_COLUMN_WIDTH.min(total_width / 3.0);
            let delta = response
                .drag_delta()
                .x
                .clamp(minimum - widths[boundary - 1], widths[boundary] - minimum);
            if delta != 0.0 {
                let mut resized = widths;
                resized[boundary - 1] += delta;
                resized[boundary] -= delta;
                *proportions = resized.map(|column_width| column_width / total_width);
                boundaries = column_boundaries(start, end, *proportions);
            }
        }
        drag_stopped |= response.drag_stopped();
        ui.painter().line_segment(
            [
                egui::pos2(x, header.top() + 6.0),
                egui::pos2(x, header.bottom() - 6.0),
            ],
            egui::Stroke::new(
                if active { 2.0 } else { 1.0_f32 },
                if active { colors.accent } else { colors.border },
            ),
        );
    }
    drag_stopped
}
