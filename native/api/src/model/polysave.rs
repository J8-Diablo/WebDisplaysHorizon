//! A traced screen as its blocks save it.
//!
//! The screen is a sheet through the corners a player traced (`model::poly`). Invisible
//! blocks stand where it passes, so that it can be aimed at and broken. One of them, the
//! master, keeps the outline beside the screen's own data; each of the others keeps where
//! the master is, so that a click or a break on any of them finds the screen.
use super::nbt::{self, Map, Value};
use super::poly::{MAX_CURVE_RADIUS, MAX_POINTS, Outline};
use super::side::Pos;

/// The key a master keeps its outline under.
pub const OUTLINE: &str = "PolyOutline";
const POINTS: &str = "Points";
const CURVED: &str = "Curved";
const FLIPPED: &str = "Flipped";
const RADIUS: &str = "Radius";
const MASTER: [&str; 3] = ["MasterX", "MasterY", "MasterZ"];

/// Puts the outline in a master's compound.
pub fn write_outline(map: &mut Map, outline: &Outline) {
    let points: Vec<i32> = outline.points().iter().flatten().copied().collect();
    map.insert(
        OUTLINE.to_owned(),
        Value::Compound(Map::from([
            (POINTS.to_owned(), Value::IntArray(points)),
            (CURVED.to_owned(), nbt::flag_value(outline.curved)),
            (FLIPPED.to_owned(), nbt::flag_value(outline.flipped)),
            (RADIUS.to_owned(), Value::Float(outline.curve_radius)),
        ])),
    );
}

/// The outline a master's compound holds; None for a block that is no master, and for an
/// outline with a coordinate missing.
pub fn read_outline(map: &Map) -> Option<Outline> {
    let Some(Value::Compound(saved)) = map.get(OUTLINE) else {
        return None;
    };
    let Some(Value::IntArray(flat)) = saved.get(POINTS) else {
        return None;
    };
    if flat.len() % 3 != 0 {
        return None;
    }
    let mut outline = Outline::from_points(flat.chunks(3).take(MAX_POINTS).map(|point| [point[0], point[1], point[2]]));
    outline.curved = nbt::flag(saved, CURVED).unwrap_or(false);
    outline.flipped = nbt::flag(saved, FLIPPED).unwrap_or(false);
    if let Some(radius) = nbt::float(saved, RADIUS).filter(|radius| radius.is_finite()) {
        outline.curve_radius = radius.clamp(0.0, MAX_CURVE_RADIUS);
    }
    Some(outline)
}

/// Puts where the master is in the compound of one of the screen's other blocks.
pub fn write_master(map: &mut Map, master: Pos) {
    for (key, part) in MASTER.into_iter().zip(master) {
        map.insert(key.to_owned(), Value::Int(part));
    }
}

/// Where the master of the screen a block belongs to is; None for a block that was never
/// told.
pub fn read_master(map: &Map) -> Option<Pos> {
    let part = |key: &str| i32::try_from(nbt::int(map, key)?).ok();
    Some([part(MASTER[0])?, part(MASTER[1])?, part(MASTER[2])?])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline() -> Outline {
        let mut outline = Outline::from_points([[0, 64, 0], [6, 64, 0], [6, 68, 0], [0, 68, 0]]);
        outline.curved = true;
        outline.flipped = true;
        outline.curve_radius = 2.5;
        outline
    }

    #[test]
    fn an_outline_reads_back_as_it_was_saved() {
        let mut map = Map::from([("WDScreens".to_owned(), Value::List(Vec::new()))]);
        write_outline(&mut map, &outline());
        assert_eq!(read_outline(&map), Some(outline()));
        assert_eq!(map.len(), 2, "beside what the block already saved");
    }

    #[test]
    fn a_block_without_an_outline_is_no_master() {
        assert_eq!(read_outline(&Map::new()), None);
        let mut map = Map::new();
        write_outline(&mut map, &outline());
        let Some(Value::Compound(saved)) = map.get_mut(OUTLINE) else { panic!() };
        saved.insert(POINTS.to_owned(), Value::IntArray(vec![1, 2, 3, 4]));
        assert_eq!(read_outline(&map), None, "a corner with a coordinate missing");
    }

    #[test]
    fn a_radius_that_makes_no_sense_is_brought_back() {
        let mut map = Map::new();
        write_outline(&mut map, &outline());
        let Some(Value::Compound(saved)) = map.get_mut(OUTLINE) else { panic!() };
        saved.insert(RADIUS.to_owned(), Value::Float(900.0));
        assert_eq!(read_outline(&map).unwrap().curve_radius, MAX_CURVE_RADIUS);
    }

    #[test]
    fn the_other_blocks_know_their_master() {
        let mut map = Map::new();
        assert_eq!(read_master(&map), None);
        write_master(&mut map, [-5, 70, 1200]);
        assert_eq!(read_master(&map), Some([-5, 70, 1200]));
    }
}
