use super::{Page, WorkbenchContext};
use crate::{features, shared::action::AppAction};
use eframe::egui;

pub(crate) fn show(ctx: &egui::Context, context: &mut WorkbenchContext<'_>) -> Option<AppAction> {
    match context.state.page {
        Page::Home => features::home::show(
            ctx,
            context.state,
            context.library,
            context.playback,
            context.settings,
            context.artwork_cache,
        ),
        Page::Albums => {
            features::albums::track_list::show(
                ctx,
                context.library,
                context.playback,
                context.lyrics,
                context.settings,
                context.artwork_cache,
                context.toasts,
            );
            features::albums::show(
                ctx,
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
                ctx,
                context.state,
                context.library,
                context.playback,
                context.toasts,
            );
            None
        }
    }
}
