//! Draws the mod's windows (`client/gui`) with the game's rectangles and text.
//!
//! A window says what to write as a `Phrase`; here it is read in the player's language
//! (`lang::plain`) just before it is drawn, so what is measured to place a text is the
//! text that is shown.
use super::gui::browsers::{self, BrowsersWindow};
use super::gui::config::{ConfigWindow, Widget};
use super::gui::field::Field;
use super::gui::pad::PadWindow;
use super::gui::redstone::{self, RedstoneWindow};
use super::gui::screens::{self, ScreensWindow};
use super::gui::url::{BUTTONS, HINT, TITLE, UrlWindow};
use super::gui::{Rect, Window, centred, inside};
use crate::lang;
use crate::model::phrase::Phrase;
use horizon_client::mod_api::{Host, HudBatch};

const PANEL: u32 = 0xF010_1014;
const RIM: u32 = 0xFF8B_8B8B;
const WHITE: u32 = 0xFF_FFFF;
const GREY: u32 = 0xC0_C0C0;
const RED: u32 = 0xFF_5555;
const SELECTION: u32 = 0xFF33_55AA;

pub fn outline(host: &dyn Host, batch: &mut HudBatch, [x, y, w, h]: Rect, argb: u32) {
    host.fill(batch, [x, y, w, 1.0], argb);
    host.fill(batch, [x, y + h - 1.0, w, 1.0], argb);
    host.fill(batch, [x, y, 1.0, h], argb);
    host.fill(batch, [x + w - 1.0, y, 1.0, h], argb);
}

pub fn panel(host: &dyn Host, batch: &mut HudBatch, rect: Rect, title: &str) {
    host.fill(batch, rect, PANEL);
    outline(host, batch, rect, RIM);
    host.text(batch, title, [rect[0] + 8.0, rect[1] + 8.0], WHITE, true);
}

pub fn button(host: &dyn Host, batch: &mut HudBatch, rect: Rect, label: &str, enabled: bool, mouse: [f32; 2]) {
    let hovered = enabled && inside(rect, mouse);
    host.fill(batch, rect, if hovered { 0xFF5A_5A8C } else { 0xFF3A_3A3A });
    outline(host, batch, rect, if hovered { 0xFFFF_FFFF } else { RIM });
    let width = host.text_width(label);
    let color = if enabled { WHITE } else { 0x80_8080 };
    host.text(batch, label, [rect[0] + (rect[2] - width) / 2.0, rect[1] + (rect[3] - 8.0) / 2.0], color, true);
}

/// A field's box, the part of its text that fits, its selection and its caret. Gives back
/// the first character shown, for the next frame's view and for clicks.
pub fn field(host: &dyn Host, batch: &mut HudBatch, rect: Rect, field: &Field, first: usize, focused: bool) -> usize {
    host.fill(batch, rect, 0xFF00_0000);
    outline(host, batch, rect, if focused { 0xFFFF_FFFF } else { 0xFFA0_A0A0 });
    let measure = |text: &str| host.text_width(text);
    let (first, last) = field.view(first, rect[2] - 8.0, measure);
    let x_of = |index: usize| -> f32 {
        let index = index.clamp(first, last);
        rect[0] + 4.0 + measure(&field.before(index)) - measure(&field.before(first))
    };
    let y = rect[1] + (rect[3] - 8.0) / 2.0;
    if let Some((from, to)) = field.selection() {
        let (left, right) = (x_of(from), x_of(to));
        host.fill(batch, [left, y - 1.0, right - left, 10.0], SELECTION);
    }
    let shown: String = field.text().chars().skip(first).take(last - first).collect();
    host.text(batch, &shown, [rect[0] + 4.0, y], WHITE, false);
    if focused {
        host.fill(batch, [x_of(field.caret()), y - 1.0, 1.0, 10.0], 0xFFFF_FFFF);
    }
    first
}

fn url(host: &dyn Host, batch: &mut HudBatch, window: &mut UrlWindow, area: Rect, mouse: [f32; 2]) {
    let rect = UrlWindow::panel(area);
    panel(host, batch, rect, &lang::plain(&TITLE));
    window.first = field(host, batch, UrlWindow::field_box(rect), &window.field, window.first, true);
    let below = UrlWindow::field_box(rect);
    let (line, color) = match &window.refused {
        Some(why) => (lang::plain(why), RED),
        None => (lang::plain(&HINT), GREY),
    };
    host.text(batch, &line, [below[0], below[1] + 20.0], color, false);
    for (index, (_, label)) in BUTTONS.iter().enumerate() {
        button(host, batch, UrlWindow::button_box(rect, index), &lang::plain(label), true, mouse);
    }
}

fn shared(host: &dyn Host, batch: &mut HudBatch, window: &mut BrowsersWindow, area: Rect, mouse: [f32; 2]) {
    let (rect, widgets) = window.widgets(area);
    host.fill(batch, rect, PANEL);
    outline(host, batch, rect, RIM);
    for (rect, widget) in widgets {
        match widget {
            browsers::Widget::Title(text) => host.text(batch, &lang::plain(&text), [rect[0], rect[1]], WHITE, true),
            browsers::Widget::Label(text) => host.text(batch, &lang::plain(&text), [rect[0], rect[1]], GREY, false),
            browsers::Widget::Button { label, enabled, .. } => button(host, batch, rect, &lang::plain(&label), enabled, mouse),
            browsers::Widget::Field(focus) => {
                if let Some(form) = window.form.as_mut() {
                    let focused = form.focus == focus;
                    form.firsts[focus as usize] = field(host, batch, rect, form.field(focus), form.firsts[focus as usize], focused);
                }
            }
        }
    }
}

fn listed(host: &dyn Host, batch: &mut HudBatch, window: &ScreensWindow, area: Rect, mouse: [f32; 2]) {
    let (rect, widgets) = window.widgets(area);
    host.fill(batch, rect, PANEL);
    outline(host, batch, rect, RIM);
    for (rect, widget) in widgets {
        match widget {
            screens::Widget::Title(text) => host.text(batch, &lang::plain(&text), [rect[0], rect[1]], WHITE, true),
            screens::Widget::Label(text) => host.text(batch, &lang::plain(&text), [rect[0], rect[1]], GREY, false),
            screens::Widget::Button { label, enabled, .. } => button(host, batch, rect, &lang::plain(&label), enabled, mouse),
        }
    }
}

/// The opened minePad: its page is drawn by the renderer; this adds the address bar above.
fn pad(host: &dyn Host, batch: &mut HudBatch, window: &mut PadWindow, area: Rect, mouse: [f32; 2]) {
    let bar = PadWindow::address_box(area);
    window.first = field(host, batch, bar, &window.address, window.first, window.typing_address);
    if window.refused {
        outline(host, batch, bar, 0xFFFF_5555);
    }
    button(host, batch, PadWindow::go_box(area), &lang::plain(&Phrase::new("webdisplays.h.pad.go", "Go")), true, mouse);
}

fn redstone(host: &dyn Host, batch: &mut HudBatch, window: &mut RedstoneWindow, area: Rect, mouse: [f32; 2]) {
    let rect = RedstoneWindow::panel(area);
    panel(host, batch, rect, &lang::plain(&redstone::TITLE));
    for index in 0..2 {
        host.text(batch, &lang::plain(&redstone::LABELS[index]), RedstoneWindow::label_at(rect, index), GREY, false);
        let focused = window.focus == index;
        window.firsts[index] = field(host, batch, RedstoneWindow::field_box(rect, index), &window.fields[index], window.firsts[index], focused);
    }
    if let Some(why) = &window.refused {
        let below = RedstoneWindow::field_box(rect, 1);
        host.text(batch, &lang::plain(why), [below[0], below[1] + 19.0], RED, false);
    }
    for (index, label) in redstone::BUTTONS.iter().enumerate() {
        button(host, batch, RedstoneWindow::button_box(rect, index), &lang::plain(label), true, mouse);
    }
}

pub fn checkbox(host: &dyn Host, batch: &mut HudBatch, rect: Rect, on: bool, enabled: bool) {
    host.fill(batch, rect, 0xFF00_0000);
    outline(host, batch, rect, if enabled { 0xFFA0_A0A0 } else { 0xFF50_5050 });
    if on {
        let mark = if enabled { 0xFFFF_FFFF } else { 0xFF80_8080 };
        host.fill(batch, [rect[0] + 3.0, rect[1] + 3.0, rect[2] - 6.0, rect[3] - 6.0], mark);
    }
}

/// A bar filled up to `value` (0 to 1), with a handle there.
pub fn slider(host: &dyn Host, batch: &mut HudBatch, rect: Rect, value: f32, enabled: bool) {
    let value = value.clamp(0.0, 1.0);
    host.fill(batch, [rect[0], rect[1] + rect[3] / 2.0 - 2.0, rect[2], 4.0], 0xFF00_0000);
    let fill = if enabled { 0xFF5A_5A8C } else { 0xFF40_4040 };
    host.fill(batch, [rect[0], rect[1] + rect[3] / 2.0 - 2.0, rect[2] * value, 4.0], fill);
    let handle = [rect[0] + rect[2] * value - 2.0, rect[1], 4.0, rect[3]];
    host.fill(batch, handle, if enabled { 0xFFFF_FFFF } else { 0xFF80_8080 });
}

fn config(host: &dyn Host, batch: &mut HudBatch, window: &mut ConfigWindow, area: Rect, mouse: [f32; 2]) {
    let (rect, widgets) = window.widgets(area);
    host.fill(batch, rect, PANEL);
    outline(host, batch, rect, RIM);
    for (rect, widget) in widgets {
        match widget {
            Widget::Title(text) => host.text(batch, &lang::plain(&text), [rect[0], rect[1]], WHITE, true),
            Widget::Label(text) => host.text(batch, &lang::plain(&text), [rect[0], rect[1]], GREY, false),
            Widget::Button { label, enabled, .. } => button(host, batch, rect, &lang::plain(&label), enabled, mouse),
            Widget::Check { on, enabled, .. } => checkbox(host, batch, rect, on, enabled),
            Widget::Slider { value, enabled } => slider(host, batch, rect, value, enabled),
            Widget::Field(focus) => {
                let first = window.firsts[focus as usize];
                let focused = window.focus == Some(focus);
                window.firsts[focus as usize] = field(host, batch, rect, window.field(focus), first, focused);
            }
        }
    }
}

/// Draws the open window over `area` (the whole window, in GUI pixels).
pub fn window(host: &dyn Host, batch: &mut HudBatch, window: &mut Window, area: Rect, mouse: [f32; 2]) {
    match window {
        Window::Url(open) => {
            host.fill(batch, area, 0x6000_0000);
            url(host, batch, open, area, mouse);
        }
        Window::Config(open) => {
            host.fill(batch, area, 0x6000_0000);
            config(host, batch, open, area, mouse);
        }
        Window::Redstone(open) => {
            host.fill(batch, area, 0x6000_0000);
            redstone(host, batch, open, area, mouse);
        }
        Window::Browsers(open) => {
            host.fill(batch, area, 0x6000_0000);
            shared(host, batch, open, area, mouse);
        }
        Window::Screens(open) => {
            host.fill(batch, area, 0x6000_0000);
            listed(host, batch, open, area, mouse);
        }
        // No veil: the page itself is under the window, drawn with the world.
        Window::Pad(open) => pad(host, batch, open, area, mouse),
        // The world stays in view: the player watches the page they type on.
        Window::Keyboard(_) => {
            // As wide as its line is in the player's language.
            let line = lang::plain(&Phrase::new("webdisplays.h.keyboard.typing", "Typing on the screen. Escape to stop."));
            let width = host.text_width(&line) + 16.0;
            let mut rect = centred(area, [width, 18.0]);
            rect[1] = area[1] + area[3] - 44.0;
            host.fill(batch, rect, PANEL);
            outline(host, batch, rect, RIM);
            host.text(batch, &line, [rect[0] + 8.0, rect[1] + 5.0], WHITE, true);
        }
    }
}
