use super::{Page, WorkbenchContext};
use crate::{features, shared::action::AppAction};
use eframe::egui;

pub(crate) fn show(ui: &mut egui::Ui, context: &mut WorkbenchContext<'_>) -> Option<AppAction> {
    match context.state.page {
        Page::Home => features::home::show(
            ui,
            context.state,
            context.library,
            context.playback,
            context.settings,
            context.artwork_cache,
        ),
        Page::Albums => {
            features::albums::track_list::show(
                ui,
                context.library,
                context.playback,
                context.lyrics,
                context.settings,
                context.artwork_cache,
                context.toasts,
            );
            features::albums::show(
                ui,
                features::albums::AlbumScreen {
                    workbench: context.state,
                    library: context.library,
                    playback: context.playback,
                    settings: context.settings,
                    artwork_cache: context.artwork_cache,
                    toasts: context.toasts,
                },
            );
            None
        }
        Page::Songs => {
            features::songs::show(
                ui,
                context.state,
                context.library,
                context.playback,
                context.settings,
                context.toasts,
            );
            None
        }
    }
}
