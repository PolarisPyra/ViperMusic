use crate::shared::artwork::ArtworkCache;
use crate::shared::ui::toasts::Toasts;
use crate::{
    features::{
        library::state::LibraryFeature, lyrics::LyricsFeature, playback::Playback,
        preferences::PreferencesState,
    },
    platform::persistence::settings::Settings,
    workbench::WorkbenchState,
};

/// Mutable application state exposed to the workbench's view orchestration.
pub(crate) struct WorkbenchContext<'a> {
    pub(crate) state: &'a mut WorkbenchState,
    pub(crate) library: &'a mut LibraryFeature,
    pub(crate) playback: &'a mut Playback,
    pub(crate) lyrics: &'a mut LyricsFeature,
    pub(crate) settings: &'a mut Settings,
    pub(crate) preferences: &'a mut PreferencesState,
    pub(crate) lastfm_service: &'a mut crate::platform::presence::lastfm::LastFmService,
    pub(crate) lastfm_settings: &'a mut crate::platform::persistence::settings::LastFmSettings,
    pub(crate) artwork_cache: &'a mut ArtworkCache,
    pub(crate) toasts: &'a mut Toasts,
}
