//! Viper is a native music library browser and player.
//!
//! The crate exposes the application entry point and feature-level library APIs.
pub mod app;
pub mod features;
pub(crate) mod platform;
pub(crate) mod shared;
pub(crate) mod workbench;

pub use app::run;
pub use platform::instance::show_running as show_running_instance;
pub use platform::updates::restart_target;
