//! How the place a sound is in colours it: the blocks between it and the listener, and the
//! walls around. Worked out here from the blocks; hcef-host plays the result (`proto::audio`).
pub mod materials;
pub mod solve;
pub mod trace;
pub mod worker;
