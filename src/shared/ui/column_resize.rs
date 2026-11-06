use crate::shared::ui::{
    controls::{sz, tooltip},
    theme::Palette,
};
use gpui::{div, prelude::*, px, relative, AppContext, CursorStyle, EmptyView, SharedString};

/// Narrowest width a flexible column may shrink to while dragging.
pub(crate) const MIN_COLUMN_WIDTH: f32 = 64.0;
/// Width of the invisible grab area centered on each column divider.
const HANDLE_GRAB_WIDTH: f32 = 12.0;

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

/// A divider being dragged between two flexible columns of a table.
pub(crate) struct ColumnDrag {
    /// Table the divider belongs to.
    pub(crate) table: &'static str,
    /// Index of the divider: 1 between the first two columns, 2 between the last two.
    pub(crate) boundary: usize,
}

/// Fractions of the flexible span each column takes, summing to one.
///
/// # Arguments
/// * `proportions` - Relative widths of the flexible columns.
///
/// # Returns
/// Each column's fraction, equal when the proportions are unusable.
pub(crate) fn column_fractions(proportions: [f32; 3]) -> [f32; 3] {
    distribute_column_widths(1.0, proportions)
}

/// Move the divider `boundary` to `x`, measured from the start of a flexible span `width` wide.
///
/// # Arguments
/// * `proportions` - Relative widths of the flexible columns, updated in place.
/// * `boundary` - Divider being dragged, 1 or 2.
/// * `x` - Pointer position from the start of the span.
/// * `width` - Width of the span.
pub(crate) fn drag_divider(proportions: &mut [f32; 3], boundary: usize, x: f32, width: f32) {
    if width <= 0.0 || !(1..3).contains(&boundary) {
        return;
    }
    let widths = distribute_column_widths(width, *proportions);
    let minimum = MIN_COLUMN_WIDTH.min(width / 3.0);
    let current = widths[..boundary].iter().sum::<f32>();
    let delta = (x - current).clamp(minimum - widths[boundary - 1], widths[boundary] - minimum);
    if delta != 0.0 {
        let mut resized = widths;
        resized[boundary - 1] += delta;
        resized[boundary] -= delta;
        *proportions = resized.map(|column_width| column_width / width);
    }
}

/// Draw the grab area of the divider `boundary`, placed `offset` of the way across its span.
///
/// The divider highlights while hovered or dragged; dragging it reports [`ColumnDrag`] moves to
/// the span's `on_drag_move` listener.
///
/// # Arguments
/// * `table` - Table the divider belongs to.
/// * `boundary` - Divider index, 1 or 2.
/// * `offset` - Fraction of the span left of the divider.
/// * `dragging` - Whether this divider is being dragged.
/// * `colors` - Active theme palette.
///
/// # Returns
/// The divider element.
pub(crate) fn column_handle(
    table: &'static str,
    boundary: usize,
    offset: f32,
    dragging: bool,
    colors: Palette,
) -> impl IntoElement {
    let line = |color| {
        div()
            .absolute()
            .top(sz(6.0))
            .bottom(sz(6.0))
            .left(sz(HANDLE_GRAB_WIDTH / 2.0 - 0.5))
            .w(px(1.0))
            .bg(color)
    };
    let group = SharedString::from(format!("{table}-divider-{boundary}"));
    div()
        .id((table, boundary))
        .absolute()
        .top_0()
        .bottom_0()
        .left(relative(offset))
        .ml(sz(-HANDLE_GRAB_WIDTH / 2.0))
        .w(sz(HANDLE_GRAB_WIDTH))
        .cursor(CursorStyle::ResizeLeftRight)
        .tooltip(tooltip("Drag to resize column"))
        .group(group.clone())
        .child(
            line(if dragging {
                colors.accent
            } else {
                colors.border
            })
            .group_hover(group, |style| style.bg(colors.accent)),
        )
        .on_drag(ColumnDrag { table, boundary }, |_, _, _, cx| {
            cx.new(|_| EmptyView)
        })
}
