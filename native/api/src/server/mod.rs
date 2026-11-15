//! The server's side: what is true of the world, whoever is looking.
pub mod browsers;
pub mod click;
pub mod index;
pub mod interact;
pub mod keyboard;
pub mod poly;
pub mod redstone;
pub mod screens;

use crate::model::link::Link;
use crate::model::poly::Outline;
use crate::model::shared::Registry;
use crate::net::msg::Target;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

/// What the server remembers from one hook to the next. Nothing here is saved: it is about
/// players who are on, and starts empty with the server.
#[derive(Default)]
pub struct State {
    /// The screen each player picked with the linking tool, until they click a peripheral.
    pub linking: HashMap<u128, Link>,
    /// The screen each player last reached through a keyboard or a remote control: what
    /// they send for it is taken from wherever they stand.
    pub remote: HashMap<u128, Target>,
    /// The world's shared browsers; None until they are first needed, then read from the
    /// world's folder.
    pub registry: Option<Registry>,
    /// The players who are on.
    pub online: HashSet<u128>,
    /// Messages waiting to go to a player, oldest first: a few leave each tick.
    pub outbox: VecDeque<(u128, Vec<u8>)>,
    /// The outline each player is tracing with the pen, until it closes.
    pub pens: HashMap<u128, Outline>,
    /// Each player's last right click on one of the mod's blocks, and when: the game says
    /// a click once for each hand, and the mod acts on it once.
    pub clicked: HashMap<u128, ([i32; 3], Instant)>,
}

impl State {
    /// Whether this is the same click of `player` on `pos` said again for the other hand.
    pub fn said_twice(&mut self, player: u128, pos: [i32; 3]) -> bool {
        let now = Instant::now();
        let again = self.clicked.get(&player).is_some_and(|(last, when)| *last == pos && now.duration_since(*when).as_millis() < 150);
        if !again {
            self.clicked.insert(player, (pos, now));
        }
        again
    }
}
