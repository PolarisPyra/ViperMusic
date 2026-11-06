use super::{content_host, shell};
use crate::{
    app::ViperApp,
    features,
    platform::updates,
    shared::ui::{
        controls::{sz, unzoomed},
        theme, toasts,
    },
};
use gpui::{div, prelude::*, AnyElement, Context, Window};

/// Draw the application shell, the active page and any open dialogs.
///
/// # Arguments
/// * `app` - Application state shared with the views.
/// * `window` - Window being drawn.
/// * `cx` - Application entity context.
///
/// # Returns
/// The window's root element.
pub(crate) fn render(
    app: &mut ViperApp,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    let colors = theme::colors(cx);
    let menu_bar = shell::menu_bar::render(cx);
    let sidebar = shell::sidebar::render(app, cx);
    let content = content_host::render(app, window, cx);
    let queue = shell::queue_panel::render(app, cx);
    let player_bar = shell::player_bar::render(app, window, cx);
    if app.ui.column_drag.is_some() && !cx.has_active_drag() {
        app.ui.column_drag = None;
        app.save_settings();
    }
    div()
        .id("viper")
        .track_focus(&app.focus)
        .key_context("Viper")
        .on_key_down(cx.listener(|app, event, window, cx| {
            if app.handle_shortcut(event, window, cx) {
                cx.stop_propagation();
                cx.notify();
            }
        }))
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .bg(colors.canvas)
        .text_color(colors.text)
        .text_size(sz(14.0))
        .font_family("Inter Variable")
        .child(menu_bar)
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(sidebar)
                .child(div().flex_1().min_w_0().h_full().child(content))
                .children(queue),
        )
        .child(player_bar)
        .children(shell::menu_bar::render_shortcuts(app, cx))
        .children(features::preferences::render(app, window, cx))
        .children(features::tag_editor::render(app, window, cx))
        .children(updates::render_card(app, cx))
        .children(toasts::render(app, cx))
        .into_any_element()
}

/// Width left for the page between the sidebar and the queue, at 100% zoom.
///
/// Pages lay out grids before they are measured, so this follows the shell's own sizes.
///
/// # Arguments
/// * `app` - Application holding the sidebar and queue state.
/// * `window` - Window being drawn.
///
/// # Returns
/// The page width.
pub(crate) fn content_width(app: &ViperApp, window: &Window) -> f32 {
    let queue = if app.workbench.show_queue {
        shell::queue_panel::PANEL_WIDTH
    } else {
        0.0
    };
    (unzoomed(window.viewport_size().width, window) - shell::sidebar::width(app) - queue).max(0.0)
}

/// Height left for the page between the menu bar and the player bar, at 100% zoom.
///
/// # Arguments
/// * `window` - Window being drawn.
///
/// # Returns
/// The page height.
pub(crate) fn content_height(window: &Window) -> f32 {
    (unzoomed(window.viewport_size().height, window)
        - shell::menu_bar::HEIGHT
        - shell::player_bar::PANEL_HEIGHT)
        .max(0.0)
}
