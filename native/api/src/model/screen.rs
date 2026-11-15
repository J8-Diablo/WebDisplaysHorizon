//! A screen: one face of a multiblock showing a browser, and everything it remembers.
//!
//! A block holds up to six of them, one for each face. They are saved under the Java mod's
//! own keys (`ScreenData.serialize`), so a compound means the same to both. What was read
//! from a save is not trusted to be sensible: names are cleaned, lists are cut to their
//! limits, numbers that name nothing fall back to their default.
use super::nbt::{self, Map, Value};
use super::rights::{Access, DEFAULTS, MAX_FRIENDS, Player};
use super::side::{LinkMode, Rotation, ShapeMode, Side};
use super::speaker::{self, Speaker};
use super::text::{MAX_LINK_ID, MAX_NAME, label};
use super::url::MAX_LEN;

/// The key a block entity saves its screens under.
pub const SCREENS: &str = "WDScreens";
/// The most upgrades a screen holds.
pub const MAX_UPGRADES: usize = 16;
/// The loudest a screen plays, and what a new one plays at.
pub const MAX_VOLUME: f32 = 100.0;
/// The upgrade that lets a laser pointer work the screen from afar.
pub const UPGRADE_LASER_MOUSE: &str = "webdisplays:upgrade_lasermouse";

/// One screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    /// The face of the blocks it is on.
    pub side: Side,
    /// Its extent in blocks, across and up, before its rotation.
    pub size: [u32; 2],
    /// The picture's size in pixels as the player set it, before the client's quality scale.
    pub resolution: [u32; 2],
    pub rotation: Rotation,
    /// The page it shows. Empty until one is set.
    pub url: String,
    pub access: Access,
    /// The ids of the upgrade items put in it, one of each at most.
    pub upgrades: Vec<String>,
    /// Quieter with distance. Kept from the Java mod; the sound engine decides what it means.
    pub auto_volume: bool,
    /// 0 to `MAX_VOLUME`.
    pub volume: f32,
    pub shape_mode: ShapeMode,
    /// The link group it belongs to; empty for none.
    pub link_id: String,
    pub link_mode: LinkMode,
    /// It owns its link group's browser.
    pub link_origin: bool,
    /// Black shows as nothing, as on an OLED panel.
    pub oled: bool,
    /// The name the player gave it; empty for none.
    pub name: String,
    /// The shared browser it shows instead of one of its own.
    pub browser: Option<u128>,
    /// The speaker blocks it plays through; none, and it plays itself.
    pub speakers: Vec<Speaker>,
}

impl Screen {
    /// A screen just made on `side` by `owner`, `size` blocks, with everything else at its
    /// default. Its resolution is left for the caller to choose (`model::resolution`).
    pub fn new(side: Side, size: [u32; 2], owner: Player) -> Self {
        Self {
            side,
            size,
            resolution: [0, 0],
            rotation: Rotation::R0,
            url: String::new(),
            access: Access::new(owner),
            upgrades: Vec::new(),
            auto_volume: true,
            volume: MAX_VOLUME,
            shape_mode: ShapeMode::None,
            link_id: String::new(),
            link_mode: LinkMode::Joined,
            link_origin: false,
            oled: false,
            name: String::new(),
            browser: None,
            speakers: Vec::new(),
        }
    }

    pub fn is_linked(&self) -> bool {
        !self.link_id.is_empty()
    }

    pub fn has_upgrade(&self, id: &str) -> bool {
        self.upgrades.iter().any(|upgrade| upgrade == id)
    }

    /// Puts an upgrade in. False when it is in already or there is no room.
    pub fn add_upgrade(&mut self, id: &str) -> bool {
        if self.has_upgrade(id) || self.upgrades.len() >= MAX_UPGRADES {
            return false;
        }
        self.upgrades.push(id.to_owned());
        true
    }

    /// Takes an upgrade out. False when it was not in.
    pub fn remove_upgrade(&mut self, id: &str) -> bool {
        let before = self.upgrades.len();
        self.upgrades.retain(|upgrade| upgrade != id);
        self.upgrades.len() != before
    }

    /// This screen as it is saved.
    pub fn write(&self) -> Map {
        let player = |player: &Player, name: &str, id: &str| {
            [(name.to_owned(), Value::String(player.name.clone())), (id.to_owned(), nbt::uuid_value(player.uuid))]
        };
        let mut map = Map::new();
        let mut put = |key: &str, value: Value| {
            map.insert(key.to_owned(), value);
        };
        put("Side", Value::Byte(self.side.index() as i8));
        put("Width", Value::Int(self.size[0] as i32));
        put("Height", Value::Int(self.size[1] as i32));
        put("ResolutionX", Value::Int(self.resolution[0] as i32));
        put("ResolutionY", Value::Int(self.resolution[1] as i32));
        put("Rotation", Value::Byte(self.rotation.index() as i8));
        put("URL", Value::String(self.url.clone()));
        if let Some(owner) = &self.access.owner {
            for (key, value) in player(owner, "OwnerName", "OwnerUUID") {
                put(&key, value);
            }
        }
        let friends = self.access.friends.iter().map(|friend| Value::Compound(Map::from(player(friend, "Name", "UUID"))));
        put("Friends", Value::List(friends.collect()));
        put("FriendRights", Value::Byte(self.access.friend_rights as i8));
        put("OtherRights", Value::Byte(self.access.other_rights as i8));
        let upgrades = self.upgrades.iter().map(|id| {
            Value::Compound(Map::from([
                ("id".to_owned(), Value::String(id.clone())),
                ("count".to_owned(), Value::Int(1)),
            ]))
        });
        put("Upgrades", Value::List(upgrades.collect()));
        put("AutoVolume", nbt::flag_value(self.auto_volume));
        put("Volume", Value::Float(self.volume));
        put("ShapeMode", Value::Byte(self.shape_mode.index() as i8));
        put("LinkId", Value::String(self.link_id.clone()));
        put("LinkMode", Value::Byte(self.link_mode.index() as i8));
        put("LinkOrigin", nbt::flag_value(self.link_origin));
        put("OledMode", nbt::flag_value(self.oled));
        put("Name", Value::String(self.name.clone()));
        if let Some(browser) = self.browser {
            put("BrowserId", nbt::uuid_value(browser));
        }
        if !self.speakers.is_empty() {
            put("Speakers", Value::List(self.speakers.iter().map(Speaker::write).collect()));
        }
        map
    }

    /// A screen as it was saved. None when the save names no side or no size: there is no
    /// screen to make of that.
    pub fn read(map: &Map) -> Option<Self> {
        let index = |key: &str| nbt::int(map, key).and_then(|v| u8::try_from(v).ok());
        let side = Side::from_index(index("Side")?)?;
        let extent = |key: &str| nbt::int(map, key).and_then(|v| u32::try_from(v).ok());
        let size = [extent("Width")?, extent("Height")?];
        if size[0] == 0 || size[1] == 0 {
            return None;
        }
        let read_player = |map: &Map, name: &str, id: &str| {
            Some(Player { uuid: nbt::uuid(map, id)?, name: label(nbt::text(map, name)?, MAX_NAME) })
        };
        let mut access = Access {
            owner: read_player(map, "OwnerName", "OwnerUUID"),
            friends: Vec::new(),
            friend_rights: index("FriendRights").unwrap_or(DEFAULTS),
            other_rights: index("OtherRights").unwrap_or(DEFAULTS),
        };
        // Through `add_friend`, so a save cannot hold the owner, a friend twice or too many.
        for friend in nbt::compounds(map, "Friends").filter_map(|friend| read_player(friend, "Name", "UUID")).take(MAX_FRIENDS) {
            let _ = access.add_friend(friend);
        }
        let mut screen = Screen {
            side,
            size,
            resolution: [extent("ResolutionX").unwrap_or(0), extent("ResolutionY").unwrap_or(0)],
            rotation: index("Rotation").and_then(Rotation::from_index).unwrap_or_default(),
            url: nbt::text(map, "URL").filter(|url| url.len() <= MAX_LEN).unwrap_or_default().to_owned(),
            access,
            upgrades: Vec::new(),
            auto_volume: nbt::flag(map, "AutoVolume").unwrap_or(true),
            volume: nbt::float(map, "Volume").filter(|v| v.is_finite()).unwrap_or(MAX_VOLUME).clamp(0.0, MAX_VOLUME),
            shape_mode: index("ShapeMode").and_then(ShapeMode::from_index).unwrap_or_default(),
            link_id: label(nbt::text(map, "LinkId").unwrap_or_default(), MAX_LINK_ID),
            link_mode: index("LinkMode").and_then(LinkMode::from_index).unwrap_or_default(),
            link_origin: nbt::flag(map, "LinkOrigin").unwrap_or(false),
            oled: nbt::flag(map, "OledMode").unwrap_or(false),
            name: label(nbt::text(map, "Name").unwrap_or_default(), MAX_NAME),
            browser: nbt::uuid(map, "BrowserId"),
            speakers: speaker::read_all(map, "Speakers"),
        };
        for upgrade in nbt::compounds(map, "Upgrades").filter_map(|upgrade| nbt::text(upgrade, "id")) {
            screen.add_upgrade(upgrade);
        }
        // A screen that is in no group owns no group's browser.
        screen.link_origin &= screen.is_linked();
        Some(screen)
    }
}

/// The screens of one block as they are saved, under `SCREENS`.
pub fn write_all(screens: &[Screen]) -> Value {
    Value::List(screens.iter().map(|screen| Value::Compound(screen.write())).collect())
}

/// The screens of one block as they were saved: one for each side at most, the first of
/// a side winning.
pub fn read_all(block: &Map) -> Vec<Screen> {
    let mut screens: Vec<Screen> = Vec::new();
    for screen in nbt::compounds(block, SCREENS).filter_map(Screen::read) {
        if !screens.iter().any(|kept| kept.side == screen.side) {
            screens.push(screen);
        }
    }
    screens
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::rights::{INTERACT, KNOWN};

    fn player(uuid: u128, name: &str) -> Player {
        Player { uuid, name: name.to_owned() }
    }

    fn full() -> Screen {
        let mut screen = Screen::new(Side::South, [4, 3], player(1, "Owner"));
        screen.resolution = [1280, 960];
        screen.rotation = Rotation::R90;
        screen.url = "https://example.org/a?b=100%25".to_owned();
        screen.access.add_friend(player(2, "Friend")).unwrap();
        screen.access.friend_rights = KNOWN;
        screen.access.other_rights = INTERACT;
        screen.add_upgrade(UPGRADE_LASER_MOUSE);
        screen.auto_volume = false;
        screen.volume = 35.5;
        screen.shape_mode = ShapeMode::SmoothLong;
        screen.link_id = "grp-12ab".to_owned();
        screen.link_mode = LinkMode::Spaced;
        screen.link_origin = true;
        screen.oled = true;
        screen.name = "Main stage".to_owned();
        screen.browser = Some(0xdead_beef_0000_0000_0000_0000_0000_0042);
        screen
    }

    #[test]
    fn a_screen_reads_back_as_it_was_saved() {
        let screen = full();
        assert_eq!(Screen::read(&screen.write()), Some(screen));
        let plain = Screen::new(Side::Top, [1, 1], player(9, "Solo"));
        assert_eq!(Screen::read(&plain.write()), Some(plain));
    }

    #[test]
    fn it_is_saved_under_the_java_mods_keys() {
        let map = full().write();
        assert_eq!(map.get("Side"), Some(&Value::Byte(3)));
        assert_eq!((map.get("Width"), map.get("Height")), (Some(&Value::Int(4)), Some(&Value::Int(3))));
        assert_eq!(map.get("ResolutionX"), Some(&Value::Int(1280)));
        assert_eq!(map.get("Rotation"), Some(&Value::Byte(1)));
        assert_eq!(map.get("OwnerUUID"), Some(&Value::IntArray(vec![0, 0, 0, 1])));
        assert_eq!(map.get("FriendRights"), Some(&Value::Byte(63)));
        assert_eq!(map.get("AutoVolume"), Some(&Value::Byte(0)));
        assert_eq!(map.get("Volume"), Some(&Value::Float(35.5)));
        assert_eq!(map.get("LinkMode"), Some(&Value::Byte(1)));
        assert_eq!(map.get("OledMode"), Some(&Value::Byte(1)));
        for key in ["URL", "OwnerName", "Friends", "OtherRights", "Upgrades", "ShapeMode", "LinkId", "LinkOrigin", "Name", "BrowserId"] {
            assert!(map.contains_key(key), "{key}");
        }
        assert!(!Screen::new(Side::Top, [1, 1], player(9, "Solo")).write().contains_key("BrowserId"));
    }

    #[test]
    fn what_a_save_lacks_takes_its_default() {
        let map = Map::from([
            ("Side".to_owned(), Value::Byte(2)),
            ("Width".to_owned(), Value::Int(2)),
            ("Height".to_owned(), Value::Int(2)),
        ]);
        let screen = Screen::read(&map).unwrap();
        assert_eq!((screen.side, screen.size, screen.resolution), (Side::North, [2, 2], [0, 0]));
        assert_eq!(screen.access.owner, None);
        assert_eq!((screen.access.friend_rights, screen.access.other_rights), (DEFAULTS, DEFAULTS));
        assert_eq!((screen.volume, screen.auto_volume), (MAX_VOLUME, true));
        assert_eq!((screen.rotation, screen.shape_mode, screen.link_mode), (Rotation::R0, ShapeMode::None, LinkMode::Joined));
        assert!(!screen.oled && !screen.is_linked() && screen.browser.is_none() && screen.url.is_empty());
    }

    #[test]
    fn a_save_that_names_no_side_or_no_size_is_no_screen() {
        let with = |key: &str, value: Value| {
            let mut map = full().write();
            map.insert(key.to_owned(), value);
            Screen::read(&map)
        };
        assert_eq!(with("Side", Value::Byte(6)), None);
        assert_eq!(with("Side", Value::Byte(-1)), None);
        assert_eq!(with("Width", Value::Int(0)), None);
        assert_eq!(with("Height", Value::Int(-4)), None);
        let mut missing = full().write();
        missing.remove("Side");
        assert_eq!(Screen::read(&missing), None);
    }

    #[test]
    fn nonsense_in_a_save_is_put_right() {
        let mut map = full().write();
        map.insert("Rotation".to_owned(), Value::Byte(9));
        map.insert("ShapeMode".to_owned(), Value::Byte(-2));
        map.insert("LinkMode".to_owned(), Value::Byte(7));
        map.insert("Volume".to_owned(), Value::Float(f32::NAN));
        map.insert("Name".to_owned(), Value::String(format!("  \u{a7}c{}  ", "n".repeat(80))));
        map.insert("URL".to_owned(), Value::String("x".repeat(MAX_LEN + 1)));
        let screen = Screen::read(&map).unwrap();
        assert_eq!((screen.rotation, screen.shape_mode, screen.link_mode), (Rotation::R0, ShapeMode::None, LinkMode::Joined));
        assert_eq!(screen.volume, MAX_VOLUME);
        assert_eq!(screen.name, format!("c{}", "n".repeat(MAX_NAME - 1)));
        assert_eq!(screen.url, "", "an address past the limit is dropped, not cut");

        map.insert("Volume".to_owned(), Value::Float(250.0));
        assert_eq!(Screen::read(&map).unwrap().volume, MAX_VOLUME);
        map.insert("LinkId".to_owned(), Value::String(String::new()));
        assert!(!Screen::read(&map).unwrap().link_origin, "no group, so no group's browser to own");
    }

    #[test]
    fn friends_and_upgrades_are_kept_within_their_limits() {
        let mut map = full().write();
        let friend = |uuid: u128| {
            Value::Compound(Map::from([
                ("Name".to_owned(), Value::String(format!("f{uuid}"))),
                ("UUID".to_owned(), nbt::uuid_value(uuid)),
            ]))
        };
        // The owner, one friend twice, then far too many.
        let friends = [1, 2, 2].into_iter().chain(10..200).map(friend).collect();
        map.insert("Friends".to_owned(), Value::List(friends));
        let upgrade = |id: &str| Value::Compound(Map::from([("id".to_owned(), Value::String(id.to_owned()))]));
        let upgrades = ["a", "a", "b"].into_iter().map(upgrade).chain((0..40).map(|i| upgrade(&format!("u{i}"))));
        map.insert("Upgrades".to_owned(), Value::List(upgrades.collect()));
        let screen = Screen::read(&map).unwrap();
        assert!(screen.access.friends.len() <= MAX_FRIENDS);
        assert!(!screen.access.friends.iter().any(|friend| friend.uuid == 1), "the owner is no friend of their own");
        assert_eq!(screen.access.friends.iter().filter(|friend| friend.uuid == 2).count(), 1);
        assert_eq!(screen.upgrades.len(), MAX_UPGRADES);
        assert_eq!(&screen.upgrades[..2], ["a", "b"]);
    }

    #[test]
    fn upgrades_go_in_once_and_come_out() {
        let mut screen = Screen::new(Side::East, [2, 2], player(1, "Owner"));
        assert!(screen.add_upgrade(UPGRADE_LASER_MOUSE));
        assert!(!screen.add_upgrade(UPGRADE_LASER_MOUSE), "one of each");
        assert!(screen.has_upgrade(UPGRADE_LASER_MOUSE));
        assert!(screen.remove_upgrade(UPGRADE_LASER_MOUSE));
        assert!(!screen.remove_upgrade(UPGRADE_LASER_MOUSE));
    }

    #[test]
    fn a_block_saves_one_screen_a_side() {
        let (south, mut top) = (full(), Screen::new(Side::Top, [2, 2], player(3, "Other")));
        top.resolution = [640, 640];
        let block = Map::from([(SCREENS.to_owned(), write_all(&[south.clone(), top.clone()]))]);
        assert_eq!(read_all(&block), [south.clone(), top.clone()]);
        // A second screen on a side already taken is dropped, and so is what is no screen.
        let mut again = south.clone();
        again.name = "Second".to_owned();
        let crowded = Value::List(vec![
            Value::Compound(south.write()),
            Value::Compound(again.write()),
            Value::Int(5),
            Value::Compound(Map::new()),
            Value::Compound(top.write()),
        ]);
        assert_eq!(read_all(&Map::from([(SCREENS.to_owned(), crowded)])), [south, top]);
        assert!(read_all(&Map::new()).is_empty());
    }
}
