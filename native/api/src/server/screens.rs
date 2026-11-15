//! Screens on the server: made by a click on a wall of screen blocks, kept in the block
//! entity of one of them, gone when a block of theirs is broken.
//!
//! A screen is the screen blocks joined to the clicked one on the clicked face. Its data
//! lives in one `screen_core` block of that group, which up to six screens can share (one
//! for each face). The first screen made on a group turns the clicked block into the core.
use super::index;
use crate::ids::{self, CORE, JOINS, MAX_RESOLUTION, MAX_SIZE, POLY_CORE, SCREEN};
use crate::lang;
use crate::model::phrase::Phrase;
use crate::model::rights::{self, Player};
use crate::model::screen::Screen;
use crate::model::shape::{Component, nearest_connected};
use crate::model::side::{Pos, Side};
use crate::model::words;
use crate::model::{resolution, url};
use crate::tags;
use horizon_client::mod_api::{Flow, Host};

/// The screens of the core at `pos`; none when there is no core there.
pub fn at(host: &dyn Host, pos: Pos) -> Vec<Screen> {
    host.mod_block_entity(pos).map(|entity| tags::screens(&entity.nbt)).unwrap_or_default()
}

/// Changes the screens of the core at `pos`, saves them and sends them to the players who
/// see it. False when there is no core there.
pub fn change(host: &mut dyn Host, pos: Pos, change: impl FnOnce(&mut Vec<Screen>)) -> bool {
    let mut change = Some(change);
    let mut after = Vec::new();
    let changed = host.change_mod_block_entity(pos, &mut |entity| {
        let mut screens = tags::screens(&entity.nbt);
        if let Some(change) = change.take() {
            change(&mut screens);
        }
        tags::set_screens(&mut entity.nbt, &screens);
        after = screens;
    });
    if changed {
        host.block_entity_sync(pos);
        // The world's list of screens follows: a name, a size or a link may have changed.
        let traced = host.block(pos) == POLY_CORE;
        index::refresh(host, pos, &after, traced);
    }
    changed
}

/// The screen the screen block at `pos` is part of on `side`: its core, and the screen.
/// None when those blocks are no screen on that face (yet).
pub fn find(host: &dyn Host, pos: Pos, side: Side) -> Option<(Pos, Screen)> {
    let world = |pos: Pos| ids::is_screen(&host.block(pos));
    // A core can hold screens of other faces only: those are other screens.
    let holds = |pos: Pos| host.block(pos) == CORE && at(host, pos).iter().any(|screen| screen.side == side);
    let core = nearest_connected(&world, pos, side, MAX_SIZE, holds)?;
    let screen = at(host, core).into_iter().find(|screen| screen.side == side)?;
    Some((core, screen))
}

/// The face a click's direction names.
pub fn side_of(face: Pos) -> Option<Side> {
    Side::ALL.into_iter().find(|side| side.forward() == face)
}

/// The command and the tools were aimed at nothing they know.
const LOOK: Phrase = Phrase::new("webdisplays.h.screen.look", "Look at a screen.");

/// A line in a player's chat, as the Java mod's messages are: prefixed with the mod's name.
/// Each client reads it in its own language.
fn tell(host: &mut dyn Host, player: u128, line: Phrase) {
    host.tell(player, lang::chat(&line));
}

/// Swaps the block at `pos` for `block`, keeping how it joins its neighbours.
fn swap(host: &mut dyn Host, pos: Pos, block: &str) -> bool {
    let joined: Vec<&str> = JOINS.into_iter().filter(|side| host.block_property(pos, side).as_deref() == Some("true")).collect();
    let Some(state) = host.default_state(block) else {
        return false;
    };
    let dimension = host.dimension();
    // With neighbour updates: the blocks around redraw their frames.
    if !host.set_block(dimension, pos, state, 3) {
        return false;
    }
    for side in joined {
        host.set_block_property_with(pos, side, "true", 2);
    }
    true
}

/// A player right-clicked `pos` on `face` with an empty hand: makes the screen there when
/// there is none. Clicks on a screen that exists are the client's to send (it knows where
/// on the face the player aimed; the server is only told which block).
pub fn right_click(host: &mut dyn Host, pos: Pos, face: Pos) -> Flow {
    if !ids::is_screen(&host.block(pos)) {
        return Flow::Continue;
    }
    // Something in hand is placed or used as it would be anywhere.
    if host.held().is_some_and(|(item, count, _)| count > 0 && item != "minecraft:air") {
        return Flow::Continue;
    }
    let (Some(player), Some(side)) = (host.acting_player(), side_of(face)) else {
        return Flow::Continue;
    };
    if find(host, pos, side).is_some() {
        return Flow::Cancel;
    }
    let (core, component) = {
        let world = |pos: Pos| ids::is_screen(&host.block(pos));
        // Any core of these blocks can hold one more screen.
        let core = nearest_connected(&world, pos, side, MAX_SIZE, |pos| host.block(pos) == CORE);
        (core, Component::find(&world, pos, side, MAX_SIZE))
    };
    let Some(component) = component else {
        return Flow::Continue;
    };
    let size = component.size();
    if size[0] < 2 && size[1] < 2 {
        tell(host, player.uuid, Phrase::new("webdisplays.h.screen.too_small", "A screen needs at least two blocks."));
        return Flow::Cancel;
    }
    if !component.fits(MAX_SIZE) {
        tell(host, player.uuid, Phrase::new("webdisplays.h.screen.too_big", "A screen can be %sx%s blocks at most.").with(MAX_SIZE[0]).with(MAX_SIZE[1]));
        return Flow::Cancel;
    }
    let core = match core {
        Some(core) => core,
        None => {
            if !swap(host, pos, CORE) {
                return Flow::Cancel;
            }
            pos
        }
    };
    let mut made = Screen::new(side, size, Player { uuid: player.uuid, name: player.name });
    made.resolution = resolution::default_resolution(size, MAX_RESOLUTION);
    change(host, core, |screens| screens.push(made));
    Flow::Cancel
}

/// A player placed the screen block at `pos`: a screen it now joins grows to take it in.
/// A picture size that was the one chosen for the old size follows; one the player set
/// stays as they set it.
pub fn placed(host: &mut dyn Host, pos: Pos) {
    if !ids::is_screen(&host.block(pos)) {
        return;
    }
    for side in Side::ALL {
        let Some((core, screen)) = find(host, pos, side) else {
            continue;
        };
        let grown = {
            let world = |pos: Pos| ids::is_screen(&host.block(pos));
            Component::find(&world, core, side, MAX_SIZE).filter(|component| component.fits(MAX_SIZE)).map(|component| component.size())
        };
        let Some(size) = grown.filter(|size| *size != screen.size) else {
            continue;
        };
        let automatic = screen.resolution == resolution::default_resolution(screen.size, MAX_RESOLUTION);
        change(host, core, |screens| {
            if let Some(screen) = screens.iter_mut().find(|screen| screen.side == side) {
                screen.size = size;
                if automatic {
                    screen.resolution = resolution::default_resolution(size, MAX_RESOLUTION);
                }
            }
        });
    }
}

/// A player is breaking the screen block at `pos`: every screen it was part of goes, and a
/// core left with no screen becomes a plain screen block again.
pub fn breaking(host: &mut dyn Host, pos: Pos) {
    if !ids::is_screen(&host.block(pos)) {
        return;
    }
    // A core that is broken takes every screen it held with it.
    if host.block(pos) == CORE {
        index::refresh(host, pos, &[], false);
    }
    for side in Side::ALL {
        let cores: Vec<Pos> = {
            let world = |pos: Pos| ids::is_screen(&host.block(pos));
            let Some(component) = Component::find(&world, pos, side, MAX_SIZE) else {
                continue;
            };
            component.blocks().filter(|block| host.block(*block) == CORE).collect()
        };
        for core in cores {
            let mut left = usize::MAX;
            change(host, core, |screens| {
                screens.retain(|screen| screen.side != side);
                left = screens.len();
            });
            // The block being broken takes its block entity with it; another core that
            // holds nothing any more is a plain block again.
            if left == 0 && core != pos {
                swap(host, core, SCREEN);
            }
        }
    }
}

/// The screen a player looks at, within reach: its core, its side, and the screen.
pub fn looked_at(host: &dyn Host) -> Result<(Pos, Screen), Phrase> {
    let hit = host.raycast(8.0).ok_or(LOOK)?;
    let pos = [hit.block_x, hit.block_y, hit.block_z];
    // A traced screen is found through any of its invisible blocks.
    if let Some(master) = super::poly::master_of(host, pos) {
        let screen = at(host, master).into_iter().next().ok_or(words::LOST_DATA)?;
        return Ok((master, screen));
    }
    if !ids::is_screen(&host.block(pos)) {
        return Err(LOOK);
    }
    // The face the look landed on: the one whose plane the hit point is in.
    let point = [hit.x, hit.y, hit.z];
    let side = Side::ALL
        .into_iter()
        .find(|side| {
            let forward = side.forward();
            (0..3).any(|i| forward[i] != 0 && (point[i] - f64::from(pos[i]) - f64::from(forward[i].max(0))).abs() < 1e-3)
        })
        .ok_or(LOOK)?;
    find(host, pos, side).ok_or(words::NOT_A_SCREEN_YET)
}

/// `player` asks for the screen they look at to show `address`. Gives the line to tell
/// them, done or refused.
pub fn set_url(host: &mut dyn Host, player: u128, address: &str) -> Result<Phrase, Phrase> {
    let (core, looked) = looked_at(host)?;
    if !looked.access.allows(player, rights::CHANGE_URL) {
        return Err(words::MAY_NOT_CHANGE_URL);
    }
    let address = url::clean(address).map_err(|refused| match refused {
        url::Refused::Empty => Phrase::new("webdisplays.h.command.which_address", "Which address?"),
        url::Refused::TooLong => words::ADDRESS_TOO_LONG,
        url::Refused::Malformed => words::BAD_ADDRESS,
    })?;
    let side = looked.side;
    let set = address.clone();
    change(host, core, |screens| {
        if let Some(screen) = screens.iter_mut().find(|screen| screen.side == side) {
            screen.url = set;
        }
    });
    Ok(Phrase::new("webdisplays.h.screen.shows", "The screen now shows %s").with(address))
}
