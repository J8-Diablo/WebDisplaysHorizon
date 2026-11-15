//! The world's shared browsers on the server (`model::shared`): kept in a file of the world,
//! sent to each player as they join, and again to everyone when one changes.
//!
//! A player gets the list one browser a message, a few each tick: a list of long addresses
//! does not fit one message, and the server sends a player only so many a tick.
use super::State;
use crate::ids::MAX_RESOLUTION;
use crate::ipc::link::token;
use crate::lang;
use crate::model::phrase::Phrase;
use crate::model::shared::{Refused, Registry};
use crate::model::words;
use crate::net::msg::{BrowserEdit, CHANNEL, ToClient};
use horizon_client::mod_api::Host;
use std::path::PathBuf;

/// Messages sent to one player in one tick, at most: under the game's own ceiling, with
/// room left for what else the mod says.
const PER_TICK: usize = 8;
/// The permission level from which a player may change anyone's browser.
const OPERATOR: u8 = 2;

fn path(host: &dyn Host) -> Option<PathBuf> {
    Some(host.world_dir()?.join("webdisplays").join("browsers.bin"))
}

/// The world's list, read from its file the first time.
pub fn registry<'a>(host: &dyn Host, state: &'a mut State) -> &'a mut Registry {
    state.registry.get_or_insert_with(|| {
        let read = path(host).and_then(|path| std::fs::read(path).ok());
        // A file that cannot be read as a list starts an empty one; it is only written
        // over when a browser is next made, changed or deleted.
        read.and_then(|bytes| Registry::decode(&bytes).ok()).unwrap_or_default()
    })
}

/// Writes the list: to a file beside its own first, then over it, so that a crash half-way
/// leaves the old list and never half of the new one.
fn save(host: &dyn Host, registry: &Registry) {
    let Some(path) = path(host) else {
        return;
    };
    let fresh = path.with_extension("new");
    let written = path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&fresh, registry.encode())).and_then(|_| std::fs::rename(&fresh, &path));
    if let Err(error) = written {
        eprintln!("[webdisplays] the shared browsers could not be saved to {}: {error}", path.display());
    }
}

/// Queues `message` for every player who is on.
fn tell_all(state: &mut State, message: &ToClient) {
    let bytes = message.encode();
    for player in state.online.clone() {
        state.outbox.push_back((player, bytes.clone()));
    }
}

/// A player joined: they are sent the whole list.
pub fn greet(host: &dyn Host, state: &mut State, player: u128) {
    state.online.insert(player);
    let list: Vec<Vec<u8>> = registry(host, state).list().iter().map(|shared| ToClient::Browser(shared.clone()).encode()).collect();
    state.outbox.extend(list.into_iter().map(|bytes| (player, bytes)));
}

pub fn left(state: &mut State, player: u128) {
    state.online.remove(&player);
    state.outbox.retain(|(to, _)| *to != player);
}

/// Once a tick: sends some of what is waiting, a few messages a player.
pub fn tick(host: &mut dyn Host, state: &mut State) {
    if state.outbox.is_empty() {
        return;
    }
    let mut sent: Vec<(u128, usize)> = Vec::new();
    let mut waiting = std::collections::VecDeque::new();
    while let Some((player, bytes)) = state.outbox.pop_front() {
        let index = sent.iter().position(|(to, _)| *to == player).unwrap_or_else(|| {
            sent.push((player, 0));
            sent.len() - 1
        });
        // Once one message of a player waits, all their later ones do: the order is kept.
        if sent[index].1 >= PER_TICK {
            waiting.push_back((player, bytes));
            continue;
        }
        sent[index].1 += 1;
        let _ = host.net_send_to_player(player, CHANNEL, &bytes);
    }
    state.outbox = waiting;
}

/// The screen-side change: a page shown by the shared browser `id` goes to `address`.
/// Whoever calls checked that the player may change the screen's address.
pub fn navigate(host: &dyn Host, state: &mut State, id: u128, address: &str) -> bool {
    let changed = registry(host, state).navigate(id, address).ok().cloned();
    let Some(shared) = changed else {
        return false;
    };
    if let Some(registry) = &state.registry {
        save(host, registry);
    }
    tell_all(state, &ToClient::Browser(shared));
    true
}

/// `player` makes, changes or deletes a shared browser.
pub fn edit(host: &mut dyn Host, state: &mut State, player: u128, edit: BrowserEdit) {
    // In a world nobody else can join there is nobody to ask about: its one player owns it.
    let operator = host.player_permission(player).is_none_or(|level| level >= OPERATOR);
    let registry = registry(host, state);
    let outcome = match &edit {
        BrowserEdit::Create { name, url, size } => registry.create(token(), player, name, url, *size, MAX_RESOLUTION).cloned().map(ToClient::Browser),
        BrowserEdit::Edit { id, name, url, size } => registry.edit(*id, player, operator, name, url, *size, MAX_RESOLUTION).cloned().map(ToClient::Browser),
        BrowserEdit::Delete { id } => registry.delete(*id, player, operator).map(|_| ToClient::BrowserGone { id: *id }),
    };
    match outcome {
        Ok(news) => {
            if let Some(registry) = &state.registry {
                save(host, registry);
            }
            tell_all(state, &news);
        }
        Err(refused) => {
            let line = match refused {
                Refused::Full => Phrase::new("webdisplays.h.browsers.full", "This world has as many shared browsers as it can."),
                Refused::Unknown => Phrase::new("webdisplays.h.browsers.unknown", "That shared browser no longer exists."),
                Refused::NotYours => Phrase::new("webdisplays.h.browsers.not_yours", "Only whoever made that shared browser, or an operator, may change it."),
                Refused::BadAddress => words::BAD_ADDRESS,
            };
            host.tell(player, lang::chat(&line));
        }
    }
}
