//! A right click on one of the mod's blocks: what it does depends on the block and on what
//! the player holds.
//!
//! On a screen: an empty hand makes the screen (`screens::right_click`); the linking tool
//! picks it, to be linked to the peripheral clicked next; the owner thief takes it; the
//! laser upgrade goes into it. On a peripheral: the linking tool links it to the screen
//! picked before, anything else uses it. Clicks on a screen's page never come here: the
//! client sends them itself (`net::msg`), knowing where on the face the player aimed.
use super::poly;
use super::screens::{self, change, side_of};
use super::State;
use crate::ids::{self, CONFIGURATOR, KEYBOARD_LEFT, KEYBOARD_RIGHT, LINKER, OWNER_THIEF, REDSTONE, REMOTE, UPGRADE_LASER};
use crate::lang;
use crate::model::phrase::Phrase;
use crate::model::redstone::Controller;
use crate::model::link::Link;
use crate::model::rights::{self, Player};
use crate::model::side::{Pos, offset};
use crate::model::speaker::{self, Kind, Speaker};
use crate::model::words;
use crate::net::msg::{CHANNEL, Target, ToClient};
use crate::tags;
use horizon_client::mod_api::{Flow, Host, NativeServerPlayer};

/// A screen's owner gave the player no right to do what they tried through a peripheral.
const OWNER_REFUSES: Phrase = Phrase::new("webdisplays.h.screen.not_allowed", "The owner of that screen does not let you do this.");

/// Says `line` in `player`'s chat, each client in its own language.
fn tell(host: &mut dyn Host, player: u128, line: Phrase) {
    host.tell(player, lang::chat(&line));
}

/// The link the peripheral at `pos` keeps; None when it was never linked.
pub fn link_at(host: &dyn Host, pos: Pos) -> Option<Link> {
    Link::read(&tags::from_compound(&host.mod_block_entity(pos)?.nbt))
}

/// Links the peripheral at `pos` to a screen. False when there is no peripheral there.
fn set_link(host: &mut dyn Host, pos: Pos, link: Link) -> bool {
    host.change_mod_block_entity(pos, &mut |entity| {
        let mut map = tags::from_compound(&entity.nbt);
        link.write(&mut map);
        entity.nbt = tags::to_compound(&map);
    })
}

/// Where the right half of a keyboard is from its left half, by the side the typist stands
/// on (the halves' `facing`): to the typist's right.
pub fn right_half_way(facing: &str) -> Option<Pos> {
    Some(match facing {
        "south" => [1, 0, 0],
        "east" => [0, 0, -1],
        "north" => [-1, 0, 0],
        "west" => [0, 0, 1],
        _ => return None,
    })
}

/// The other half of the keyboard half at `pos`, when it is where it should be.
pub fn other_half(host: &dyn Host, pos: Pos, block: &str) -> Option<Pos> {
    let way = right_half_way(&host.block_property(pos, "facing")?)?;
    let (steps, wanted) = if block == KEYBOARD_LEFT { (1, KEYBOARD_RIGHT) } else { (-1, KEYBOARD_LEFT) };
    let other = offset(pos, way, steps);
    (host.block(other) == wanted).then_some(other)
}

/// The block that keeps the link of the peripheral at `pos`: itself, or for the right half
/// of a keyboard its left half.
fn link_holder(host: &dyn Host, pos: Pos, block: &str) -> Option<Pos> {
    if block != KEYBOARD_RIGHT {
        return Some(pos);
    }
    other_half(host, pos, block)
}

/// Takes the speaker at `pos` off the list of the screen `link` names.
pub fn forget_speaker(host: &mut dyn Host, link: Link, pos: Pos) {
    change(host, link.core, |screens| {
        if let Some(screen) = screens.iter_mut().find(|screen| screen.side == link.side) {
            screen.speakers.retain(|speaker| speaker.pos != pos);
        }
    });
}

/// Links the peripheral at `holder` to the screen `player` picked with the linking tool.
fn link(host: &mut dyn Host, state: &mut State, player: &NativeServerPlayer, holder: Pos, block: &str) {
    // The screen stays picked: a keyboard, a remote and four speakers are linked to it
    // one after the other without going back to it each time.
    let Some(link) = state.linking.get(&player.uuid).copied() else {
        tell(host, player.uuid, Phrase::new("webdisplays.h.linker.pick_first", "Click a screen with the linking tool first."));
        return;
    };
    let Some(screen) = screens::at(host, link.core).into_iter().find(|screen| screen.side == link.side) else {
        tell(host, player.uuid, Phrase::new("webdisplays.h.linker.gone", "The screen you picked is gone."));
        return;
    };
    // A speaker is also put on its screen's list: the screen plays through what is on it.
    if let Some(kind) = Kind::of_block(block) {
        if !screen.access.allows(player.uuid, rights::MODIFY_SCREEN) {
            tell(host, player.uuid, OWNER_REFUSES);
            return;
        }
        if let Some(old) = link_at(host, holder) {
            forget_speaker(host, old, holder);
        }
        let mut added = false;
        change(host, link.core, |screens| {
            if let Some(screen) = screens.iter_mut().find(|screen| screen.side == link.side) {
                added = speaker::put(&mut screen.speakers, Speaker::new(holder, kind));
            }
        });
        if !added {
            tell(host, player.uuid, Phrase::new("webdisplays.h.linker.speakers_full", "A screen plays through %s speakers at most.").with(speaker::MAX_SPEAKERS));
            return;
        }
    }
    if set_link(host, holder, link) {
        tell(host, player.uuid, Phrase::new("webdisplays.h.linker.linked", "Linked to the screen. It stays picked: click the next thing to link to it."));
        host.trigger_criterion(player.uuid, "webdisplays:link_peripheral", &|_| true);
    } else {
        tell(host, player.uuid, Phrase::new("webdisplays.h.linker.cannot", "This cannot be linked."));
    }
}

/// A click on the linked speaker at `pos` chooses which channel it plays; sneaking, how loud.
fn tune_speaker(host: &mut dyn Host, player: &NativeServerPlayer, link: Link, pos: Pos) {
    let sneaking = host.sneaking();
    let mut said = None;
    change(host, link.core, |screens| {
        let Some(screen) = screens.iter_mut().find(|screen| screen.side == link.side) else {
            return;
        };
        if !screen.access.allows(player.uuid, rights::MODIFY_SCREEN) {
            said = Some(Phrase::new("webdisplays.h.speaker.not_allowed", "The owner of its screen does not let you change this speaker."));
            return;
        }
        if let Some(speaker) = screen.speakers.iter_mut().find(|speaker| speaker.pos == pos) {
            if sneaking {
                speaker.volume = speaker.next_volume();
            } else {
                speaker.channel = speaker.channel.next();
            }
            said = Some(Phrase::new("webdisplays.h.speaker.tuned", "Speaker: %s, %s%% (click: channel, sneak-click: volume)").with(speaker.channel.label()).with(speaker.volume));
        }
    });
    let line = said.unwrap_or(Phrase::new("webdisplays.h.speaker.screen_gone", "The screen it was linked to is gone: link it again."));
    host.action_bar(player.uuid, lang::text(&line));
}

/// A player right-clicked `pos` on `face`.
pub fn right_click(host: &mut dyn Host, state: &mut State, pos: Pos, face: Pos) -> Flow {
    let block = host.block(pos);
    // A traced screen is a screen like another for the tools; its page is the client's.
    let traced = ids::is_poly(&block);
    let screen = ids::is_screen(&block) || traced;
    if !screen && !ids::is_peripheral(&block) {
        return Flow::Continue;
    }
    let Some(player) = host.acting_player() else {
        return Flow::Continue;
    };
    let held = host.held().filter(|(item, count, _)| *count > 0 && item != "minecraft:air").map(|(item, _, _)| item);
    if state.said_twice(player.uuid, pos) {
        return Flow::Cancel;
    }
    if screen {
        return match held.as_deref() {
            None if traced => Flow::Cancel,
            None => screens::right_click(host, pos, face),
            Some(LINKER | OWNER_THIEF | UPGRADE_LASER | CONFIGURATOR) => {
                let found = if traced {
                    poly::master_of(host, pos).and_then(|master| Some((master, screens::at(host, master).into_iter().next()?)))
                } else {
                    side_of(face).and_then(|side| screens::find(host, pos, side))
                };
                on_screen(host, state, &player, pos, found, held.as_deref().unwrap_or_default());
                Flow::Cancel
            }
            // Anything else is placed or used as it would be on any block.
            Some(_) => Flow::Continue,
        };
    }
    let Some(holder) = link_holder(host, pos, &block) else {
        tell(host, player.uuid, Phrase::new("webdisplays.h.keyboard.half_lost", "This half of a keyboard has lost its other half."));
        return Flow::Cancel;
    };
    if held.as_deref() == Some(LINKER) {
        link(host, state, &player, holder, &block);
        return Flow::Cancel;
    }
    // Something to place against a speaker or a controller is placed, when sneaking.
    if held.is_some() && host.sneaking() {
        return Flow::Continue;
    }
    use_peripheral(host, state, &player, holder);
    Flow::Cancel
}

/// One of the mod's tools used on the screen block at `pos`.
fn on_screen(host: &mut dyn Host, state: &mut State, player: &NativeServerPlayer, pos: Pos, found: Option<(Pos, crate::model::screen::Screen)>, tool: &str) {
    let Some((core, screen)) = found else {
        tell(host, player.uuid, words::NOT_A_SCREEN_YET);
        return;
    };
    let side = screen.side;
    match tool {
        LINKER => {
            state.linking.insert(player.uuid, Link { core, side });
            tell(host, player.uuid, Phrase::new("webdisplays.h.linker.picked", "Screen picked. Now click each keyboard, remote, controller or speaker to link to it."));
        }
        CONFIGURATOR => {
            // Anyone may look; what they may change is the window's and the server's to say.
            let opened = ToClient::OpenConfig { target: Target { core, side }, player: player.uuid };
            let _ = host.net_send_to_player(player.uuid, CHANNEL, &opened.encode());
        }
        OWNER_THIEF => {
            if player.game_mode != 1 {
                tell(host, player.uuid, Phrase::new("webdisplays.h.thief.creative_only", "Only in creative mode."));
                return;
            }
            let owner = Player { uuid: player.uuid, name: player.name.clone() };
            change(host, core, |screens| {
                if let Some(screen) = screens.iter_mut().find(|screen| screen.side == side) {
                    screen.access.owner = Some(owner);
                }
            });
            tell(host, player.uuid, Phrase::new("webdisplays.h.thief.yours", "The screen is yours."));
        }
        UPGRADE_LASER => {
            if !screen.access.allows(player.uuid, rights::MANAGE_UPGRADES) {
                tell(host, player.uuid, Phrase::new("webdisplays.h.upgrade.not_allowed", "You may not change this screen's upgrades."));
                return;
            }
            if screen.has_upgrade(UPGRADE_LASER) {
                tell(host, player.uuid, Phrase::new("webdisplays.h.upgrade.already", "This screen has that upgrade already."));
                return;
            }
            // Taken from the hand only once it is in the screen (creative keeps it).
            if player.game_mode != 1 && host.take_items(UPGRADE_LASER, 1) == 0 {
                return;
            }
            change(host, core, |screens| {
                if let Some(screen) = screens.iter_mut().find(|screen| screen.side == side) {
                    screen.add_upgrade(UPGRADE_LASER);
                }
            });
            tell(host, player.uuid, Phrase::new("webdisplays.h.upgrade.laser_installed", "Laser upgrade installed: a laser pointer now works this screen from afar."));
            host.trigger_criterion(player.uuid, "webdisplays:upgrade_screen", &|_| true);
            host.play_sound("webdisplays:upgrade_add", [0, 1, 2].map(|i| f64::from(pos[i]) + 0.5), 4, 1.0, 1.0);
        }
        _ => {}
    }
}

/// A peripheral used with anything but the linking tool: `holder` keeps its link.
fn use_peripheral(host: &mut dyn Host, state: &mut State, player: &NativeServerPlayer, holder: Pos) {
    // The redstone controller is set up linked or not: its two addresses are its own.
    if host.block(holder) == REDSTONE {
        let controller = host.mod_block_entity(holder).map(|entity| Controller::read(&tags::from_compound(&entity.nbt))).unwrap_or_default();
        let opened = ToClient::OpenRedstone { pos: holder, rising: controller.rising, falling: controller.falling };
        let _ = host.net_send_to_player(player.uuid, CHANNEL, &opened.encode());
        return;
    }
    let Some(link) = link_at(host, holder) else {
        tell(host, player.uuid, Phrase::new("webdisplays.h.linker.not_linked", "Not linked to a screen: click a screen with the linking tool, then this."));
        return;
    };
    if Kind::of_block(&host.block(holder)).is_some() {
        tune_speaker(host, player, link, holder);
        return;
    }
    let target = Target { core: link.core, side: link.side };
    let Some(screen) = screens::at(host, link.core).into_iter().find(|screen| screen.side == link.side) else {
        tell(host, player.uuid, Phrase::new("webdisplays.h.linker.screen_gone", "The screen it was linked to is gone, or too far away to answer."));
        return;
    };
    let block = host.block(holder);
    let (right, message) = match block.as_str() {
        KEYBOARD_LEFT => (rights::INTERACT, ToClient::OpenKeyboard { target }),
        REMOTE => (rights::CHANGE_URL, ToClient::OpenUrl { target, url: screen.url.clone() }),
        _ => return,
    };
    if !screen.access.allows(player.uuid, right) {
        tell(host, player.uuid, OWNER_REFUSES);
        return;
    }
    // What the player sends next for this screen is taken from wherever they stand.
    state.remote.insert(player.uuid, target);
    let _ = host.net_send_to_player(player.uuid, CHANNEL, &message.encode());
}
