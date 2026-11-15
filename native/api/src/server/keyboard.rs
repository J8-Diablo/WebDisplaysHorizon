//! The keyboard is two blocks: the item places the left half and this places the right one
//! beside it, and breaking either takes both.
//!
//! The content mod cannot say "two blocks": its item places `kb_left` and nothing more.
use super::click::{forget_speaker, link_at, other_half, right_half_way};
use crate::ids::{KEYBOARD, KEYBOARD_LEFT, KEYBOARD_RIGHT};
use crate::lang;
use crate::model::phrase::Phrase;
use crate::model::side::{Pos, offset};
use crate::model::speaker::Kind;
use horizon_client::mod_api::Host;

const AIR: &str = "minecraft:air";

fn is_free(host: &dyn Host, pos: Pos) -> bool {
    matches!(host.block(pos).as_str(), AIR | "minecraft:cave_air" | "minecraft:void_air")
}

fn clear(host: &mut dyn Host, pos: Pos) {
    if let Some(air) = host.default_state(AIR) {
        let dimension = host.dimension();
        host.set_block(dimension, pos, air, 3);
    }
}

/// A player placed the block now at `pos`. A keyboard's left half gets its right half; with
/// no room for it, the half is taken back.
pub fn placed(host: &mut dyn Host, pos: Pos) {
    if host.block(pos) != KEYBOARD_LEFT {
        return;
    }
    let facing = host.block_property(pos, "facing");
    let beside = facing.as_deref().and_then(right_half_way).map(|way| offset(pos, way, 1));
    let player = host.acting_player();
    let Some(beside) = beside.filter(|beside| is_free(host, *beside)) else {
        clear(host, pos);
        if let Some(player) = player {
            // In creative the stack was not used up.
            if player.game_mode != 1 {
                host.give_items(KEYBOARD, 1);
            }
            host.action_bar(player.uuid, lang::text(&Phrase::new("webdisplays.h.keyboard.no_room", "A keyboard needs a free block to its right.")));
        }
        return;
    };
    let (Some(state), Some(facing)) = (host.default_state(KEYBOARD_RIGHT), facing) else {
        return;
    };
    let dimension = host.dimension();
    if host.set_block(dimension, beside, state, 3) {
        host.set_block_property_with(beside, "facing", &facing, 2);
    }
}

/// A player is breaking the block `block` at `pos`: a keyboard half takes its other half
/// with it, and a speaker leaves its screen's list.
pub fn breaking(host: &mut dyn Host, pos: Pos, block: &str) {
    if Kind::of_block(block).is_some() {
        if let Some(link) = link_at(host, pos) {
            forget_speaker(host, link, pos);
        }
        return;
    }
    if block != KEYBOARD_LEFT && block != KEYBOARD_RIGHT {
        return;
    }
    let Some(other) = other_half(host, pos, block) else {
        return;
    };
    clear(host, other);
    // Only the left half drops the keyboard: broken by its right half, the item is handed
    // over instead (in creative nothing drops either way).
    if block == KEYBOARD_RIGHT && host.acting_player().is_some_and(|player| player.game_mode != 1) {
        host.give_items(KEYBOARD, 1);
    }
}
