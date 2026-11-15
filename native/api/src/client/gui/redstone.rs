//! The redstone controller's window: the address its screen shows when the signal comes
//! on, and the one it shows when the signal goes off.
use super::field::{Field, Outcome};
use super::{Action, Rect, centred, inside, modifiers};
use crate::model::phrase::Phrase;
use crate::model::side::Pos;
use crate::model::url;
use crate::model::words;
use crate::net::msg::{MAX_URL, ToServer};

/// The panel's size in GUI pixels.
pub const SIZE: [f32; 2] = [300.0, 124.0];

pub struct RedstoneWindow {
    /// The controller block.
    pub pos: Pos,
    /// The two addresses: signal on, signal off.
    pub fields: [Field; 2],
    /// The field that has the keyboard.
    pub focus: usize,
    /// The first character shown of each field.
    pub firsts: [usize; 2],
    /// Why the addresses were not saved, when they were refused.
    pub refused: Option<Phrase>,
}

/// The panel's title.
pub const TITLE: Phrase = Phrase::new("webdisplays.h.redstone.title", "Redstone controller");
/// What stands above each field.
pub const LABELS: [Phrase; 2] = [
    Phrase::new("webdisplays.h.redstone.rising", "When the signal comes on, show"),
    Phrase::new("webdisplays.h.redstone.falling", "When it goes off, show"),
];
pub const BUTTONS: [Phrase; 2] = [words::SAVE, words::CANCEL];

impl RedstoneWindow {
    pub fn new(pos: Pos, rising: &str, falling: &str) -> Self {
        Self { pos, fields: [Field::new(rising, MAX_URL), Field::new(falling, MAX_URL)], focus: 0, firsts: [0; 2], refused: None }
    }

    pub fn panel(window: Rect) -> Rect {
        centred(window, SIZE)
    }

    pub fn label_at(panel: Rect, index: usize) -> [f32; 2] {
        [panel[0] + 8.0, panel[1] + 24.0 + index as f32 * 34.0]
    }

    pub fn field_box(panel: Rect, index: usize) -> Rect {
        [panel[0] + 8.0, panel[1] + 35.0 + index as f32 * 34.0, panel[2] - 16.0, 16.0]
    }

    pub fn button_box(panel: Rect, index: usize) -> Rect {
        let width = (panel[2] - 16.0 - 6.0) / 2.0;
        [panel[0] + 8.0 + index as f32 * (width + 6.0), panel[1] + panel[3] - 26.0, width, 18.0]
    }

    /// An address as it is saved: empty stays empty (that change shows nothing).
    fn cleaned(text: &str) -> Result<String, url::Refused> {
        if text.trim().is_empty() { Ok(String::new()) } else { url::clean(text) }
    }

    fn save(&mut self) -> Vec<Action> {
        match (Self::cleaned(&self.fields[0].text()), Self::cleaned(&self.fields[1].text())) {
            (Ok(rising), Ok(falling)) => vec![Action::Send(ToServer::SetRedstone { pos: self.pos, rising, falling }), Action::Close],
            _ => {
                self.refused = Some(Phrase::new("webdisplays.h.redstone.refused", "One of these is not an address a browser can open."));
                Vec::new()
            }
        }
    }

    pub fn click(&mut self, at: [f32; 2], window: Rect, measure: &dyn Fn(&str) -> f32) -> Vec<Action> {
        let panel = Self::panel(window);
        for index in 0..2 {
            let field = Self::field_box(panel, index);
            if inside(field, at) {
                self.focus = index;
                let caret = self.fields[index].index_at(self.firsts[index], at[0] - field[0] - 4.0, measure);
                self.fields[index].move_to(caret, false);
                return Vec::new();
            }
        }
        if inside(Self::button_box(panel, 0), at) {
            return self.save();
        }
        if inside(Self::button_box(panel, 1), at) {
            return vec![Action::Close];
        }
        Vec::new()
    }

    pub fn key(&mut self, key: &str, held: u32) -> Vec<Action> {
        self.refused = None;
        if key.ends_with(".tab") || key == "tab" {
            self.focus = 1 - self.focus;
            return Vec::new();
        }
        match self.fields[self.focus].key(key, held & modifiers::SHIFT != 0, held & modifiers::CONTROL != 0) {
            Outcome::Submit => self.save(),
            Outcome::Copy(text) => vec![Action::Copy(text)],
            Outcome::Paste => vec![Action::Paste],
            Outcome::Done | Outcome::Ignored => Vec::new(),
        }
    }

    pub fn typed(&mut self, c: char) {
        self.refused = None;
        self.fields[self.focus].insert(c.encode_utf8(&mut [0; 4]));
    }

    pub fn paste(&mut self, text: &str) {
        self.fields[self.focus].insert(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Rect = [0.0, 0.0, 480.0, 270.0];
    const POS: Pos = [4, 64, -2];

    fn measure(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    fn middle(rect: Rect) -> [f32; 2] {
        [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0]
    }

    #[test]
    fn nothing_overlaps_and_all_is_in_the_panel() {
        let panel = RedstoneWindow::panel(WINDOW);
        let boxes = [RedstoneWindow::field_box(panel, 0), RedstoneWindow::field_box(panel, 1), RedstoneWindow::button_box(panel, 0), RedstoneWindow::button_box(panel, 1)];
        for (index, a) in boxes.iter().enumerate() {
            assert!(a[0] >= panel[0] && a[0] + a[2] <= panel[0] + panel[2] && a[1] + a[3] <= panel[1] + panel[3], "{a:?}");
            for b in &boxes[index + 1..] {
                let apart = a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1];
                assert!(apart, "{a:?} and {b:?}");
            }
        }
        assert!(RedstoneWindow::label_at(panel, 1)[1] >= boxes[0][1] + boxes[0][3], "the second label is under the first field");
    }

    #[test]
    fn both_addresses_are_typed_and_saved_together() {
        let mut window = RedstoneWindow::new(POS, "", "https://off.example/");
        for c in "on.example".chars() {
            window.typed(c);
        }
        window.key("key.keyboard.tab", 0);
        assert_eq!(window.focus, 1);
        let asked = window.click(middle(RedstoneWindow::button_box(RedstoneWindow::panel(WINDOW), 0)), WINDOW, &measure);
        let sent = ToServer::SetRedstone { pos: POS, rising: "http://on.example".to_owned(), falling: "https://off.example/".to_owned() };
        assert_eq!(asked, [Action::Send(sent), Action::Close]);
    }

    #[test]
    fn an_empty_address_is_saved_as_none_and_a_bad_one_is_refused() {
        let mut window = RedstoneWindow::new(POS, "  ", "");
        let sent = ToServer::SetRedstone { pos: POS, rising: String::new(), falling: String::new() };
        assert_eq!(window.key("enter", 0), [Action::Send(sent), Action::Close]);
        let mut window = RedstoneWindow::new(POS, "http://", "");
        assert_eq!(window.key("enter", 0), []);
        assert_eq!(window.refused.as_ref().map(Phrase::english).as_deref(), Some("One of these is not an address a browser can open."));
        let panel = RedstoneWindow::panel(WINDOW);
        assert_eq!(window.click(middle(RedstoneWindow::button_box(panel, 1)), WINDOW, &measure), [Action::Close]);
    }

    #[test]
    fn a_click_chooses_the_field() {
        let mut window = RedstoneWindow::new(POS, "abc", "defgh");
        let field = RedstoneWindow::field_box(RedstoneWindow::panel(WINDOW), 1);
        window.click([field[0] + 4.0 + 12.5, field[1] + 4.0], WINDOW, &measure);
        assert_eq!((window.focus, window.fields[1].caret()), (1, 2));
    }
}
