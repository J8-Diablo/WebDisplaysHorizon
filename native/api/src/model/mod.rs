//! What a screen is, with no game around it: the rules the server enforces and the client
//! draws by. Everything here is plain data and arithmetic, tested by `tools/dev-logic`
//! without building Horizon.
pub mod configure;
pub mod groups;
pub mod hit;
pub mod home;
pub mod index;
pub mod keys;
pub mod letterbox;
pub mod link;
pub mod nbt;
pub mod poly;
pub mod polysave;
pub mod redstone;
pub mod resolution;
pub mod rights;
pub mod screen;
pub mod shape;
pub mod shared;
pub mod side;
pub mod speaker;
pub mod text;
pub mod url;
pub mod words;

/// A sentence shown to a player: HCEF's type, so that its sentences and ours are one kind.
pub use hcef::phrase;
