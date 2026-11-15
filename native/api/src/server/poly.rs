//! Traced screens on the server: a player marks corners with the pen, and when the outline
//! closes the screen is built: invisible blocks where the sheet passes, one of them (the
//! master) keeping the outline and the screen's data.
//!
//! The client says which corner the player aimed at (the server is only told which block
//! is clicked, not where on it); the server keeps each player's unfinished outline, checks
//! that they stand by the corner, and does all the building.
use super::State;
use super::index;
use crate::ids::{MAX_RESOLUTION, POLY, POLY_CORE};
use crate::lang;
use crate::model::phrase::Phrase;
use crate::model::poly::{MIN_CURVE_RADIUS, Outline, Surface, plan_build, plan_demolish, plan_refit};
use crate::model::rights;
use crate::model::polysave;
use crate::model::resolution;
use crate::model::rights::Player;
use crate::model::screen::Screen;
use crate::model::side::{Pos, Side};
use crate::model::words;
use crate::net::msg::{CHANNEL, Reshape, ToClient};
use crate::tags;
use horizon_client::mod_api::{Host, NativeServerPlayer};

const AIR: &str = "minecraft:air";
/// How far a player may stand from a corner they mark, in blocks.
const REACH: f64 = 10.0;
/// The face a traced screen is filed under in its master: it has no face of its own, but
/// everything that names a screen names one.
pub const FACE: Side = Side::South;

fn is_free(host: &dyn Host, pos: Pos) -> bool {
    matches!(host.block(pos).as_str(), AIR | "minecraft:cave_air" | "minecraft:void_air")
}

/// Tells `player`'s client the outline they have so far, to draw it.
fn show(host: &mut dyn Host, player: u128, outline: &Outline) {
    let _ = host.net_send_to_player(player, CHANNEL, &ToClient::Pen { points: outline.points().to_vec() }.encode());
}

/// `player` marked `corner` with the pen (None: they wiped their outline).
pub fn pen(host: &mut dyn Host, state: &mut State, players: &[NativeServerPlayer], player: u128, corner: Option<Pos>) {
    let Some(corner) = corner else {
        state.pens.remove(&player);
        show(host, player, &Outline::new());
        return;
    };
    // In a world nobody else can join the list is empty, and its one player is believed.
    let who = players.iter().find(|other| other.uuid == player);
    if !players.is_empty() && !who.is_some_and(|who| (0..3).all(|i| (who.position[i] - f64::from(corner[i])).abs() <= REACH)) {
        return;
    }
    let outline = state.pens.entry(player).or_default();
    if outline.would_close(corner) {
        let traced = state.pens.remove(&player).unwrap_or_default();
        let owner = who.map(|who| Player { uuid: who.uuid, name: who.name.clone() });
        let said = build(host, player, owner, &traced);
        host.action_bar(player, lang::text(&said));
        show(host, player, &Outline::new());
        return;
    }
    // A corner marked already is taken away again: the way to correct a slip. (The first
    // corner closes the outline instead, once there are enough to close.)
    let said = if let Some(index) = outline.index_of(corner) {
        outline.remove_at(index);
        Phrase::new("webdisplays.h.poly.corner_removed", "Corner removed: %s left.").with(outline.len())
    } else if outline.is_full() {
        Phrase::new("webdisplays.h.poly.full", "That outline has as many corners as it can: click its first corner to close it.")
    } else if outline.add(corner) {
        Phrase::new("webdisplays.h.poly.corners", "%s corner(s). Click the first one to close; click a corner again to remove it; sneak-click wipes all.").with(outline.len())
    } else {
        Phrase::new("webdisplays.h.poly.cannot_add", "That corner cannot be added.")
    };
    let outline = outline.clone();
    host.action_bar(player, lang::text(&said));
    show(host, player, &outline);
}

/// Builds the screen `outline` traces. Gives the line to tell the player.
fn build(host: &mut dyn Host, player: u128, owner: Option<Player>, outline: &Outline) -> Phrase {
    let plan = {
        let world = |pos: Pos| is_free(host, pos);
        plan_build(outline, &world)
    };
    let plan = match plan {
        Ok(plan) => plan,
        Err(refused) => return Phrase::new("webdisplays.h.poly.refused", "That outline cannot be a screen (%s).").with(refused.reason()),
    };
    let dimension = host.dimension();
    for cell in &plan.cells {
        let block = if *cell == plan.master { POLY_CORE } else { POLY };
        let Some(state) = host.default_state(block) else {
            return Phrase::new("webdisplays.h.poly.blocks_missing", "The screen's blocks are missing from the game.");
        };
        host.set_block(&dimension, *cell, state, 3);
    }
    for cell in plan.cells.iter().filter(|cell| **cell != plan.master) {
        host.change_mod_block_entity(*cell, &mut |entity| {
            let mut map = tags::from_compound(&entity.nbt);
            polysave::write_master(&mut map, plan.master);
            entity.nbt = tags::to_compound(&map);
        });
        host.block_entity_sync(*cell);
    }
    // Nobody to own it in a world whose player list is empty: its one player does.
    let owner = owner.unwrap_or(Player { uuid: player, name: String::new() });
    let mut screen = Screen::new(FACE, plan.size, owner);
    screen.resolution = resolution::default_resolution(plan.size, MAX_RESOLUTION);
    let saved = host.change_mod_block_entity(plan.master, &mut |entity| {
        tags::set_screens(&mut entity.nbt, std::slice::from_ref(&screen));
        let mut map = tags::from_compound(&entity.nbt);
        polysave::write_outline(&mut map, outline);
        entity.nbt = tags::to_compound(&map);
    });
    if !saved {
        return Phrase::new("webdisplays.h.poly.cannot_build", "The screen could not be built here.");
    }
    host.block_entity_sync(plan.master);
    index::refresh(host, plan.master, std::slice::from_ref(&screen), true);
    Phrase::new("webdisplays.h.poly.traced", "Screen traced: %sx%s blocks. Right-click it to choose a page.").with(plan.size[0]).with(plan.size[1])
}

/// The master of the traced screen the block at `pos` belongs to.
pub fn master_of(host: &dyn Host, pos: Pos) -> Option<Pos> {
    match host.block(pos).as_str() {
        POLY_CORE => Some(pos),
        POLY => polysave::read_master(&tags::from_compound(&host.mod_block_entity(pos)?.nbt)),
        _ => None,
    }
}

/// `player` changes the outline of the traced screen they look at (rounds its corners,
/// turns it to face the other way): its blocks move to where the sheet now passes. Gives
/// the line to tell them.
pub fn reshape(host: &mut dyn Host, player: u128, change: impl FnOnce(&mut Outline) -> Phrase) -> Result<Phrase, Phrase> {
    const LOOK: Phrase = Phrase::new("webdisplays.h.poly.look", "Look at a traced screen.");
    let looked = host.raycast(16.0).ok_or(LOOK)?;
    let master = master_of(host, [looked.block_x, looked.block_y, looked.block_z]).ok_or(LOOK)?;
    reshape_at(host, player, master, change)
}

/// Changes `outline` as asked, and gives the line that says so.
pub fn apply(outline: &mut Outline, asked: Reshape) -> Phrase {
    match asked {
        Reshape::Curved(true) => {
            outline.curved = true;
            Phrase::new("webdisplays.h.poly.rounded", "The screen's corners are now rounded.")
        }
        Reshape::Curved(false) => {
            outline.curved = false;
            Phrase::new("webdisplays.h.poly.sharp", "The screen's corners are now sharp.")
        }
        Reshape::Flipped(flipped) => {
            outline.flipped = flipped;
            Phrase::new("webdisplays.h.poly.flipped", "The screen now faces the other way.")
        }
        Reshape::Radius(radius) => {
            // What is not a number changes nothing.
            // No further than the widest radius that still changes this sheet.
            if radius.is_finite() {
                outline.curve_radius = radius.clamp(MIN_CURVE_RADIUS, outline.widest_radius().max(MIN_CURVE_RADIUS));
            }
            Phrase::new("webdisplays.h.poly.radius", "The screen's corners now have a radius of %s blocks.").with(format!("{:.1}", outline.curve_radius))
        }
    }
}

/// The same for the traced screen whose master is at `master`: its settings window names
/// it, where the command takes the one looked at.
pub fn reshape_at(host: &mut dyn Host, player: u128, master: Pos, change: impl FnOnce(&mut Outline) -> Phrase) -> Result<Phrase, Phrase> {
    let entity = host.mod_block_entity(master).ok_or(words::LOST_DATA)?;
    let allowed = tags::screens(&entity.nbt).first().is_some_and(|screen| screen.access.allows(player, rights::MODIFY_SCREEN));
    if !allowed {
        return Err(Phrase::new("webdisplays.h.poly.not_allowed", "The owner of this screen does not let you change it."));
    }
    let mut outline = polysave::read_outline(&tags::from_compound(&entity.nbt)).ok_or(Phrase::new("webdisplays.h.poly.lost_outline", "That screen has lost its outline."))?;
    let said = change(&mut outline);
    let surface = Surface::of(&outline).ok_or(Phrase::new("webdisplays.h.poly.no_surface", "That outline would be no surface."))?;
    let plan = {
        let world = |pos: Pos| is_free(host, pos);
        let owned = |cell: Pos| cell == master || master_of(host, cell) == Some(master);
        plan_refit(&surface, master, &world, owned)
    };
    let dimension = host.dimension();
    if let Some(air) = host.default_state(AIR) {
        for cell in &plan.remove {
            host.set_block(&dimension, *cell, air, 3);
        }
    }
    if let Some(state) = host.default_state(POLY) {
        for cell in &plan.add {
            host.set_block(&dimension, *cell, state, 3);
            host.change_mod_block_entity(*cell, &mut |entity| {
                let mut map = tags::from_compound(&entity.nbt);
                polysave::write_master(&mut map, master);
                entity.nbt = tags::to_compound(&map);
            });
            host.block_entity_sync(*cell);
        }
    }
    host.change_mod_block_entity(master, &mut |entity| {
        let mut map = tags::from_compound(&entity.nbt);
        polysave::write_outline(&mut map, &outline);
        entity.nbt = tags::to_compound(&map);
    });
    host.block_entity_sync(master);
    Ok(said)
}

/// A player is breaking the block at `pos` of a traced screen: the whole screen goes, and
/// only that screen (the Java mod took its neighbours' blocks with it).
pub fn breaking(host: &mut dyn Host, pos: Pos) {
    let Some(master) = master_of(host, pos) else {
        return;
    };
    let outline = host.mod_block_entity(master).and_then(|entity| polysave::read_outline(&tags::from_compound(&entity.nbt)));
    let cells = match outline.as_ref().and_then(Surface::of) {
        Some(surface) => {
            let owned = |cell: Pos| cell == master || master_of(host, cell) == Some(master);
            plan_demolish(&surface, master, Some(pos), owned)
        }
        // A master that lost its outline: at least it goes.
        None => vec![master],
    };
    index::refresh(host, master, &[], true);
    let Some(air) = host.default_state(AIR) else {
        return;
    };
    let dimension = host.dimension();
    for cell in cells.into_iter().filter(|cell| *cell != pos) {
        host.set_block(&dimension, cell, air, 3);
    }
}
