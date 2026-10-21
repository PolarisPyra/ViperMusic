use super::{content_host, shell, WorkbenchContext};
use crate::shared::action::AppAction;
use eframe::egui;

/// Draw the application shell and active page for one frame.
///
/// # Arguments
/// * `ctx` - egui context for the frame.
/// * `context` - Mutable application state shared with the views.
///
/// # Returns
/// Actions requested by the user, to be handled once drawing has finished.
pub(crate) fn show(ui: &mut egui::Ui, context: &mut WorkbenchContext<'_>) -> Vec<AppAction> {
    let ctx = &ui.ctx().clone();
    let mut actions = Vec::with_capacity(2);
    actions.extend(shell::menu_bar::show(
        ui,
        context.state,
        context.library,
        context.playback,
    ));
    shell::player_bar::show(
        ui,
        context.state,
        context.playback,
        context.library,
        context.settings,
        context.artwork_cache,
        context.toasts,
    );
    if let Some(error) = shell::sidebar::show(ui, context.settings, context.state) {
        context.toasts.push(error);
    }
    shell::queue_panel::show(ui, context.state, context.playback, context.library);
    actions.extend(content_host::show(ui, context));
    actions.extend(crate::features::preferences::show(
        ctx,
        context.state,
        context.preferences,
        context.settings,
        context.library,
        context.lastfm_service,
        context.lastfm_settings,
    ));
    actions
}
