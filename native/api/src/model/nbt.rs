//! Saved data as plain values, with no game type in it.
//!
//! The game's own NBT type belongs to Horizon, which the model must not need (its tests
//! run without the game). What a screen saves is described here in these values, and
//! `tags.rs` turns them into the game's tags and back, one to one.
use std::collections::BTreeMap;

/// A compound: named values.
pub type Map = BTreeMap<String, Value>;

/// One saved value. The kinds are NBT's, less the ones the mod never writes.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Byte(i8),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    IntArray(Vec<i32>),
    List(Vec<Value>),
    Compound(Map),
}

/// A whole number under `key`, whatever width it was saved in; none when absent or not a
/// number.
pub fn int(map: &Map, key: &str) -> Option<i64> {
    match map.get(key)? {
        Value::Byte(v) => Some(i64::from(*v)),
        Value::Int(v) => Some(i64::from(*v)),
        Value::Long(v) => Some(*v),
        _ => None,
    }
}

/// A number under `key` as a float; none when absent or not a number.
pub fn float(map: &Map, key: &str) -> Option<f32> {
    match map.get(key)? {
        Value::Float(v) => Some(*v),
        Value::Double(v) => Some(*v as f32),
        _ => int(map, key).map(|v| v as f32),
    }
}

/// A flag under `key` (saved as a byte, as the game saves booleans); none when absent.
pub fn flag(map: &Map, key: &str) -> Option<bool> {
    int(map, key).map(|v| v != 0)
}

/// The text under `key`; none when absent or not text.
pub fn text<'a>(map: &'a Map, key: &str) -> Option<&'a str> {
    match map.get(key)? {
        Value::String(v) => Some(v),
        _ => None,
    }
}

/// The compounds of the list under `key`; empty when absent.
pub fn compounds<'a>(map: &'a Map, key: &str) -> impl Iterator<Item = &'a Map> {
    let list = match map.get(key) {
        Some(Value::List(list)) => list.as_slice(),
        _ => &[],
    };
    list.iter().filter_map(|value| match value {
        Value::Compound(map) => Some(map),
        _ => None,
    })
}

/// A player's or a browser's id under `key`, saved as the game saves a UUID: four ints,
/// most significant first.
pub fn uuid(map: &Map, key: &str) -> Option<u128> {
    match map.get(key)? {
        Value::IntArray(ints) if ints.len() == 4 => {
            Some(ints.iter().fold(0u128, |id, part| (id << 32) | u128::from(*part as u32)))
        }
        _ => None,
    }
}

/// `id` as the game saves a UUID.
pub fn uuid_value(id: u128) -> Value {
    Value::IntArray((0..4).rev().map(|part| (id >> (part * 32)) as u32 as i32).collect())
}

/// A flag as the game saves one.
pub fn flag_value(on: bool) -> Value {
    Value::Byte(i8::from(on))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uuid_is_four_ints_most_significant_first() {
        let id = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210u128;
        let expected = vec![0x0123_4567, 0x89ab_cdefu32 as i32, 0xfedc_ba98u32 as i32, 0x7654_3210];
        assert_eq!(uuid_value(id), Value::IntArray(expected));
        let map = Map::from([("Id".to_owned(), uuid_value(id))]);
        assert_eq!(uuid(&map, "Id"), Some(id));
        assert_eq!(uuid(&map, "Other"), None);
        let short = Map::from([("Id".to_owned(), Value::IntArray(vec![1, 2, 3]))]);
        assert_eq!(uuid(&short, "Id"), None, "three ints are not a UUID");
    }

    #[test]
    fn numbers_are_read_whatever_width_they_were_saved_in() {
        let map = Map::from([
            ("b".to_owned(), Value::Byte(-3)),
            ("i".to_owned(), Value::Int(70_000)),
            ("f".to_owned(), Value::Float(1.5)),
            ("d".to_owned(), Value::Double(2.25)),
            ("s".to_owned(), Value::String("7".to_owned())),
        ]);
        assert_eq!((int(&map, "b"), int(&map, "i")), (Some(-3), Some(70_000)));
        assert_eq!(int(&map, "f"), None, "a float is not a whole number");
        assert_eq!((float(&map, "f"), float(&map, "d"), float(&map, "i")), (Some(1.5), Some(2.25), Some(70_000.0)));
        assert_eq!((flag(&map, "b"), flag(&map, "missing")), (Some(true), None));
        assert_eq!((int(&map, "s"), text(&map, "s"), text(&map, "i")), (None, Some("7"), None));
    }

    #[test]
    fn a_list_gives_its_compounds_and_skips_the_rest() {
        let entry = |n: i32| Value::Compound(Map::from([("n".to_owned(), Value::Int(n))]));
        let map = Map::from([("list".to_owned(), Value::List(vec![entry(1), Value::Int(9), entry(2)]))]);
        let seen: Vec<_> = compounds(&map, "list").filter_map(|entry| int(entry, "n")).collect();
        assert_eq!(seen, [1, 2]);
        assert_eq!(compounds(&map, "missing").count(), 0);
    }
}
