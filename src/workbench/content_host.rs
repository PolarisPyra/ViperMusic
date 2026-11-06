use super::Page;
use crate::{app::ViperApp, features};
use gpui::{prelude::*, AnyElement, Context, Window};

/// Draw the page chosen in the sidebar.
///
/// # Arguments
/// * `app` - Application state shared with the views.
/// * `window` - Window being drawn.
/// * `cx` - Application entity context.
///
/// # Returns
/// The page element.
pub(super) fn render(
    app: &mut ViperApp,
    window: &mut Window,
    cx: &mut Context<ViperApp>,
) -> AnyElement {
    match app.workbench.page {
        Page::Home => features::home::render(app, window, cx).into_any_element(),
        Page::Albums => features::albums::render(app, window, cx).into_any_element(),
        Page::Songs => features::songs::render(app, window, cx).into_any_element(),
        Page::Ripper => features::ripper::render(app, window, cx).into_any_element(),
    }
}
