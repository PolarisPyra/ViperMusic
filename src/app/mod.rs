//! Native application state and startup entry point.
mod application;
mod bootstrap;
mod library_loader;
mod library_watcher;
mod runtime;
mod shortcuts;

pub use application::ViperApp;
pub use bootstrap::run;
