//! Shared browsers: a page with a name, an address and a size of its own, which any number
//! of screens can show, each fitting the whole picture inside itself (`model::letterbox`).
//!
//! Not the same as linked screens, which cut one picture between them (`model::groups`):
//! here every screen shows all of it. This is what puts the same page on a wall of blocks
//! and on a traced screen.
//!
//! The server keeps the list (in a file of the world) and sends it to the players; a
//! screen only remembers which one it shows. One page means one mouse and one keyboard:
//! what a player does on one screen is done to the page everyone sees.
use super::resolution;
use super::text::{MAX_NAME, label};
use super::url;
use crate::proto::wire::{Bad, In, Out};

/// The most shared browsers a world has.
pub const MAX_BROWSERS: usize = 64;
/// What starts the file the list is kept in.
const MAGIC: u32 = 0x5744_4231; // "WDB1"

/// One shared browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shared {
    pub id: u128,
    pub name: String,
    /// The page it shows.
    pub url: String,
    /// The picture's size in pixels.
    pub resolution: [u32; 2],
    /// Who made it: they, and the server's operators, may change or delete it.
    pub owner: u128,
}

/// Why a change to the list was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// `MAX_BROWSERS` reached.
    Full,
    /// No browser by that id.
    Unknown,
    /// Neither its maker nor an operator.
    NotYours,
    /// Not an address a browser can open.
    BadAddress,
}

/// The shared browsers of a world.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Registry {
    list: Vec<Shared>,
}

impl Shared {
    pub fn write(&self, out: &mut Out) {
        out.u128(self.id).text(&self.name).text(&self.url).u32(self.resolution[0]).u32(self.resolution[1]).u128(self.owner);
    }

    pub fn read(input: &mut In) -> Result<Shared, Bad> {
        Ok(Shared {
            id: input.u128()?,
            name: input.text(MAX_NAME * 4)?,
            url: input.text(url::MAX_LEN)?,
            resolution: [input.u32()?, input.u32()?],
            owner: input.u128()?,
        })
    }
}

/// A name as kept: cleaned, and never empty.
fn named(name: &str) -> String {
    let name = label(name, MAX_NAME);
    if name.is_empty() { "Browser".to_owned() } else { name }
}

impl Registry {
    pub fn list(&self) -> &[Shared] {
        &self.list
    }

    pub fn get(&self, id: u128) -> Option<&Shared> {
        self.list.iter().find(|shared| shared.id == id)
    }

    /// Makes a browser for `owner`. `id` is a fresh random number the caller draws.
    pub fn create(&mut self, id: u128, owner: u128, name: &str, address: &str, size: [u32; 2], max: [u32; 2]) -> Result<&Shared, Refused> {
        if self.list.len() >= MAX_BROWSERS {
            return Err(Refused::Full);
        }
        let address = url::clean(address).map_err(|_| Refused::BadAddress)?;
        // An id already taken would hand one browser's screens to another: draw again.
        if self.get(id).is_some() {
            return Err(Refused::Full);
        }
        self.list.push(Shared { id, name: named(name), url: address, resolution: resolution::clamp(size.map(|side| side.max(1)), max), owner });
        Ok(&self.list[self.list.len() - 1])
    }

    fn owned(&mut self, id: u128, by: u128, operator: bool) -> Result<&mut Shared, Refused> {
        let shared = self.list.iter_mut().find(|shared| shared.id == id).ok_or(Refused::Unknown)?;
        if shared.owner == by || operator { Ok(shared) } else { Err(Refused::NotYours) }
    }

    /// Changes a browser's name, address and size, on behalf of `by`.
    pub fn edit(&mut self, id: u128, by: u128, operator: bool, name: &str, address: &str, size: [u32; 2], max: [u32; 2]) -> Result<&Shared, Refused> {
        let address = url::clean(address).map_err(|_| Refused::BadAddress)?;
        let shared = self.owned(id, by, operator)?;
        shared.name = named(name);
        shared.url = address;
        shared.resolution = resolution::clamp(size.map(|side| side.max(1)), max);
        Ok(shared)
    }

    /// Sends a browser to another address: what anyone who may change the address of a
    /// screen showing it can do (the screen's rights were checked by the caller).
    pub fn navigate(&mut self, id: u128, address: &str) -> Result<&Shared, Refused> {
        let address = url::clean(address).map_err(|_| Refused::BadAddress)?;
        let shared = self.list.iter_mut().find(|shared| shared.id == id).ok_or(Refused::Unknown)?;
        shared.url = address;
        Ok(shared)
    }

    pub fn delete(&mut self, id: u128, by: u128, operator: bool) -> Result<(), Refused> {
        self.owned(id, by, operator)?;
        self.list.retain(|shared| shared.id != id);
        Ok(())
    }

    /// The list as the world's file keeps it.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Out::new();
        out.u32(MAGIC).u16(self.list.len() as u16);
        for shared in &self.list {
            shared.write(&mut out);
        }
        out.finish()
    }

    /// The list a file holds. A file that is not one, or is cut short, is refused whole.
    pub fn decode(bytes: &[u8]) -> Result<Registry, Bad> {
        let mut input = In::new(bytes);
        if input.u32()? != MAGIC {
            return Err(Bad::Invalid);
        }
        let count = input.count(MAX_BROWSERS)?;
        let mut list = Vec::with_capacity(count);
        for _ in 0..count {
            list.push(Shared::read(&mut input)?);
        }
        input.end()?;
        Ok(Registry { list })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: [u32; 2] = [1920, 1080];
    const ALEX: u128 = 1;
    const SAM: u128 = 2;

    fn registry() -> Registry {
        let mut registry = Registry::default();
        registry.create(10, ALEX, "Cinema", "example.org/film", [1280, 720], MAX).unwrap();
        registry
    }

    #[test]
    fn a_browser_is_made_with_a_clean_name_address_and_size() {
        let mut registry = Registry::default();
        let made = registry.create(10, ALEX, "  Cinema  ", "example.org/film", [9000, 0], MAX).unwrap().clone();
        assert_eq!((made.name.as_str(), made.url.as_str(), made.owner), ("Cinema", "http://example.org/film", ALEX));
        assert!(made.resolution[0] <= MAX[0] && made.resolution[1] >= 1, "{:?}", made.resolution);
        assert_eq!(registry.create(11, ALEX, "", "example.org", [640, 360], MAX).unwrap().name, "Browser", "a name is never empty");
        assert_eq!(registry.create(12, ALEX, "x", "http://", [640, 360], MAX), Err(Refused::BadAddress));
        assert_eq!(registry.create(10, SAM, "x", "example.org", [640, 360], MAX), Err(Refused::Full), "an id is never given twice");
    }

    #[test]
    fn only_its_maker_or_an_operator_changes_or_deletes_it() {
        let mut registry = registry();
        assert_eq!(registry.edit(10, SAM, false, "Mine", "example.org", [640, 360], MAX), Err(Refused::NotYours));
        assert_eq!(registry.delete(10, SAM, false), Err(Refused::NotYours));
        assert_eq!(registry.edit(10, SAM, true, "Lobby", "example.org/lobby", [640, 360], MAX).unwrap().name, "Lobby");
        assert_eq!(registry.get(10).unwrap().resolution, [640, 360]);
        assert_eq!(registry.edit(99, ALEX, false, "x", "example.org", [1, 1], MAX), Err(Refused::Unknown));
        assert_eq!(registry.delete(10, ALEX, false), Ok(()));
        assert!(registry.list().is_empty());
    }

    #[test]
    fn anyone_let_at_a_screen_sends_its_shared_page_elsewhere() {
        let mut registry = registry();
        assert_eq!(registry.navigate(10, "example.org/other").unwrap().url, "http://example.org/other");
        assert_eq!(registry.navigate(10, ""), Err(Refused::BadAddress));
        assert_eq!(registry.get(10).unwrap().url, "http://example.org/other", "a refused address changes nothing");
    }

    #[test]
    fn a_world_has_only_so_many() {
        let mut registry = Registry::default();
        for id in 0..MAX_BROWSERS as u128 {
            registry.create(id + 1, ALEX, "b", "example.org", [640, 360], MAX).unwrap();
        }
        assert_eq!(registry.create(999, ALEX, "b", "example.org", [640, 360], MAX), Err(Refused::Full));
    }

    #[test]
    fn the_list_is_kept_in_a_file_and_read_back() {
        let mut registry = registry();
        registry.create(11, SAM, "Écran d'accueil", "https://example.org/é", [1920, 1080], MAX).unwrap();
        let bytes = registry.encode();
        assert_eq!(Registry::decode(&bytes), Ok(registry));
        assert_eq!(Registry::decode(&Registry::default().encode()), Ok(Registry::default()));
        assert!(Registry::decode(&bytes[..bytes.len() - 3]).is_err(), "cut short");
        assert!(Registry::decode(b"not a list at all").is_err());
    }
}
