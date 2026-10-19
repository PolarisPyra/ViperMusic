use super::theme;
use eframe::egui;
use std::{collections::VecDeque, time::Duration};

/// Seconds a notification stays visible unless the user dismisses it first.
const TOAST_LIFETIME: f64 = 10.0;
/// Oldest notifications are dropped once this many are queued.
const MAX_TOASTS: usize = 5;

struct Toast {
    id: u64,
    message: String,
    /// Context time when the toast was first drawn; `None` until then.
    shown_at: Option<f64>,
}

/// Stacked, dismissible notifications shown in the bottom-right corner.
#[derive(Default)]
pub(crate) struct Toasts {
    items: VecDeque<Toast>,
    next_id: u64,
}

impl Toasts {
    /// Queue a notification, replacing an identical one that is already visible.
    ///
    /// # Arguments
    /// * `message` - Text shown to the user.
    pub(crate) fn push(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.items.retain(|toast| toast.message != message);
        if self.items.len() == MAX_TOASTS {
            self.items.pop_front();
        }
        self.items.push_back(Toast {
            id: self.next_id,
            message,
            shown_at: None,
        });
        self.next_id = self.next_id.wrapping_add(1);
    }

    /// Draw queued notifications above the player bar and expire old ones.
    ///
    /// # Arguments
    /// * `ctx` - egui context for the frame.
    pub(crate) fn show(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        self.items.retain(|toast| {
            toast
                .shown_at
                .is_none_or(|shown_at| now - shown_at < TOAST_LIFETIME)
        });
        if self.items.is_empty() {
            return;
        }
        let colors = theme::colors(ctx);
        let mut dismissed = None;
        egui::Area::new(egui::Id::new("toasts"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -128.0))
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                for toast in &mut self.items {
                    let shown_at = *toast.shown_at.get_or_insert(now);
                    egui::Frame::new()
                        .fill(colors.surface)
                        .stroke(egui::Stroke::new(1.0_f32, colors.danger))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(egui_phosphor::regular::WARNING_CIRCLE)
                                        .font(theme::icon_font(18.0))
                                        .color(colors.danger),
                                );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&toast.message)
                                            .size(12.0)
                                            .color(colors.text),
                                    )
                                    .wrap(),
                                );
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(egui_phosphor::regular::X)
                                                .font(theme::icon_font(14.0))
                                                .color(colors.muted),
                                        )
                                        .frame(false),
                                    )
                                    .on_hover_text("Dismiss")
                                    .clicked()
                                {
                                    dismissed = Some(toast.id);
                                }
                            });
                        });
                    let remaining = TOAST_LIFETIME - (now - shown_at);
                    ctx.request_repaint_after(Duration::from_secs_f64(remaining.max(0.0)));
                }
            });
        if let Some(id) = dismissed {
            self.items.retain(|toast| toast.id != id);
        }
    }
}
