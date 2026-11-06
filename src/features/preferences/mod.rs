//! Preferences UI and state.
mod screen;
mod state;

pub(crate) use screen::render;
pub use screen::Category;
pub(crate) use state::PreferencesState;
