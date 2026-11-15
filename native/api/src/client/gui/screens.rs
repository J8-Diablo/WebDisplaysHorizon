//! The list of the world's screens: where each is, what it is called, and a button to join
//! the screen the window was opened from with any of them (`model::index`).
//!
//! The list is the server's and is asked for a page at a time: it can be long, and the
//! screens on it far away.
use super::{Action, Rect, centred, inside};
use crate::model::index::Entry;
use crate::model::phrase::Phrase;
use crate::model::words;
use crate::net::msg::{Target, ToServer};

/// The panel's size in GUI pixels.
pub const SIZE: [f32; 2] = [410.0, 220.0];
/// Screens shown at once: half a page of the server's.
pub const ROWS: usize = 8;
const PAD: f32 = 8.0;

/// What a click on a button does.
#[derive(Clone, Debug, PartialEq)]
pub enum Press {
    /// Join the window's screen with this one.
    Join(Target),
    Previous,
    Next,
    Close,
}

/// One thing drawn on the panel.
#[derive(Clone, Debug, PartialEq)]
pub enum Widget {
    Title(Phrase),
    Label(Phrase),
    Button { label: Phrase, press: Press, enabled: bool },
}

pub struct ScreensWindow {
    /// The screen the window was opened from.
    pub target: Target,
    /// The page of the server's list last received, and how many pages it has.
    pub entries: Vec<Entry>,
    pub page: usize,
    pub pages: usize,
    /// Which half of that page is shown.
    pub second_half: bool,
    /// Whether the server answered yet.
    pub loaded: bool,
}

/// Where a screen is, as a row says it: a screen that faces up lies on a floor, one that
/// faces down hangs from a ceiling, the others are named by the way they face.
fn side_name(side: u8) -> Phrase {
    match side {
        0 => Phrase::new("webdisplays.h.screens.side.ceiling", "ceiling"),
        1 => Phrase::new("webdisplays.h.screens.side.floor", "floor"),
        2 => Phrase::new("webdisplays.h.screens.side.north", "north"),
        3 => Phrase::new("webdisplays.h.screens.side.south", "south"),
        4 => Phrase::new("webdisplays.h.screens.side.west", "west"),
        5 => Phrase::new("webdisplays.h.screens.side.east", "east"),
        _ => Phrase::untranslated("?"),
    }
}

impl ScreensWindow {
    /// The window, and the request for the list's first page.
    pub fn open(target: Target) -> (Self, Action) {
        (Self { target, entries: Vec::new(), page: 0, pages: 1, second_half: false, loaded: false }, Action::Send(ToServer::Screens { page: 0 }))
    }

    /// The server sent a page.
    pub fn received(&mut self, page: usize, pages: usize, entries: Vec<Entry>) {
        (self.page, self.pages, self.entries, self.loaded) = (page, pages.max(1), entries, true);
        if self.second_half && self.entries.len() <= ROWS {
            self.second_half = false;
        }
    }

    fn shown(&self) -> &[Entry] {
        let start = if self.second_half { ROWS.min(self.entries.len()) } else { 0 };
        &self.entries[start..(start + ROWS).min(self.entries.len())]
    }

    fn has_next(&self) -> bool {
        (!self.second_half && self.entries.len() > ROWS) || self.page + 1 < self.pages
    }

    fn has_previous(&self) -> bool {
        self.second_half || self.page > 0
    }

    pub fn widgets(&self, area: Rect) -> (Rect, Vec<(Rect, Widget)>) {
        let panel = centred(area, SIZE);
        let (x, y, wide) = (panel[0] + PAD, panel[1] + PAD, panel[2] - 2.0 * PAD);
        let button = |label: Phrase, press: Press, enabled: bool| Widget::Button { label, press, enabled };
        let mut list = vec![([x, y, wide, 10.0], Widget::Title(Phrase::new("webdisplays.h.screens.title", "Screens of this world")))];
        if self.loaded && self.entries.is_empty() {
            list.push(([x, y + 22.0, wide, 10.0], Widget::Label(Phrase::new("webdisplays.h.screens.none", "None listed yet."))));
        }
        for (row, entry) in self.shown().iter().enumerate() {
            let top = y + 18.0 + row as f32 * 20.0;
            let name = if entry.name.is_empty() {
                Phrase::new("webdisplays.h.screens.no_name", "(no name)")
            } else {
                Phrase::verbatim(entry.name.chars().take(18).collect::<String>())
            };
            let place = if entry.traced { Phrase::new("webdisplays.h.screens.traced", "traced") } else { side_name(entry.side) };
            let link = if entry.link.is_empty() {
                Phrase::untranslated("")
            } else {
                Phrase::untranslated("  [%s]").with(entry.link.chars().take(10).collect::<String>())
            };
            let [px, py, pz] = entry.core;
            // Name, place, size, face and link: an order that is the same in every language.
            let row = Phrase::untranslated("%s  %s %s %s  %sx%s %s%s");
            let text = row.with(name).with(px).with(py).with(pz).with(entry.size[0]).with(entry.size[1]).with(place).with(link);
            list.push(([x, top + 4.0, wide - 56.0, 10.0], Widget::Label(text)));
            let this = Target { core: entry.core, side: crate::model::side::Side::from_index(entry.side).unwrap_or(self.target.side) };
            let itself = this == self.target;
            list.push(([x + wide - 50.0, top, 50.0, 16.0], button(if itself { Phrase::new("webdisplays.h.screens.this", "This") } else { Phrase::new("webdisplays.h.screens.join", "Join") }, Press::Join(this), !itself)));
        }
        let bottom = panel[1] + panel[3] - 26.0;
        let third = (wide - 2.0 * 6.0) / 3.0;
        let at = |index: usize| [x + index as f32 * (third + 6.0), bottom, third, 18.0];
        list.push((at(0), button(Phrase::new("webdisplays.h.screens.previous", "Previous"), Press::Previous, self.has_previous())));
        list.push((at(1), button(Phrase::new("webdisplays.h.screens.next", "Next"), Press::Next, self.has_next())));
        list.push((at(2), button(words::CLOSE, Press::Close, true)));
        (panel, list)
    }

    fn press(&mut self, press: Press) -> Vec<Action> {
        let ask = |page: usize| vec![Action::Send(ToServer::Screens { page: page as u16 })];
        match press {
            // The page is asked for again: the rows of both screens now say their link.
            Press::Join(other) => vec![Action::Send(ToServer::Join { target: self.target, other }), Action::Send(ToServer::Screens { page: self.page as u16 })],
            Press::Close => vec![Action::Close],
            Press::Next if !self.second_half && self.entries.len() > ROWS => {
                self.second_half = true;
                Vec::new()
            }
            Press::Next => {
                self.second_half = false;
                ask(self.page + 1)
            }
            Press::Previous if self.second_half => {
                self.second_half = false;
                Vec::new()
            }
            Press::Previous => ask(self.page.saturating_sub(1)),
        }
    }

    pub fn click(&mut self, at: [f32; 2], area: Rect) -> Vec<Action> {
        let (_, widgets) = self.widgets(area);
        for (rect, widget) in widgets {
            if let Widget::Button { press, enabled: true, .. } = widget
                && inside(rect, at)
            {
                return self.press(press);
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::side::Side;

    const AREA: Rect = [0.0, 0.0, 426.0, 240.0];
    const TARGET: Target = Target { core: [0, 64, 0], side: Side::North };

    fn entry(x: i32) -> Entry {
        Entry { core: [x, 64, 0], side: 2, dimension: "minecraft:overworld".to_owned(), name: format!("Screen {x}"), size: [4, 3], link: String::new(), traced: false }
    }

    fn click(window: &mut ScreensWindow, wanted: Press) -> Vec<Action> {
        let (_, widgets) = window.widgets(AREA);
        let (rect, _) = widgets.into_iter().find(|(_, widget)| matches!(widget, Widget::Button { press, .. } if *press == wanted)).expect("no such button");
        window.click([rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0], AREA)
    }

    fn window(count: i32) -> ScreensWindow {
        let (mut window, asked) = ScreensWindow::open(TARGET);
        assert_eq!(asked, Action::Send(ToServer::Screens { page: 0 }));
        window.received(0, 3, (0..count).map(entry).collect());
        window
    }

    #[test]
    fn everything_is_in_the_panel_and_rows_do_not_overlap() {
        let window = window(16);
        let (panel, widgets) = window.widgets(AREA);
        assert!(panel[0] >= 0.0 && panel[0] + panel[2] <= AREA[2] && panel[1] >= 0.0 && panel[1] + panel[3] <= AREA[3]);
        let buttons: Vec<Rect> = widgets.iter().filter(|(_, widget)| matches!(widget, Widget::Button { .. })).map(|(rect, _)| *rect).collect();
        for (rect, widget) in &widgets {
            assert!(rect[0] >= panel[0] && rect[0] + rect[2] <= panel[0] + panel[2] + 0.5 && rect[1] + rect[3] <= panel[1] + panel[3] + 0.5, "{widget:?} {rect:?}");
        }
        for (index, a) in buttons.iter().enumerate() {
            for b in &buttons[index + 1..] {
                let apart = a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1];
                assert!(apart, "{a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn a_row_says_where_a_screen_is_and_what_it_is() {
        let (mut window, _) = ScreensWindow::open(TARGET);
        let unnamed = Entry { name: String::new(), side: 1, link: "a-link-id-too-long".to_owned(), ..entry(-7) };
        let traced = Entry { traced: true, ..entry(9) };
        window.received(0, 1, vec![entry(0), unnamed, traced]);
        let (_, widgets) = window.widgets(AREA);
        let rows: Vec<String> = widgets.iter().filter_map(|(_, widget)| if let Widget::Label(text) = widget { Some(text.english()) } else { None }).collect();
        assert_eq!(rows, ["Screen 0  0 64 0  4x3 north", "(no name)  -7 64 0  4x3 floor  [a-link-id-]", "Screen 9  9 64 0  4x3 traced"]);
        let labels: Vec<String> = widgets.iter().filter_map(|(_, widget)| if let Widget::Button { label, press: Press::Join(_), .. } = widget { Some(label.english()) } else { None }).collect();
        assert_eq!(labels, ["This", "Join", "Join"]);
    }

    #[test]
    fn a_screen_is_joined_with_another_but_not_with_itself() {
        let mut window = window(3);
        let other = Target { core: [2, 64, 0], side: Side::North };
        assert_eq!(click(&mut window, Press::Join(other)), [Action::Send(ToServer::Join { target: TARGET, other }), Action::Send(ToServer::Screens { page: 0 })]);
        assert_eq!(click(&mut window, Press::Join(TARGET)), [], "the row of the screen itself");
        assert_eq!(click(&mut window, Press::Close), [Action::Close]);
    }

    #[test]
    fn the_list_is_walked_half_a_page_then_a_page_at_a_time() {
        let mut window = window(16);
        assert_eq!(click(&mut window, Press::Previous), [], "at the start");
        assert_eq!(click(&mut window, Press::Next), [], "the second half of the page is already here");
        assert!(window.second_half);
        assert_eq!(click(&mut window, Press::Next), [Action::Send(ToServer::Screens { page: 1 })]);
        window.received(1, 3, (16..20).map(entry).collect());
        assert_eq!(click(&mut window, Press::Next), [Action::Send(ToServer::Screens { page: 2 })], "a short page has no second half");
        window.received(2, 3, vec![entry(40)]);
        assert_eq!(click(&mut window, Press::Next), [], "the last page");
        assert_eq!(click(&mut window, Press::Previous), [Action::Send(ToServer::Screens { page: 1 })]);
    }
}
