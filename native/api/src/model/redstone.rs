//! The redstone controller: a block that changes a screen's address when the redstone
//! signal reaching it comes on or goes off.
//!
//! It keeps two addresses, one for each change; an empty one changes nothing. In the Java
//! mod its window could not be opened; here it can.
use super::link::Link;
use super::nbt::{self, Map, Value};
use super::url::MAX_LEN;

const RISING: &str = "RisingURL";
const FALLING: &str = "FallingURL";
const POWERED: &str = "Powered";

/// What a redstone controller remembers.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Controller {
    /// The screen it works; None until it is linked.
    pub link: Option<Link>,
    /// The address shown when the signal comes on; empty for none.
    pub rising: String,
    /// The address shown when the signal goes off; empty for none.
    pub falling: String,
    /// Whether a signal reached it when it last looked.
    pub powered: bool,
}

impl Controller {
    pub fn read(map: &Map) -> Controller {
        let address = |key: &str| nbt::text(map, key).filter(|url| url.len() <= MAX_LEN).unwrap_or_default().to_owned();
        Controller { link: Link::read(map), rising: address(RISING), falling: address(FALLING), powered: nbt::flag(map, POWERED).unwrap_or(false) }
    }

    /// Writes it into a block entity's compound, leaving what else is there.
    pub fn write(&self, map: &mut Map) {
        match &self.link {
            Some(link) => link.write(map),
            None => Link::clear(map),
        }
        map.insert(RISING.to_owned(), Value::String(self.rising.clone()));
        map.insert(FALLING.to_owned(), Value::String(self.falling.clone()));
        map.insert(POWERED.to_owned(), nbt::flag_value(self.powered));
    }

    /// The signal is now on or off. Gives the screen to change and the address to show
    /// when that is a change, the controller is linked and that change has an address.
    pub fn signal(&mut self, powered: bool) -> Option<(Link, &str)> {
        if powered == self.powered {
            return None;
        }
        self.powered = powered;
        let address = if powered { &self.rising } else { &self.falling };
        Some((self.link?, address.as_str())).filter(|(_, address)| !address.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::side::Side;

    fn controller() -> Controller {
        Controller {
            link: Some(Link { core: [1, 64, 2], side: Side::East }),
            rising: "https://on.example/".to_owned(),
            falling: "https://off.example/".to_owned(),
            powered: false,
        }
    }

    #[test]
    fn a_controller_reads_back_as_it_was_saved() {
        let mut map = Map::from([("Other".to_owned(), Value::Int(1))]);
        controller().write(&mut map);
        assert_eq!(Controller::read(&map), controller());
        let unlinked = Controller { link: None, powered: true, ..controller() };
        unlinked.write(&mut map);
        assert_eq!(Controller::read(&map), unlinked, "the link is taken out when there is none");
        assert_eq!(Controller::read(&Map::new()), Controller::default(), "a new block remembers nothing");
    }

    #[test]
    fn each_change_of_the_signal_shows_its_address_once() {
        let mut controller = controller();
        let link = controller.link.unwrap();
        assert_eq!(controller.signal(false), None, "it was off already");
        assert_eq!(controller.signal(true), Some((link, "https://on.example/")));
        assert_eq!(controller.signal(true), None, "still on");
        assert_eq!(controller.signal(false), Some((link, "https://off.example/")));
    }

    #[test]
    fn a_change_without_an_address_or_a_screen_is_only_remembered() {
        let mut silent = Controller { falling: String::new(), ..controller() };
        assert!(silent.signal(true).is_some());
        assert_eq!(silent.signal(false), None, "nothing to show when it goes off");
        assert!(!silent.powered, "but it knows it went off");
        let mut unlinked = Controller { link: None, ..controller() };
        assert_eq!(unlinked.signal(true), None);
        assert!(unlinked.powered);
    }
}
