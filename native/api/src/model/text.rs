//! Names players give things (screens, shared browsers, link groups).

/// The longest name of a screen or of a shared browser, in characters.
pub const MAX_NAME: usize = 32;
/// The longest id of a link group, in characters.
pub const MAX_LINK_ID: usize = 64;

/// A name as typed, made safe to show as it is: trimmed, without control characters or
/// section signs (which would start a formatting code), and no longer than `max` characters.
pub fn label(typed: &str, max: usize) -> String {
    typed.trim().chars().filter(|c| !c.is_control() && *c != '\u{a7}').take(max).collect::<String>().trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_is_trimmed_cleaned_and_capped() {
        assert_eq!(label("  Main stage  ", MAX_NAME), "Main stage");
        assert_eq!(label("\u{a7}cRed\u{a7}r\tscreen\n", MAX_NAME), "cRedrscreen");
        assert_eq!(label("abcdef", 3), "abc");
        assert_eq!(label("ab   cd", 3), "ab", "a cut does not leave a trailing space");
        assert_eq!(label("éàü", 2), "éà", "characters, not bytes");
        assert_eq!(label("   ", MAX_NAME), "");
    }
}
