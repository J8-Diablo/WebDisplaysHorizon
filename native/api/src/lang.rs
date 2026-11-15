//! The mod's sentences (`model::phrase`) as the game's text.
//!
//! This is the one place where a `Phrase` meets the game. A translated sentence becomes a
//! translatable component carrying its key, its English text and its arguments: sent by
//! the server it is read by each client in that client's language, and built on a client
//! (for a window, which draws plain strings) it is resolved in the current language at
//! once. The server never chooses a language.
use crate::model::phrase::Phrase;
use horizon_client::mod_api::Text;

/// A sentence as the game's text, and in this client's language: HCEF's, which has the
/// type.
pub use hcef::lang::{plain, text};

/// What starts every line the mod writes in a player's chat.
const PREFIX: &str = "[WebDisplays] ";

/// The sentence as a line of chat, after the mod's name.
pub fn chat(phrase: &Phrase) -> Text {
    Text::literal(PREFIX).append(text(phrase))
}

// These need the game (`CARGO_CMD=test tools/check.sh`). No language file holds their keys,
// so what is read back is the English the sentence carries.
#[cfg(test)]
mod tests {
    use super::*;

    fn owned() -> Phrase {
        Phrase::new("webdisplays.h.test.owned", "Screen %sx%s, owned by %s").with(4u32).with(3u32).with("Alex 100%s")
    }

    #[test]
    fn a_sentence_is_sent_under_its_key_with_its_english_and_reads_as_the_pure_code_says() {
        let sent = text(&owned());
        assert_eq!(sent.translation_key(), Some("webdisplays.h.test.owned"));
        let component = sent.component();
        assert_eq!(component.get("fallback").and_then(|fallback| fallback.as_str()), Some("Screen %sx%s, owned by %s"));
        assert_eq!(component.get("with").and_then(|with| with.as_array()).map(Vec::len), Some(3));
        assert_eq!(plain(&owned()), owned().english());
        assert_eq!(plain(&owned()), "Screen 4x3, owned by Alex 100%s");
    }

    #[test]
    fn a_sentence_inside_another_stays_translatable() {
        let channel = Phrase::new("webdisplays.h.test.left", "left channel");
        let tuned = Phrase::new("webdisplays.h.test.tuned", "Speaker: %s, %s%%").with(channel).with(75u8);
        let sent = text(&tuned);
        assert_eq!(sent.plain(), "Speaker: left channel, 75%");
        let inner = sent.component().get("with").and_then(|with| with.get(0)).and_then(|first| first.get("translate")).cloned();
        assert_eq!(inner.as_ref().and_then(|key| key.as_str()), Some("webdisplays.h.test.left"));
    }

    #[test]
    fn a_line_of_chat_starts_with_the_mods_name_outside_the_translated_sentence() {
        let line = chat(&Phrase::new("webdisplays.h.test.linked", "Linked!"));
        assert_eq!(line.plain(), "[WebDisplays] Linked!");
        assert_eq!(line.translation_key(), None, "the prefix is the root, the sentence its sibling");
        let sibling = line.component().get("extra").and_then(|extra| extra.get(0)).and_then(|first| first.get("translate")).cloned();
        assert_eq!(sibling.as_ref().and_then(|key| key.as_str()), Some("webdisplays.h.test.linked"));
    }

    #[test]
    fn what_is_the_same_in_every_language_is_sent_as_it_is() {
        let floor = Phrase::new("webdisplays.h.test.floor", "floor");
        let row = Phrase::untranslated("%s  %sx%s %s").with(Phrase::verbatim("50% off")).with(4u32).with(3u32).with(floor);
        let sent = text(&row);
        assert_eq!(sent.plain(), "50% off  4x3 floor");
        assert_eq!(sent.translation_key(), None);
        assert_eq!(plain(&Phrase::untranslated("%s and %s").with("one")), "%s and %s", "as the game shows a template short of an argument");
    }
}
