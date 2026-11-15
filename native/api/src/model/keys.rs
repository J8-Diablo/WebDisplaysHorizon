//! The game's names for keys as the codes a browser wants.
//!
//! Horizon names a key as vanilla does (`key.keyboard.left.shift`); Chromium takes Windows
//! virtual-key codes whatever the system. A page reads them as `event.keyCode` and derives
//! `event.key` from them for the keys that type nothing (arrows, Enter, F5).

/// The modifier bits the game gives with a key (GLFW's).
pub mod held {
    pub const SHIFT: u32 = 1;
    pub const CONTROL: u32 = 2;
    pub const ALT: u32 = 4;
}

/// The Windows virtual-key code of the key the game calls `name`. None for a key a page
/// has no use for, or a name that is not a keyboard key's.
pub fn virtual_key(name: &str) -> Option<u32> {
    let key = name.strip_prefix("key.keyboard.")?;
    if let Some(digit) = key.strip_prefix("keypad.").and_then(|digit| digit.parse::<u32>().ok()) {
        return (digit <= 9).then_some(0x60 + digit);
    }
    if let Some(number) = key.strip_prefix('f').and_then(|number| number.parse::<u32>().ok()) {
        return (1..=24).contains(&number).then(|| 0x70 + number - 1);
    }
    let mut letters = key.chars();
    if let (Some(only), None) = (letters.next(), letters.next()) {
        return match only {
            'a'..='z' => Some(only as u32 - 'a' as u32 + 0x41),
            '0'..='9' => Some(only as u32),
            _ => None,
        };
    }
    Some(match key {
        "space" => 0x20,
        "enter" | "keypad.enter" => 0x0D,
        "tab" => 0x09,
        "backspace" => 0x08,
        "escape" => 0x1B,
        "insert" => 0x2D,
        "delete" => 0x2E,
        "home" => 0x24,
        "end" => 0x23,
        "page.up" => 0x21,
        "page.down" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "left.shift" | "right.shift" => 0x10,
        "left.control" | "right.control" => 0x11,
        "left.alt" | "right.alt" => 0x12,
        "left.win" => 0x5B,
        "right.win" => 0x5C,
        "menu" => 0x5D,
        "caps.lock" => 0x14,
        "num.lock" => 0x90,
        "scroll.lock" => 0x91,
        "pause" => 0x13,
        "print.screen" => 0x2C,
        "keypad.multiply" => 0x6A,
        "keypad.add" => 0x6B,
        "keypad.subtract" => 0x6D,
        "keypad.decimal" => 0x6E,
        "keypad.divide" => 0x6F,
        "semicolon" => 0xBA,
        "equal" | "keypad.equal" => 0xBB,
        "comma" => 0xBC,
        "minus" => 0xBD,
        "period" => 0xBE,
        "slash" => 0xBF,
        "grave.accent" => 0xC0,
        "left.bracket" => 0xDB,
        "backslash" => 0xDC,
        "right.bracket" => 0xDD,
        "apostrophe" => 0xDE,
        "world.1" | "world.2" => 0xE2,
        _ => return None,
    })
}

/// The game's modifier bits as Chromium's (`proto::ctl::modifiers`: shift 2, control 4,
/// alt 8).
pub fn browser_modifiers(game: u32) -> u32 {
    let mut bits = 0;
    if game & held::SHIFT != 0 {
        bits |= 1 << 1;
    }
    if game & held::CONTROL != 0 {
        bits |= 1 << 2;
    }
    if game & held::ALT != 0 {
        bits |= 1 << 3;
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_function_keys_are_in_their_rows() {
        assert_eq!(virtual_key("key.keyboard.a"), Some(0x41));
        assert_eq!(virtual_key("key.keyboard.z"), Some(0x5A));
        assert_eq!(virtual_key("key.keyboard.0"), Some(0x30));
        assert_eq!(virtual_key("key.keyboard.9"), Some(0x39));
        assert_eq!(virtual_key("key.keyboard.f1"), Some(0x70));
        assert_eq!(virtual_key("key.keyboard.f5"), Some(0x74));
        assert_eq!(virtual_key("key.keyboard.f24"), Some(0x87));
        assert_eq!(virtual_key("key.keyboard.f25"), None, "Windows stops at F24");
        assert_eq!(virtual_key("key.keyboard.keypad.0"), Some(0x60));
        assert_eq!(virtual_key("key.keyboard.keypad.9"), Some(0x69));
    }

    #[test]
    fn the_keys_that_move_and_edit_are_known() {
        for (name, code) in [
            ("enter", 0x0D),
            ("keypad.enter", 0x0D),
            ("backspace", 0x08),
            ("tab", 0x09),
            ("left", 0x25),
            ("down", 0x28),
            ("page.up", 0x21),
            ("delete", 0x2E),
            ("left.shift", 0x10),
            ("right.control", 0x11),
            ("space", 0x20),
            ("grave.accent", 0xC0),
        ] {
            assert_eq!(virtual_key(&format!("key.keyboard.{name}")), Some(code), "{name}");
        }
    }

    #[test]
    fn what_is_no_keyboard_key_has_no_code() {
        assert_eq!(virtual_key("key.mouse.left"), None);
        assert_eq!(virtual_key("key.keyboard.unknown"), None);
        assert_eq!(virtual_key("a"), None);
        assert_eq!(virtual_key("key.keyboard.é"), None);
        assert_eq!(virtual_key("key.keyboard."), None);
    }

    #[test]
    fn modifiers_change_places_on_the_way_to_the_browser() {
        assert_eq!(browser_modifiers(0), 0);
        assert_eq!(browser_modifiers(held::SHIFT), 2);
        assert_eq!(browser_modifiers(held::CONTROL | held::ALT), 4 | 8);
        assert_eq!(browser_modifiers(8), 0, "the system key is not passed on");
    }
}
