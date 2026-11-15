//! The mod's own windows: what is open, and what the player asked of it.
//!
//! The game calls a screen's hooks with a host that cannot act (no message can be sent from
//! a click), so a window only records what was asked (`Action`) and the next frame carries
//! it out (`client::world`). Nothing here needs the game: `client/paint.rs` draws the windows.
pub mod browsers;
pub mod config;
pub mod field;
pub mod keyboard;
pub mod pad;
pub mod redstone;
pub mod screens;
pub mod url;

use std::collections::HashSet;

use crate::model::keys::held;
use crate::net::msg::{Input, Target, ToServer};

/// What a window asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Close the window.
    Close,
    /// Tell the server.
    Send(ToServer),
    /// Put this on the clipboard.
    Copy(String),
    /// Type what the clipboard holds into the window.
    Paste,
    /// Open the shared browsers' window for this screen, in place of this one.
    Browsers(Target),
    /// Open the list of the world's screens for this screen, in place of this window.
    Screens(Target),
    /// Do this to the minePad's page: it is the player's own, so it goes to the browser.
    Pad(Input),
    /// Send the minePad's page to this address.
    PadNavigate(String),
}

/// A rectangle in GUI pixels: x, y, width, height.
pub type Rect = [f32; 4];

pub fn inside(rect: Rect, at: [f32; 2]) -> bool {
    at[0] >= rect[0] && at[0] < rect[0] + rect[2] && at[1] >= rect[1] && at[1] < rect[1] + rect[3]
}

/// A rectangle `size` wide and high in the middle of `window`, on whole pixels.
pub fn centred(window: Rect, size: [f32; 2]) -> Rect {
    [(window[0] + (window[2] - size[0]) / 2.0).round(), (window[1] + (window[3] - size[1]) / 2.0).round(), size[0], size[1]]
}

/// The modifier bits the game gives a screen's hooks (GLFW's).
pub mod modifiers {
    pub const SHIFT: u32 = 1;
    pub const CONTROL: u32 = 2;
}

/// The window that is open, if one is.
pub enum Window {
    Url(url::UrlWindow),
    Keyboard(keyboard::KeyboardWindow),
    Config(Box<config::ConfigWindow>),
    Redstone(redstone::RedstoneWindow),
    Browsers(browsers::BrowsersWindow),
    Pad(pad::PadWindow),
    Screens(screens::ScreensWindow),
}

/// Which of the keyboard's events a window is told.
///
/// A window opens on a click, often with a key held: the sneak key of a sneak-click, which
/// a player may have put on a letter. The keyboard goes on repeating that key, and a field
/// would fill with the letter. So a key that repeats without having gone down since the
/// window opened is not the window's, and neither is the character it types. The letter of
/// a shortcut (Control+V) is not text either.
#[derive(Default)]
pub struct Keys {
    /// The keys that went down since the window opened and are still held.
    fresh: HashSet<String>,
    /// Whether the character the last key press types is to be dropped.
    mute: bool,
}

/// With Control held a key is a shortcut. With Alt too it is AltGr, which types ("@").
fn shortcut(modifiers: u32) -> bool {
    modifiers & held::CONTROL != 0 && modifiers & held::ALT == 0
}

impl Keys {
    /// A key went down, or repeats. False when the window is not to hear of it.
    pub fn pressed(&mut self, key: &str, repeat: bool, modifiers: u32) -> bool {
        let stale = repeat && !self.fresh.contains(key);
        if !repeat {
            self.fresh.insert(key.to_owned());
        }
        self.mute = stale || shortcut(modifiers);
        !stale
    }

    /// A key came up. False for one that never went down in the window.
    pub fn released(&mut self, key: &str) -> bool {
        self.fresh.remove(key)
    }

    /// A character was typed. False when it is not text for the window.
    pub fn typed(&self, modifiers: u32) -> bool {
        !self.mute && !shortcut(modifiers)
    }
}

/// The mod's windows on this client.
#[derive(Default)]
pub struct Ui {
    pub open: Option<Window>,
    /// Which key events the open window is told: started again each time one opens.
    pub keys: Keys,
    /// What the windows asked for since the last frame.
    pub actions: Vec<Action>,
}

impl Ui {
    /// Runs `change` on the open window and keeps what it asks for. False when none is open.
    pub fn on(&mut self, change: impl FnOnce(&mut Window) -> Vec<Action>) -> bool {
        let Some(window) = self.open.as_mut() else {
            return false;
        };
        let asked = change(window);
        self.actions.extend(asked);
        true
    }

    /// A key went down or repeats. False when no window is open.
    pub fn key(&mut self, key: &str, repeat: bool, modifiers: u32) -> bool {
        let told = self.keys.pressed(key, repeat, modifiers);
        self.on(|window| if told { window.key(key, modifiers) } else { Vec::new() })
    }

    /// A key came up. False when no window is open.
    pub fn released(&mut self, key: &str, modifiers: u32) -> bool {
        let told = self.keys.released(key);
        self.on(|window| if told { window.released(key, modifiers) } else { Vec::new() })
    }

    /// A character was typed. False when no window is open.
    pub fn typed(&mut self, c: char, modifiers: u32) -> bool {
        let told = self.keys.typed(modifiers);
        self.on(|window| if told { window.typed(c, modifiers) } else { Vec::new() })
    }
}

impl Window {
    pub fn click(&mut self, at: [f32; 2], window: Rect, measure: &dyn Fn(&str) -> f32) -> Vec<Action> {
        match self {
            Window::Url(url) => url.click(at, window, measure),
            Window::Keyboard(_) => Vec::new(),
            Window::Config(config) => config.click(at, window, measure),
            Window::Redstone(redstone) => redstone.click(at, window, measure),
            Window::Browsers(browsers) => browsers.click(at, window, measure),
            Window::Pad(pad) => pad.click(at, window, measure),
            Window::Screens(screens) => screens.click(at, window),
        }
    }

    /// The mouse moved with its button down.
    pub fn drag(&mut self, at: [f32; 2], window: Rect) -> Vec<Action> {
        match self {
            Window::Config(config) => config.drag(at, window),
            _ => Vec::new(),
        }
    }

    /// The mouse button came up.
    pub fn release(&mut self) -> Vec<Action> {
        match self {
            Window::Config(config) => {
                config.release();
                Vec::new()
            }
            Window::Pad(pad) => pad.release(),
            _ => Vec::new(),
        }
    }

    /// The mouse is at `at`, said once a frame.
    pub fn moved(&mut self, at: [f32; 2], window: Rect) -> Vec<Action> {
        match self {
            Window::Pad(pad) => pad.moved(at, window),
            _ => Vec::new(),
        }
    }

    /// The wheel turned by `steps` with the mouse at `at`.
    pub fn scroll(&mut self, at: [f32; 2], window: Rect, steps: f64) -> Vec<Action> {
        match self {
            Window::Pad(pad) => pad.scroll(at, window, steps),
            _ => Vec::new(),
        }
    }

    /// A key went down, or repeats.
    pub fn key(&mut self, key: &str, modifiers: u32) -> Vec<Action> {
        match self {
            Window::Url(url) => url.key(key, modifiers),
            Window::Keyboard(keyboard) => keyboard.key(key, modifiers),
            Window::Config(config) => config.key(key, modifiers),
            Window::Redstone(redstone) => redstone.key(key, modifiers),
            Window::Browsers(browsers) => browsers.key(key, modifiers),
            Window::Pad(pad) => pad.key(key, modifiers),
            Window::Screens(_) => Vec::new(),
        }
    }

    pub fn released(&mut self, key: &str, modifiers: u32) -> Vec<Action> {
        match self {
            Window::Keyboard(keyboard) => {
                keyboard.released(key, modifiers);
                Vec::new()
            }
            Window::Pad(pad) => pad.released(key, modifiers),
            _ => Vec::new(),
        }
    }

    pub fn typed(&mut self, c: char, modifiers: u32) -> Vec<Action> {
        match self {
            Window::Url(url) => url.typed(c),
            Window::Keyboard(keyboard) => keyboard.typed(c, modifiers),
            Window::Config(config) => config.typed(c),
            Window::Redstone(redstone) => redstone.typed(c),
            Window::Browsers(browsers) => browsers.typed(c),
            Window::Pad(pad) => return pad.typed(c, modifiers),
            Window::Screens(_) => {}
        }
        Vec::new()
    }

    /// The clipboard's text arrives, after an `Action::Paste`.
    pub fn paste(&mut self, text: &str) -> Vec<Action> {
        match self {
            Window::Url(url) => {
                url.paste(text);
                Vec::new()
            }
            Window::Keyboard(keyboard) => keyboard.paste(text),
            Window::Config(config) => {
                config.paste(text);
                Vec::new()
            }
            Window::Redstone(redstone) => {
                redstone.paste(text);
                Vec::new()
            }
            Window::Browsers(browsers) => {
                browsers.paste(text);
                Vec::new()
            }
            Window::Pad(pad) => pad.paste(text),
            Window::Screens(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::side::Side;

    fn ui() -> Ui {
        let target = Target { core: [0, 64, 0], side: Side::North };
        Ui { open: Some(Window::Url(url::UrlWindow::new(target, ""))), ..Ui::default() }
    }

    fn address(ui: &Ui) -> String {
        match &ui.open {
            Some(Window::Url(window)) => window.field.text(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn a_key_held_since_before_the_window_opened_types_nothing() {
        // Sneak is on "A" and still held after the sneak-click that opened the window.
        let mut ui = ui();
        for _ in 0..5 {
            assert!(ui.key("key.keyboard.a", true, 0), "the event is still the window's to take");
            ui.typed('a', 0);
        }
        assert_eq!(address(&ui), "");
        // Another key pressed meanwhile types.
        ui.key("key.keyboard.h", false, 0);
        ui.typed('h', 0);
        assert_eq!(address(&ui), "h");
        // Let go and pressed again, "A" is a letter like another, repeats included.
        ui.released("key.keyboard.a", 0);
        ui.key("key.keyboard.a", false, 0);
        ui.typed('a', 0);
        ui.key("key.keyboard.a", true, 0);
        ui.typed('a', 0);
        assert_eq!(address(&ui), "haa");
    }

    #[test]
    fn the_letter_of_a_shortcut_is_not_typed() {
        let mut ui = ui();
        ui.key("key.keyboard.left.control", false, held::CONTROL);
        ui.key("key.keyboard.v", false, held::CONTROL);
        // Whether or not the game says Control with the character.
        ui.typed('v', held::CONTROL);
        ui.typed('v', 0);
        assert_eq!(address(&ui), "");
        assert_eq!(ui.actions, [Action::Paste]);
        ui.on(|window| window.paste("https://example.org"));
        assert_eq!(address(&ui), "https://example.org");
        // AltGr is Control and Alt together, and types.
        ui.key("key.keyboard.0", false, held::CONTROL | held::ALT);
        ui.typed('@', held::CONTROL | held::ALT);
        assert_eq!(address(&ui), "https://example.org@");
    }

    #[test]
    fn nothing_is_taken_without_a_window() {
        let mut ui = Ui::default();
        assert!(!ui.key("key.keyboard.a", false, 0));
        assert!(!ui.typed('a', 0));
        assert!(!ui.released("key.keyboard.a", 0));
    }
}
