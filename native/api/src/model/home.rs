//! The page a screen with no address shows: the mod's own, saying what to do.
//!
//! It is written out in full in a `data:` address, so that it needs neither the network
//! nor a file. Its one sentence is the player's to read, so it comes in their language:
//! the client builds the address from the translated sentence (`client::world`).
use super::phrase::Phrase;

/// What the page says under the mod's name.
pub const HINT: Phrase = Phrase::new("webdisplays.h.screen.home", "Right-click to choose a page");

/// The page up to the sentence, and after it.
const HEAD: &str = concat!(
    "data:text/html;charset=utf-8,",
    "<body%20style=%22margin:0;background:%23101014;color:%23c8c8d0;font:5vw%20sans-serif;display:flex;",
    "align-items:center;justify-content:center;height:100vh;text-align:center%22><div>",
    "<b%20style=%22font-size:10vw;color:%23fff%22>WebDisplays</b><br>"
);
const TAIL: &str = "</div></body>";

/// The address of the page saying `hint`. The sentence is text, whatever it holds: what
/// would be markup is escaped, and every byte an address may not carry as it is (a space,
/// an accent, a quote) is written `%XX`, the page being UTF-8.
pub fn address(hint: &str) -> String {
    let mut out = String::from(HEAD);
    let escaped = hint.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    for byte in escaped.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out.push_str(TAIL);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_english_page_is_the_address_it_always_was() {
        let before = concat!(
            "data:text/html;charset=utf-8,",
            "<body%20style=%22margin:0;background:%23101014;color:%23c8c8d0;font:5vw%20sans-serif;display:flex;",
            "align-items:center;justify-content:center;height:100vh;text-align:center%22><div>",
            "<b%20style=%22font-size:10vw;color:%23fff%22>WebDisplays</b><br>Right-click%20to%20choose%20a%20page</div></body>"
        );
        assert_eq!(address(&HINT.english()), before);
    }

    #[test]
    fn a_translated_sentence_is_carried_as_utf_8_and_stays_text() {
        let page = address("Écran : l'<b> & 100%");
        assert!(page.starts_with("data:text/html;charset=utf-8,") && page.ends_with("</div></body>"));
        assert!(page.contains("<br>%C3%89cran%20%3A%20l%27%26lt%3Bb%26gt%3B%20%26amp%3B%20100%25</div>"), "{page}");
        assert!(page.is_ascii());
    }
}
