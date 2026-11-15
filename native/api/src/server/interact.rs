//! What players do to screens, as their clients say it (`net::msg`): checked here, then
//! passed on to everyone near the screen.
//!
//! A client's message is the player's own bytes. Before anything is believed the screen must
//! exist, the player must stand near it and have the right to do that to it. The Java mod
//! checked none of the three.
use super::State;
use super::browsers;
use super::index;
use crate::ipc::link::token;
use super::poly;
use super::redstone;
use super::screens::{at, change};
use crate::ids::{MAX_RESOLUTION, MAX_SIZE, REACH};
use crate::lang;
use crate::model::configure::{self, Change, Refused};
use crate::model::phrase::Phrase;
use crate::model::rights::{self, Player};
use crate::model::screen::Screen;
use crate::model::side::Pos;
use crate::model::url;
use crate::model::words;
use crate::net::msg::{CHANNEL, Target, ToClient, ToServer};
use horizon_client::mod_api::{Host, NativeServerPlayer};

/// How far from a screen its players are told what is done to it, in blocks: as far as a
/// client keeps a screen's browser (`client::world`), and a little more.
const HEARD: f64 = 112.0;

/// The distance between a player and a screen's core block.
fn away(player: &NativeServerPlayer, core: Pos) -> f64 {
    (0..3).map(|i| (f64::from(core[i]) + 0.5 - player.position[i]).powi(2)).sum::<f64>().sqrt()
}

/// The screen `target` names, when `player` is near enough to act on it.
///
/// The player list is empty in a world nobody else can join: its only player is then the
/// one who sent the message, and is taken at their word on where they stand.
///
/// A player who reached the screen through a keyboard or a remote control (`afar`) is not
/// asked to stand near it: the peripheral was checked when they used it.
fn reachable(host: &dyn Host, players: &[NativeServerPlayer], player: u128, target: &Target, afar: bool) -> Option<Screen> {
    let screen = at(host, target.core).into_iter().find(|screen| screen.side == target.side)?;
    if players.is_empty() || afar {
        return Some(screen);
    }
    let sender = players.iter().find(|other| other.uuid == player)?;
    // The core can be anywhere in the screen: allow for its whole diagonal.
    let slack = f64::from(MAX_SIZE[0] + MAX_SIZE[1]);
    (away(sender, target.core) <= REACH + slack).then_some(screen)
}

/// A message from `player`'s client.
pub fn from_client(host: &mut dyn Host, state: &mut State, player: u128, data: &[u8]) {
    // What cannot be read is dropped whole: nothing of it is guessed at.
    let Ok(message) = ToServer::decode(data) else {
        return;
    };
    let players = host.server_players();
    match message {
        ToServer::Input { target, input } => {
            let afar = state.remote.get(&player) == Some(&target);
            let Some(screen) = reachable(host, &players, player, &target, afar) else {
                crate::trace::say(|| format!("input refused, no screen in reach: {target:?}"));
                return;
            };
            if !screen.access.allows(player, rights::INTERACT) || (screen.url.is_empty() && screen.browser.is_none()) {
                crate::trace::say(|| format!("input refused, not allowed or no page: {target:?}"));
                return;
            }
            let relayed = ToClient::Input { target, input }.encode();
            if players.is_empty() {
                let _ = host.net_send_to_player(player, CHANNEL, &relayed);
                return;
            }
            let dimension = players.iter().find(|other| other.uuid == player).map(|sender| sender.dimension.clone());
            for other in &players {
                if Some(&other.dimension) == dimension.as_ref() && away(other, target.core) <= HEARD {
                    let _ = host.net_send_to_player(other.uuid, CHANNEL, &relayed);
                }
            }
        }
        ToServer::SetUrl { target, url: address } => {
            let afar = state.remote.get(&player) == Some(&target);
            let Some(screen) = reachable(host, &players, player, &target, afar) else {
                return;
            };
            if !screen.access.allows(player, rights::CHANGE_URL) {
                host.tell(player, lang::chat(&words::MAY_NOT_CHANGE_URL));
                return;
            }
            // The client cleaned it already; a client that is not ours may not have.
            let Ok(address) = url::clean(&address) else {
                return;
            };
            // A screen that shows a shared browser has no address of its own: the browser
            // goes there, for every screen that shows it.
            if let Some(id) = screen.browser
                && browsers::navigate(host, state, id, &address)
            {
                return;
            }
            change(host, target.core, |screens| {
                if let Some(screen) = screens.iter_mut().find(|screen| screen.side == target.side) {
                    screen.url = address;
                }
            });
        }
        ToServer::SetRedstone { pos, rising, falling } => redstone::set(host, &players, player, pos, &rising, &falling),
        ToServer::Pen { corner } => poly::pen(host, state, &players, player, corner),
        ToServer::Browser(edit) => browsers::edit(host, state, player, edit),
        ToServer::Screens { page } => {
            let (pages, entries) = index::page(host, usize::from(page));
            let listed = ToClient::Screens { page: page.min(pages.saturating_sub(1) as u16), pages: pages as u16, entries };
            state.outbox.push_back((player, listed.encode()));
        }
        ToServer::Join { target, other } => {
            let say = |host: &mut dyn Host, line: Phrase| host.tell(player, lang::chat(&line));
            let afar = state.remote.get(&player) == Some(&target);
            let Some(mine) = reachable(host, &players, player, &target, afar) else {
                return;
            };
            let Some(theirs) = at(host, other.core).into_iter().find(|screen| screen.side == other.side) else {
                say(host, Phrase::new("webdisplays.h.join.too_far", "That screen is too far away to be joined now: go nearer to it once, or load its chunk."));
                return;
            };
            if !mine.access.allows(player, rights::MODIFY_SCREEN) || !theirs.access.allows(player, rights::MODIFY_SCREEN) {
                say(host, Phrase::new("webdisplays.h.join.not_allowed", "You must be allowed to change both screens to join them."));
                return;
            }
            // The link one of them has already, or a new one.
            let link = [&theirs, &mine].into_iter().find(|screen| screen.is_linked()).map_or_else(|| format!("link-{:06x}", token() as u32 & 0xFF_FFFF), |screen| screen.link_id.clone());
            for joined in [target, other] {
                let link = link.clone();
                change(host, joined.core, |screens| {
                    if let Some(screen) = screens.iter_mut().find(|screen| screen.side == joined.side) {
                        screen.link_id = link;
                        screen.link_origin = false;
                    }
                });
            }
            if mine.side == theirs.side && mine.rotation == theirs.rotation {
                say(host, Phrase::new("webdisplays.h.join.done", "Joined: the two screens now show one picture between them."));
            } else {
                say(host, Phrase::new("webdisplays.h.join.facing", "Joined, but they face different ways, so they cannot share one picture: give them a shared browser instead."));
            }
        }
        // A traced screen's sheet, bent or turned from its settings window.
        ToServer::Reshape { target, change: asked } => {
            let afar = state.remote.get(&player) == Some(&target);
            if reachable(host, &players, player, &target, afar).is_none() {
                return;
            }
            // Done, the window shows it; refused, a line of chat says why.
            if let Err(refused) = poly::reshape_at(host, player, target.core, |outline| poly::apply(outline, asked)) {
                host.tell(player, lang::chat(&refused));
            }
        }
        ToServer::Configure { target, change: asked } => {
            let afar = state.remote.get(&player) == Some(&target);
            if reachable(host, &players, player, &target, afar).is_none() {
                return;
            }
            // A friend is added by name, and must be on the server to be found.
            let friend = match &asked {
                Change::AddFriend(name) => players.iter().find(|other| other.name.eq_ignore_ascii_case(name)).map(|other| Player { uuid: other.uuid, name: other.name.clone() }),
                _ => None,
            };
            // A screen is only given a shared browser that exists.
            if let Change::Browser(Some(id)) = &asked
                && browsers::registry(host, state).get(*id).is_none()
            {
                return;
            }
            let mut outcome = Err(Refused::Nothing);
            change(host, target.core, |screens| {
                if let Some(screen) = screens.iter_mut().find(|screen| screen.side == target.side) {
                    outcome = configure::apply(screen, player, &asked, friend, MAX_RESOLUTION);
                }
            });
            let said = match (outcome, &asked) {
                // The upgrade taken out goes back to whoever took it out. The id is one the
                // screen held, which only the mod puts there.
                (Ok(true), Change::RemoveUpgrade(item)) => {
                    if let Some(stack) = host.item_from_snbt(&format!("{{id:\"{item}\",count:1}}")) {
                        let _ = host.give_stack(player, stack);
                    }
                    None
                }
                (Ok(_), _) => None,
                (Err(Refused::NotAllowed), _) => Some(Phrase::new("webdisplays.h.config.refused.not_allowed", "The owner of this screen does not let you change that.")),
                (Err(Refused::NobodyByThatName), _) => Some(Phrase::new("webdisplays.h.config.refused.nobody", "Nobody by that name is on the server.")),
                (Err(Refused::Friend(_)), _) => Some(Phrase::new("webdisplays.h.config.refused.friend", "They are on the list already, or the list is full.")),
                (Err(Refused::Nothing), _) => None,
            };
            if let Some(line) = said {
                host.tell(player, lang::chat(&line));
            }
        }
    }
}
