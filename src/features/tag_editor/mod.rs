//! Editing an album's tags by hand or from a MusicBrainz release.
mod screen;
mod state;
mod writer;

pub(crate) use screen::{render, request_close};
pub(crate) use state::TagEditor;
