use super::{
    controls::{click, sz, tooltip},
    icons::{icon, IconName},
    theme,
};
use crate::app::ViperApp;
use gpui::{div, prelude::*, AnyElement, Context};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// How long a notification stays visible unless the user dismisses it first.
const TOAST_LIFETIME: Duration = Duration::from_secs(10);
/// Oldest notifications are dropped once this many are queued.
const MAX_TOASTS: usize = 5;

struct Toast {
    id: u64,
    message: String,
    /// When the toast was first drawn; `None` until then.
    shown_at: Option<Instant>,
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

    fn dismiss(&mut self, id: u64) {
        self.items.retain(|toast| toast.id != id);
    }
}

/// Draw queued notifications above the player bar and expire old ones.
///
/// # Arguments
/// * `app` - Application holding the notifications.
/// * `cx` - Application entity context.
///
/// # Returns
/// The notification stack, or `None` when nothing is queued.
pub(crate) fn render(app: &mut ViperApp, cx: &mut Context<ViperApp>) -> Option<AnyElement> {
    let now = Instant::now();
    let toasts = &mut app.toasts;
    toasts.items.retain(|toast| {
        toast
            .shown_at
            .is_none_or(|shown_at| now - shown_at < TOAST_LIFETIME)
    });
    if toasts.items.is_empty() {
        return None;
    }
    let colors = theme::colors(cx);
    let mut stack = div()
        .absolute()
        .right(sz(16.0))
        .bottom(sz(128.0))
        .w(sz(360.0))
        .flex()
        .flex_col()
        .gap(sz(8.0));
    for toast in &mut toasts.items {
        let shown_at = *toast.shown_at.get_or_insert(now);
        app.repaint
            .request_repaint_after(TOAST_LIFETIME.saturating_sub(now - shown_at));
        let id = toast.id;
        stack = stack.child(
            div()
                .id(("toast", id))
                .occlude()
                .flex()
                .items_center()
                .gap(sz(8.0))
                .bg(colors.surface)
                .border_1()
                .border_color(colors.danger)
                .rounded(sz(8.0))
                .px(sz(12.0))
                .py(sz(10.0))
                .child(icon(IconName::CircleAlert, 18.0, colors.danger))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(sz(12.0))
                        .text_color(colors.text)
                        .child(toast.message.clone()),
                )
                .child(
                    div()
                        .id(("dismiss-toast", id))
                        .flex_none()
                        .cursor_pointer()
                        .tooltip(tooltip("Dismiss"))
                        .on_click(click(cx, move |app, _, _| app.toasts.dismiss(id)))
                        .child(icon(IconName::X, 14.0, colors.muted)),
                ),
        );
    }
    Some(stack.into_any_element())
}
