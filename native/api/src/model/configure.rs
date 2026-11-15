//! What the configuration window can change about a screen, and who may change it.
//!
//! The window sends one change at a time. Each needs a right (`model::rights`): the picture
//! and the sound are for whoever may modify the screen, the friends for whoever manages
//! them, the upgrades likewise; what friends may do is the owner's alone to decide. A
//! change that asks for more than is allowed is refused whole, except rights, where the
//! part the asker may give is kept (`Access::set_rights`).
use super::resolution;
use super::rights::{self, FriendError, Player};
use super::screen::{MAX_VOLUME, Screen};
use super::side::{LinkMode, Rotation, ShapeMode};
use super::text::{MAX_LINK_ID, MAX_NAME, label};

/// One thing changed about a screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// How the picture is turned: 0 to 3 quarter turns (`Rotation`).
    Rotation(u8),
    /// The picture's size in pixels.
    Resolution([u32; 2]),
    /// 0 to 100.
    Volume(f32),
    /// Quieter with distance.
    AutoVolume(bool),
    /// How the edge of a screen that is no rectangle is drawn (`ShapeMode`).
    ShapeMode(u8),
    /// Black shows as nothing.
    Oled(bool),
    Name(String),
    /// What the owner's friends and everyone else may do.
    Rights { friends: u8, others: u8 },
    /// A player by name, who must be on the server.
    AddFriend(String),
    RemoveFriend(u128),
    /// Takes an upgrade out, by its item's id; it goes back to whoever took it out.
    RemoveUpgrade(String),
    /// The link the screen belongs to: screens of one link on one face share a picture.
    /// Empty for none.
    Link(String),
    /// How a link's screens share the picture (`LinkMode`).
    LinkMode(u8),
    /// The shared browser the screen shows instead of a page of its own; None for its own.
    Browser(Option<u128>),
}

/// Why a change was not made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The player has not the right this change needs.
    NotAllowed,
    /// The change names something that does not exist: a fifth rotation, a friend who is
    /// not one, an upgrade the screen has not.
    Nothing,
    /// `AddFriend` named nobody who is on the server.
    NobodyByThatName,
    Friend(FriendError),
}

impl Change {
    /// The right a player needs to make this change. None for the rights themselves, which
    /// `Access::set_rights` sorts out bit by bit.
    pub fn needs(&self) -> Option<u8> {
        Some(match self {
            Change::Rotation(_) | Change::Resolution(_) | Change::Volume(_) | Change::AutoVolume(_) => rights::MODIFY_SCREEN,
            Change::ShapeMode(_) | Change::Oled(_) | Change::Name(_) => rights::MODIFY_SCREEN,
            Change::Link(_) | Change::LinkMode(_) | Change::Browser(_) => rights::MODIFY_SCREEN,
            Change::AddFriend(_) | Change::RemoveFriend(_) => rights::MANAGE_FRIEND_LIST,
            Change::RemoveUpgrade(_) => rights::MANAGE_UPGRADES,
            Change::Rights { .. } => return None,
        })
    }
}

/// Makes `change` to `screen` on behalf of `by`. `friend` is the player `AddFriend` named,
/// when the server found them. `max_resolution` is the server's ceiling. `Ok(true)` when
/// something changed, `Ok(false)` when the screen was like that already.
pub fn apply(screen: &mut Screen, by: u128, change: &Change, friend: Option<Player>, max_resolution: [u32; 2]) -> Result<bool, Refused> {
    if let Some(right) = change.needs()
        && !screen.access.allows(by, right)
    {
        return Err(Refused::NotAllowed);
    }
    let before = screen.clone();
    match change {
        Change::Rotation(turns) => screen.rotation = Rotation::from_index(*turns).ok_or(Refused::Nothing)?,
        Change::Resolution(size) => {
            if size[0] == 0 || size[1] == 0 {
                return Err(Refused::Nothing);
            }
            screen.resolution = resolution::clamp(*size, max_resolution);
        }
        Change::Volume(volume) => {
            if !volume.is_finite() {
                return Err(Refused::Nothing);
            }
            screen.volume = volume.clamp(0.0, MAX_VOLUME);
        }
        Change::AutoVolume(on) => screen.auto_volume = *on,
        Change::ShapeMode(mode) => screen.shape_mode = ShapeMode::from_index(*mode).ok_or(Refused::Nothing)?,
        Change::Oled(on) => screen.oled = *on,
        Change::Name(name) => screen.name = label(name, MAX_NAME),
        Change::Rights { friends, others } => {
            // Neither the owner nor someone who may manage the others' rights: nothing of
            // this is theirs to decide.
            if !screen.access.is_owner(by) && !screen.access.allows(by, rights::MANAGE_OTHER_RIGHTS) {
                return Err(Refused::NotAllowed);
            }
            screen.access.set_rights(by, *friends, *others);
        }
        Change::AddFriend(_) => {
            let friend = friend.ok_or(Refused::NobodyByThatName)?;
            screen.access.add_friend(friend).map_err(Refused::Friend)?;
        }
        Change::RemoveFriend(uuid) => {
            if !screen.access.remove_friend(*uuid) {
                return Err(Refused::Nothing);
            }
        }
        Change::RemoveUpgrade(item) => {
            if !screen.remove_upgrade(item) {
                return Err(Refused::Nothing);
            }
        }
        Change::Link(id) => {
            screen.link_id = label(id, MAX_LINK_ID);
            // Which screen of a link owns its browser is worked out from where they are.
            screen.link_origin = false;
        }
        Change::LinkMode(mode) => screen.link_mode = LinkMode::from_index(*mode).ok_or(Refused::Nothing)?,
        // Whether that browser exists is the server's to check: the screen only names it.
        Change::Browser(id) => screen.browser = *id,
    }
    Ok(*screen != before)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::screen::UPGRADE_LASER_MOUSE;
    use crate::model::side::Side;

    const OWNER: u128 = 1;
    const FRIEND: u128 = 2;
    const STRANGER: u128 = 3;
    const MAX: [u32; 2] = [1920, 1080];

    fn player(uuid: u128, name: &str) -> Player {
        Player { uuid, name: name.to_owned() }
    }

    fn screen() -> Screen {
        let mut screen = Screen::new(Side::North, [4, 3], player(OWNER, "Owner"));
        screen.resolution = [1280, 720];
        screen.access.add_friend(player(FRIEND, "Friend")).unwrap();
        screen
    }

    #[test]
    fn the_owner_changes_the_picture_and_the_sound() {
        let mut screen = screen();
        assert_eq!(apply(&mut screen, OWNER, &Change::Rotation(1), None, MAX), Ok(true));
        assert_eq!(screen.rotation, Rotation::R90);
        assert_eq!(apply(&mut screen, OWNER, &Change::Rotation(1), None, MAX), Ok(false), "already so");
        assert_eq!(apply(&mut screen, OWNER, &Change::Resolution([640, 360]), None, MAX), Ok(true));
        assert_eq!(screen.resolution, [640, 360]);
        assert_eq!(apply(&mut screen, OWNER, &Change::Volume(250.0), None, MAX), Ok(false), "no louder than the loudest, where it was");
        assert_eq!(apply(&mut screen, OWNER, &Change::Volume(30.0), None, MAX), Ok(true));
        assert_eq!(apply(&mut screen, OWNER, &Change::AutoVolume(false), None, MAX), Ok(true));
        assert_eq!(apply(&mut screen, OWNER, &Change::Oled(true), None, MAX), Ok(true));
        assert_eq!(apply(&mut screen, OWNER, &Change::ShapeMode(1), None, MAX), Ok(true));
        assert_eq!(apply(&mut screen, OWNER, &Change::Name("  Salon  ".to_owned()), None, MAX), Ok(true));
        assert_eq!((screen.volume, screen.auto_volume, screen.oled, screen.shape_mode, screen.name.as_str()), (30.0, false, true, ShapeMode::SmoothOne, "Salon"));
    }

    #[test]
    fn what_names_nothing_is_refused_and_changes_nothing() {
        let mut screen = screen();
        let before = screen.clone();
        for change in [
            Change::Rotation(4),
            Change::ShapeMode(9),
            Change::Resolution([0, 720]),
            Change::Volume(f32::NAN),
            Change::RemoveFriend(STRANGER),
            Change::RemoveUpgrade(UPGRADE_LASER_MOUSE.to_owned()),
        ] {
            assert_eq!(apply(&mut screen, OWNER, &change, None, MAX), Err(Refused::Nothing), "{change:?}");
        }
        assert_eq!(screen, before);
    }

    #[test]
    fn a_resolution_is_kept_within_the_servers_ceiling() {
        let mut screen = screen();
        apply(&mut screen, OWNER, &Change::Resolution([8000, 4500]), None, MAX).unwrap();
        assert!(screen.resolution[0] <= MAX[0] && screen.resolution[1] <= MAX[1], "{:?}", screen.resolution);
    }

    #[test]
    fn a_right_taken_away_stops_the_change() {
        let mut screen = screen();
        // Others may do nothing; friends everything but manage.
        apply(&mut screen, OWNER, &Change::Rights { friends: rights::MODIFY_SCREEN | rights::CHANGE_URL, others: rights::NONE }, None, MAX).unwrap();
        assert_eq!(apply(&mut screen, STRANGER, &Change::Rotation(2), None, MAX), Err(Refused::NotAllowed));
        assert_eq!(apply(&mut screen, FRIEND, &Change::Rotation(2), None, MAX), Ok(true));
        assert_eq!(apply(&mut screen, FRIEND, &Change::AddFriend("X".to_owned()), Some(player(9, "X")), MAX), Err(Refused::NotAllowed));
        assert_eq!(apply(&mut screen, FRIEND, &Change::Rights { friends: 63, others: 63 }, None, MAX), Err(Refused::NotAllowed));
        assert_eq!(apply(&mut screen, STRANGER, &Change::RemoveUpgrade("x".to_owned()), None, MAX), Err(Refused::NotAllowed));
    }

    #[test]
    fn friends_come_and_go() {
        let mut screen = screen();
        assert_eq!(apply(&mut screen, OWNER, &Change::AddFriend("Nobody".to_owned()), None, MAX), Err(Refused::NobodyByThatName));
        assert_eq!(apply(&mut screen, OWNER, &Change::AddFriend("Alex".to_owned()), Some(player(7, "Alex")), MAX), Ok(true));
        let again = apply(&mut screen, OWNER, &Change::AddFriend("Alex".to_owned()), Some(player(7, "Alex")), MAX);
        assert_eq!(again, Err(Refused::Friend(FriendError::AlreadyFriend)));
        assert_eq!(apply(&mut screen, OWNER, &Change::RemoveFriend(7), None, MAX), Ok(true));
        assert!(!screen.access.friends.iter().any(|friend| friend.uuid == 7));
    }

    #[test]
    fn a_screen_joins_and_leaves_a_link() {
        let mut screen = screen();
        assert_eq!(apply(&mut screen, OWNER, &Change::Link(" wall ".to_owned()), None, MAX), Ok(true));
        assert_eq!(screen.link_id, "wall");
        assert!(screen.is_linked());
        assert_eq!(apply(&mut screen, OWNER, &Change::LinkMode(1), None, MAX), Ok(true));
        assert_eq!(apply(&mut screen, OWNER, &Change::LinkMode(7), None, MAX), Err(Refused::Nothing));
        assert_eq!(apply(&mut screen, OWNER, &Change::Link(String::new()), None, MAX), Ok(true));
        assert!(!screen.is_linked());
    }

    #[test]
    fn an_upgrade_comes_out_once() {
        let mut screen = screen();
        screen.add_upgrade(UPGRADE_LASER_MOUSE);
        let out = Change::RemoveUpgrade(UPGRADE_LASER_MOUSE.to_owned());
        assert_eq!(apply(&mut screen, OWNER, &out, None, MAX), Ok(true));
        assert_eq!(apply(&mut screen, OWNER, &out, None, MAX), Err(Refused::Nothing));
    }
}
