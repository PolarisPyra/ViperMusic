//! Ripping audio CDs into the library as tagged FLAC files, with tags from MusicBrainz.
mod drive;
mod musicbrainz;
mod rip;
mod screen;
mod state;

pub(crate) use screen::{show, RipperScreen};
pub(crate) use state::RipperFeature;
