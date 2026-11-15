//! What the mod says between client and server, and how it gets across. The bytes themselves
//! are written and read with `proto::wire`, which the browser process shares.
pub mod chunk;
pub mod msg;
