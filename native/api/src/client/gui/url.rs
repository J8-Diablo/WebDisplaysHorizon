//! The window a screen's address is typed in (the Java mod's "set URL" screen).
//!
//! Its state and what its clicks and keys do are plain data, tested without the game;
//! `paint.rs` draws it, and reads what it says (`Phrase`) in the player's language.
use super::field::{Field, Outcome};
use super::{Action, Rect, centred, inside, modifiers};
use crate::model::phrase::Phrase;
use crate::model::url;
use crate::model::words;
use crate::net::msg::{MAX_URL, Target, ToServer};

/// The panel's size in GUI pixels.
pub const SIZE: [f32; 2] = [280.0, 86.0];

pub struct UrlWindow {
    pub target: Target,
    pub field: Field,
    /// The first character of the address that is shown: the view follows the caret.
    pub first: usize,
    /// Why the address was not sent, when it was refused.
    pub refused: Option<Phrase>,
}

/// The panel's title.
pub const TITLE: Phrase = Phrase::new("webdisplays.h.url.title", "Screen address");
/// What stands under the field while nothing was refused.
pub const HINT: Phrase = Phrase::new("webdisplays.h.url.hint", "Enter opens it, Escape closes.");

/// The window's buttons, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Open,
    YouTube,
    Cancel,
}

pub const BUTTONS: [(Button, Phrase); 3] = [
    (Button::Open, Phrase::new("webdisplays.h.url.open", "Open")),
    // A name: the same in every language.
    (Button::YouTube, Phrase::untranslated("YouTube")),
    (Button::Cancel, words::CANCEL),
];

impl UrlWindow {
    /// The window for the screen `target`, showing `address` selected so that typing
    /// replaces it.
    pub fn new(target: Target, address: &str) -> Self {
        let mut field = Field::new(address, MAX_URL);
        field.select_all();
        Self { target, field, first: 0, refused: None }
    }

    pub fn panel(window: Rect) -> Rect {
        centred(window, SIZE)
    }

    /// The box the address is typed in.
    pub fn field_box(panel: Rect) -> Rect {
        [panel[0] + 8.0, panel[1] + 24.0, panel[2] - 16.0, 16.0]
    }

    /// The width text has inside the field's box.
    pub fn field_width(panel: Rect) -> f32 {
        Self::field_box(panel)[2] - 8.0
    }

    pub fn button_box(panel: Rect, index: usize) -> Rect {
        let width = (panel[2] - 16.0 - 2.0 * 6.0) / 3.0;
        [panel[0] + 8.0 + index as f32 * (width + 6.0), panel[1] + panel[3] - 26.0, width, 18.0]
    }

    /// Sends the address if a browser can open it; says why not otherwise.
    fn submit(&mut self) -> Vec<Action> {
        match url::clean(&self.field.text()) {
            Ok(address) => vec![Action::Send(ToServer::SetUrl { target: self.target, url: address }), Action::Close],
            Err(refused) => {
                self.refused = Some(match refused {
                    url::Refused::Empty => Phrase::new("webdisplays.h.url.empty", "Type an address."),
                    url::Refused::TooLong => words::ADDRESS_TOO_LONG,
                    url::Refused::Malformed => words::BAD_ADDRESS,
                });
                Vec::new()
            }
        }
    }

    fn press(&mut self, button: Button) -> Vec<Action> {
        match button {
            Button::Open => self.submit(),
            Button::YouTube => {
                self.field.set("https://www.youtube.com/");
                self.submit()
            }
            Button::Cancel => vec![Action::Close],
        }
    }

    pub fn click(&mut self, at: [f32; 2], window: Rect, measure: &dyn Fn(&str) -> f32) -> Vec<Action> {
        let panel = Self::panel(window);
        let field = Self::field_box(panel);
        if inside(field, at) {
            let index = self.field.index_at(self.first, at[0] - field[0] - 4.0, measure);
            self.field.move_to(index, false);
            return Vec::new();
        }
        for (index, (button, _)) in BUTTONS.iter().enumerate() {
            if inside(Self::button_box(panel, index), at) {
                return self.press(*button);
            }
        }
        Vec::new()
    }

    pub fn key(&mut self, key: &str, held: u32) -> Vec<Action> {
        self.refused = None;
        match self.field.key(key, held & modifiers::SHIFT != 0, held & modifiers::CONTROL != 0) {
            Outcome::Submit => self.submit(),
            Outcome::Copy(text) => vec![Action::Copy(text)],
            Outcome::Paste => vec![Action::Paste],
            Outcome::Done | Outcome::Ignored => Vec::new(),
        }
    }

    pub fn typed(&mut self, c: char) {
        self.refused = None;
        self.field.insert(c.encode_utf8(&mut [0; 4]));
    }

    pub fn paste(&mut self, text: &str) {
        self.field.insert(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::side::Side;

    const WINDOW: Rect = [0.0, 0.0, 480.0, 270.0];
    const TARGET: Target = Target { core: [1, 2, 3], side: Side::North };

    fn measure(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    fn middle(rect: Rect) -> [f32; 2] {
        [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0]
    }

    #[test]
    fn the_buttons_are_in_the_panel_and_apart() {
        let panel = UrlWindow::panel(WINDOW);
        let boxes: Vec<Rect> = (0..3).map(|index| UrlWindow::button_box(panel, index)).collect();
        for (index, rect) in boxes.iter().enumerate() {
            assert!(rect[0] >= panel[0] && rect[0] + rect[2] <= panel[0] + panel[2] + 0.01, "button {index} sticks out");
            assert!(rect[1] + rect[3] <= panel[1] + panel[3]);
            if index > 0 {
                assert!(rect[0] >= boxes[index - 1][0] + boxes[index - 1][2], "button {index} overlaps the one before");
            }
        }
        assert!(UrlWindow::field_box(panel)[1] + 16.0 <= boxes[0][1], "the field is above the buttons");
    }

    #[test]
    fn typing_over_the_old_address_and_enter_sends_the_new_one() {
        let mut window = UrlWindow::new(TARGET, "https://old.example/");
        for c in "example.org".chars() {
            window.typed(c);
        }
        let asked = window.key("key.keyboard.enter", 0);
        let sent = ToServer::SetUrl { target: TARGET, url: "http://example.org".to_owned() };
        assert_eq!(asked, [Action::Send(sent), Action::Close]);
    }

    #[test]
    fn an_address_that_cannot_be_opened_is_not_sent_and_the_window_says_why() {
        let mut window = UrlWindow::new(TARGET, "");
        assert_eq!(window.click(middle(UrlWindow::button_box(UrlWindow::panel(WINDOW), 0)), WINDOW, &measure), []);
        assert_eq!(window.refused.as_ref().map(Phrase::english).as_deref(), Some("Type an address."));
        assert_eq!(window.refused.as_ref().map(|why| why.key), Some("webdisplays.h.url.empty"));
        window.typed('x');
        assert_eq!(window.refused, None, "typing clears it");
    }

    #[test]
    fn the_buttons_do_what_they_say() {
        let panel = UrlWindow::panel(WINDOW);
        let mut window = UrlWindow::new(TARGET, "https://example.org/");
        assert_eq!(window.click(middle(UrlWindow::button_box(panel, 2)), WINDOW, &measure), [Action::Close]);
        let asked = window.click(middle(UrlWindow::button_box(panel, 1)), WINDOW, &measure);
        let sent = ToServer::SetUrl { target: TARGET, url: "https://www.youtube.com/".to_owned() };
        assert_eq!(asked, [Action::Send(sent), Action::Close]);
        assert_eq!(window.click([1.0, 1.0], WINDOW, &measure), [], "outside the panel");
    }

    #[test]
    fn an_address_no_browser_opens_is_refused_in_the_words_the_server_uses() {
        let mut window = UrlWindow::new(TARGET, "http://");
        assert_eq!(window.key("enter", 0), []);
        assert_eq!(window.refused, Some(words::BAD_ADDRESS));
        assert_eq!(BUTTONS.map(|(_, label)| label.english()), ["Open", "YouTube", "Cancel"]);
        assert!(!BUTTONS[1].1.is_translated(), "a name is not translated");
    }

    #[test]
    fn a_click_in_the_field_puts_the_caret_there_and_the_clipboard_keys_ask() {
        let mut window = UrlWindow::new(TARGET, "abcdef");
        let field = UrlWindow::field_box(UrlWindow::panel(WINDOW));
        window.click([field[0] + 4.0 + 12.5, field[1] + 5.0], WINDOW, &measure);
        assert_eq!((window.field.caret(), window.field.selection()), (2, None));
        assert_eq!(window.key("v", modifiers::CONTROL), [Action::Paste]);
        window.paste("XY");
        assert_eq!(window.field.text(), "abXYcdef");
        window.key("a", modifiers::CONTROL);
        assert_eq!(window.key("c", modifiers::CONTROL), [Action::Copy("abXYcdef".to_owned())]);
    }
}
