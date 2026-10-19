//! Audio playback state, queue controls, and desktop media integration.
#[cfg(target_os = "linux")]
pub(crate) mod mpris;
mod player;

pub use player::Playback;
pub(crate) use player::RepeatMode;
