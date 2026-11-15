//! The player's side: which browsers to keep, what to draw, what the keyboard and mouse do.
pub mod gui;
pub mod pad;
pub mod paint;
pub mod pointer;
pub mod settings;
pub mod sound;
pub mod world;

/// Which browsers to keep: HCEF's.
pub use hcef::browsers;
