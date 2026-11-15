//! Sentences that more than one part of the mod says: each has one key and one wording,
//! whichever window or server check says it.
use super::phrase::Phrase;

/// An address was typed that no browser can open.
pub const BAD_ADDRESS: Phrase = Phrase::new("webdisplays.h.address.malformed", "That is not an address a browser can open.");
/// An address longer than a screen keeps.
pub const ADDRESS_TOO_LONG: Phrase = Phrase::new("webdisplays.h.address.too_long", "That address is too long.");
/// A tool or a command aimed at screen blocks that were not made a screen.
pub const NOT_A_SCREEN_YET: Phrase = Phrase::new("webdisplays.h.screen.not_yet", "That is not a screen yet: right-click it with an empty hand.");
/// The player has not the right to change a screen's address.
pub const MAY_NOT_CHANGE_URL: Phrase = Phrase::new("webdisplays.h.screen.url_not_allowed", "You may not change what this screen shows.");
/// A screen's block entity is gone or unreadable.
pub const LOST_DATA: Phrase = Phrase::new("webdisplays.h.screen.lost_data", "That screen has lost its data.");

/// The label of a name's field.
pub const NAME: Phrase = Phrase::new("webdisplays.h.gui.name", "Name");
/// The label of a picture's size, in pixels.
pub const PIXELS: Phrase = Phrase::new("webdisplays.h.gui.pixels", "Pixels");
/// Between a width and a height, in every language.
pub const TIMES: Phrase = Phrase::untranslated("x");
/// The windows' buttons.
pub const CLOSE: Phrase = Phrase::new("webdisplays.h.gui.close", "Close");
pub const SAVE: Phrase = Phrase::new("webdisplays.h.gui.save", "Save");
/// The Java mod's own key: the same button.
pub const CANCEL: Phrase = Phrase::new("webdisplays.gui.seturl.cancel", "Cancel");
