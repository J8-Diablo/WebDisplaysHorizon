//! The keyboard's window: while it is open, what the player types goes to a screen's page.
//!
//! It draws almost nothing (the world stays in view, so the player sees the page they type
//! on). Keys are gathered as they come and sent a few times a second in one message
//! (`take`), as the Java mod did: a key held down repeats, and the server takes only so
//! many messages a tick.
//!
//! Control+V is not passed on as a key. Every player near the screen has their own copy of
//! the page, in their own browser, with their own clipboard: the paste is sent as the text
//! itself, so that all the copies get the same.
use super::Action;
use crate::model::keys::{browser_modifiers, held, virtual_key};
use crate::net::msg::{Input, Key, MAX_KEYS, MAX_TEXT, Target, ToServer};

pub struct KeyboardWindow {
    pub target: Target,
    /// Typed since the last message, oldest first.
    pending: Vec<Key>,
}

impl KeyboardWindow {
    pub fn new(target: Target) -> Self {
        Self { target, pending: Vec::new() }
    }

    fn push(&mut self, kind: u8, key: u32, character: u32, modifiers: u32) {
        // Typing faster than messages leave: the oldest are let go rather than the newest.
        if self.pending.len() >= MAX_KEYS * 4 {
            self.pending.remove(0);
        }
        self.pending.push(Key { kind, key, native: 0, character, modifiers });
    }

    /// A key went down or repeats.
    pub fn key(&mut self, name: &str, modifiers: u32) -> Vec<Action> {
        let Some(code) = virtual_key(name) else {
            return Vec::new();
        };
        if modifiers & held::CONTROL != 0 && name.ends_with(".v") {
            return vec![Action::Paste];
        }
        self.push(0, code, 0, browser_modifiers(modifiers));
        Vec::new()
    }

    pub fn released(&mut self, name: &str, modifiers: u32) {
        if let Some(code) = virtual_key(name) {
            self.push(1, code, 0, browser_modifiers(modifiers));
        }
    }

    /// A character was typed. Those of the upper planes go as their two UTF-16 halves, as a
    /// browser takes them.
    pub fn typed(&mut self, c: char, modifiers: u32) {
        // With Control held the key is a shortcut, not text (Control+C types no "c").
        if modifiers & held::CONTROL != 0 && modifiers & held::ALT == 0 {
            return;
        }
        for unit in c.encode_utf16(&mut [0; 2]) {
            self.push(2, u32::from(*unit), u32::from(*unit), browser_modifiers(modifiers));
        }
    }

    /// The clipboard's text, to enter in the page as one piece.
    pub fn paste(&mut self, text: &str) -> Vec<Action> {
        // Cut on a character, never inside one.
        let mut end = text.len().min(MAX_TEXT);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == 0 {
            return Vec::new();
        }
        vec![Action::Send(ToServer::Input { target: self.target, input: Input::Text(text[..end].to_owned()) })]
    }

    /// What was typed since the last call, as the message to send; None when nothing was.
    /// At most one message's worth: the rest waits for the next call.
    pub fn take(&mut self) -> Option<ToServer> {
        if self.pending.is_empty() {
            return None;
        }
        let count = self.pending.len().min(MAX_KEYS);
        let keys: Vec<Key> = self.pending.drain(..count).collect();
        Some(ToServer::Input { target: self.target, input: Input::Keys(keys) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::side::Side;

    const TARGET: Target = Target { core: [1, 2, 3], side: Side::North };

    fn keys(message: Option<ToServer>) -> Vec<Key> {
        match message {
            Some(ToServer::Input { input: Input::Keys(keys), .. }) => keys,
            other => panic!("not keys: {other:?}"),
        }
    }

    #[test]
    fn a_typed_letter_is_a_key_down_a_character_and_a_key_up() {
        let mut window = KeyboardWindow::new(TARGET);
        window.key("key.keyboard.a", held::SHIFT);
        window.typed('A', held::SHIFT);
        window.released("key.keyboard.a", 0);
        let sent = keys(window.take());
        assert_eq!(sent.iter().map(|key| (key.kind, key.key, key.character, key.modifiers)).collect::<Vec<_>>(), [
            (0, 0x41, 0, 2),
            (2, 0x41, 0x41, 2),
            (1, 0x41, 0, 0),
        ]);
        assert_eq!(window.take(), None, "nothing more until something is typed");
    }

    #[test]
    fn keys_that_type_nothing_still_go_and_unknown_ones_do_not() {
        let mut window = KeyboardWindow::new(TARGET);
        window.key("key.keyboard.enter", 0);
        window.key("key.keyboard.f5", 0);
        window.key("key.mouse.left", 0);
        window.key("key.keyboard.unknown", 0);
        assert_eq!(keys(window.take()).iter().map(|key| key.key).collect::<Vec<_>>(), [0x0D, 0x74]);
    }

    #[test]
    fn a_shortcut_types_no_text_but_alt_gr_does() {
        let mut window = KeyboardWindow::new(TARGET);
        window.key("key.keyboard.c", held::CONTROL);
        window.typed('c', held::CONTROL);
        assert_eq!(keys(window.take()).len(), 1, "the key, without a character");
        // AltGr reaches the game as Control and Alt together, and types (@ on a French keyboard).
        window.typed('@', held::CONTROL | held::ALT);
        assert_eq!(keys(window.take())[0].character, u32::from('@'));
    }

    #[test]
    fn a_character_of_the_upper_planes_goes_as_two_halves() {
        let mut window = KeyboardWindow::new(TARGET);
        window.typed('😀', 0);
        let sent = keys(window.take());
        assert_eq!(sent.iter().map(|key| key.character).collect::<Vec<_>>(), [0xD83D, 0xDE00]);
    }

    #[test]
    fn control_v_asks_for_the_clipboard_and_sends_it_as_text() {
        let mut window = KeyboardWindow::new(TARGET);
        assert_eq!(window.key("key.keyboard.v", held::CONTROL), [Action::Paste]);
        assert_eq!(window.take(), None, "the key itself is not passed on");
        let sent = window.paste("collé");
        assert_eq!(sent, [Action::Send(ToServer::Input { target: TARGET, input: Input::Text("collé".to_owned()) })]);
        assert_eq!(window.paste(""), [], "an empty clipboard pastes nothing");
        let long = "é".repeat(MAX_TEXT);
        let [Action::Send(ToServer::Input { input: Input::Text(cut), .. })] = &window.paste(&long)[..] else {
            panic!("not a text");
        };
        assert_eq!(cut.len(), MAX_TEXT, "cut to what a message holds, on a character");
    }

    #[test]
    fn more_than_a_message_holds_waits_for_the_next() {
        let mut window = KeyboardWindow::new(TARGET);
        for _ in 0..MAX_KEYS + 5 {
            window.key("key.keyboard.x", 0);
        }
        assert_eq!(keys(window.take()).len(), MAX_KEYS);
        assert_eq!(keys(window.take()).len(), 5);
    }
}
