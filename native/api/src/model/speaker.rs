//! A speaker block linked to a screen: where it is, which part of the sound it plays.
//!
//! New with the Horizon mod. A screen with speakers is silent itself and plays through
//! them; each is a tweeter (the treble), a bass speaker or a full-range one, by its block,
//! and plays the page's left channel, its right one, or both mixed.
//!
//! The screen keeps the list (in its saved data, sent to clients with the rest), so a
//! client that knows a screen knows where its sound comes from without having looked at
//! each speaker.
use super::nbt::{self, Map, Value};
use super::phrase::Phrase;
use super::side::Pos;

/// The most speakers a screen plays through.
pub const MAX_SPEAKERS: usize = 16;

/// Which part of the spectrum a speaker plays, by the block it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// Full range ("mix").
    Full = 0,
    Tweeter = 1,
    Bass = 2,
}

/// Which of the page's channels a speaker plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Channel {
    /// Left and right mixed.
    #[default]
    Both = 0,
    Left = 1,
    Right = 2,
}

impl Channel {
    pub fn from_index(index: u8) -> Option<Channel> {
        [Channel::Both, Channel::Left, Channel::Right].into_iter().find(|channel| *channel as u8 == index)
    }

    /// The next one, round and round: what a click on the speaker chooses.
    pub fn next(self) -> Channel {
        match self {
            Channel::Both => Channel::Left,
            Channel::Left => Channel::Right,
            Channel::Right => Channel::Both,
        }
    }

    /// The channel as a player is told it.
    pub fn label(self) -> Phrase {
        match self {
            Channel::Both => Phrase::new("webdisplays.h.speaker.both", "both channels"),
            Channel::Left => Phrase::new("webdisplays.h.speaker.left", "left channel"),
            Channel::Right => Phrase::new("webdisplays.h.speaker.right", "right channel"),
        }
    }
}

impl Kind {
    pub fn from_index(index: u8) -> Option<Kind> {
        [Kind::Full, Kind::Tweeter, Kind::Bass].into_iter().find(|kind| *kind as u8 == index)
    }

    /// The kind of speaker the block `block` is; None for a block that is none.
    pub fn of_block(block: &str) -> Option<Kind> {
        match block {
            "webdisplays:speaker_mix" => Some(Kind::Full),
            "webdisplays:speaker_tweeter" => Some(Kind::Tweeter),
            "webdisplays:speaker_bass" => Some(Kind::Bass),
            _ => None,
        }
    }
}

/// The volumes a click on a speaker goes through, in percent.
pub const VOLUME_STEPS: [u8; 4] = [100, 75, 50, 25];

/// One speaker of a screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Speaker {
    pub pos: Pos,
    pub kind: Kind,
    pub channel: Channel,
    /// 0 to 100.
    pub volume: u8,
}

impl Speaker {
    pub fn new(pos: Pos, kind: Kind) -> Self {
        Self { pos, kind, channel: Channel::Both, volume: 100 }
    }

    /// The volume a click chooses after this one.
    pub fn next_volume(&self) -> u8 {
        let at = VOLUME_STEPS.iter().position(|step| *step == self.volume);
        VOLUME_STEPS[at.map_or(0, |at| (at + 1) % VOLUME_STEPS.len())]
    }

    pub fn write(&self) -> Value {
        Value::Compound(Map::from([
            ("X".to_owned(), Value::Int(self.pos[0])),
            ("Y".to_owned(), Value::Int(self.pos[1])),
            ("Z".to_owned(), Value::Int(self.pos[2])),
            ("Kind".to_owned(), Value::Byte(self.kind as i8)),
            ("Channel".to_owned(), Value::Byte(self.channel as i8)),
            ("Volume".to_owned(), Value::Byte(self.volume as i8)),
        ]))
    }

    pub fn read(map: &Map) -> Option<Speaker> {
        let part = |key: &str| i32::try_from(nbt::int(map, key)?).ok();
        let index = |key: &str| u8::try_from(nbt::int(map, key)?).ok();
        Some(Speaker {
            pos: [part("X")?, part("Y")?, part("Z")?],
            kind: Kind::from_index(index("Kind")?)?,
            channel: index("Channel").and_then(Channel::from_index).unwrap_or_default(),
            volume: index("Volume").unwrap_or(100).min(100),
        })
    }
}

/// The speakers of a saved list: at most `MAX_SPEAKERS`, each position once.
pub fn read_all(map: &Map, key: &str) -> Vec<Speaker> {
    let mut speakers: Vec<Speaker> = Vec::new();
    for speaker in nbt::compounds(map, key).filter_map(Speaker::read) {
        if speakers.len() < MAX_SPEAKERS && !speakers.iter().any(|other| other.pos == speaker.pos) {
            speakers.push(speaker);
        }
    }
    speakers
}

/// Puts `speaker` in `list`, in place of the one at the same position. False when the list
/// is full.
pub fn put(list: &mut Vec<Speaker>, speaker: Speaker) -> bool {
    if let Some(known) = list.iter_mut().find(|known| known.pos == speaker.pos) {
        *known = speaker;
        return true;
    }
    if list.len() >= MAX_SPEAKERS {
        return false;
    }
    list.push(speaker);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_speaker_reads_back_as_it_was_saved() {
        let speaker = Speaker { pos: [-3, 70, 12], kind: Kind::Tweeter, channel: Channel::Right, volume: 50 };
        let Value::Compound(map) = speaker.write() else { panic!("not a compound") };
        assert_eq!(Speaker::read(&map), Some(speaker));
    }

    #[test]
    fn a_save_that_names_no_speaker_gives_none_and_odd_values_fall_back() {
        assert_eq!(Speaker::read(&Map::new()), None);
        let Value::Compound(mut map) = Speaker::new([1, 2, 3], Kind::Bass).write() else { panic!() };
        map.insert("Channel".to_owned(), Value::Byte(9));
        map.insert("Volume".to_owned(), Value::Int(250));
        let read = Speaker::read(&map).unwrap();
        assert_eq!((read.channel, read.volume), (Channel::Both, 100));
        map.insert("Kind".to_owned(), Value::Byte(7));
        assert_eq!(Speaker::read(&map), None, "a kind of speaker that does not exist");
    }

    #[test]
    fn a_list_holds_each_place_once_and_no_more_than_its_limit() {
        let mut list = Vec::new();
        for x in 0..MAX_SPEAKERS as i32 {
            assert!(put(&mut list, Speaker::new([x, 0, 0], Kind::Full)));
        }
        assert!(!put(&mut list, Speaker::new([99, 0, 0], Kind::Full)), "full");
        let louder = Speaker { volume: 25, ..Speaker::new([3, 0, 0], Kind::Bass) };
        assert!(put(&mut list, louder), "one already there is replaced, full or not");
        assert_eq!((list.len(), list[3]), (MAX_SPEAKERS, louder));

        let saved = Map::from([("Speakers".to_owned(), Value::List(list.iter().chain(&list).map(Speaker::write).collect()))]);
        assert_eq!(read_all(&saved, "Speakers"), list, "twice the same list reads as one");
    }

    #[test]
    fn clicks_go_round_the_channels_and_the_volumes() {
        assert_eq!(Channel::Both.next().next().next(), Channel::Both);
        assert_eq!(Channel::Both.next().label().english(), "left channel");
        assert_ne!(Channel::Left.label().key, Channel::Right.label().key);
        let mut speaker = Speaker::new([0, 0, 0], Kind::Full);
        let mut seen = Vec::new();
        for _ in 0..5 {
            speaker.volume = speaker.next_volume();
            seen.push(speaker.volume);
        }
        assert_eq!(seen, [75, 50, 25, 100, 75]);
        speaker.volume = 33;
        assert_eq!(speaker.next_volume(), 100, "a volume that is no step starts again");
    }

    #[test]
    fn a_speaker_is_known_by_its_block() {
        assert_eq!(Kind::of_block("webdisplays:speaker_tweeter"), Some(Kind::Tweeter));
        assert_eq!(Kind::of_block("webdisplays:speaker_bass"), Some(Kind::Bass));
        assert_eq!(Kind::of_block("webdisplays:speaker_mix"), Some(Kind::Full));
        assert_eq!(Kind::of_block("webdisplays:screen"), None);
    }
}
