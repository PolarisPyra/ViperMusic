use super::theme;
use eframe::egui;
use std::hash::Hash;

pub(crate) const ALBUMS_PER_PAGE: usize = 70;

pub(crate) fn show(
    ctx: &egui::Context,
    total_items: usize,
    page_size: usize,
    key: impl Hash,
) -> std::ops::Range<usize> {
    let colors = theme::colors(ctx);
    let page_count = total_items.div_ceil(page_size);
    let state_id = egui::Id::new(("pagination-page", key));
    let mut page = ctx.data_mut(|data| data.get_temp::<usize>(state_id).unwrap_or_default());
    page = page.min(page_count.saturating_sub(1));

    if page_count > 1 {
        egui::TopBottomPanel::bottom("album-pagination")
            .exact_height(48.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.canvas)
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().interact_size.y = 32.0;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let next = ui.add_enabled(
                        page + 1 < page_count,
                        egui::Button::new(
                            egui::RichText::new(egui_phosphor::regular::CARET_RIGHT)
                                .font(theme::icon_font(17.0)),
                        )
                        .min_size(egui::vec2(34.0, 32.0)),
                    );
                    if next.clicked() {
                        page += 1;
                    }
                    ui.label(
                        egui::RichText::new(format!("Page {} of {page_count}", page + 1))
                            .size(12.0)
                            .color(colors.muted),
                    );
                    let previous = ui.add_enabled(
                        page > 0,
                        egui::Button::new(
                            egui::RichText::new(egui_phosphor::regular::CARET_LEFT)
                                .font(theme::icon_font(17.0)),
                        )
                        .min_size(egui::vec2(34.0, 32.0)),
                    );
                    if previous.clicked() {
                        page = page.saturating_sub(1);
                    }
                    let first_item = page * page_size + 1;
                    let last_item = (first_item + page_size - 1).min(total_items);
                    ui.label(
                        egui::RichText::new(format!(
                            "{first_item}–{last_item} of {total_items} albums"
                        ))
                        .size(12.0)
                        .color(colors.muted),
                    );
                });
            });
    }

    ctx.data_mut(|data| data.insert_temp(state_id, page));
    let start = (page * page_size).min(total_items);
    let end = (start + page_size).min(total_items);
    start..end
}
