//! One line of text being typed: a caret, a selection, and the keys that move them.
//!
//! The game gives a mod's screen key names and typed characters and nothing else, so the
//! editing is the mod's. No game type here: the screens hand it the key's name and whether
//! shift and control are held, and it is tested without the game.

/// A line of text and where the caret is in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    text: Vec<char>,
    /// Before which character the caret is: 0 to the text's length.
    caret: usize,
    /// Where the selection started; it runs from here to the caret. None without one.
    anchor: Option<usize>,
    /// The most bytes the text may come to.
    max_bytes: usize,
}

/// What a key did, for the screen to follow up on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing of the field's: the screen may use the key.
    Ignored,
    /// The field took it.
    Done,
    /// Put this on the clipboard.
    Copy(String),
    /// Insert what the clipboard holds (`Field::insert`).
    Paste,
    /// Enter: the line is finished.
    Submit,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Field {
    pub fn new(text: &str, max_bytes: usize) -> Self {
        let mut field = Self { text: Vec::new(), caret: 0, anchor: None, max_bytes };
        field.insert(text);
        field
    }

    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// How many characters come before the caret.
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The selected characters, first to one past the last. None when nothing is.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        (anchor != self.caret).then(|| (anchor.min(self.caret), anchor.max(self.caret)))
    }

    pub fn selected(&self) -> String {
        self.selection().map(|(from, to)| self.text[from..to].iter().collect()).unwrap_or_default()
    }

    /// The text before character `index`, to measure where it is drawn.
    pub fn before(&self, index: usize) -> String {
        self.text[..index.min(self.text.len())].iter().collect()
    }

    pub fn set(&mut self, text: &str) {
        self.text.clear();
        self.caret = 0;
        self.anchor = None;
        self.insert(text);
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.text.len();
    }

    /// Puts the caret before character `index`; with `extend`, selects up to there.
    pub fn move_to(&mut self, index: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = index.min(self.text.len());
    }

    fn remove_selection(&mut self) -> bool {
        let Some((from, to)) = self.selection() else {
            self.anchor = None;
            return false;
        };
        self.text.drain(from..to);
        self.caret = from;
        self.anchor = None;
        true
    }

    /// Types `text` at the caret, over the selection when there is one. Characters a line
    /// cannot hold (line breaks, controls) are dropped, and what would pass the limit is
    /// left out.
    pub fn insert(&mut self, text: &str) {
        self.remove_selection();
        let mut bytes: usize = self.text.iter().map(|c| c.len_utf8()).sum();
        for c in text.chars().filter(|c| !c.is_control()) {
            if bytes + c.len_utf8() > self.max_bytes {
                break;
            }
            bytes += c.len_utf8();
            self.text.insert(self.caret, c);
            self.caret += 1;
        }
    }

    /// Where the caret lands one word to the left.
    fn word_left(&self) -> usize {
        let mut at = self.caret;
        while at > 0 && !is_word(self.text[at - 1]) {
            at -= 1;
        }
        while at > 0 && is_word(self.text[at - 1]) {
            at -= 1;
        }
        at
    }

    /// Where the caret lands one word to the right.
    fn word_right(&self) -> usize {
        let mut at = self.caret;
        while at < self.text.len() && is_word(self.text[at]) {
            at += 1;
        }
        while at < self.text.len() && !is_word(self.text[at]) {
            at += 1;
        }
        at
    }

    /// A key went down (or repeats). `key` is the game's name for it (`key.keyboard.left`,
    /// or just `left`).
    pub fn key(&mut self, key: &str, shift: bool, control: bool) -> Outcome {
        let key = key.rsplit('.').next().unwrap_or(key);
        match key {
            "left" => {
                // Without shift, a selection collapses to its left end.
                let to = match (self.selection(), shift, control) {
                    (Some((from, _)), false, _) => from,
                    (_, _, true) => self.word_left(),
                    _ => self.caret.saturating_sub(1),
                };
                self.move_to(to, shift);
            }
            "right" => {
                let to = match (self.selection(), shift, control) {
                    (Some((_, to)), false, _) => to,
                    (_, _, true) => self.word_right(),
                    _ => self.caret + 1,
                };
                self.move_to(to, shift);
            }
            "home" => self.move_to(0, shift),
            "end" => self.move_to(self.text.len(), shift),
            "backspace" => {
                if !self.remove_selection() {
                    let from = if control { self.word_left() } else { self.caret.saturating_sub(1) };
                    self.text.drain(from..self.caret);
                    self.caret = from;
                }
            }
            "delete" => {
                if !self.remove_selection() {
                    let to = if control { self.word_right() } else { (self.caret + 1).min(self.text.len()) };
                    self.text.drain(self.caret..to);
                }
            }
            "a" if control => self.select_all(),
            "c" if control => return Outcome::Copy(self.selected()),
            "x" if control => {
                let cut = self.selected();
                self.remove_selection();
                return Outcome::Copy(cut);
            }
            "v" if control => return Outcome::Paste,
            "enter" | "return" | "kp_enter" | "numpad_enter" | "keypad.enter" => return Outcome::Submit,
            _ => return Outcome::Ignored,
        }
        Outcome::Done
    }

    /// Which characters to draw in a box `width` wide, and where the caret is in it:
    /// the first character shown, one past the last, kept so that the caret is in view.
    /// `first` is where the view started last time; `measure` gives a text's width.
    pub fn view(&self, first: usize, width: f32, measure: impl Fn(&str) -> f32) -> (usize, usize) {
        let piece = |from: usize, to: usize| -> f32 { measure(&self.text[from..to].iter().collect::<String>()) };
        let mut first = first.min(self.caret).min(self.text.len());
        // The caret past the right edge: move the view right until it fits.
        while first < self.caret && piece(first, self.caret) > width {
            first += 1;
        }
        // Room left over on the right with text hidden on the left: show more of it.
        while first > 0 && piece(first - 1, self.text.len()) <= width {
            first -= 1;
        }
        let mut last = first;
        while last < self.text.len() && piece(first, last + 1) <= width {
            last += 1;
        }
        (first, last)
    }

    /// The character boundary nearest to `x` pixels from the start of what `view` showed
    /// from `first`: where a click puts the caret.
    pub fn index_at(&self, first: usize, x: f32, measure: impl Fn(&str) -> f32) -> usize {
        let mut best = (first.min(self.text.len()), f32::MAX);
        for index in first.min(self.text.len())..=self.text.len() {
            let away = (measure(&self.text[first.min(index)..index].iter().collect::<String>()) - x).abs();
            if away < best.1 {
                best = (index, away);
            } else {
                break;
            }
        }
        best.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character six pixels wide, as the game's font nearly is.
    fn measure(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    #[test]
    fn typing_goes_where_the_caret_is() {
        let mut field = Field::new("", 100);
        field.insert("exmple");
        field.move_to(2, false);
        field.insert("a");
        assert_eq!((field.text().as_str(), field.caret()), ("example", 3));
    }

    #[test]
    fn typing_replaces_what_is_selected() {
        let mut field = Field::new("https://old.example/", 100);
        field.select_all();
        field.insert("new");
        assert_eq!((field.text().as_str(), field.caret(), field.selection()), ("new", 3, None));
    }

    #[test]
    fn a_line_holds_no_line_break_and_stops_at_its_limit() {
        let mut field = Field::new("", 5);
        field.insert("a\nb\tcé-too-long");
        // é is two bytes: a, b, c are three, é makes five.
        assert_eq!(field.text(), "abcé");
        field.insert("x");
        assert_eq!(field.text(), "abcé", "full");
    }

    #[test]
    fn arrows_move_by_character_and_by_word() {
        let mut field = Field::new("one two.three", 100);
        assert_eq!(field.key("key.keyboard.left", false, true), Outcome::Done);
        assert_eq!(field.caret(), 8, "before three");
        field.key("left", false, true);
        assert_eq!(field.caret(), 4, "before two");
        field.key("right", false, true);
        assert_eq!(field.caret(), 8, "past two and the dot");
        field.key("left", false, false);
        assert_eq!(field.caret(), 7);
        field.key("home", false, false);
        assert_eq!(field.caret(), 0);
        field.key("left", false, false);
        assert_eq!(field.caret(), 0, "no further");
        field.key("end", false, false);
        field.key("right", false, false);
        assert_eq!(field.caret(), 13, "no further");
    }

    #[test]
    fn shift_selects_and_an_arrow_alone_collapses_to_that_end() {
        let mut field = Field::new("abcdef", 100);
        field.move_to(1, false);
        field.key("right", true, false);
        field.key("right", true, false);
        assert_eq!((field.selection(), field.selected().as_str()), (Some((1, 3)), "bc"));
        field.key("left", false, false);
        assert_eq!((field.caret(), field.selection()), (1, None));
        field.key("end", true, false);
        field.key("right", false, false);
        assert_eq!((field.caret(), field.selection()), (6, None));
    }

    #[test]
    fn backspace_and_delete_take_a_character_a_word_or_the_selection() {
        let mut field = Field::new("one two three", 100);
        field.key("backspace", false, false);
        assert_eq!(field.text(), "one two thre");
        field.key("backspace", false, true);
        assert_eq!(field.text(), "one two ");
        field.key("home", false, false);
        field.key("delete", false, false);
        assert_eq!(field.text(), "ne two ");
        field.key("delete", false, true);
        assert_eq!(field.text(), "two ");
        field.select_all();
        field.key("backspace", false, false);
        assert_eq!((field.text().as_str(), field.caret()), ("", 0));
        assert_eq!(field.key("delete", false, false), Outcome::Done, "nothing left is no error");
    }

    #[test]
    fn the_clipboard_keys_say_what_to_do_with_it() {
        let mut field = Field::new("abcdef", 100);
        field.move_to(2, false);
        field.move_to(4, true);
        assert_eq!(field.key("c", false, true), Outcome::Copy("cd".to_owned()));
        assert_eq!(field.text(), "abcdef");
        assert_eq!(field.key("x", false, true), Outcome::Copy("cd".to_owned()));
        assert_eq!((field.text().as_str(), field.caret()), ("abef", 2));
        assert_eq!(field.key("v", false, true), Outcome::Paste);
        field.insert("CD");
        assert_eq!(field.text(), "abCDef");
        field.key("a", false, true);
        assert_eq!(field.selected(), "abCDef");
        assert_eq!(field.key("c", false, false), Outcome::Ignored, "a plain c is typed, not a key of the field's");
        assert_eq!(field.key("enter", false, false), Outcome::Submit);
        assert_eq!(field.key("f5", false, false), Outcome::Ignored);
    }

    #[test]
    fn the_view_follows_the_caret() {
        // Ten characters fit.
        let mut field = Field::new("0123456789abcdefghij", 100);
        assert_eq!(field.view(0, 60.0, measure), (10, 20), "the caret is at the end");
        field.move_to(12, false);
        assert_eq!(field.view(10, 60.0, measure), (10, 20), "still in view: nothing moves");
        field.move_to(3, false);
        assert_eq!(field.view(10, 60.0, measure), (3, 13), "the view starts at the caret");
        let short = Field::new("abc", 100);
        assert_eq!(short.view(2, 60.0, measure), (0, 3), "room to spare shows the start again");
    }

    #[test]
    fn a_click_puts_the_caret_at_the_nearest_boundary() {
        let field = Field::new("0123456789", 100);
        assert_eq!(field.index_at(0, 0.0, measure), 0);
        assert_eq!(field.index_at(0, 8.0, measure), 1);
        assert_eq!(field.index_at(0, 9.5, measure), 2);
        assert_eq!(field.index_at(4, 13.0, measure), 6, "counted from the first character shown");
        assert_eq!(field.index_at(0, 500.0, measure), 10, "past the end");
    }
}
