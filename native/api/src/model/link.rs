//! A peripheral's link to the screen it works: a keyboard, a remote control, a redstone
//! controller or a speaker remembers one screen, by its core block and its face.
//!
//! Saved in the peripheral's block entity. The screen may be gone by the time the link is
//! used: whoever follows it checks that a screen is still there.
use super::nbt::{self, Map, Value};
use super::side::{Pos, Side};

/// The screen a peripheral works.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Link {
    pub core: Pos,
    pub side: Side,
}

const X: &str = "ScreenX";
const Y: &str = "ScreenY";
const Z: &str = "ScreenZ";
const SIDE: &str = "ScreenSide";

impl Link {
    /// Writes the link into a block entity's compound, over the one it held.
    pub fn write(&self, map: &mut Map) {
        for (key, part) in [X, Y, Z].into_iter().zip(self.core) {
            map.insert(key.to_owned(), Value::Int(part));
        }
        map.insert(SIDE.to_owned(), Value::Byte(self.side.index() as i8));
    }

    /// The link a compound holds. None for a peripheral never linked, and for a compound
    /// that names no screen (a side that does not exist, a coordinate past what an int holds).
    pub fn read(map: &Map) -> Option<Link> {
        let part = |key: &str| i32::try_from(nbt::int(map, key)?).ok();
        let side = Side::from_index(u8::try_from(nbt::int(map, SIDE)?).ok()?)?;
        Some(Link { core: [part(X)?, part(Y)?, part(Z)?], side })
    }

    /// Takes the link out of a compound.
    pub fn clear(map: &mut Map) {
        for key in [X, Y, Z, SIDE] {
            map.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_reads_back_as_it_was_written() {
        let link = Link { core: [-31, 68, 6], side: Side::South };
        let mut map = Map::from([("Other".to_owned(), Value::Int(1))]);
        link.write(&mut map);
        assert_eq!(Link::read(&map), Some(link));
        let again = Link { core: [5, -60, 1000], side: Side::Top };
        again.write(&mut map);
        assert_eq!(Link::read(&map), Some(again), "a new link replaces the old one");
        Link::clear(&mut map);
        assert_eq!(Link::read(&map), None);
        assert_eq!(map.len(), 1, "what else the compound held is left alone");
    }

    #[test]
    fn a_compound_that_names_no_screen_is_no_link() {
        assert_eq!(Link::read(&Map::new()), None, "never linked");
        let mut map = Map::new();
        Link { core: [1, 2, 3], side: Side::North }.write(&mut map);
        map.insert(SIDE.to_owned(), Value::Byte(9));
        assert_eq!(Link::read(&map), None, "a side that does not exist");
        map.insert(SIDE.to_owned(), Value::Byte(2));
        map.insert(X.to_owned(), Value::Long(1 << 40));
        assert_eq!(Link::read(&map), None, "a coordinate no block has");
        map.remove(X);
        assert_eq!(Link::read(&map), None, "a coordinate missing");
    }
}
