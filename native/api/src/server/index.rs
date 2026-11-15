//! The list of the world's screens on the server (`model::index`): brought up to date where
//! screens change, kept in a file of the world, read to players who ask.
//!
//! Screens change in one place (`screens::change`), which has no room to carry the
//! server's state around, so the list is this module's own: one for the process, read
//! from the world's folder when first needed and dropped when the server stops.
use crate::model::index::{Entry, Index};
use crate::model::screen::Screen;
use crate::model::side::Pos;
use horizon_client::mod_api::Host;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

static INDEX: Mutex<Option<Index>> = Mutex::new(None);
/// The cores met since the server started, by dimension: each is put on the list once
/// when its chunk is loaded, then only when it changes.
static MET: Mutex<Option<HashSet<(&'static str, Pos)>>> = Mutex::new(None);

fn path(host: &dyn Host) -> Option<PathBuf> {
    Some(host.world_dir()?.join("webdisplays").join("screens.bin"))
}

/// Runs `with` on the list, read from the world's file the first time.
fn with<T>(host: &dyn Host, with: impl FnOnce(&mut Index) -> T) -> Option<T> {
    let mut slot = INDEX.lock().ok()?;
    let index = slot.get_or_insert_with(|| {
        let read = path(host).and_then(|path| std::fs::read(path).ok());
        read.and_then(|bytes| Index::decode(&bytes).ok()).unwrap_or_default()
    });
    Some(with(index))
}

fn save(host: &dyn Host, index: &Index) {
    let Some(path) = path(host) else {
        return;
    };
    let fresh = path.with_extension("new");
    let written = path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&fresh, index.encode())).and_then(|_| std::fs::rename(&fresh, &path));
    if let Err(error) = written {
        eprintln!("[webdisplays] the list of screens could not be saved to {}: {error}", path.display());
    }
}

/// The core at `core` now holds `screens` (none: it is gone). `traced` for a traced
/// screen's master.
pub fn refresh(host: &dyn Host, core: Pos, screens: &[Screen], traced: bool) {
    let dimension = host.dimension().to_string();
    let entries: Vec<Entry> = screens.iter().map(|screen| Entry::of(core, &dimension, screen, traced)).collect();
    with(host, |index| {
        if index.set_core(&dimension, core, entries) {
            save(host, index);
        }
    });
}

/// The core at `core` is ticking, so it is loaded: a screen made before the list existed,
/// or changed while the list's file was away, is put on it the first time it is met.
pub fn met(host: &dyn Host, core: Pos, screens: impl FnOnce() -> Vec<Screen>, traced: bool) {
    let first = MET.lock().is_ok_and(|mut met| met.get_or_insert_with(HashSet::new).insert((host.dimension(), core)));
    if first {
        refresh(host, core, &screens(), traced);
    }
}

/// One page of the list, and how many there are.
pub fn page(host: &dyn Host, page: usize) -> (usize, Vec<Entry>) {
    with(host, |index| {
        let (pages, entries) = index.page(page);
        (pages, entries.to_vec())
    })
    .unwrap_or((1, Vec::new()))
}

/// The server is stopping: the next world has its own list.
pub fn forget() {
    if let Ok(mut slot) = INDEX.lock() {
        *slot = None;
    }
    if let Ok(mut met) = MET.lock() {
        *met = None;
    }
}
