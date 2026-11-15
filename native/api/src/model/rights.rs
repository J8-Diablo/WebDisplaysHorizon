//! Who may do what to a screen: its owner, the owner's friends, and everyone else.
//!
//! The bits and their defaults are the Java mod's (`ScreenRights`), so a screen saved by
//! either reads the same. Two things differ on purpose: rights are masked to the bits that
//! exist, and someone who is not the owner cannot hand out a right they do not hold.

/// Change the page a screen shows.
pub const CHANGE_URL: u8 = 1;
/// Click and type on it.
pub const INTERACT: u8 = 2;
pub const MANAGE_FRIEND_LIST: u8 = 4;
/// Change what everyone else may do.
pub const MANAGE_OTHER_RIGHTS: u8 = 8;
/// Upgrades, peripherals (speakers too) and volume.
pub const MANAGE_UPGRADES: u8 = 16;
/// Resolution, rotation, shape, name, links.
pub const MODIFY_SCREEN: u8 = 32;

pub const NONE: u8 = 0;
/// Every right there is.
pub const KNOWN: u8 = CHANGE_URL | INTERACT | MANAGE_FRIEND_LIST | MANAGE_OTHER_RIGHTS | MANAGE_UPGRADES | MODIFY_SCREEN;
/// What friends and everyone else get on a new screen.
pub const DEFAULTS: u8 = CHANGE_URL | INTERACT | MANAGE_UPGRADES | MODIFY_SCREEN;
/// A screen's friend list stops here: it travels to every player who sees the screen.
pub const MAX_FRIENDS: usize = 64;

/// A player as a screen remembers them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub uuid: u128,
    pub name: String,
}

/// Why a friend was not added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FriendError {
    /// The owner needs no place in the list.
    IsOwner,
    AlreadyFriend,
    /// `MAX_FRIENDS` reached.
    Full,
}

/// One screen's owner, friends and what each group may do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access {
    /// None for a screen nobody owns (yet): then everyone has `other_rights`.
    pub owner: Option<Player>,
    pub friends: Vec<Player>,
    pub friend_rights: u8,
    pub other_rights: u8,
}

impl Access {
    /// A new screen's access: `owner` may do everything, the rest get the defaults.
    pub fn new(owner: Player) -> Self {
        Self { owner: Some(owner), friends: Vec::new(), friend_rights: DEFAULTS, other_rights: DEFAULTS }
    }

    pub fn is_owner(&self, uuid: u128) -> bool {
        self.owner.as_ref().is_some_and(|owner| owner.uuid == uuid)
    }

    /// The rights `uuid` has here.
    pub fn rights_for(&self, uuid: u128) -> u8 {
        if self.is_owner(uuid) {
            KNOWN
        } else if self.friends.iter().any(|friend| friend.uuid == uuid) {
            self.friend_rights & KNOWN
        } else {
            self.other_rights & KNOWN
        }
    }

    /// Whether `uuid` has every bit of `right`.
    pub fn allows(&self, uuid: u128, right: u8) -> bool {
        self.rights_for(uuid) & right == right
    }

    pub fn add_friend(&mut self, player: Player) -> Result<(), FriendError> {
        if self.is_owner(player.uuid) {
            return Err(FriendError::IsOwner);
        }
        if self.friends.iter().any(|friend| friend.uuid == player.uuid) {
            return Err(FriendError::AlreadyFriend);
        }
        if self.friends.len() >= MAX_FRIENDS {
            return Err(FriendError::Full);
        }
        self.friends.push(player);
        Ok(())
    }

    /// False when `uuid` was no friend.
    pub fn remove_friend(&mut self, uuid: u128) -> bool {
        let before = self.friends.len();
        self.friends.retain(|friend| friend.uuid != uuid);
        self.friends.len() != before
    }

    /// `by` asks for these rights for friends and for everyone else. Only the owner
    /// changes what friends may do. Someone with `MANAGE_OTHER_RIGHTS` changes what the
    /// others may do, but can neither give them a right they lack themselves nor take
    /// away one they could not have given. Says whether anything changed.
    pub fn set_rights(&mut self, by: u128, friend_rights: u8, other_rights: u8) -> bool {
        let before = (self.friend_rights, self.other_rights);
        if self.is_owner(by) {
            self.friend_rights = friend_rights & KNOWN;
            self.other_rights = other_rights & KNOWN;
        } else if self.allows(by, MANAGE_OTHER_RIGHTS) {
            let held = self.rights_for(by);
            // The bits `by` holds follow the request; the others stay as they were.
            self.other_rights = (other_rights & held) | (self.other_rights & KNOWN & !held);
        }
        before != (self.friend_rights, self.other_rights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: u128 = 1;
    const FRIEND: u128 = 2;
    const STRANGER: u128 = 3;

    fn player(uuid: u128) -> Player {
        Player { uuid, name: format!("player{uuid}") }
    }

    fn screen() -> Access {
        let mut access = Access::new(player(OWNER));
        access.add_friend(player(FRIEND)).unwrap();
        access
    }

    #[test]
    fn the_bits_are_the_java_mods() {
        assert_eq!((CHANGE_URL, INTERACT, MANAGE_FRIEND_LIST), (1, 2, 4));
        assert_eq!((MANAGE_OTHER_RIGHTS, MANAGE_UPGRADES, MODIFY_SCREEN), (8, 16, 32));
        assert_eq!(DEFAULTS, 51, "what a Java screen saves for friends and others");
    }

    #[test]
    fn owner_friends_and_others_get_their_own_rights() {
        let mut access = screen();
        access.friend_rights = CHANGE_URL | INTERACT;
        access.other_rights = INTERACT;
        assert_eq!(access.rights_for(OWNER), KNOWN);
        assert_eq!(access.rights_for(FRIEND), CHANGE_URL | INTERACT);
        assert_eq!(access.rights_for(STRANGER), INTERACT);
        assert!(access.allows(FRIEND, CHANGE_URL | INTERACT));
        assert!(!access.allows(STRANGER, CHANGE_URL | INTERACT), "every bit is needed");
        assert!(access.allows(STRANGER, NONE));
    }

    #[test]
    fn bits_that_do_not_exist_grant_nothing() {
        let mut access = screen();
        access.friend_rights = 0xFF;
        assert_eq!(access.rights_for(FRIEND), KNOWN, "a Java save may hold 0xFF");
        assert!(access.set_rights(OWNER, 0xC0 | INTERACT, 0x80));
        assert_eq!((access.friend_rights, access.other_rights), (INTERACT, NONE));
    }

    #[test]
    fn a_screen_without_owner_treats_everyone_as_other() {
        let access = Access { owner: None, friends: Vec::new(), friend_rights: KNOWN, other_rights: INTERACT };
        assert!(!access.is_owner(0));
        assert_eq!(access.rights_for(0), INTERACT);
    }

    #[test]
    fn the_friend_list_refuses_the_owner_doubles_and_overflow() {
        let mut access = screen();
        assert_eq!(access.add_friend(player(OWNER)), Err(FriendError::IsOwner));
        assert_eq!(access.add_friend(player(FRIEND)), Err(FriendError::AlreadyFriend));
        for uuid in 100..100 + MAX_FRIENDS as u128 - 1 {
            access.add_friend(player(uuid)).unwrap();
        }
        assert_eq!(access.add_friend(player(999)), Err(FriendError::Full));
        assert!(access.remove_friend(FRIEND));
        assert!(!access.remove_friend(FRIEND));
        assert_eq!(access.rights_for(FRIEND), access.other_rights, "no longer a friend");
    }

    #[test]
    fn only_the_owner_changes_what_friends_may_do() {
        let mut access = screen();
        access.friend_rights = KNOWN;
        assert!(!access.set_rights(FRIEND, NONE, access.other_rights), "even with every right");
        assert_eq!(access.friend_rights, KNOWN);
        assert!(access.set_rights(OWNER, INTERACT, access.other_rights));
        assert_eq!(access.friend_rights, INTERACT);
    }

    #[test]
    fn a_manager_cannot_hand_out_rights_they_lack() {
        let mut access = screen();
        access.friend_rights = INTERACT | MANAGE_OTHER_RIGHTS;
        access.other_rights = CHANGE_URL;
        // The friend asks for everything for everyone: only their own bits follow, and the
        // bit they do not hold (CHANGE_URL) is not theirs to take away either.
        assert!(access.set_rights(FRIEND, KNOWN, KNOWN));
        assert_eq!(access.other_rights, INTERACT | MANAGE_OTHER_RIGHTS | CHANGE_URL);
        assert!(access.set_rights(FRIEND, KNOWN, NONE));
        assert_eq!(access.other_rights, CHANGE_URL);
        // Without the right to manage, nothing moves.
        assert!(!access.set_rights(STRANGER, KNOWN, KNOWN));
        assert_eq!(access.other_rights, CHANGE_URL);
    }
}
