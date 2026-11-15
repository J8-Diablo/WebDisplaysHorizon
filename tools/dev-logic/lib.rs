//! The game-free modules of the mod, for `cargo test` (see Cargo.toml). Each is the mod's
//! own file: a test that passes here passes on what the game runs.
/// How a place colours a sound; its worker thread (`acoustics/worker.rs`) needs the game.
pub mod acoustics {
    #[path = "../../../native/api/src/acoustics/materials.rs"]
    pub mod materials;
    #[path = "../../../native/api/src/acoustics/solve.rs"]
    pub mod solve;
    #[path = "../../../native/api/src/acoustics/trace.rs"]
    pub mod trace;
}
#[path = "../../native/api/src/model/mod.rs"]
pub mod model;
#[path = "../../native/api/src/net/mod.rs"]
pub mod net;
// The browser's side is HCEF's, tested there: its game-free modules stand in for the mod.
pub use hcef::{ipc, proto};

pub mod client {
    pub use hcef::browsers;
    #[path = "../../../native/api/src/client/gui/mod.rs"]
    pub mod gui;
    #[path = "../../../native/api/src/client/pad.rs"]
    pub mod pad;
    #[path = "../../../native/api/src/client/pointer.rs"]
    pub mod pointer;
    #[path = "../../../native/api/src/client/settings.rs"]
    pub mod settings;
    #[path = "../../../native/api/src/client/sound.rs"]
    pub mod sound;
}

/// The renderer's arithmetic; its GPU half (`render/gpu.rs`) needs the game.
pub mod render {
    #[path = "../../../native/api/src/render/geometry.rs"]
    pub mod geometry;
    #[path = "../../../native/api/src/render/glow.rs"]
    pub mod glow;
    #[path = "../../../native/api/src/render/poly.rs"]
    pub mod poly;
    #[path = "../../../native/api/src/render/screens.rs"]
    pub mod screens;
}
