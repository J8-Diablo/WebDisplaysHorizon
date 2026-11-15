//! The world's screens, listed: where each is, what it is called, how large, which link
//! it belongs to.
//!
//! A screen's own data is in its block, which is only there while its chunk is loaded. The
//! list is what lets a player find a screen by name from the other end of the world, and
//! join two that are not in sight of one another. The server keeps it in a file of the
//! world and brings it up to date each time a screen is made, changed or destroyed.
use super::screen::Screen;
use super::side::Pos;
use crate::proto::wire::{Bad, In, Out};

/// The most screens listed: past it, new ones are left out (they work all the same).
pub const MAX_SCREENS: usize = 2048;
/// Screens sent to a player in one message.
pub const PER_PAGE: usize = 16;
const MAGIC: u32 = 0x5744_5331; // "WDS1"
const MAX_TEXT: usize = 256;

/// One screen, as the list keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The block that holds its data.
    pub core: Pos,
    /// The face it is on (`Side`'s number).
    pub side: u8,
    pub dimension: String,
    /// The name its owner gave it; empty for none.
    pub name: String,
    /// Its size in blocks.
    pub size: [u32; 2],
    /// The link it belongs to; empty for none.
    pub link: String,
    /// A traced screen, which has no face to speak of.
    pub traced: bool,
}

impl Entry {
    pub fn of(core: Pos, dimension: &str, screen: &Screen, traced: bool) -> Entry {
        Entry { core, side: screen.side.index(), dimension: dimension.to_owned(), name: screen.name.clone(), size: screen.size, link: screen.link_id.clone(), traced }
    }

    pub fn write(&self, out: &mut Out) {
        out.pos(self.core).u8(self.side).text(&self.dimension).text(&self.name).u32(self.size[0]).u32(self.size[1]).text(&self.link).flag(self.traced);
    }

    pub fn read(input: &mut In) -> Result<Entry, Bad> {
        Ok(Entry {
            core: input.pos()?,
            side: input.u8()?,
            dimension: input.text(MAX_TEXT)?,
            name: input.text(MAX_TEXT)?,
            size: [input.u32()?, input.u32()?],
            link: input.text(MAX_TEXT)?,
            traced: input.flag()?,
        })
    }
}

/// The world's screens.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Index {
    entries: Vec<Entry>,
}

impl Index {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The screens of the core at `core` in `dimension` are now `screens`: none takes it off
    /// the list. Says whether the list changed, so that it is only saved when it did.
    pub fn set_core(&mut self, dimension: &str, core: Pos, screens: Vec<Entry>) -> bool {
        let here = |entry: &Entry| entry.core == core && entry.dimension == dimension;
        let before: Vec<Entry> = self.entries.iter().filter(|entry| here(entry)).cloned().collect();
        if before == screens {
            return false;
        }
        self.entries.retain(|entry| !here(entry));
        let room = MAX_SCREENS.saturating_sub(self.entries.len());
        self.entries.extend(screens.into_iter().take(room));
        // Always in the same order, so that a screen keeps its row when it changes.
        self.entries.sort_by(|a, b| (&a.dimension, a.core, a.side).cmp(&(&b.dimension, b.core, b.side)));
        true
    }

    /// How many pages the list takes, and one of them. A list with nothing in it is one
    /// empty page.
    pub fn page(&self, page: usize) -> (usize, &[Entry]) {
        let pages = self.entries.len().div_ceil(PER_PAGE).max(1);
        let start = page.min(pages - 1) * PER_PAGE;
        (pages, &self.entries[start.min(self.entries.len())..(start + PER_PAGE).min(self.entries.len())])
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Out::new();
        out.u32(MAGIC).u32(self.entries.len() as u32);
        for entry in &self.entries {
            entry.write(&mut out);
        }
        out.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Index, Bad> {
        let mut input = In::new(bytes);
        if input.u32()? != MAGIC {
            return Err(Bad::Invalid);
        }
        let count = input.u32()? as usize;
        if count > MAX_SCREENS {
            return Err(Bad::TooLong);
        }
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            entries.push(Entry::read(&mut input)?);
        }
        input.end()?;
        Ok(Index { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(x: i32, name: &str) -> Entry {
        Entry { core: [x, 64, 0], side: 2, dimension: "minecraft:overworld".to_owned(), name: name.to_owned(), size: [4, 3], link: String::new(), traced: false }
    }

    #[test]
    fn a_core_is_listed_changed_and_taken_off() {
        let mut index = Index::default();
        assert!(index.set_core("minecraft:overworld", [1, 64, 0], vec![entry(1, "Cinema")]));
        assert!(!index.set_core("minecraft:overworld", [1, 64, 0], vec![entry(1, "Cinema")]), "nothing changed");
        assert!(index.set_core("minecraft:overworld", [1, 64, 0], vec![entry(1, "Lobby")]));
        assert_eq!(index.entries().iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(), ["Lobby"]);
        // The same block in another dimension is another screen.
        let mut nether = entry(1, "Hell");
        nether.dimension = "minecraft:the_nether".to_owned();
        assert!(index.set_core("minecraft:the_nether", [1, 64, 0], vec![nether]));
        assert_eq!(index.entries().len(), 2);
        assert!(index.set_core("minecraft:overworld", [1, 64, 0], Vec::new()));
        assert_eq!(index.entries().len(), 1);
        assert!(!index.set_core("minecraft:overworld", [9, 64, 0], Vec::new()), "it was not listed");
    }

    #[test]
    fn the_list_is_read_a_page_at_a_time() {
        let mut index = Index::default();
        assert_eq!(index.page(0), (1, &[][..]), "an empty list is one empty page");
        for x in 0..PER_PAGE as i32 + 3 {
            index.set_core("minecraft:overworld", [x, 64, 0], vec![entry(x, "s")]);
        }
        let (pages, first) = index.page(0);
        assert_eq!((pages, first.len()), (2, PER_PAGE));
        assert_eq!(index.page(1).1.len(), 3);
        assert_eq!(index.page(99).1.len(), 3, "past the end is the last page");
    }

    #[test]
    fn the_list_is_kept_in_a_file_and_read_back() {
        let mut index = Index::default();
        index.set_core("minecraft:overworld", [1, 64, 0], vec![entry(1, "Écran du salon"), Entry { side: 4, link: "wall".to_owned(), traced: true, ..entry(1, "") }]);
        let bytes = index.encode();
        assert_eq!(Index::decode(&bytes), Ok(index));
        assert!(Index::decode(&bytes[..bytes.len() - 1]).is_err());
        assert!(Index::decode(b"nothing like a list").is_err());
    }
}
