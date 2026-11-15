//! The opened minePad's window: the page in the middle of the picture (drawn by the
//! renderer, `client/pad.rs` says where), an address bar above it, and the mouse and the
//! keyboard given to the page.
//!
//! The pad's page is the player's own, so what is done to it goes straight to the browser
//! (`Action::Pad`), not through the server.
use super::field::{Field, Outcome};
use super::{Action, Rect, inside, modifiers};
use crate::client::pad::{self, Place};
use crate::model::keys::{browser_modifiers, held, virtual_key};
use crate::model::url;
use crate::net::msg::{Input, Key, MAX_TEXT, MAX_URL, Pointer};

pub struct PadWindow {
    pub address: Field,
    /// The first character of the address that is shown.
    pub first: usize,
    /// The address bar has the keyboard; otherwise the page does.
    pub typing_address: bool,
    /// Why the address was not opened, when it was refused.
    pub refused: bool,
    /// The last pixel the mouse was on, and whether its button is down on the page.
    last: Option<[u32; 2]>,
    down: bool,
}

impl PadWindow {
    pub fn new(address: &str) -> Self {
        Self { address: Field::new(address, MAX_URL), first: 0, typing_address: false, refused: false, last: None, down: false }
    }

    pub fn place() -> Place {
        pad::opened()
    }

    /// The address bar, above the page.
    pub fn address_box(window: Rect) -> Rect {
        let page = pad::window_rect(Self::place(), window);
        [page[0], page[1] - 22.0, page[2] - 46.0, 16.0]
    }

    pub fn go_box(window: Rect) -> Rect {
        let page = pad::window_rect(Self::place(), window);
        [page[0] + page[2] - 42.0, page[1] - 22.0, 42.0, 16.0]
    }

    fn navigate(&mut self) -> Vec<Action> {
        match url::clean(&self.address.text()) {
            Ok(address) => {
                self.address.set(&address);
                self.typing_address = false;
                vec![Action::PadNavigate(address)]
            }
            Err(_) => {
                self.refused = true;
                Vec::new()
            }
        }
    }

    pub fn click(&mut self, at: [f32; 2], window: Rect, measure: &dyn Fn(&str) -> f32) -> Vec<Action> {
        let bar = Self::address_box(window);
        if inside(bar, at) {
            // The first click takes the whole address, as in a browser: what is typed
            // next replaces it. A click once in the bar puts the caret where it lands.
            if self.typing_address {
                let index = self.address.index_at(self.first, at[0] - bar[0] - 4.0, measure);
                self.address.move_to(index, false);
            } else {
                self.typing_address = true;
                self.address.select_all();
            }
            return Vec::new();
        }
        if inside(Self::go_box(window), at) {
            return self.navigate();
        }
        let Some(pixel) = pad::to_pixel(Self::place(), at, window) else {
            return Vec::new();
        };
        self.typing_address = false;
        self.down = true;
        self.last = Some(pixel);
        vec![Action::Pad(Input::Pointer { what: Pointer::Down, pixel, button: 0 })]
    }

    /// The mouse button came up.
    pub fn release(&mut self) -> Vec<Action> {
        if !std::mem::take(&mut self.down) {
            return Vec::new();
        }
        vec![Action::Pad(Input::Pointer { what: Pointer::Up, pixel: self.last.unwrap_or([0, 0]), button: 0 })]
    }

    /// The mouse is at `at`: the page is told when that is another of its pixels.
    pub fn moved(&mut self, at: [f32; 2], window: Rect) -> Vec<Action> {
        let Some(pixel) = pad::to_pixel(Self::place(), at, window) else {
            return Vec::new();
        };
        if self.last == Some(pixel) {
            return Vec::new();
        }
        self.last = Some(pixel);
        vec![Action::Pad(Input::Pointer { what: Pointer::Move, pixel, button: 0 })]
    }

    /// The wheel turned by `steps` with the mouse at `at`.
    pub fn scroll(&mut self, at: [f32; 2], window: Rect, steps: f64) -> Vec<Action> {
        match pad::to_pixel(Self::place(), at, window) {
            Some(pixel) => vec![Action::Pad(Input::Scroll { pixel, delta: [0, (steps * 120.0) as i32] })],
            None => Vec::new(),
        }
    }

    fn page_key(kind: u8, name: &str, held: u32) -> Vec<Action> {
        match virtual_key(name) {
            Some(key) => vec![Action::Pad(Input::Keys(vec![Key { kind, key, native: 0, character: 0, modifiers: browser_modifiers(held) }]))],
            None => Vec::new(),
        }
    }

    pub fn key(&mut self, name: &str, mods: u32) -> Vec<Action> {
        if !self.typing_address {
            // The clipboard is the window's to fetch: the page is given the text.
            if mods & held::CONTROL != 0 && name.ends_with(".v") {
                return vec![Action::Paste];
            }
            return Self::page_key(0, name, mods);
        }
        self.refused = false;
        match self.address.key(name, mods & modifiers::SHIFT != 0, mods & modifiers::CONTROL != 0) {
            Outcome::Submit => self.navigate(),
            Outcome::Copy(text) => vec![Action::Copy(text)],
            Outcome::Paste => vec![Action::Paste],
            Outcome::Done | Outcome::Ignored => Vec::new(),
        }
    }

    pub fn released(&mut self, name: &str, mods: u32) -> Vec<Action> {
        if self.typing_address { Vec::new() } else { Self::page_key(1, name, mods) }
    }

    pub fn typed(&mut self, c: char, mods: u32) -> Vec<Action> {
        if self.typing_address {
            self.refused = false;
            self.address.insert(c.encode_utf8(&mut [0; 4]));
            return Vec::new();
        }
        // With Control held the key is a shortcut, not text.
        if mods & held::CONTROL != 0 && mods & held::ALT == 0 {
            return Vec::new();
        }
        let keys = c.encode_utf16(&mut [0; 2]).iter().map(|unit| Key { kind: 2, key: u32::from(*unit), native: 0, character: u32::from(*unit), modifiers: browser_modifiers(mods) }).collect();
        vec![Action::Pad(Input::Keys(keys))]
    }

    pub fn paste(&mut self, text: &str) -> Vec<Action> {
        if self.typing_address {
            self.address.insert(text);
            return Vec::new();
        }
        let mut end = text.len().min(MAX_TEXT);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == 0 { Vec::new() } else { vec![Action::Pad(Input::Text(text[..end].to_owned()))] }
    }

    /// The page went to another address by itself: the bar follows, unless being typed in.
    pub fn went_to(&mut self, address: &str) {
        // The mod's own home page has no address worth showing: it is a page written
        // out in full in a `data:` address.
        let shown = if address.starts_with("data:") { "" } else { address };
        if !self.typing_address && self.address.text() != shown {
            self.address.set(shown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Rect = [0.0, 0.0, 480.0, 270.0];

    fn measure(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    fn middle(rect: Rect) -> [f32; 2] {
        [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0]
    }

    #[test]
    fn the_bar_is_above_the_page_and_in_the_window() {
        let page = pad::window_rect(PadWindow::place(), WINDOW);
        let (bar, go) = (PadWindow::address_box(WINDOW), PadWindow::go_box(WINDOW));
        assert!(bar[1] >= 0.0 && bar[1] + bar[3] <= page[1], "{bar:?} {page:?}");
        assert!(bar[0] + bar[2] <= go[0] && go[0] + go[2] <= page[0] + page[2] + 0.01);
    }

    #[test]
    fn a_click_on_the_page_presses_there_and_the_release_lets_go() {
        let mut window = PadWindow::new("https://example.org/");
        let page = pad::window_rect(PadWindow::place(), WINDOW);
        let down = window.click(middle(page), WINDOW, &measure);
        let [Action::Pad(Input::Pointer { what: Pointer::Down, pixel, .. })] = &down[..] else { panic!("{down:?}") };
        assert_eq!(window.release(), [Action::Pad(Input::Pointer { what: Pointer::Up, pixel: *pixel, button: 0 })]);
        assert_eq!(window.release(), [], "it is up already");
        assert_eq!(window.click([1.0, 1.0], WINDOW, &measure), [], "beside the pad");
    }

    #[test]
    fn the_mouse_is_told_to_the_page_when_it_moves_over_it() {
        let mut window = PadWindow::new("");
        let page = pad::window_rect(PadWindow::place(), WINDOW);
        assert_eq!(window.moved(middle(page), WINDOW).len(), 1);
        assert_eq!(window.moved(middle(page), WINDOW), [], "it did not move");
        assert_eq!(window.moved([0.0, 0.0], WINDOW), [], "off the page");
        assert_eq!(window.scroll(middle(page), WINDOW, -1.0).len(), 1);
    }

    #[test]
    fn keys_go_to_the_page_until_the_address_bar_is_clicked() {
        let mut window = PadWindow::new("https://example.org/");
        let typed = window.typed('a', 0);
        assert!(matches!(&typed[..], [Action::Pad(Input::Keys(keys))] if keys[0].character == u32::from('a')));
        assert_eq!(window.key("key.keyboard.enter", 0).len(), 1, "Enter is the page's");
        assert_eq!(window.key("key.keyboard.v", held::CONTROL), [Action::Paste]);
        assert_eq!(window.paste("collé"), [Action::Pad(Input::Text("collé".to_owned()))]);

        window.click(middle(PadWindow::address_box(WINDOW)), WINDOW, &measure);
        assert!(window.typing_address);
        assert_eq!(window.address.selected(), "https://example.org/", "the click took the whole address");
        for c in "example.org/other".chars() {
            assert_eq!(window.typed(c, 0), []);
        }
        assert_eq!(window.key("enter", 0), [Action::PadNavigate("http://example.org/other".to_owned())]);
        assert!(!window.typing_address, "the page has the keyboard again");
        assert_eq!(window.address.text(), "http://example.org/other");
    }

    #[test]
    fn an_address_that_cannot_be_opened_stays_in_the_bar() {
        let mut window = PadWindow::new("");
        window.click(middle(PadWindow::address_box(WINDOW)), WINDOW, &measure);
        assert_eq!(window.click(middle(PadWindow::go_box(WINDOW)), WINDOW, &measure), []);
        assert!(window.refused && window.typing_address);
    }

    #[test]
    fn the_bar_follows_the_page_unless_it_is_being_typed_in() {
        let mut window = PadWindow::new("https://example.org/");
        window.went_to("https://example.org/next");
        assert_eq!(window.address.text(), "https://example.org/next");
        window.went_to("data:text/html,<b>home</b>");
        assert_eq!(window.address.text(), "", "the home page shows no address");
        window.went_to("https://example.org/next");
        window.click(middle(PadWindow::address_box(WINDOW)), WINDOW, &measure);
        window.went_to("https://example.org/elsewhere");
        assert_eq!(window.address.text(), "https://example.org/next");
    }
}
