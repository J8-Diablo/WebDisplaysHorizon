//! Lines for whoever is working on the mod: what it did, said in the game's console when
//! the environment variable `WD_DEBUG` is set, and nothing otherwise.
use std::sync::OnceLock;

/// Says `line()` on the console when `WD_DEBUG` is set. The line is only made then.
pub fn say(line: impl FnOnce() -> String) {
    static ON: OnceLock<bool> = OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("WD_DEBUG").is_some()) {
        eprintln!("[webdisplays] {}", line());
    }
}
