//! Ripping audio CDs into the library as tagged FLAC files, with tags from MusicBrainz.
mod drive;
pub(crate) mod musicbrainz;
mod rip;
mod screen;
mod state;

pub(crate) use screen::render;
pub(crate) use state::RipperFeature;
