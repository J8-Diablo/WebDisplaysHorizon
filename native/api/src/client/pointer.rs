//! The crosshair as a mouse on the screen it points at: what to tell the server as the
//! player aims, presses and lets go.
//!
//! The use key going down is a button press where the crosshair is; while it stays down the
//! pointer drags, and stays with the screen it went down on even when the crosshair leaves
//! it. Without a button the pointer hovers, so that a page's menus and players react.
//! Positions are sent no faster than a page needs: the server takes 32 messages a tick
//! from a player and passes each on to everyone near.
//!
//! Plain data and a clock handed in, so it is tested without the game.
use crate::net::msg::{Input, Pointer as What, Target, ToServer};

/// Seconds between two positions sent while a button is down.
const DRAG_EVERY: f64 = 0.05;
/// Seconds between two positions sent while hovering.
const HOVER_EVERY: f64 = 0.1;
/// Seconds without the use key saying it is still held after which the button is taken to
/// be up. The game says a held use key again every fifth of a second; it does not always
/// say when the key comes up (a quick click can come up before the game has acted on it),
/// and a button left down on a page makes every later click a drag.
const HELD_FOR: f64 = 0.45;

/// Whether the crosshair is a mouse at all. Only for a player who means it: sneaking with
/// an empty hand, or holding the laser pointer. A page does not react to someone walking
/// past its screen, or looking at it with a block in hand.
pub fn working(sneaking: bool, empty_hand: bool, laser: bool) -> bool {
    laser || (sneaking && empty_hand)
}

/// Where the crosshair is on a screen: which one, and the pixel (in its saved resolution).
pub type At = (Target, [u32; 2]);

#[derive(Default)]
pub struct Pointer {
    /// The screen the button went down on, until it comes up.
    down: Option<Target>,
    /// The last position sent.
    last: Option<At>,
    /// When it was sent, on the caller's clock.
    sent: f64,
    /// When the use key last said it was down.
    held_at: f64,
}

fn message(target: Target, what: What, pixel: [u32; 2]) -> ToServer {
    ToServer::Input { target, input: Input::Pointer { what, pixel, button: 0 } }
}

impl Pointer {
    pub fn is_down(&self) -> bool {
        self.down.is_some()
    }

    /// Once a frame: the crosshair is at `at` (None off every screen). `hover` says whether
    /// a pointer without a button is shown to the page at all.
    pub fn aim(&mut self, now: f64, at: Option<At>, hover: bool) -> Option<ToServer> {
        let (target, pixel) = at?;
        let every = match self.down {
            // A drag stays on its screen: elsewhere there is nothing to say.
            Some(held) if held != target => return None,
            Some(_) => DRAG_EVERY,
            None if hover => HOVER_EVERY,
            None => return None,
        };
        if self.last == Some((target, pixel)) || now - self.sent < every {
            return None;
        }
        self.last = Some((target, pixel));
        self.sent = now;
        Some(message(target, What::Move, pixel))
    }

    /// The use key went down with the crosshair at `at`. A key that repeats while held is
    /// one press: only the first says so.
    pub fn press(&mut self, now: f64, at: At) -> Vec<ToServer> {
        let (target, pixel) = at;
        self.held_at = now;
        if self.down == Some(target) {
            return Vec::new();
        }
        // A button still down elsewhere (its release was never heard) comes up first.
        let mut said: Vec<ToServer> = self.release().into_iter().collect();
        self.down = Some(target);
        self.last = Some(at);
        self.sent = now;
        said.push(message(target, What::Down, pixel));
        said
    }

    /// Whether the button is down but the use key has not said so for too long: it came
    /// up without the game saying so, and should be released.
    pub fn is_stale(&self, now: f64) -> bool {
        self.down.is_some() && now - self.held_at > HELD_FOR
    }

    /// The button came up: said where the pointer last was on that screen.
    pub fn release(&mut self) -> Option<ToServer> {
        let target = self.down.take()?;
        let pixel = match self.last {
            Some((last, pixel)) if last == target => pixel,
            _ => [0, 0],
        };
        Some(message(target, What::Up, pixel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::side::Side;

    const A: Target = Target { core: [0, 64, 0], side: Side::North };
    const B: Target = Target { core: [9, 64, 0], side: Side::North };

    #[test]
    fn a_press_drags_until_released_where_the_pointer_last_was() {
        let mut pointer = Pointer::default();
        assert_eq!(pointer.press(1.0, (A, [10, 10])), [message(A, What::Down, [10, 10])]);
        assert!(pointer.is_down());
        assert_eq!(pointer.aim(1.01, Some((A, [11, 10])), false), None, "too soon after the press");
        assert_eq!(pointer.aim(1.06, Some((A, [12, 10])), false), Some(message(A, What::Move, [12, 10])));
        assert_eq!(pointer.aim(1.2, Some((A, [12, 10])), false), None, "it did not move");
        assert_eq!(pointer.release(), Some(message(A, What::Up, [12, 10])));
        assert_eq!(pointer.release(), None, "it is up already");
    }

    #[test]
    fn a_use_key_that_repeats_while_held_is_one_press() {
        let mut pointer = Pointer::default();
        assert_eq!(pointer.press(1.0, (A, [10, 10])).len(), 1);
        assert_eq!(pointer.press(1.2, (A, [30, 10])), [], "the key repeating");
        assert_eq!(pointer.release(), Some(message(A, What::Up, [10, 10])));
        assert_eq!(pointer.press(1.4, (A, [30, 10])), [message(A, What::Down, [30, 10])], "a new press");
    }

    #[test]
    fn a_button_nobody_says_is_still_held_is_taken_to_be_up() {
        let mut pointer = Pointer::default();
        pointer.press(1.0, (A, [10, 10]));
        assert!(!pointer.is_stale(1.3), "a click is not over in a moment");
        pointer.press(1.2, (A, [10, 10]));
        pointer.press(1.4, (A, [10, 10]));
        assert!(!pointer.is_stale(1.8), "the key keeps saying it is held: a long press, a drag");
        assert!(pointer.is_stale(1.9), "silence: it came up");
        assert_eq!(pointer.release(), Some(message(A, What::Up, [10, 10])));
        assert!(!pointer.is_stale(5.0), "nothing is down any more");
    }

    #[test]
    fn a_drag_stays_with_its_screen() {
        let mut pointer = Pointer::default();
        pointer.press(1.0, (A, [10, 10]));
        assert_eq!(pointer.aim(2.0, Some((B, [5, 5])), true), None, "the crosshair wandered onto another screen");
        assert_eq!(pointer.aim(2.1, None, true), None);
        assert_eq!(pointer.release(), Some(message(A, What::Up, [10, 10])));
    }

    #[test]
    fn a_press_elsewhere_lets_go_of_the_first_screen() {
        let mut pointer = Pointer::default();
        pointer.press(1.0, (A, [10, 10]));
        let said = pointer.press(2.0, (B, [5, 5]));
        assert_eq!(said, [message(A, What::Up, [10, 10]), message(B, What::Down, [5, 5])]);
    }

    #[test]
    fn a_page_is_worked_sneaking_with_an_empty_hand_or_with_the_laser() {
        assert!(working(true, true, false), "sneaking, nothing in hand");
        assert!(!working(false, true, false), "walking past");
        assert!(!working(true, false, false), "sneaking with a block in hand");
        assert!(!working(false, false, false));
        assert!(working(false, false, true) && working(true, false, true), "the laser pointer is one, sneaking or not");
    }

    #[test]
    fn hovering_is_said_slowly_and_only_when_asked_for() {
        let mut pointer = Pointer::default();
        assert_eq!(pointer.aim(1.0, Some((A, [1, 1])), false), None, "no hovering");
        assert_eq!(pointer.aim(1.0, Some((A, [1, 1])), true), Some(message(A, What::Move, [1, 1])));
        assert_eq!(pointer.aim(1.05, Some((A, [2, 1])), true), None, "slower than a drag");
        assert_eq!(pointer.aim(1.11, Some((A, [2, 1])), true), Some(message(A, What::Move, [2, 1])));
        assert_eq!(pointer.aim(1.3, Some((B, [2, 1])), true), Some(message(B, What::Move, [2, 1])), "another screen");
    }
}
