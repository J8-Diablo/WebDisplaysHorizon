//! What the client and the server say to each other about screens.
//!
//! A click on a screen is worked out on the client, which knows where on the face the
//! player aimed: it sends the pixel. The server checks who is asking (their rights on that
//! screen, how far they stand) and passes the event on to every player near the screen, the
//! sender included, so that each one's own browser gets the same click: the page is not
//! shared between players, only what is done to it.
//!
//! A screen's own state (its address, size, rights) does not travel here: it is the block
//! entity's, and the game syncs that.
pub use crate::model::configure::Change;
use crate::model::index::{Entry, PER_PAGE};
use crate::model::shared::Shared;
use crate::model::side::{Pos, Side};
use crate::proto::wire::{Bad, In, Out};

/// The channel the mod's messages travel on (`mod_id:name`).
pub const CHANNEL: &str = "webdisplays_native:screens";
/// The longest address in a message, in bytes.
pub const MAX_URL: usize = 2048;
/// The most key events in one message: a fast typist's batch, not a file.
pub const MAX_KEYS: usize = 64;
/// The longest text entered at once (a paste), in bytes.
pub const MAX_TEXT: usize = 4096;

/// Which screen: its core block and its face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Target {
    pub core: Pos,
    pub side: Side,
}

/// What the pointer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Pointer {
    Move = 0,
    Down = 1,
    Up = 2,
}

/// A key event, as the browser takes it (`proto::ctl::ToHost::Key`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    /// 0 down, 1 up, 2 a character typed.
    pub kind: u8,
    /// The Windows virtual-key code.
    pub key: u32,
    /// The scan code.
    pub native: u32,
    /// The UTF-16 unit typed; 0 when none.
    pub character: u32,
    pub modifiers: u32,
}

/// Something done to a screen's page.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    /// `pixel` is counted in the screen's saved resolution; `button` is 0 left, 1 middle,
    /// 2 right (ignored for a move).
    Pointer { what: Pointer, pixel: [u32; 2], button: u8 },
    Scroll { pixel: [u32; 2], delta: [i32; 2] },
    Keys(Vec<Key>),
    /// Text entered at once: a paste, or what an input method composed.
    Text(String),
}

/// The client to the server.
#[derive(Clone, Debug, PartialEq)]
pub enum ToServer {
    Input { target: Target, input: Input },
    /// Show this address on the screen.
    SetUrl { target: Target, url: String },
    Configure { target: Target, change: Change },
    /// The two addresses of the redstone controller at `pos`: signal on, signal off.
    SetRedstone { pos: Pos, rising: String, falling: String },
    /// The player marked this block corner with the pen; None wipes their outline.
    Pen { corner: Option<Pos> },
    /// Makes, changes or deletes a shared browser.
    Browser(BrowserEdit),
    /// Asks for a page of the list of the world's screens.
    Screens { page: u16 },
    /// Joins two screens: gives them one link, so that they share a picture.
    Join { target: Target, other: Target },
    /// Bends or turns the sheet of a traced screen.
    Reshape { target: Target, change: Reshape },
}

/// What a player asks of a traced screen's sheet (`model::poly::Outline`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reshape {
    /// Its corners rounded, or left sharp.
    Curved(bool),
    /// The page on its other face.
    Flipped(bool),
    /// The radius of a rounded corner, in blocks.
    Radius(f32),
}

/// What a player asks of the world's shared browsers (`model::shared`).
#[derive(Clone, Debug, PartialEq)]
pub enum BrowserEdit {
    Create { name: String, url: String, size: [u32; 2] },
    Edit { id: u128, name: String, url: String, size: [u32; 2] },
    Delete { id: u128 },
}

/// The most corners of an outline in a message (`model::poly::MAX_POINTS`).
pub const MAX_CORNERS: usize = 64;

/// The longest name of a player or of an item in a message, in bytes.
pub const MAX_NAME: usize = 128;

fn write_change(out: &mut Out, change: &Change) {
    match change {
        Change::Rotation(turns) => {
            out.u8(0).u8(*turns);
        }
        Change::Resolution(size) => {
            out.u8(1).u32(size[0]).u32(size[1]);
        }
        Change::Volume(volume) => {
            out.u8(2).f32(*volume);
        }
        Change::AutoVolume(on) => {
            out.u8(3).flag(*on);
        }
        Change::ShapeMode(mode) => {
            out.u8(4).u8(*mode);
        }
        Change::Oled(on) => {
            out.u8(5).flag(*on);
        }
        Change::Name(name) => {
            out.u8(6).text(name);
        }
        Change::Rights { friends, others } => {
            out.u8(7).u8(*friends).u8(*others);
        }
        Change::AddFriend(name) => {
            out.u8(8).text(name);
        }
        Change::RemoveFriend(uuid) => {
            out.u8(9).u128(*uuid);
        }
        Change::RemoveUpgrade(item) => {
            out.u8(10).text(item);
        }
        Change::Link(id) => {
            out.u8(11).text(id);
        }
        Change::LinkMode(mode) => {
            out.u8(12).u8(*mode);
        }
        Change::Browser(id) => {
            out.u8(13).id(*id);
        }
    }
}

fn read_change(input: &mut In) -> Result<Change, Bad> {
    Ok(match input.u8()? {
        0 => Change::Rotation(input.u8()?),
        1 => Change::Resolution([input.u32()?, input.u32()?]),
        2 => Change::Volume(input.f32()?),
        3 => Change::AutoVolume(input.flag()?),
        4 => Change::ShapeMode(input.u8()?),
        5 => Change::Oled(input.flag()?),
        6 => Change::Name(input.text(MAX_NAME)?),
        7 => Change::Rights { friends: input.u8()?, others: input.u8()? },
        8 => Change::AddFriend(input.text(MAX_NAME)?),
        9 => Change::RemoveFriend(input.u128()?),
        10 => Change::RemoveUpgrade(input.text(MAX_NAME)?),
        11 => Change::Link(input.text(MAX_NAME)?),
        12 => Change::LinkMode(input.u8()?),
        13 => Change::Browser(input.id()?),
        _ => return Err(Bad::Invalid),
    })
}

/// The server to a client.
#[derive(Clone, Debug, PartialEq)]
pub enum ToClient {
    /// A player near the screen did this to its page: do the same to yours.
    Input { target: Target, input: Input },
    /// The player used a keyboard linked to this screen: what they type now goes to it.
    OpenKeyboard { target: Target },
    /// The player used a remote control linked to this screen, which shows `url`: let them
    /// type another address for it. The screen may be too far for their client to know it.
    OpenUrl { target: Target, url: String },
    /// The player used the configurator on this screen. `player` is who they are to the
    /// server: the window shows them what they may change.
    OpenConfig { target: Target, player: u128 },
    /// The player used the redstone controller at `pos`, which holds these two addresses.
    OpenRedstone { pos: Pos, rising: String, falling: String },
    /// The corners the player has marked with the pen so far, to draw; none once the
    /// outline is closed or wiped.
    Pen { points: Vec<Pos> },
    /// A shared browser of the world, new or changed.
    Browser(Shared),
    /// A shared browser was deleted.
    BrowserGone { id: u128 },
    /// One page of the list of the world's screens, of `pages`.
    Screens { page: u16, pages: u16, entries: Vec<Entry> },
}

fn write_target(out: &mut Out, target: &Target) {
    out.pos(target.core).u8(target.side.index());
}

fn read_target(input: &mut In) -> Result<Target, Bad> {
    let core = input.pos()?;
    let side = Side::from_index(input.u8()?).ok_or(Bad::Invalid)?;
    Ok(Target { core, side })
}

fn write_input(out: &mut Out, input: &Input) {
    match input {
        Input::Pointer { what, pixel, button } => {
            out.u8(0).u8(*what as u8).u32(pixel[0]).u32(pixel[1]).u8(*button);
        }
        Input::Scroll { pixel, delta } => {
            out.u8(1).u32(pixel[0]).u32(pixel[1]).i32(delta[0]).i32(delta[1]);
        }
        Input::Keys(keys) => {
            let keys = &keys[..keys.len().min(MAX_KEYS)];
            out.u8(2).u16(keys.len() as u16);
            for key in keys {
                out.u8(key.kind).u32(key.key).u32(key.native).u32(key.character).u32(key.modifiers);
            }
        }
        Input::Text(text) => {
            out.u8(3).text(text);
        }
    }
}

fn read_input(input: &mut In) -> Result<Input, Bad> {
    Ok(match input.u8()? {
        0 => {
            let what = match input.u8()? {
                0 => Pointer::Move,
                1 => Pointer::Down,
                2 => Pointer::Up,
                _ => return Err(Bad::Invalid),
            };
            let pixel = [input.u32()?, input.u32()?];
            let button = input.u8()?;
            if button > 2 {
                return Err(Bad::Invalid);
            }
            Input::Pointer { what, pixel, button }
        }
        1 => Input::Scroll { pixel: [input.u32()?, input.u32()?], delta: [input.i32()?, input.i32()?] },
        2 => {
            let count = input.count(MAX_KEYS)?;
            let mut keys = Vec::with_capacity(count);
            for _ in 0..count {
                let kind = input.u8()?;
                if kind > 2 {
                    return Err(Bad::Invalid);
                }
                keys.push(Key { kind, key: input.u32()?, native: input.u32()?, character: input.u32()?, modifiers: input.u32()? });
            }
            Input::Keys(keys)
        }
        3 => Input::Text(input.text(MAX_TEXT)?),
        _ => return Err(Bad::Invalid),
    })
}

impl ToServer {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Out::new();
        match self {
            ToServer::Input { target, input } => {
                out.u8(0);
                write_target(&mut out, target);
                write_input(&mut out, input);
            }
            ToServer::SetUrl { target, url } => {
                out.u8(1);
                write_target(&mut out, target);
                out.text(url);
            }
            ToServer::Configure { target, change } => {
                out.u8(2);
                write_target(&mut out, target);
                write_change(&mut out, change);
            }
            ToServer::SetRedstone { pos, rising, falling } => {
                out.u8(3).pos(*pos).text(rising).text(falling);
            }
            ToServer::Pen { corner } => {
                out.u8(4).flag(corner.is_some());
                if let Some(corner) = corner {
                    out.pos(*corner);
                }
            }
            ToServer::Browser(BrowserEdit::Create { name, url, size }) => {
                out.u8(5).u8(0).text(name).text(url).u32(size[0]).u32(size[1]);
            }
            ToServer::Browser(BrowserEdit::Edit { id, name, url, size }) => {
                out.u8(5).u8(1).u128(*id).text(name).text(url).u32(size[0]).u32(size[1]);
            }
            ToServer::Browser(BrowserEdit::Delete { id }) => {
                out.u8(5).u8(2).u128(*id);
            }
            ToServer::Screens { page } => {
                out.u8(6).u16(*page);
            }
            ToServer::Join { target, other } => {
                out.u8(7);
                write_target(&mut out, target);
                write_target(&mut out, other);
            }
            ToServer::Reshape { target, change } => {
                out.u8(8);
                write_target(&mut out, target);
                match change {
                    Reshape::Curved(on) => out.u8(0).flag(*on),
                    Reshape::Flipped(on) => out.u8(1).flag(*on),
                    Reshape::Radius(radius) => out.u8(2).f32(*radius),
                };
            }
        }
        out.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Bad> {
        let mut input = In::new(bytes);
        let message = match input.u8()? {
            0 => ToServer::Input { target: read_target(&mut input)?, input: read_input(&mut input)? },
            1 => ToServer::SetUrl { target: read_target(&mut input)?, url: input.text(MAX_URL)? },
            2 => ToServer::Configure { target: read_target(&mut input)?, change: read_change(&mut input)? },
            3 => ToServer::SetRedstone { pos: input.pos()?, rising: input.text(MAX_URL)?, falling: input.text(MAX_URL)? },
            4 => ToServer::Pen { corner: if input.flag()? { Some(input.pos()?) } else { None } },
            6 => ToServer::Screens { page: input.u16()? },
            7 => ToServer::Join { target: read_target(&mut input)?, other: read_target(&mut input)? },
            8 => {
                let target = read_target(&mut input)?;
                let change = match input.u8()? {
                    0 => Reshape::Curved(input.flag()?),
                    1 => Reshape::Flipped(input.flag()?),
                    2 => Reshape::Radius(input.f32()?),
                    _ => return Err(Bad::Invalid),
                };
                ToServer::Reshape { target, change }
            }
            5 => ToServer::Browser(match input.u8()? {
                0 => BrowserEdit::Create { name: input.text(MAX_NAME)?, url: input.text(MAX_URL)?, size: [input.u32()?, input.u32()?] },
                1 => BrowserEdit::Edit { id: input.u128()?, name: input.text(MAX_NAME)?, url: input.text(MAX_URL)?, size: [input.u32()?, input.u32()?] },
                2 => BrowserEdit::Delete { id: input.u128()? },
                _ => return Err(Bad::Invalid),
            }),
            _ => return Err(Bad::Invalid),
        };
        input.end()?;
        Ok(message)
    }
}

impl ToClient {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Out::new();
        match self {
            ToClient::Input { target, input } => {
                out.u8(0);
                write_target(&mut out, target);
                write_input(&mut out, input);
            }
            ToClient::OpenKeyboard { target } => {
                out.u8(1);
                write_target(&mut out, target);
            }
            ToClient::OpenUrl { target, url } => {
                out.u8(2);
                write_target(&mut out, target);
                out.text(url);
            }
            ToClient::OpenConfig { target, player } => {
                out.u8(3);
                write_target(&mut out, target);
                out.u128(*player);
            }
            ToClient::OpenRedstone { pos, rising, falling } => {
                out.u8(4).pos(*pos).text(rising).text(falling);
            }
            ToClient::Browser(shared) => {
                out.u8(6);
                shared.write(&mut out);
            }
            ToClient::BrowserGone { id } => {
                out.u8(7).u128(*id);
            }
            ToClient::Screens { page, pages, entries } => {
                let entries = &entries[..entries.len().min(PER_PAGE)];
                out.u8(8).u16(*page).u16(*pages).u16(entries.len() as u16);
                for entry in entries {
                    entry.write(&mut out);
                }
            }
            ToClient::Pen { points } => {
                let points = &points[..points.len().min(MAX_CORNERS)];
                out.u8(5).u16(points.len() as u16);
                for point in points {
                    out.pos(*point);
                }
            }
        }
        out.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Bad> {
        let mut input = In::new(bytes);
        let message = match input.u8()? {
            0 => ToClient::Input { target: read_target(&mut input)?, input: read_input(&mut input)? },
            1 => ToClient::OpenKeyboard { target: read_target(&mut input)? },
            2 => ToClient::OpenUrl { target: read_target(&mut input)?, url: input.text(MAX_URL)? },
            3 => ToClient::OpenConfig { target: read_target(&mut input)?, player: input.u128()? },
            4 => ToClient::OpenRedstone { pos: input.pos()?, rising: input.text(MAX_URL)?, falling: input.text(MAX_URL)? },
            6 => ToClient::Browser(Shared::read(&mut input)?),
            7 => ToClient::BrowserGone { id: input.u128()? },
            8 => {
                let (page, pages) = (input.u16()?, input.u16()?);
                let count = input.count(PER_PAGE)?;
                let mut entries = Vec::with_capacity(count);
                for _ in 0..count {
                    entries.push(Entry::read(&mut input)?);
                }
                ToClient::Screens { page, pages, entries }
            }
            5 => {
                let count = input.count(MAX_CORNERS)?;
                let mut points = Vec::with_capacity(count);
                for _ in 0..count {
                    points.push(input.pos()?);
                }
                ToClient::Pen { points }
            }
            _ => return Err(Bad::Invalid),
        };
        input.end()?;
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: Target = Target { core: [-31, 68, 6], side: Side::North };

    fn inputs() -> Vec<Input> {
        vec![
            Input::Pointer { what: Pointer::Move, pixel: [0, 0], button: 0 },
            Input::Pointer { what: Pointer::Down, pixel: [1919, 1079], button: 2 },
            Input::Pointer { what: Pointer::Up, pixel: [640, 360], button: 1 },
            Input::Scroll { pixel: [10, 20], delta: [0, -120] },
            Input::Keys(vec![
                Key { kind: 0, key: 0x41, native: 30, character: 0, modifiers: 2 },
                Key { kind: 2, key: 0xe9, native: 0, character: 0xe9, modifiers: 0 },
                Key { kind: 1, key: 0x41, native: 30, character: 0, modifiers: 0 },
            ]),
            Input::Keys(Vec::new()),
            Input::Text("collé depuis le presse-papiers".to_owned()),
        ]
    }

    #[test]
    fn every_message_reads_back_as_it_was_written() {
        for input in inputs() {
            let up = ToServer::Input { target: TARGET, input: input.clone() };
            assert_eq!(ToServer::decode(&up.encode()), Ok(up), "{input:?}");
            let down = ToClient::Input { target: TARGET, input: input.clone() };
            assert_eq!(ToClient::decode(&down.encode()), Ok(down), "{input:?}");
        }
        let url = ToServer::SetUrl { target: TARGET, url: "https://example.org/é?x=100%25".to_owned() };
        assert_eq!(ToServer::decode(&url.encode()), Ok(url));
        let opens = [
            ToClient::OpenKeyboard { target: TARGET },
            ToClient::OpenUrl { target: TARGET, url: "https://example.org/".to_owned() },
            ToClient::OpenConfig { target: TARGET, player: 0x0123_4567_89ab_cdef_0011_2233_4455_6677 },
            ToClient::OpenRedstone { pos: [1, -2, 3], rising: "https://on.example/".to_owned(), falling: String::new() },
            ToClient::Pen { points: vec![[0, 64, 0], [6, 64, 0], [6, 68, -2]] },
            ToClient::Pen { points: Vec::new() },
        ];
        for pen in [ToServer::Pen { corner: Some([4, 70, -9]) }, ToServer::Pen { corner: None }] {
            assert_eq!(ToServer::decode(&pen.encode()), Ok(pen));
        }
        let redstone = ToServer::SetRedstone { pos: [1, -2, 3], rising: String::new(), falling: "https://off.example/".to_owned() };
        assert_eq!(ToServer::decode(&redstone.encode()), Ok(redstone));
        for down in opens {
            assert_eq!(ToClient::decode(&down.encode()), Ok(down));
        }
        let changes = [
            Change::Rotation(3),
            Change::Resolution([1280, 720]),
            Change::Volume(42.5),
            Change::AutoVolume(false),
            Change::ShapeMode(2),
            Change::Oled(true),
            Change::Name("Salon".to_owned()),
            Change::Rights { friends: 63, others: 2 },
            Change::AddFriend("Alex".to_owned()),
            Change::RemoveFriend(7),
            Change::RemoveUpgrade("webdisplays:upgrade_lasermouse".to_owned()),
            Change::Link("wall".to_owned()),
            Change::LinkMode(1),
            Change::Browser(Some(77)),
            Change::Browser(None),
        ];
        let shared = Shared { id: 9, name: "Cinema".to_owned(), url: "https://example.org/".to_owned(), resolution: [1280, 720], owner: 4 };
        for down in [ToClient::Browser(shared), ToClient::BrowserGone { id: 9 }] {
            assert_eq!(ToClient::decode(&down.encode()), Ok(down));
        }
        let edits = [
            BrowserEdit::Create { name: "Cinema".to_owned(), url: "example.org".to_owned(), size: [1280, 720] },
            BrowserEdit::Edit { id: 9, name: "Lobby".to_owned(), url: "example.org/lobby".to_owned(), size: [640, 360] },
            BrowserEdit::Delete { id: 9 },
        ];
        for edit in edits {
            let up = ToServer::Browser(edit);
            assert_eq!(ToServer::decode(&up.encode()), Ok(up));
        }
        let other = Target { core: [5, 70, -3], side: Side::Top };
        let reshapes = [Reshape::Curved(true), Reshape::Flipped(false), Reshape::Radius(2.5)].map(|change| ToServer::Reshape { target: TARGET, change });
        for up in reshapes {
            assert_eq!(ToServer::decode(&up.encode()), Ok(up));
        }
        for up in [ToServer::Screens { page: 3 }, ToServer::Join { target: TARGET, other }] {
            assert_eq!(ToServer::decode(&up.encode()), Ok(up));
        }
        let listed = Entry { core: [1, 64, 2], side: 3, dimension: "minecraft:overworld".to_owned(), name: "Salon".to_owned(), size: [4, 3], link: "wall".to_owned(), traced: false };
        let page = ToClient::Screens { page: 1, pages: 4, entries: vec![listed.clone(), Entry { traced: true, name: String::new(), ..listed }] };
        assert_eq!(ToClient::decode(&page.encode()), Ok(page));
        for change in changes {
            let up = ToServer::Configure { target: TARGET, change };
            assert_eq!(ToServer::decode(&up.encode()), Ok(up.clone()), "{up:?}");
            let bytes = up.encode();
            for cut in 0..bytes.len() {
                assert!(ToServer::decode(&bytes[..cut]).is_err(), "{up:?} cut at {cut}");
            }
        }
    }

    #[test]
    fn a_message_cut_short_or_with_bytes_to_spare_is_refused() {
        for input in inputs() {
            let bytes = ToServer::Input { target: TARGET, input }.encode();
            for cut in 0..bytes.len() {
                assert!(ToServer::decode(&bytes[..cut]).is_err(), "cut at {cut} of {}", bytes.len());
            }
            let mut longer = bytes;
            longer.push(0);
            assert_eq!(ToServer::decode(&longer), Err(Bad::Trailing));
        }
    }

    #[test]
    fn what_names_nothing_is_refused() {
        let pointer = ToServer::Input { target: TARGET, input: Input::Pointer { what: Pointer::Down, pixel: [1, 2], button: 0 } };
        let bytes = pointer.encode();
        let with = |at: usize, value: u8| {
            let mut changed = bytes.clone();
            changed[at] = value;
            ToServer::decode(&changed)
        };
        assert_eq!(with(0, 9), Err(Bad::Invalid), "a kind of message that does not exist");
        assert_eq!(with(13, 6), Err(Bad::Invalid), "a seventh side");
        assert_eq!(with(14, 9), Err(Bad::Invalid), "a kind of input that does not exist");
        assert_eq!(with(15, 3), Err(Bad::Invalid), "a fourth thing a pointer does");
        assert_eq!(with(24, 3), Err(Bad::Invalid), "a fourth mouse button");
    }

    #[test]
    fn more_keys_or_text_than_a_message_holds_is_refused_on_arrival() {
        // A client of ours never sends more: it cuts the batch.
        let many = vec![Key { kind: 0, key: 1, native: 1, character: 0, modifiers: 0 }; MAX_KEYS + 10];
        let sent = ToServer::Input { target: TARGET, input: Input::Keys(many) }.encode();
        let ToServer::Input { input: Input::Keys(keys), .. } = ToServer::decode(&sent).unwrap() else {
            panic!("not keys");
        };
        assert_eq!(keys.len(), MAX_KEYS);
        // One that is not ours, announcing more than a batch.
        let mut lying = ToServer::Input { target: TARGET, input: Input::Keys(Vec::new()) }.encode();
        let count = lying.len() - 2;
        lying[count..].copy_from_slice(&((MAX_KEYS + 1) as u16).to_le_bytes());
        assert_eq!(ToServer::decode(&lying), Err(Bad::TooLong));
        let long = ToServer::SetUrl { target: TARGET, url: "a".repeat(MAX_URL + 1) }.encode();
        assert_eq!(ToServer::decode(&long), Err(Bad::TooLong));
    }
}
