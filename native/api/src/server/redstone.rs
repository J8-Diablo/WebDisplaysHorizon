//! The redstone controller on the server: it watches the signal reaching it, and changes
//! its screen's address when the signal comes on or goes off (`model::redstone`).
use super::screens::change;
use crate::ids::REDSTONE;
use crate::lang;
use crate::model::phrase::Phrase;
use crate::model::redstone::Controller;
use crate::model::side::Pos;
use crate::model::url;
use crate::tags;
use horizon_client::mod_api::{Host, ModBlockEntity, NativeServerPlayer};

/// How far a player may stand from a controller to set it up, in blocks.
const REACH: f64 = 8.0;

/// Once a tick for each controller: looks at the signal.
pub fn tick(host: &mut dyn Host, pos: Pos, entity: &mut ModBlockEntity) {
    let dimension = host.dimension();
    let powered = host.neighbor_signal(&dimension, pos) > 0;
    let mut map = tags::from_compound(&entity.nbt);
    let mut controller = Controller::read(&map);
    if controller.powered == powered {
        return;
    }
    let shown = controller.signal(powered).map(|(link, address)| (link, address.to_owned()));
    controller.write(&mut map);
    entity.nbt = tags::to_compound(&map);
    if let Some((link, address)) = shown {
        change(host, link.core, |screens| {
            if let Some(screen) = screens.iter_mut().find(|screen| screen.side == link.side) {
                screen.url = address;
            }
        });
    }
}

/// `player` sets the two addresses of the controller at `pos`.
pub fn set(host: &mut dyn Host, players: &[NativeServerPlayer], player: u128, pos: Pos, rising: &str, falling: &str) {
    if host.block(pos) != REDSTONE {
        return;
    }
    // In a world nobody else can join the list is empty, and its one player is believed.
    let near = players.is_empty()
        || players.iter().any(|other| other.uuid == player && (0..3).all(|i| (other.position[i] - f64::from(pos[i]) - 0.5).abs() <= REACH));
    if !near {
        return;
    }
    // Empty means "nothing to show for that change"; anything else must be an address.
    let cleaned = |address: &str| if address.trim().is_empty() { Ok(String::new()) } else { url::clean(address) };
    let (Ok(rising), Ok(falling)) = (cleaned(rising), cleaned(falling)) else {
        return;
    };
    let saved = host.change_mod_block_entity(pos, &mut |entity| {
        let mut map = tags::from_compound(&entity.nbt);
        let mut controller = Controller::read(&map);
        controller.rising = rising.clone();
        controller.falling = falling.clone();
        controller.write(&mut map);
        entity.nbt = tags::to_compound(&map);
    });
    if saved {
        host.tell(player, lang::chat(&Phrase::new("webdisplays.h.redstone.set", "Redstone controller set.")));
    }
}
