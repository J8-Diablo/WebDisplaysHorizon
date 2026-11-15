//! The model's saved values as the game's NBT, and back: one to one, nothing decided here.
//!
//! The model describes what it saves in its own plain values (`model::nbt`) so that it
//! needs no game type. This is the only place where the two meet.
use crate::model::nbt::{Map, Value};
use crate::model::screen::{self, SCREENS, Screen};
use horizon_client::mod_api::{Compound, Tag};
use std::collections::BTreeMap;

/// The game's compounds are keyed by its own string type; this names it without importing it.
pub trait Keyed {
    type Key;
}
impl<K, V> Keyed for BTreeMap<K, V> {
    type Key = K;
}
pub type Key = <Compound as Keyed>::Key;

/// The game's key for `name`.
pub fn key(name: &str) -> Key {
    Key::from(name)
}

/// A model value as the game's tag.
pub fn to_tag(value: &Value) -> Tag {
    match value {
        Value::Byte(v) => Tag::Byte(*v),
        Value::Int(v) => Tag::Int(*v),
        Value::Long(v) => Tag::Long(*v),
        Value::Float(v) => Tag::Float(v.to_bits()),
        Value::Double(v) => Tag::Double(v.to_bits()),
        Value::String(v) => Tag::String(Key::from(v.as_str())),
        Value::IntArray(v) => Tag::IntArray(v.clone()),
        Value::List(v) => Tag::List(v.iter().map(to_tag).collect()),
        Value::Compound(v) => Tag::Compound(to_compound(v)),
    }
}

/// A model compound as the game's.
pub fn to_compound(map: &Map) -> Compound {
    map.iter().map(|(name, value)| (key(name), to_tag(value))).collect()
}

/// The game's tag as a model value. None for the kinds the mod never writes (shorts, byte
/// and long arrays): a compound that holds one simply reads as if it did not.
pub fn from_tag(tag: &Tag) -> Option<Value> {
    Some(match tag {
        Tag::Byte(v) => Value::Byte(*v),
        Tag::Short(v) => Value::Int(i32::from(*v)),
        Tag::Int(v) => Value::Int(*v),
        Tag::Long(v) => Value::Long(*v),
        Tag::Float(bits) => Value::Float(f32::from_bits(*bits)),
        Tag::Double(bits) => Value::Double(f64::from_bits(*bits)),
        Tag::String(v) => Value::String(v.to_string_lossy()),
        Tag::IntArray(v) => Value::IntArray(v.clone()),
        Tag::List(v) => Value::List(v.iter().filter_map(from_tag).collect()),
        Tag::Compound(v) => Value::Compound(from_compound(v)),
        Tag::End | Tag::ByteArray(_) | Tag::LongArray(_) => return None,
    })
}

/// The game's compound as a model one.
pub fn from_compound(compound: &Compound) -> Map {
    compound.iter().filter_map(|(name, tag)| Some((name.to_string_lossy(), from_tag(tag)?))).collect()
}

/// The screens a core's saved compound holds: the same on the server, which saved them,
/// and on a client, which was sent them.
pub fn screens(nbt: &Compound) -> Vec<Screen> {
    screen::read_all(&from_compound(nbt))
}

/// Puts `screens` in a core's compound, as it is saved and sent.
pub fn set_screens(nbt: &mut Compound, screens: &[Screen]) {
    nbt.insert(key(SCREENS), to_tag(&screen::write_all(screens)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_cross_to_the_games_tags_and_back_unchanged() {
        let inner = Map::from([("n".to_owned(), Value::Int(7))]);
        let map = Map::from([
            ("byte".to_owned(), Value::Byte(-2)),
            ("int".to_owned(), Value::Int(123_456)),
            ("long".to_owned(), Value::Long(-9_000_000_000)),
            ("float".to_owned(), Value::Float(35.5)),
            ("double".to_owned(), Value::Double(0.125)),
            ("text".to_owned(), Value::String("écran §".to_owned())),
            ("ints".to_owned(), Value::IntArray(vec![1, -1])),
            ("list".to_owned(), Value::List(vec![Value::Compound(inner.clone()), Value::Compound(inner)])),
        ]);
        let compound = to_compound(&map);
        assert_eq!(compound.get(&key("float")), Some(&Tag::Float(35.5f32.to_bits())));
        assert_eq!(from_compound(&compound), map);
    }

    #[test]
    fn kinds_the_mod_never_writes_read_as_absent() {
        let mut compound = Compound::new();
        compound.insert(key("kept"), Tag::Short(5));
        compound.insert(key("bytes"), Tag::ByteArray(vec![1, 2]));
        compound.insert(key("longs"), Tag::LongArray(vec![3]));
        assert_eq!(from_compound(&compound), Map::from([("kept".to_owned(), Value::Int(5))]));
    }
}
