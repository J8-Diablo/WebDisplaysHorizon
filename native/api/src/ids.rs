//! The names the two halves of the mod agree on (the content mod's blocks and items, the
//! native mod's channels) and the limits a server sets. The limits are constants until the
//! server's configuration file exists.

/// The native mod's id (`horizon-mixin.toml`): its settings, data folder and channels.
pub const ID: &str = "webdisplays_native";

/// The block players place.
pub const SCREEN: &str = "webdisplays:screen";
/// The same block holding a screen's data: one block of a screen is swapped for it when
/// the screen is made (a block entity cannot be given to some blocks of an id only).
pub const CORE: &str = "webdisplays:screen_core";

/// The two halves of a keyboard. The left one holds the link to the screen it types on.
pub const KEYBOARD_LEFT: &str = "webdisplays:kb_left";
pub const KEYBOARD_RIGHT: &str = "webdisplays:kb_right";
/// The remote control: changes a screen's address from afar.
pub const REMOTE: &str = "webdisplays:rctrl";
/// The redstone controller: changes a screen's address when its signal changes.
pub const REDSTONE: &str = "webdisplays:redctrl";

/// The invisible blocks a traced screen stands in, and the one of them that keeps its
/// outline and its data.
pub const POLY: &str = "webdisplays:polyscreen";
pub const POLY_CORE: &str = "webdisplays:polyscreen_core";
/// The pen that traces a screen's outline, corner by corner.
pub const PEN: &str = "webdisplays:screenpen";

/// Whether `block` is one of a traced screen's.
pub fn is_poly(block: &str) -> bool {
    block == POLY || block == POLY_CORE
}

/// The minePad: a browser in the hand.
pub const MINEPAD: &str = "webdisplays:minepad";

/// The three speakers: the treble, the bass, everything.
pub const SPEAKERS: [&str; 3] = ["webdisplays:speaker_tweeter", "webdisplays:speaker_bass", "webdisplays:speaker_mix"];
/// The item that places a keyboard.
pub const KEYBOARD: &str = "webdisplays:keyboard";

/// The linking tool: click a screen, then the peripheral that should work it.
pub const LINKER: &str = "webdisplays:linker";
/// Makes whoever uses it on a screen its owner. Creative only.
pub const OWNER_THIEF: &str = "webdisplays:ownerthief";
/// The screen configurator.
pub const CONFIGURATOR: &str = "webdisplays:screencfg";
/// The laser pointer: clicks a screen from afar, when the screen has the laser upgrade.
pub const LASER_POINTER: &str = "webdisplays:laserpointer";
/// The upgrade that lets a laser pointer work a screen.
pub use crate::model::screen::UPGRADE_LASER_MOUSE as UPGRADE_LASER;

/// Whether `block` is something a screen can be linked to with the linking tool.
pub fn is_peripheral(block: &str) -> bool {
    matches!(block, KEYBOARD_LEFT | KEYBOARD_RIGHT | REMOTE | REDSTONE) || SPEAKERS.contains(&block)
}

/// Whether `block` keeps a link to a screen in a block entity of its own. The keyboard's
/// right half keeps none: it is its left half's.
pub fn has_link(block: &str) -> bool {
    matches!(block, KEYBOARD_LEFT | REMOTE | REDSTONE) || SPEAKERS.contains(&block)
}

/// The six properties a screen block joins its neighbours by, which the game keeps.
pub const JOINS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

/// The largest screen, in blocks.
pub const MAX_SIZE: [u32; 2] = [16, 16];
/// The largest picture, in pixels.
pub const MAX_RESOLUTION: [u32; 2] = [1920, 1080];
/// How far a player may be from a screen to act on it, in blocks. Further than the hand
/// reaches, as a laser pointer works from afar.
pub const REACH: f64 = 64.0;

/// Whether `block` is part of a screen.
pub fn is_screen(block: &str) -> bool {
    block == SCREEN || block == CORE
}
