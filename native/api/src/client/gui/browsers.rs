//! The shared browsers' window: the world's list, which of them a screen shows, and a form
//! to make or change one (`model::shared`).
//!
//! The list is the server's, sent again whenever it changes (`sync`). Nothing here changes
//! it: the window asks, and shows what comes back.
use super::field::{Field, Outcome};
use super::{Action, Rect, centred, inside, modifiers};
use crate::model::configure::Change;
use crate::model::phrase::Phrase;
use crate::model::shared::Shared;
use crate::model::text::MAX_NAME;
use crate::model::words;
use crate::net::msg::{BrowserEdit, MAX_URL, Target, ToServer};

/// The panel's size in GUI pixels.
pub const SIZE: [f32; 2] = [400.0, 214.0];
/// Browsers listed at once; the wheel is not read, so the list starts at `first`.
pub const ROWS: usize = 7;
const PAD: f32 = 8.0;

/// What a click on a button does.
#[derive(Clone, Debug, PartialEq)]
pub enum Press {
    /// Show this browser on the screen.
    Use(u128),
    /// Show the screen's own page again.
    Own,
    Edit(u128),
    Delete(u128),
    New,
    /// The list's next page.
    More,
    Close,
    Save,
    Back,
}

/// The form's fields, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Name,
    Url,
    Width,
    Height,
}

/// The form a browser is made or changed in.
pub struct Form {
    /// The browser being changed; None for a new one.
    pub editing: Option<u128>,
    pub name: Field,
    pub url: Field,
    pub width: Field,
    pub height: Field,
    pub focus: Focus,
    pub firsts: [usize; 4],
    pub refused: Option<Phrase>,
}

/// One thing drawn on the panel.
#[derive(Clone, Debug, PartialEq)]
pub enum Widget {
    Title(Phrase),
    Label(Phrase),
    Button { label: Phrase, press: Press, enabled: bool },
    Field(Focus),
}

pub struct BrowsersWindow {
    /// The screen the window was opened from.
    pub target: Target,
    /// The world's browsers, as last sent.
    pub list: Vec<Shared>,
    /// The browser the screen shows now.
    pub current: Option<u128>,
    /// The first browser listed.
    pub first: usize,
    pub form: Option<Form>,
}

impl Form {
    fn new(editing: Option<&Shared>) -> Self {
        let (name, url, size) = editing.map_or(("", "", [1280, 720]), |shared| (shared.name.as_str(), shared.url.as_str(), shared.resolution));
        Self {
            editing: editing.map(|shared| shared.id),
            name: Field::new(name, MAX_NAME * 2),
            url: Field::new(url, MAX_URL),
            width: Field::new(&size[0].to_string(), 5),
            height: Field::new(&size[1].to_string(), 5),
            focus: if editing.is_some() { Focus::Url } else { Focus::Name },
            firsts: [0; 4],
            refused: None,
        }
    }

    pub fn field(&self, focus: Focus) -> &Field {
        match focus {
            Focus::Name => &self.name,
            Focus::Url => &self.url,
            Focus::Width => &self.width,
            Focus::Height => &self.height,
        }
    }

    fn field_mut(&mut self, focus: Focus) -> &mut Field {
        match focus {
            Focus::Name => &mut self.name,
            Focus::Url => &mut self.url,
            Focus::Width => &mut self.width,
            Focus::Height => &mut self.height,
        }
    }

    /// What the form asks of the server; None (and why) when it is not filled in.
    fn edit(&mut self) -> Option<BrowserEdit> {
        let size = (self.width.text().trim().parse::<u32>(), self.height.text().trim().parse::<u32>());
        const BAD_SIZE: Phrase = Phrase::new("webdisplays.h.browsers.bad_size", "The size is two numbers of pixels.");
        let (Ok(width), Ok(height)) = size else {
            self.refused = Some(BAD_SIZE);
            return None;
        };
        if width == 0 || height == 0 {
            self.refused = Some(BAD_SIZE);
            return None;
        }
        if crate::model::url::clean(&self.url.text()).is_err() {
            self.refused = Some(words::BAD_ADDRESS);
            return None;
        }
        let (name, url, size) = (self.name.text(), self.url.text(), [width, height]);
        Some(match self.editing {
            Some(id) => BrowserEdit::Edit { id, name, url, size },
            None => BrowserEdit::Create { name, url, size },
        })
    }
}

impl BrowsersWindow {
    pub fn new(target: Target, list: Vec<Shared>, current: Option<u128>) -> Self {
        Self { target, list, current, first: 0, form: None }
    }

    /// The server's list and the screen's choice, as they are now.
    pub fn sync(&mut self, list: Vec<Shared>, current: Option<u128>) {
        self.list = list;
        self.current = current;
        if self.first >= self.list.len() {
            self.first = 0;
        }
    }

    /// The panel and everything on it.
    pub fn widgets(&self, area: Rect) -> (Rect, Vec<(Rect, Widget)>) {
        let panel = centred(area, SIZE);
        let (x, y, wide) = (panel[0] + PAD, panel[1] + PAD, panel[2] - 2.0 * PAD);
        let button = |label: Phrase, press: Press, enabled: bool| Widget::Button { label, press, enabled };
        let mut list = Vec::new();
        let bottom = panel[1] + panel[3] - 26.0;
        if let Some(form) = &self.form {
            let title = if form.editing.is_some() {
                Phrase::new("webdisplays.h.browsers.form.change", "Change a shared browser")
            } else {
                Phrase::new("webdisplays.h.browsers.form.new", "New shared browser")
            };
            list.push(([x, y, wide, 10.0], Widget::Title(title)));
            let rows = [(words::NAME, Focus::Name), (Phrase::new("webdisplays.h.browsers.form.address", "Address"), Focus::Url)];
            for (index, (label, focus)) in rows.into_iter().enumerate() {
                let top = y + 20.0 + index as f32 * 24.0;
                list.push(([x, top + 4.0, 50.0, 10.0], Widget::Label(label)));
                list.push(([x + 54.0, top, wide - 54.0, 16.0], Widget::Field(focus)));
            }
            let top = y + 20.0 + 2.0 * 24.0;
            list.push(([x, top + 4.0, 50.0, 10.0], Widget::Label(words::PIXELS)));
            list.push(([x + 54.0, top, 50.0, 16.0], Widget::Field(Focus::Width)));
            list.push(([x + 108.0, top + 4.0, 8.0, 10.0], Widget::Label(words::TIMES)));
            list.push(([x + 118.0, top, 50.0, 16.0], Widget::Field(Focus::Height)));
            if let Some(why) = &form.refused {
                list.push(([x, top + 26.0, wide, 10.0], Widget::Label(why.clone())));
            }
            let half = (wide - 6.0) / 2.0;
            list.push(([x, bottom, half, 18.0], button(words::SAVE, Press::Save, true)));
            list.push(([x + half + 6.0, bottom, half, 18.0], button(Phrase::new("webdisplays.h.browsers.back", "Back"), Press::Back, true)));
            return (panel, list);
        }
        list.push(([x, y, wide, 10.0], Widget::Title(Phrase::new("webdisplays.h.browsers.title", "Shared browsers: one page on as many screens as you like"))));
        if self.list.is_empty() {
            list.push(([x, y + 22.0, wide, 10.0], Widget::Label(Phrase::new("webdisplays.h.browsers.none", "None yet. Make one, then choose it on each screen that should show it."))));
        }
        for (row, shared) in self.list.iter().skip(self.first).take(ROWS).enumerate() {
            let top = y + 18.0 + row as f32 * 22.0;
            let used = self.current == Some(shared.id);
            let name: String = shared.name.chars().take(24).collect();
            // A name and a size: the same in every language.
            let row = Phrase::untranslated("%s  %sx%s").with(name).with(shared.resolution[0]).with(shared.resolution[1]);
            list.push(([x, top + 4.0, wide - 190.0, 10.0], Widget::Label(row)));
            let right = x + wide;
            let show = if used { Phrase::new("webdisplays.h.browsers.shown", "Shown") } else { Phrase::new("webdisplays.h.browsers.show", "Show") };
            list.push(([right - 184.0, top, 64.0, 16.0], button(show, Press::Use(shared.id), !used)));
            list.push(([right - 116.0, top, 56.0, 16.0], button(Phrase::new("webdisplays.h.browsers.change", "Change"), Press::Edit(shared.id), true)));
            // The Java mod's own keys, here and for "New": the same buttons.
            list.push(([right - 56.0, top, 56.0, 16.0], button(Phrase::new("webdisplays.gui.seturl.browser.delete", "Delete"), Press::Delete(shared.id), true)));
        }
        let quarter = (wide - 3.0 * 6.0) / 4.0;
        let at = |index: usize| [x + index as f32 * (quarter + 6.0), bottom, quarter, 18.0];
        list.push((at(0), button(Phrase::new("webdisplays.gui.seturl.browser.new", "New"), Press::New, true)));
        list.push((at(1), button(Phrase::new("webdisplays.h.browsers.own", "Own page"), Press::Own, self.current.is_some())));
        list.push((at(2), button(Phrase::new("webdisplays.h.browsers.more", "More"), Press::More, self.list.len() > ROWS)));
        list.push((at(3), button(words::CLOSE, Press::Close, true)));
        (panel, list)
    }

    fn press(&mut self, press: Press) -> Vec<Action> {
        let configure = |change: Change| vec![Action::Send(ToServer::Configure { target: self.target, change })];
        match press {
            Press::Use(id) => configure(Change::Browser(Some(id))),
            Press::Own => configure(Change::Browser(None)),
            Press::Delete(id) => vec![Action::Send(ToServer::Browser(BrowserEdit::Delete { id }))],
            Press::Edit(id) => {
                self.form = self.list.iter().find(|shared| shared.id == id).map(|shared| Form::new(Some(shared)));
                Vec::new()
            }
            Press::New => {
                self.form = Some(Form::new(None));
                Vec::new()
            }
            Press::More => {
                self.first = if self.first + ROWS < self.list.len() { self.first + ROWS } else { 0 };
                Vec::new()
            }
            Press::Close => vec![Action::Close],
            Press::Back => {
                self.form = None;
                Vec::new()
            }
            Press::Save => {
                let Some(edit) = self.form.as_mut().and_then(Form::edit) else {
                    return Vec::new();
                };
                self.form = None;
                vec![Action::Send(ToServer::Browser(edit))]
            }
        }
    }

    pub fn click(&mut self, at: [f32; 2], area: Rect, measure: &dyn Fn(&str) -> f32) -> Vec<Action> {
        let (_, widgets) = self.widgets(area);
        for (rect, widget) in widgets {
            if !inside(rect, at) {
                continue;
            }
            return match widget {
                Widget::Button { press, enabled: true, .. } => self.press(press),
                Widget::Field(focus) => {
                    if let Some(form) = self.form.as_mut() {
                        form.focus = focus;
                        let index = form.field(focus).index_at(form.firsts[focus as usize], at[0] - rect[0] - 4.0, measure);
                        form.field_mut(focus).move_to(index, false);
                    }
                    Vec::new()
                }
                _ => Vec::new(),
            };
        }
        Vec::new()
    }

    pub fn key(&mut self, key: &str, held: u32) -> Vec<Action> {
        let Some(form) = self.form.as_mut() else {
            return Vec::new();
        };
        form.refused = None;
        if key.ends_with(".tab") || key == "tab" {
            form.focus = match form.focus {
                Focus::Name => Focus::Url,
                Focus::Url => Focus::Width,
                Focus::Width => Focus::Height,
                Focus::Height => Focus::Name,
            };
            return Vec::new();
        }
        let focus = form.focus;
        match form.field_mut(focus).key(key, held & modifiers::SHIFT != 0, held & modifiers::CONTROL != 0) {
            Outcome::Submit => self.press(Press::Save),
            Outcome::Copy(text) => vec![Action::Copy(text)],
            Outcome::Paste => vec![Action::Paste],
            Outcome::Done | Outcome::Ignored => Vec::new(),
        }
    }

    pub fn typed(&mut self, c: char) {
        if let Some(form) = self.form.as_mut() {
            let focus = form.focus;
            if matches!(focus, Focus::Width | Focus::Height) && !c.is_ascii_digit() {
                return;
            }
            form.refused = None;
            form.field_mut(focus).insert(c.encode_utf8(&mut [0; 4]));
        }
    }

    pub fn paste(&mut self, text: &str) {
        if let Some(form) = self.form.as_mut() {
            let focus = form.focus;
            let digits = matches!(focus, Focus::Width | Focus::Height);
            let kept: String = text.chars().filter(|c| !digits || c.is_ascii_digit()).collect();
            form.field_mut(focus).insert(&kept);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::side::Side;

    const AREA: Rect = [0.0, 0.0, 426.0, 240.0];
    const TARGET: Target = Target { core: [1, 2, 3], side: Side::North };

    fn measure(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    fn shared(id: u128, name: &str) -> Shared {
        Shared { id, name: name.to_owned(), url: "https://example.org/".to_owned(), resolution: [1280, 720], owner: 1 }
    }

    fn window() -> BrowsersWindow {
        BrowsersWindow::new(TARGET, vec![shared(10, "Cinema"), shared(11, "Lobby")], Some(11))
    }

    fn click(window: &mut BrowsersWindow, wanted: Press) -> Vec<Action> {
        let (_, widgets) = window.widgets(AREA);
        let (rect, _) = widgets.into_iter().find(|(_, widget)| matches!(widget, Widget::Button { press, .. } if *press == wanted)).expect("no such button");
        window.click([rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0], AREA, &measure)
    }

    #[test]
    fn everything_is_in_the_panel_and_buttons_do_not_overlap() {
        for form in [false, true] {
            let mut window = window();
            if form {
                click(&mut window, Press::New);
            }
            let (panel, widgets) = window.widgets(AREA);
            assert!(panel[0] >= 0.0 && panel[0] + panel[2] <= AREA[2] && panel[1] >= 0.0 && panel[1] + panel[3] <= AREA[3], "{panel:?}");
            let solid: Vec<Rect> = widgets.iter().filter(|(_, widget)| !matches!(widget, Widget::Label(_) | Widget::Title(_))).map(|(rect, _)| *rect).collect();
            for (rect, widget) in &widgets {
                assert!(rect[0] >= panel[0] && rect[0] + rect[2] <= panel[0] + panel[2] + 0.5 && rect[1] + rect[3] <= panel[1] + panel[3] + 0.5, "{widget:?} {rect:?}");
            }
            for (index, a) in solid.iter().enumerate() {
                for b in &solid[index + 1..] {
                    let apart = a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1];
                    assert!(apart, "{a:?} and {b:?}");
                }
            }
        }
    }

    #[test]
    fn a_screen_is_given_a_shared_browser_or_its_own_page_back() {
        let mut window = window();
        assert_eq!(click(&mut window, Press::Use(10)), [Action::Send(ToServer::Configure { target: TARGET, change: Change::Browser(Some(10)) })]);
        assert_eq!(click(&mut window, Press::Use(11)), [], "the one it shows already");
        assert_eq!(click(&mut window, Press::Own), [Action::Send(ToServer::Configure { target: TARGET, change: Change::Browser(None) })]);
        assert_eq!(click(&mut window, Press::Delete(10)), [Action::Send(ToServer::Browser(BrowserEdit::Delete { id: 10 }))]);
        assert_eq!(click(&mut window, Press::Close), [Action::Close]);
    }

    #[test]
    fn a_new_browser_is_typed_in_and_sent() {
        let mut window = window();
        click(&mut window, Press::New);
        for c in "Cinema".chars() {
            window.typed(c);
        }
        window.key("key.keyboard.tab", 0);
        for c in "example.org/film".chars() {
            window.typed(c);
        }
        window.key("tab", 0);
        window.key("a", modifiers::CONTROL);
        for c in "19x20".chars() {
            window.typed(c);
        }
        let sent = click(&mut window, Press::Save);
        let expected = BrowserEdit::Create { name: "Cinema".to_owned(), url: "example.org/film".to_owned(), size: [1920, 720] };
        assert_eq!(sent, [Action::Send(ToServer::Browser(expected))]);
        assert!(window.form.is_none(), "back to the list");
    }

    #[test]
    fn a_form_not_filled_in_says_why_and_stays() {
        let mut window = window();
        click(&mut window, Press::New);
        assert_eq!(click(&mut window, Press::Save), []);
        assert_eq!(window.form.as_ref().unwrap().refused, Some(words::BAD_ADDRESS), "no address");
        click(&mut window, Press::Back);
        assert!(window.form.is_none());
    }

    #[test]
    fn a_row_says_the_browser_and_whether_the_screen_shows_it() {
        let window = BrowsersWindow::new(TARGET, vec![shared(10, "100% Cinema, a name far too long to show whole"), shared(11, "Lobby")], Some(11));
        let (_, widgets) = window.widgets(AREA);
        let said: Vec<String> = widgets.iter().filter_map(|(_, widget)| match widget {
            Widget::Label(text) => Some(text.english()),
            Widget::Button { label, press: Press::Use(_), .. } => Some(label.english()),
            _ => None,
        }).collect();
        assert_eq!(said, ["100% Cinema, a name far   1280x720", "Show", "Lobby  1280x720", "Shown"], "a name is cut at 24 characters, and its % is no placeholder");
    }

    #[test]
    fn changing_one_starts_from_what_it_is() {
        let mut window = window();
        click(&mut window, Press::Edit(10));
        let form = window.form.as_ref().unwrap();
        assert_eq!((form.name.text().as_str(), form.url.text().as_str(), form.editing), ("Cinema", "https://example.org/", Some(10)));
        let sent = window.key("enter", 0);
        let expected = BrowserEdit::Edit { id: 10, name: "Cinema".to_owned(), url: "https://example.org/".to_owned(), size: [1280, 720] };
        assert_eq!(sent, [Action::Send(ToServer::Browser(expected))]);
    }

    #[test]
    fn a_long_list_is_paged_and_follows_the_server() {
        let many: Vec<Shared> = (0..10).map(|id| shared(id, "b")).collect();
        let mut window = BrowsersWindow::new(TARGET, many, None);
        click(&mut window, Press::More);
        assert_eq!(window.first, ROWS);
        click(&mut window, Press::More);
        assert_eq!(window.first, 0, "round again");
        window.first = ROWS;
        window.sync(vec![shared(1, "only")], Some(1));
        assert_eq!((window.first, window.current), (0, Some(1)), "the page it was on is gone");
    }
}
