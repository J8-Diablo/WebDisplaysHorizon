//! The screen's configuration window (the Java mod's "screen configurator" screen): its
//! name, picture, sound, who may do what, friends and upgrades.
//!
//! The window shows the screen as the server last sent it (`sync`, every frame) and sends
//! one change at a time (`model::configure::Change`): nothing is "applied" at the end, so
//! what is on the window is what is true, for every player looking at it. What the player
//! may not change is shown and greyed.
//!
//! It is described as a list of widgets with their rectangles (`widgets`), which both the
//! painter and the clicks go through, so the two cannot disagree. No game type here: what
//! a widget says is a `Phrase`, which the painter reads in the player's language.
//!
//! The boxes are fixed and the words are not: a translation of a label must fit the box
//! the English one was laid out for (`tools/check_content.py` knows the boxes).
use super::field::{Field, Outcome};
use super::{Action, Rect, centred, inside, modifiers};
use crate::model::configure::Change;
use crate::model::phrase::Phrase;
use crate::model::poly::MIN_CURVE_RADIUS;
use crate::model::resolution;
use crate::model::rights;
use crate::model::screen::{MAX_VOLUME, Screen};
use crate::model::side::{LinkMode, Rotation, ShapeMode};
use crate::model::text::{MAX_LINK_ID, MAX_NAME};
use crate::model::words;
use crate::net::msg::{Reshape, Target, ToServer};

/// The panel's width in GUI pixels; its height follows what it lists.
pub const WIDTH: f32 = 2.0 * PAD + 2.0 * COLUMN + GAP;
/// The width of each of its two columns, and the space between them.
const COLUMN: f32 = 196.0;
const GAP: f32 = 12.0;
const ROW: f32 = 18.0;
const PAD: f32 = 8.0;
/// The largest picture the window lets a player ask for; the server has the last word.
const MAX_RESOLUTION: [u32; 2] = [1920, 1080];
/// What one click adds to or takes from the radius of a traced screen's corners, in blocks.
const RADIUS_STEP: f32 = 0.5;
/// Friends listed at once; the rest are reached by removing some.
const FRIENDS_SHOWN: usize = 3;

/// The rights, in the order the window lists them.
pub const RIGHTS: [(u8, Phrase); 6] = [
    (rights::CHANGE_URL, Phrase::new("webdisplays.h.config.right.url", "Change the address")),
    (rights::INTERACT, Phrase::new("webdisplays.h.config.right.interact", "Click and type")),
    (rights::MODIFY_SCREEN, Phrase::new("webdisplays.h.config.right.settings", "Change settings")),
    (rights::MANAGE_UPGRADES, Phrase::new("webdisplays.h.config.right.upgrades", "Manage upgrades")),
    // The Java mod's own key: the same right under the same words.
    (rights::MANAGE_FRIEND_LIST, Phrase::new("webdisplays.gui.screencfg.friendlist", "Manage friends")),
    (rights::MANAGE_OTHER_RIGHTS, Phrase::new("webdisplays.h.config.right.others", "Set others' rights")),
];

/// The button beside a field: it sends what the field holds.
const SET: Phrase = Phrase::new("webdisplays.h.config.set", "Set");
/// "Friends": the column of their rights, and the field one is added in.
const FRIENDS: Phrase = Phrase::new("webdisplays.h.config.friends", "Friends");

/// Which field has the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Name,
    Width,
    Height,
    Friend,
    Link,
}

/// What a click on a button or a checkbox does.
#[derive(Clone, Debug, PartialEq)]
pub enum Press {
    SetName,
    SetResolution,
    NativeResolution,
    Rotate,
    Shape,
    /// A traced screen's corners rounded or sharp.
    Curve,
    /// The radius of those corners, one step more or less.
    Radius { more: bool },
    /// The page on the other face of a traced screen.
    Flip,
    Oled,
    AutoVolume,
    SetLink,
    LinkMode,
    /// Open the shared browsers' window.
    Browsers,
    /// Open the list of the world's screens.
    Screens,
    /// One right of the friends (`friends`) or of everyone else.
    Right { friends: bool, bit: u8 },
    AddFriend,
    RemoveFriend(u128),
    RemoveUpgrade(String),
    Close,
}

/// One thing drawn on the panel.
#[derive(Clone, Debug, PartialEq)]
pub enum Widget {
    Title(Phrase),
    Label(Phrase),
    Button { label: Phrase, press: Press, enabled: bool },
    Check { on: bool, press: Press, enabled: bool },
    Field(Focus),
    /// The volume, 0 to 1 along the bar.
    Slider { value: f32, enabled: bool },
}

/// How the sheet of a traced screen is bent and turned (`model::poly::Outline`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Traced {
    pub curved: bool,
    pub flipped: bool,
    /// The radius of a rounded corner, in blocks.
    pub radius: f32,
    /// The largest radius that still changes this sheet: its walls leave room for no more.
    pub widest: f32,
}

pub struct ConfigWindow {
    pub target: Target,
    /// The player looking at the window, as the server knows them.
    pub me: u128,
    /// The screen as the server last sent it.
    pub screen: Screen,
    /// Its sheet when it is a traced screen, as the server last sent it; None for a screen
    /// of blocks. Set by who opens the window, and each frame after.
    pub traced: Option<Traced>,
    pub name: Field,
    pub width: Field,
    pub height: Field,
    pub friend: Field,
    /// The link the screen belongs to.
    pub link: Field,
    pub focus: Option<Focus>,
    /// The first character shown of each field (`Field::view`), by `Focus` order.
    pub firsts: [usize; 5],
    /// The last volume sent while the slider is dragged, so that it is sent once.
    sent_volume: Option<f32>,
    /// The mouse button went down on the slider and has not come up.
    sliding: bool,
}

/// An upgrade as the window lists it; one the mod does not know is listed by its id.
fn upgrade_name(item: &str) -> Phrase {
    match item {
        "webdisplays:upgrade_lasermouse" => Phrase::new("webdisplays.h.config.upgrade.laser", "Laser pointer upgrade"),
        other => Phrase::verbatim(other),
    }
}

impl ConfigWindow {
    pub fn new(target: Target, me: u128, screen: Screen) -> Self {
        let name = Field::new(&screen.name, MAX_NAME);
        let width = Field::new(&screen.resolution[0].to_string(), 5);
        let height = Field::new(&screen.resolution[1].to_string(), 5);
        let link = Field::new(&screen.link_id, MAX_LINK_ID);
        Self { target, me, screen, traced: None, name, width, height, friend: Field::new("", 16), link, focus: None, firsts: [0; 5], sent_volume: None, sliding: false }
    }

    /// The server sent the screen again. A field the player is not typing in follows.
    pub fn sync(&mut self, screen: &Screen) {
        if self.screen.name != screen.name && self.focus != Some(Focus::Name) {
            self.name.set(&screen.name);
        }
        if self.screen.link_id != screen.link_id && self.focus != Some(Focus::Link) {
            self.link.set(&screen.link_id);
        }
        if self.screen.resolution != screen.resolution {
            if self.focus != Some(Focus::Width) {
                self.width.set(&screen.resolution[0].to_string());
            }
            if self.focus != Some(Focus::Height) {
                self.height.set(&screen.resolution[1].to_string());
            }
        }
        self.screen = screen.clone();
    }

    fn may(&self, right: u8) -> bool {
        self.screen.access.allows(self.me, right)
    }

    pub fn field(&self, focus: Focus) -> &Field {
        match focus {
            Focus::Name => &self.name,
            Focus::Width => &self.width,
            Focus::Height => &self.height,
            Focus::Friend => &self.friend,
            Focus::Link => &self.link,
        }
    }

    fn field_mut(&mut self, focus: Focus) -> &mut Field {
        match focus {
            Focus::Name => &mut self.name,
            Focus::Width => &mut self.width,
            Focus::Height => &mut self.height,
            Focus::Friend => &mut self.friend,
            Focus::Link => &mut self.link,
        }
    }

    /// Everything on the panel and the panel itself (first). Two columns: the picture and
    /// the sound on the left, who may do what on the right.
    pub fn widgets(&self, area: Rect) -> (Rect, Vec<(Rect, Widget)>) {
        let screen = &self.screen;
        let (modify, friends, upgrades) = (self.may(rights::MODIFY_SCREEN), self.may(rights::MANAGE_FRIEND_LIST), self.may(rights::MANAGE_UPGRADES));
        let owner = screen.access.is_owner(self.me);
        let others = owner || self.may(rights::MANAGE_OTHER_RIGHTS);
        // Laid out from the panel's top-left corner, then moved to the middle of `area`.
        let mut list: Vec<(Rect, Widget)> = Vec::new();
        let button = |label: Phrase, press: Press, enabled: bool| Widget::Button { label, press, enabled };
        let label = Widget::Label;

        let owned = screen.access.owner.as_ref().map_or(Phrase::new("webdisplays.h.config.nobody", "nobody"), |owner| Phrase::verbatim(owner.name.as_str()));
        let title = Phrase::new("webdisplays.h.config.title", "Screen %sx%s, owned by %s").with(screen.size[0]).with(screen.size[1]).with(owned);
        list.push(([PAD, PAD, WIDTH - 2.0 * PAD, 10.0], Widget::Title(title)));
        let top = PAD + 16.0;

        // ---- left: the picture and the sound
        let (x, wide) = (PAD, COLUMN);
        let mut y = top;
        list.push(([x, y + 4.0, 40.0, 10.0], label(words::NAME)));
        list.push(([x + 44.0, y, wide - 44.0 - 44.0, 16.0], Widget::Field(Focus::Name)));
        list.push(([x + wide - 40.0, y, 40.0, 16.0], button(SET, Press::SetName, modify)));
        y += ROW + 2.0;

        list.push(([x, y + 4.0, 40.0, 10.0], label(words::PIXELS)));
        list.push(([x + 44.0, y, 44.0, 16.0], Widget::Field(Focus::Width)));
        list.push(([x + 92.0, y + 4.0, 8.0, 10.0], label(words::TIMES)));
        list.push(([x + 102.0, y, 44.0, 16.0], Widget::Field(Focus::Height)));
        list.push(([x + wide - 40.0, y, 40.0, 16.0], button(SET, Press::SetResolution, modify)));
        y += ROW + 2.0;

        let half = (wide - 4.0) / 2.0;
        list.push(([x, y, half, 16.0], button(Phrase::new("webdisplays.h.config.best_fit", "Best fit"), Press::NativeResolution, modify)));
        let turned = Phrase::new("webdisplays.h.config.turned", "Turned %s°").with(screen.rotation.degrees() as u32);
        list.push(([x + half + 4.0, y, half, 16.0], button(turned, Press::Rotate, modify)));
        y += ROW + 2.0;

        match self.traced {
            // A traced screen has no blocks to take edges from: its sheet is bent instead.
            Some(traced) => {
                list.push(([x, y + 2.0, 12.0, 12.0], Widget::Check { on: traced.curved, press: Press::Curve, enabled: modify }));
                list.push(([x + 16.0, y + 4.0, wide - 16.0 - 68.0, 10.0], label(Phrase::new("webdisplays.h.config.rounded", "Rounded corners"))));
                // Sharp corners have no radius to set.
                let rounded = modify && traced.curved;
                list.push(([x + wide - 64.0, y, 18.0, 16.0], button(Phrase::untranslated("-"), Press::Radius { more: false }, rounded && traced.radius > MIN_CURVE_RADIUS)));
                list.push(([x + wide - 42.0, y + 4.0, 22.0, 10.0], label(Phrase::verbatim(format!("{:.1}", traced.radius)))));
                list.push(([x + wide - 18.0, y, 18.0, 16.0], button(Phrase::untranslated("+"), Press::Radius { more: true }, rounded && traced.radius < traced.widest)));
            }
            None => {
                let edges = match screen.shape_mode {
                    ShapeMode::None => Phrase::new("webdisplays.h.config.edges.blocks", "Edges: blocks"),
                    ShapeMode::SmoothOne => Phrase::new("webdisplays.h.config.edges.cut", "Edges: cut corners"),
                    ShapeMode::SmoothLong => Phrase::new("webdisplays.h.config.edges.slanted", "Edges: slanted"),
                };
                list.push(([x, y, wide, 16.0], button(edges, Press::Shape, modify)));
            }
        }
        y += ROW + 2.0;

        list.push(([x, y + 2.0, 12.0, 12.0], Widget::Check { on: screen.oled, press: Press::Oled, enabled: modify }));
        list.push(([x + 16.0, y + 4.0, wide - 16.0, 10.0], label(Phrase::new("webdisplays.h.config.oled", "Black is see-through"))));
        y += 16.0;
        list.push(([x, y + 2.0, 12.0, 12.0], Widget::Check { on: screen.auto_volume, press: Press::AutoVolume, enabled: modify }));
        list.push(([x + 16.0, y + 4.0, wide - 16.0, 10.0], label(Phrase::new("webdisplays.h.config.auto_volume", "Quieter with distance"))));
        y += 16.0 + 2.0;

        list.push(([x, y + 4.0, 40.0, 10.0], label(Phrase::new("webdisplays.h.config.volume", "Volume"))));
        list.push(([x + 44.0, y + 2.0, wide - 44.0 - 34.0, 12.0], Widget::Slider { value: screen.volume / MAX_VOLUME, enabled: modify }));
        list.push(([x + wide - 30.0, y + 4.0, 30.0, 10.0], label(Phrase::new("webdisplays.h.config.percent", "%s%%").with(screen.volume.round() as u32))));
        y += ROW + 2.0;

        // Screens given the same link, on the same face, show one picture between them.
        list.push(([x, y + 4.0, 40.0, 10.0], label(Phrase::new("webdisplays.h.config.link", "Link"))));
        list.push(([x + 44.0, y, wide - 44.0 - 44.0, 16.0], Widget::Field(Focus::Link)));
        list.push(([x + wide - 40.0, y, 40.0, 16.0], button(SET, Press::SetLink, modify)));
        y += ROW + 2.0;
        let shares = match screen.link_mode {
            LinkMode::Joined => Phrase::new("webdisplays.h.config.link_mode.joined", "Linked screens: as if touching"),
            LinkMode::Spaced => Phrase::new("webdisplays.h.config.link_mode.spaced", "Linked screens: gaps hide picture"),
        };
        list.push(([x, y, wide, 16.0], button(shares, Press::LinkMode, modify && screen.is_linked())));
        let left_end = y + ROW;
        // In the right column, under what is listed there (placed below).
        let shared = if screen.browser.is_some() {
            Phrase::new("webdisplays.h.config.shared_on", "Shared: on...")
        } else {
            Phrase::new("webdisplays.h.config.shared", "Shared...")
        };

        // ---- right: who may do what, friends, upgrades
        let x = PAD + COLUMN + GAP;
        let (column_friends, column_others) = (x + wide - 70.0, x + wide - 28.0);
        let mut y = top;
        list.push(([x, y, 100.0, 10.0], label(Phrase::new("webdisplays.h.config.may", "May:"))));
        list.push(([column_friends - 14.0, y, 40.0, 10.0], label(FRIENDS)));
        // The Java mod's own key: the same column.
        list.push(([column_others - 10.0, y, 36.0, 10.0], label(Phrase::new("webdisplays.gui.screencfg.others", "Others"))));
        y += 12.0;
        for (bit, text) in RIGHTS {
            list.push(([x + 4.0, y + 2.0, 110.0, 10.0], label(text)));
            let on = screen.access.friend_rights & bit != 0;
            list.push(([column_friends, y, 12.0, 12.0], Widget::Check { on, press: Press::Right { friends: true, bit }, enabled: owner }));
            let on = screen.access.other_rights & bit != 0;
            list.push(([column_others, y, 12.0, 12.0], Widget::Check { on, press: Press::Right { friends: false, bit }, enabled: others }));
            y += 14.0;
        }
        y += 4.0;

        list.push(([x, y + 4.0, 40.0, 10.0], label(FRIENDS)));
        list.push(([x + 44.0, y, wide - 44.0 - 44.0, 16.0], Widget::Field(Focus::Friend)));
        list.push(([x + wide - 40.0, y, 40.0, 16.0], button(Phrase::new("webdisplays.h.config.add", "Add"), Press::AddFriend, friends)));
        y += ROW;
        for friend in screen.access.friends.iter().take(FRIENDS_SHOWN) {
            list.push(([x + 4.0, y + 3.0, wide - 60.0, 10.0], label(Phrase::verbatim(friend.name.as_str()))));
            list.push(([x + wide - 50.0, y, 50.0, 14.0], button(Phrase::new("webdisplays.h.config.remove", "Remove"), Press::RemoveFriend(friend.uuid), friends)));
            y += 16.0;
        }
        if screen.access.friends.len() > FRIENDS_SHOWN {
            let more = screen.access.friends.len() - FRIENDS_SHOWN;
            list.push(([x + 4.0, y + 3.0, wide, 10.0], label(Phrase::new("webdisplays.h.config.more_friends", "and %s more").with(more))));
            y += 14.0;
        }
        for upgrade in &screen.upgrades {
            list.push(([x, y + 3.0, wide - 60.0, 10.0], label(upgrade_name(upgrade))));
            list.push(([x + wide - 50.0, y, 50.0, 14.0], button(Phrase::new("webdisplays.h.config.take_out", "Take out"), Press::RemoveUpgrade(upgrade.clone()), upgrades)));
            y += 16.0;
        }

        let mut y = left_end.max(y) + 4.0;
        let half = (COLUMN - 4.0) / 2.0;
        list.push(([PAD, y, half, 18.0], button(shared, Press::Browsers, modify)));
        list.push(([PAD + half + 4.0, y, half, 18.0], button(Phrase::new("webdisplays.h.config.all_screens", "All screens..."), Press::Screens, modify)));
        match self.traced {
            // Which face of a traced sheet shows the page.
            Some(_) => {
                list.push(([PAD + COLUMN + GAP, y, half, 18.0], button(Phrase::new("webdisplays.h.config.flip", "Turn around"), Press::Flip, modify)));
                list.push(([PAD + COLUMN + GAP + half + 4.0, y, half, 18.0], button(words::CLOSE, Press::Close, true)));
            }
            None => list.push(([PAD + COLUMN + GAP, y, COLUMN, 18.0], button(words::CLOSE, Press::Close, true))),
        }
        y += 18.0 + PAD;

        let panel = centred(area, [WIDTH, y]);
        for (rect, _) in &mut list {
            rect[0] += panel[0];
            rect[1] += panel[1];
        }
        (panel, list)
    }

    fn send(&self, change: Change) -> Vec<Action> {
        vec![Action::Send(ToServer::Configure { target: self.target, change })]
    }

    /// Asks the server to bend or turn the sheet; nothing on a screen of blocks.
    fn reshape(&self, ask: impl FnOnce(Traced) -> Reshape) -> Vec<Action> {
        self.traced.map_or_else(Vec::new, |traced| vec![Action::Send(ToServer::Reshape { target: self.target, change: ask(traced) })])
    }

    fn press(&mut self, press: Press) -> Vec<Action> {
        let access = &self.screen.access;
        match press {
            Press::SetName => self.send(Change::Name(self.name.text())),
            Press::SetResolution => match (self.width.text().trim().parse::<u32>(), self.height.text().trim().parse::<u32>()) {
                (Ok(width), Ok(height)) if width > 0 && height > 0 => self.send(Change::Resolution([width, height])),
                _ => Vec::new(),
            },
            Press::NativeResolution => self.send(Change::Resolution(resolution::default_resolution(self.screen.size, MAX_RESOLUTION))),
            Press::Rotate => self.send(Change::Rotation(self.screen.rotation.next().index())),
            Press::Shape => self.send(Change::ShapeMode(self.screen.shape_mode.next().index())),
            Press::Curve => self.reshape(|traced| Reshape::Curved(!traced.curved)),
            Press::Flip => self.reshape(|traced| Reshape::Flipped(!traced.flipped)),
            Press::Radius { more } => self.reshape(|traced| {
                let step = if more { RADIUS_STEP } else { -RADIUS_STEP };
                Reshape::Radius((traced.radius + step).clamp(MIN_CURVE_RADIUS, traced.widest.max(MIN_CURVE_RADIUS)))
            }),
            Press::Oled => self.send(Change::Oled(!self.screen.oled)),
            Press::AutoVolume => self.send(Change::AutoVolume(!self.screen.auto_volume)),
            Press::Browsers => vec![Action::Browsers(self.target)],
            Press::Screens => vec![Action::Screens(self.target)],
            Press::SetLink => self.send(Change::Link(self.link.text())),
            Press::LinkMode => {
                let next = match self.screen.link_mode {
                    LinkMode::Joined => LinkMode::Spaced,
                    LinkMode::Spaced => LinkMode::Joined,
                };
                self.send(Change::LinkMode(next.index()))
            }
            Press::Right { friends: true, bit } => self.send(Change::Rights { friends: access.friend_rights ^ bit, others: access.other_rights }),
            Press::Right { friends: false, bit } => self.send(Change::Rights { friends: access.friend_rights, others: access.other_rights ^ bit }),
            Press::AddFriend => {
                let name = self.friend.text().trim().to_owned();
                if name.is_empty() {
                    return Vec::new();
                }
                self.friend.set("");
                self.send(Change::AddFriend(name))
            }
            Press::RemoveFriend(uuid) => self.send(Change::RemoveFriend(uuid)),
            Press::RemoveUpgrade(item) => self.send(Change::RemoveUpgrade(item)),
            Press::Close => vec![Action::Close],
        }
    }

    /// The volume a mouse at `x` over the slider `bar` asks for, sent once per whole percent.
    fn slide(&mut self, bar: Rect, x: f32) -> Vec<Action> {
        let volume = (((x - bar[0]) / bar[2].max(1.0)).clamp(0.0, 1.0) * MAX_VOLUME).round();
        if self.sent_volume == Some(volume) {
            return Vec::new();
        }
        self.sent_volume = Some(volume);
        self.send(Change::Volume(volume))
    }

    pub fn click(&mut self, at: [f32; 2], area: Rect, measure: &dyn Fn(&str) -> f32) -> Vec<Action> {
        let (_, widgets) = self.widgets(area);
        self.focus = None;
        for (rect, widget) in widgets {
            if !inside(rect, at) {
                continue;
            }
            return match widget {
                Widget::Button { press, enabled: true, .. } | Widget::Check { press, enabled: true, .. } => self.press(press),
                Widget::Field(focus) => {
                    self.focus = Some(focus);
                    let first = self.firsts[focus as usize];
                    let index = self.field(focus).index_at(first, at[0] - rect[0] - 4.0, measure);
                    self.field_mut(focus).move_to(index, false);
                    Vec::new()
                }
                Widget::Slider { enabled: true, .. } => {
                    self.sliding = true;
                    self.sent_volume = None;
                    self.slide(rect, at[0])
                }
                _ => Vec::new(),
            };
        }
        Vec::new()
    }

    /// The mouse moved with its button down.
    pub fn drag(&mut self, at: [f32; 2], area: Rect) -> Vec<Action> {
        if !self.sliding {
            return Vec::new();
        }
        let (_, widgets) = self.widgets(area);
        let bar = widgets.iter().find(|(_, widget)| matches!(widget, Widget::Slider { .. })).map(|(rect, _)| *rect);
        bar.map(|bar| self.slide(bar, at[0])).unwrap_or_default()
    }

    /// The mouse button came up.
    pub fn release(&mut self) {
        self.sliding = false;
    }

    pub fn key(&mut self, key: &str, held: u32) -> Vec<Action> {
        let Some(focus) = self.focus else {
            return Vec::new();
        };
        if key.ends_with(".tab") || key == "tab" {
            self.focus = Some(match focus {
                Focus::Name => Focus::Width,
                Focus::Width => Focus::Height,
                Focus::Height => Focus::Link,
                Focus::Link => Focus::Friend,
                Focus::Friend => Focus::Name,
            });
            return Vec::new();
        }
        match self.field_mut(focus).key(key, held & modifiers::SHIFT != 0, held & modifiers::CONTROL != 0) {
            // Enter does what the button beside the field does.
            Outcome::Submit => self.press(match focus {
                Focus::Name => Press::SetName,
                Focus::Width | Focus::Height => Press::SetResolution,
                Focus::Friend => Press::AddFriend,
                Focus::Link => Press::SetLink,
            }),
            Outcome::Copy(text) => vec![Action::Copy(text)],
            Outcome::Paste => vec![Action::Paste],
            Outcome::Done | Outcome::Ignored => Vec::new(),
        }
    }

    pub fn typed(&mut self, c: char) {
        let Some(focus) = self.focus else {
            return;
        };
        // A size is digits only.
        if matches!(focus, Focus::Width | Focus::Height) && !c.is_ascii_digit() {
            return;
        }
        self.field_mut(focus).insert(c.encode_utf8(&mut [0; 4]));
    }

    pub fn paste(&mut self, text: &str) {
        if let Some(focus) = self.focus {
            let digits_only = matches!(focus, Focus::Width | Focus::Height);
            let kept: String = text.chars().filter(|c| !digits_only || c.is_ascii_digit()).collect();
            self.field_mut(focus).insert(&kept);
        }
    }
}

/// What the rotation button shows next: used by the tests to name the turn.
pub fn next_rotation(rotation: Rotation) -> Rotation {
    rotation.next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::rights::Player;
    use crate::model::side::Side;

    /// The game's window at 1280x720 with a GUI scale of 3: the smallest the panel must fit.
    const AREA: Rect = [0.0, 0.0, 426.0, 240.0];
    const TARGET: Target = Target { core: [1, 2, 3], side: Side::North };
    const OWNER: u128 = 1;
    const STRANGER: u128 = 3;

    fn measure(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    fn screen() -> Screen {
        let mut screen = Screen::new(Side::North, [4, 3], Player { uuid: OWNER, name: "Owner".to_owned() });
        screen.resolution = [1280, 720];
        screen.access.add_friend(Player { uuid: 2, name: "Friend".to_owned() }).unwrap();
        screen.add_upgrade("webdisplays:upgrade_lasermouse");
        screen
    }

    fn window(me: u128) -> ConfigWindow {
        ConfigWindow::new(TARGET, me, screen())
    }

    /// The middle of the first widget `pick` accepts.
    fn find(window: &ConfigWindow, pick: impl Fn(&Widget) -> bool) -> [f32; 2] {
        let (_, widgets) = window.widgets(AREA);
        let (rect, _) = widgets.into_iter().find(|(_, widget)| pick(widget)).expect("no such widget");
        [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0]
    }

    fn button(window: &ConfigWindow, wanted: Press) -> [f32; 2] {
        find(window, |widget| matches!(widget, Widget::Button { press, .. } | Widget::Check { press, .. } if *press == wanted))
    }

    fn sent(actions: Vec<Action>) -> Change {
        match &actions[..] {
            [Action::Send(ToServer::Configure { target, change })] => {
                assert_eq!(*target, TARGET);
                change.clone()
            }
            other => panic!("not one change: {other:?}"),
        }
    }

    #[test]
    fn everything_is_inside_the_panel_and_nothing_clickable_overlaps() {
        let window = window(OWNER);
        let (panel, widgets) = window.widgets(AREA);
        assert!(panel[1] >= 0.0 && panel[1] + panel[3] <= AREA[3], "the panel fits the smallest window: {panel:?}");
        assert!(panel[0] >= 0.0 && panel[0] + panel[2] <= AREA[2], "and is no wider: {panel:?}");
        let clickable: Vec<Rect> = widgets
            .iter()
            .filter(|(_, widget)| !matches!(widget, Widget::Label(_) | Widget::Title(_)))
            .map(|(rect, _)| *rect)
            .collect();
        for (rect, widget) in &widgets {
            assert!(rect[0] >= panel[0] && rect[0] + rect[2] <= panel[0] + panel[2] + 0.5, "{widget:?} sticks out sideways: {rect:?}");
            assert!(rect[1] >= panel[1] && rect[1] + rect[3] <= panel[1] + panel[3] + 0.5, "{widget:?} sticks out: {rect:?}");
        }
        for (index, a) in clickable.iter().enumerate() {
            for b in &clickable[index + 1..] {
                let apart = a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1];
                assert!(apart, "{a:?} and {b:?} overlap");
            }
        }
    }

    /// Everything the panel says, in English.
    fn said(window: &ConfigWindow) -> Vec<String> {
        let (_, widgets) = window.widgets(AREA);
        let texts = widgets.into_iter().filter_map(|(_, widget)| match widget {
            Widget::Title(text) | Widget::Label(text) | Widget::Button { label: text, .. } => Some(text.english()),
            _ => None,
        });
        texts.collect()
    }

    #[test]
    fn what_the_panel_says_are_sentences_with_their_numbers_and_names_as_arguments() {
        let mut theirs = screen();
        theirs.rotation = Rotation::R90;
        theirs.volume = 75.0;
        for uuid in 10..14 {
            theirs.access.add_friend(Player { uuid, name: format!("friend{uuid}") }).unwrap();
        }
        let window = ConfigWindow::new(TARGET, OWNER, theirs.clone());
        let said = said(&window);
        assert_eq!(said[0], "Screen 4x3, owned by Owner");
        for expected in ["Turned 90°", "75%", "Edges: blocks", "Friend", "and 2 more", "Laser pointer upgrade", "Shared...", "x", "Manage friends"] {
            assert!(said.iter().any(|text| text == expected), "{expected:?} is not in {said:?}");
        }
        let (_, widgets) = window.widgets(AREA);
        let Widget::Title(title) = &widgets[0].1 else { panic!("the title comes first") };
        assert_eq!((title.key, title.args.len()), ("webdisplays.h.config.title", 3), "one sentence, whatever the size and the owner");
        assert!(!Phrase::verbatim("Friend").is_translated() && widgets.iter().any(|(_, widget)| *widget == Widget::Label(Phrase::verbatim("Friend"))), "a name is no key");

        theirs.access.owner = None;
        theirs.browser = Some(7);
        theirs.add_upgrade("othermod:upgrade");
        let said = self::said(&ConfigWindow::new(TARGET, STRANGER, theirs));
        assert_eq!(said[0], "Screen 4x3, owned by nobody");
        assert!(said.iter().any(|text| text == "Shared: on...") && said.iter().any(|text| text == "othermod:upgrade"), "{said:?}");
    }

    #[test]
    fn each_button_sends_its_one_change() {
        let mut window = window(OWNER);
        assert_eq!(sent(window.click(button(&window, Press::Rotate), AREA, &measure)), Change::Rotation(1));
        assert_eq!(sent(window.click(button(&window, Press::Shape), AREA, &measure)), Change::ShapeMode(1));
        assert_eq!(sent(window.click(button(&window, Press::Oled), AREA, &measure)), Change::Oled(true));
        assert_eq!(sent(window.click(button(&window, Press::AutoVolume), AREA, &measure)), Change::AutoVolume(false));
        assert_eq!(sent(window.click(button(&window, Press::RemoveFriend(2)), AREA, &measure)), Change::RemoveFriend(2));
        let out = Press::RemoveUpgrade("webdisplays:upgrade_lasermouse".to_owned());
        assert_eq!(sent(window.click(button(&window, out), AREA, &measure)), Change::RemoveUpgrade("webdisplays:upgrade_lasermouse".to_owned()));
        assert_eq!(window.click(button(&window, Press::Close), AREA, &measure), [Action::Close]);
        assert_eq!(window.click([1.0, 1.0], AREA, &measure), [], "outside the panel");
    }

    #[test]
    fn a_right_is_flipped_for_its_column_only() {
        let mut window = window(OWNER);
        let before = (window.screen.access.friend_rights, window.screen.access.other_rights);
        let change = sent(window.click(button(&window, Press::Right { friends: true, bit: rights::MANAGE_FRIEND_LIST }), AREA, &measure));
        assert_eq!(change, Change::Rights { friends: before.0 ^ rights::MANAGE_FRIEND_LIST, others: before.1 });
        let change = sent(window.click(button(&window, Press::Right { friends: false, bit: rights::INTERACT }), AREA, &measure));
        assert_eq!(change, Change::Rights { friends: before.0, others: before.1 ^ rights::INTERACT });
    }

    #[test]
    fn the_fields_are_typed_in_and_sent_by_their_button_or_enter() {
        let mut window = window(OWNER);
        window.click(find(&window, |widget| *widget == Widget::Field(Focus::Name)), AREA, &measure);
        assert_eq!(window.focus, Some(Focus::Name));
        for c in "Salon".chars() {
            window.typed(c);
        }
        assert_eq!(sent(window.key("key.keyboard.enter", 0)), Change::Name("Salon".to_owned()));

        window.click(find(&window, |widget| *widget == Widget::Field(Focus::Width)), AREA, &measure);
        window.key("a", modifiers::CONTROL);
        window.typed('6');
        window.typed('x');
        window.typed('4');
        window.typed('0');
        assert_eq!(window.width.text(), "640", "a size takes digits only");
        window.key("key.keyboard.tab", 0);
        assert_eq!(window.focus, Some(Focus::Height));
        window.key("a", modifiers::CONTROL);
        window.paste("36 0px");
        assert_eq!(sent(window.click(button(&window, Press::SetResolution), AREA, &measure)), Change::Resolution([640, 360]));

        window.height.set("");
        assert_eq!(window.click(button(&window, Press::SetResolution), AREA, &measure), [], "half a size is not sent");
        assert_eq!(sent(window.click(button(&window, Press::NativeResolution), AREA, &measure)), Change::Resolution(resolution::default_resolution([4, 3], MAX_RESOLUTION)));

        window.click(find(&window, |widget| *widget == Widget::Field(Focus::Friend)), AREA, &measure);
        assert_eq!(window.key("enter", 0), [], "nobody named");
        for c in " Alex ".chars() {
            window.typed(c);
        }
        assert_eq!(sent(window.click(button(&window, Press::AddFriend), AREA, &measure)), Change::AddFriend("Alex".to_owned()));
        assert_eq!(window.friend.text(), "", "ready for the next name");
    }

    #[test]
    fn a_link_is_typed_and_its_sharing_chosen_once_linked() {
        let mut window = window(OWNER);
        assert_eq!(window.click(button(&window, Press::LinkMode), AREA, &measure), [], "nothing to share while unlinked");
        window.click(find(&window, |widget| *widget == Widget::Field(Focus::Link)), AREA, &measure);
        for c in "wall".chars() {
            window.typed(c);
        }
        assert_eq!(sent(window.key("enter", 0)), Change::Link("wall".to_owned()));
        let mut linked = screen();
        linked.link_id = "wall".to_owned();
        window.sync(&linked);
        assert_eq!(sent(window.click(button(&window, Press::LinkMode), AREA, &measure)), Change::LinkMode(LinkMode::Spaced.index()));
    }

    #[test]
    fn the_volume_follows_the_mouse_and_is_sent_once_per_step() {
        let mut window = window(OWNER);
        let (_, widgets) = window.widgets(AREA);
        let bar = widgets.iter().find(|(_, widget)| matches!(widget, Widget::Slider { .. })).unwrap().0;
        let y = bar[1] + 3.0;
        assert_eq!(sent(window.click([bar[0] + bar[2] / 2.0, y], AREA, &measure)), Change::Volume(50.0));
        assert_eq!(window.drag([bar[0] + bar[2] / 2.0, y], AREA), [], "it did not move");
        assert_eq!(sent(window.drag([bar[0] + bar[2] * 2.0, y + 90.0], AREA)), Change::Volume(100.0), "past the end is the end");
        assert_eq!(sent(window.drag([bar[0] - 50.0, y], AREA)), Change::Volume(0.0));
        window.release();
        assert_eq!(window.drag([bar[0] + bar[2] / 2.0, y], AREA), [], "the button is up");
    }

    #[test]
    fn what_a_player_may_not_change_does_not_answer() {
        let mut theirs = screen();
        theirs.access.other_rights = rights::INTERACT;
        let mut window = ConfigWindow::new(TARGET, STRANGER, theirs);
        for press in [Press::Rotate, Press::Oled, Press::SetName, Press::AddFriend, Press::RemoveFriend(2), Press::Right { friends: false, bit: rights::INTERACT }] {
            assert_eq!(window.click(button(&window, press.clone()), AREA, &measure), [], "{press:?}");
        }
        let (_, widgets) = window.widgets(AREA);
        let bar = widgets.iter().find(|(_, widget)| matches!(widget, Widget::Slider { .. })).unwrap().0;
        assert_eq!(window.click([bar[0] + 5.0, bar[1] + 3.0], AREA, &measure), []);
        assert_eq!(window.click(button(&window, Press::Close), AREA, &measure), [Action::Close], "closing is always allowed");
    }

    #[test]
    fn the_window_follows_the_server_but_not_under_the_players_fingers() {
        let mut window = window(OWNER);
        window.click(find(&window, |widget| *widget == Widget::Field(Focus::Name)), AREA, &measure);
        window.typed('X');
        let mut changed = screen();
        changed.name = "Elsewhere".to_owned();
        changed.resolution = [800, 600];
        changed.rotation = next_rotation(changed.rotation);
        window.sync(&changed);
        assert_eq!(window.name.text(), "X", "the field being typed in is left alone");
        assert_eq!((window.width.text().as_str(), window.height.text().as_str()), ("800", "600"));
        assert_eq!(window.screen.rotation, Rotation::R90);
    }

    fn traced(me: u128, curved: bool, radius: f32) -> ConfigWindow {
        let mut window = window(me);
        window.traced = Some(Traced { curved, flipped: false, radius, widest: 5.0 });
        window
    }

    fn reshaped(actions: Vec<Action>) -> Reshape {
        match &actions[..] {
            [Action::Send(ToServer::Reshape { target, change })] => {
                assert_eq!(*target, TARGET);
                *change
            }
            other => panic!("not one reshaping: {other:?}"),
        }
    }

    fn presses(window: &ConfigWindow) -> Vec<Press> {
        let (_, widgets) = window.widgets(AREA);
        widgets
            .into_iter()
            .filter_map(|(_, widget)| match widget {
                Widget::Button { press, .. } | Widget::Check { press, .. } => Some(press),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_traced_screen_is_bent_and_turned_from_the_window() {
        // A screen of blocks has edges to choose and no sheet to bend.
        let of_blocks = presses(&window(OWNER));
        assert!(of_blocks.contains(&Press::Shape) && !of_blocks.contains(&Press::Curve) && !of_blocks.contains(&Press::Flip));

        let mut sharp = traced(OWNER, false, 1.5);
        let of_sheet = presses(&sharp);
        assert!(!of_sheet.contains(&Press::Shape) && of_sheet.contains(&Press::Curve) && of_sheet.contains(&Press::Flip));
        // The panel is no taller for it, and nothing lies on anything else.
        let (panel, widgets) = sharp.widgets(AREA);
        assert_eq!(panel, window(OWNER).widgets(AREA).0);
        let clickable: Vec<Rect> = widgets.iter().filter(|(_, widget)| !matches!(widget, Widget::Label(_) | Widget::Title(_))).map(|(rect, _)| *rect).collect();
        for (index, a) in clickable.iter().enumerate() {
            for b in &clickable[index + 1..] {
                let apart = a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1];
                assert!(apart, "{a:?} and {b:?}");
            }
        }
        for (rect, widget) in &widgets {
            assert!(rect[0] >= panel[0] && rect[0] + rect[2] <= panel[0] + panel[2] + 0.5, "{widget:?} sticks out sideways: {rect:?}");
        }

        let click = |window: &mut ConfigWindow, press: Press| {
            let at = button(window, press);
            window.click(at, AREA, &measure)
        };
        assert_eq!(reshaped(click(&mut sharp, Press::Curve)), Reshape::Curved(true));
        assert_eq!(reshaped(click(&mut sharp, Press::Flip)), Reshape::Flipped(true));
        assert_eq!(click(&mut sharp, Press::Radius { more: true }), [], "sharp corners have no radius to set");

        let mut rounded = traced(OWNER, true, 1.5);
        assert_eq!(reshaped(click(&mut rounded, Press::Curve)), Reshape::Curved(false));
        assert_eq!(reshaped(click(&mut rounded, Press::Radius { more: true })), Reshape::Radius(2.0));
        assert_eq!(reshaped(click(&mut rounded, Press::Radius { more: false })), Reshape::Radius(1.0));
        // The walls of this one leave room for 5 and no more.
        let mut nearly = traced(OWNER, true, 4.75);
        assert_eq!(reshaped(click(&mut nearly, Press::Radius { more: true })), Reshape::Radius(5.0), "no further than the widest");
        let mut widest = traced(OWNER, true, 5.0);
        assert_eq!(click(&mut widest, Press::Radius { more: true }), []);
        assert_eq!(reshaped(click(&mut widest, Press::Radius { more: false })), Reshape::Radius(4.5));
        let mut tightest = traced(OWNER, true, MIN_CURVE_RADIUS);
        assert_eq!(click(&mut tightest, Press::Radius { more: false }), []);

        // Who may not change the screen's settings may not bend it either.
        let mut theirs = traced(STRANGER, true, 1.5);
        theirs.screen.access.other_rights = rights::INTERACT;
        for press in [Press::Curve, Press::Flip, Press::Radius { more: true }] {
            assert_eq!(click(&mut theirs, press.clone()), [], "{press:?}");
        }
    }
}
