//! The screens the player can see: found as the game draws their cores, kept while they are
//! near, each given a browser and turned into quads for the renderer.
//!
//! The game tells a client mod nothing when a block entity changes, and only draws the ones
//! within 64 blocks and in view. So the cores seen being drawn are remembered and looked at
//! again every frame (`mod_block_entity` is a cheap copy, and its revision says when it is
//! worth reading), until they are gone or too far.
use super::browsers::{Change, Set, Want};
use super::gui::browsers::BrowsersWindow;
use super::gui::config::{ConfigWindow, Traced};
use super::gui::keyboard::KeyboardWindow;
use super::gui::pad::PadWindow;
use super::pad;
use super::gui::redstone::RedstoneWindow;
use super::gui::screens::ScreensWindow;
use super::gui::url::UrlWindow;
use super::gui::{Action, Keys, Ui, Window};
use super::pointer::{At, Pointer};
use super::settings::Settings;
use super::sound;
use crate::acoustics::solve::Heard;
use crate::acoustics::worker::{self, Job, Table, Worker};
use crate::lang;
use crate::ids::{self, CONFIGURATOR, ID, LASER_POINTER, LINKER, MAX_RESOLUTION, MAX_SIZE, MINEPAD, OWNER_THIEF, PEN, REACH, UPGRADE_LASER};
use crate::model::poly::{Surface, nearest_corner};
use crate::model::polysave;
use crate::ipc::host::{Host as WdHost, Status};
use crate::model::groups::{self, Member};
use crate::model::hit::{self, Face, Tile};
use crate::model::home;
use crate::model::url;
use crate::model::letterbox::Fit;
use crate::model::phrase::Phrase;
use crate::model::shared::Shared;
use crate::model::side::Rotation;
use crate::model::resolution;
use crate::model::screen::{MAX_VOLUME, Screen};
use crate::model::shape::{self, Shape, connected_origin};
use crate::model::side::{Pos, Side};
use crate::model::speaker::{Channel, Kind};
use crate::net::msg::{CHANNEL, Input, Pointer as What, Target, ToClient, ToServer};
use crate::proto::audio::{Band, Emitter, Scene, Source};
use crate::proto::ctl::{Button, KeyKind, ToHost, modifiers};
use crate::render::geometry::Quad;
use crate::render::gpu::{MARK_PAGE, NO_PAGE, PANEL_PAGE, Renderer};
use crate::render::poly as traced;
use crate::render::glow::Glow;
use crate::render::screens::{Placement, glows, quads};
use crate::tags;
use horizon_client::mod_api::{Host, ModBlockEntity};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Blocks within which a screen gets a browser.
const LOAD: f64 = 64.0;
/// Blocks beyond which a screen loses its browser and is forgotten. Further than `LOAD`,
/// so that walking along the edge does not open and close it over and over.
const UNLOAD: f64 = 96.0;
/// Frames between two looks at the blocks of a screen: its shape only changes when blocks
/// are placed or broken, and asking the world about every block every frame is wasteful.
const RESHAPE_EVERY: u32 = 20;
/// Pictures a second asked of a browser.
const FPS: u32 = 60;
/// How far the hand reaches, in blocks: a screen further away takes a laser pointer.
const HAND: f64 = 5.0;
/// What one step of the mouse wheel scrolls a page by, as Windows counts it.
const WHEEL_STEP: f32 = 120.0;
/// Seconds between two scenes sent to hcef-host, which glides from one to the next.
const SCENE_EVERY: f64 = 0.05;
/// How much of the way to the acoustics' newest answer a scene goes: with a scene every
/// `SCENE_EVERY`, a change is heard over about a third of a second.
const GLIDE: f32 = 0.15;

/// What a screen with no address shows: the mod's own page (`model::home`), saying what to
/// do in the player's language.
fn home() -> String {
    home::address(&lang::plain(&home::HINT))
}

/// The key the minePad's browser is kept under: like a shared browser's, a place no block
/// can be at.
const PAD_KEY: Key = ([0, i32::MIN, 1], Side::Top);
/// The setting the minePad's address is kept under.
const PAD_SETTING: &str = "pad_url";

/// What a browser is for: a screen, by its core and face.
pub type Key = (Pos, Side);

/// A core's screens as last read, and what was worked out from them.
struct Seen {
    /// The block entity the screens were read from: its revision says when to read again.
    read_from: Option<ModBlockEntity>,
    screens: Vec<Screen>,
    /// Each screen's place and shape, by side; worked out again every `RESHAPE_EVERY` frames.
    shapes: HashMap<Side, (Placement, Shape)>,
    /// The sheet of a traced screen, which has no blocks to take a shape from.
    sheet: Option<Surface>,
    /// How that sheet is bent and turned, for the settings window.
    traced: Option<Traced>,
    /// The corners that sheet was traced on, shown to a player holding the pen.
    corners: Vec<Pos>,
}

/// The key a shared browser is kept under among the browsers, which are otherwise kept by
/// screen: a place no block can be at (no world is that deep), made from the browser's id.
fn shared_key(id: u128) -> Key {
    ([id as u32 as i32, i32::MIN, (id >> 32) as u32 as i32], Side::Bottom)
}

/// The browser whose picture a screen shows: a shared one it was given (when the world
/// still has it), else its link group's, else its own.
fn page_key(shared: &HashMap<u128, Shared>, linked: &HashMap<Key, Linked>, pos: Pos, screen: &Screen) -> Key {
    if let Some(id) = screen.browser.filter(|id| shared.contains_key(id)) {
        return shared_key(id);
    }
    linked.get(&(pos, screen.side)).map_or((pos, screen.side), |linked| linked.origin)
}

/// What to tell hcef-host for something done to a page. `held` is the mouse buttons the
/// browser was last told are down, kept up to date; `on_page` turns the pixel that
/// travelled into a pixel of what the browser renders.
fn to_host(browser: u32, held: &mut u32, on_page: &dyn Fn([u32; 2]) -> [i32; 2], input: Input) -> Vec<ToHost> {
    let mut commands = Vec::new();
    match input {
        Input::Pointer { what: What::Move, pixel, .. } => {
            commands.push(ToHost::MouseMove { browser, at: on_page(pixel), modifiers: *held, leave: false });
        }
        Input::Pointer { what, pixel, button } => {
            let (button, bit) = match button {
                1 => (Button::Middle, modifiers::MIDDLE_BUTTON),
                2 => (Button::Right, modifiers::RIGHT_BUTTON),
                _ => (Button::Left, modifiers::LEFT_BUTTON),
            };
            let down = what == What::Down;
            let at = on_page(pixel);
            if down {
                // The page under the click takes the keyboard, as in any browser, and
                // hears the pointer arrive before the button.
                commands.push(ToHost::Focus { browser, focused: true });
                commands.push(ToHost::MouseMove { browser, at, modifiers: *held, leave: false });
                *held |= bit;
            } else {
                *held &= !bit;
            }
            commands.push(ToHost::MouseButton { browser, at, button, down, clicks: 1, modifiers: *held });
        }
        Input::Scroll { pixel, delta } => commands.push(ToHost::Wheel { browser, at: on_page(pixel), delta, modifiers: *held }),
        Input::Keys(keys) => {
            for key in keys {
                let kind = match key.kind {
                    0 => KeyKind::Down,
                    1 => KeyKind::Up,
                    _ => KeyKind::Char,
                };
                commands.push(ToHost::Key { browser, kind, key: key.key, native: key.native, character: key.character, modifiers: key.modifiers });
            }
        }
        Input::Text(text) => commands.push(ToHost::Commit { browser, text }),
    }
    commands
}

/// A linked screen's place in its group: screens of one link on one face share a picture.
#[derive(Clone, Copy)]
struct Linked {
    /// The screen of the group that owns its browser.
    origin: Key,
    /// The part of the group's picture this screen shows.
    tile: Tile,
    /// The group's picture: as saved, as this client renders it, and the browser's size.
    resolution: [u32; 2],
    effective: [u32; 2],
    browser_size: [u32; 2],
}

/// A window to open on the next frame.
enum Wanted {
    /// A screen's address; its current one when the server gave it (a screen out of sight).
    Url(Target, Option<String>),
    Keyboard(Target),
    /// A screen's settings, for the player the server says this client is.
    Config(Target, u128),
    /// A redstone controller's two addresses.
    Redstone(Pos, String, String),
    /// The minePad, opened.
    Pad,
}

pub struct Client {
    cores: HashMap<Pos, Seen>,
    browsers: Set<Key>,
    wd_host: Option<WdHost>,
    frame: u32,
    /// The last thing said about hcef-host, so that it is said once.
    said: Option<Status>,
    /// The crosshair as a mouse on the screen it points at.
    pointer: Pointer,
    /// The mouse buttons each browser was last told are down, as CEF's modifier bits.
    held: HashMap<Key, u32>,
    /// The mod's windows.
    pub ui: Ui,
    wanted: Option<Wanted>,
    /// What the pointer's pacing is counted from.
    started: Instant,
    /// Works out what the place does to each sound, on its own thread.
    acoustics: Option<Worker>,
    /// What each block state does to sound, for the world this client is in.
    materials: Option<Table>,
    /// What each emitter is heard as, gliding toward the acoustics' answers.
    heard: HashMap<u32, Heard>,
    /// When the last scene went to hcef-host.
    scene_sent: f64,
    /// The player's settings; None until they are read, at the first frame.
    settings: Option<Settings>,
    /// The linked screens this client sees, by screen.
    linked: HashMap<Key, Linked>,
    /// The corners of the outline the player is tracing with the pen.
    pen: Vec<Pos>,
    /// The world's shared browsers, as the server sent them.
    shared: HashMap<u128, Shared>,
    /// The address the minePad shows; None until it is read from the settings.
    pad_url: Option<String>,
}

impl Default for Client {
    fn default() -> Self {
        Self {
            cores: HashMap::new(),
            browsers: Set::default(),
            wd_host: None,
            frame: 0,
            said: None,
            pointer: Pointer::default(),
            held: HashMap::new(),
            ui: Ui::default(),
            wanted: None,
            started: Instant::now(),
            acoustics: None,
            materials: None,
            heard: HashMap::new(),
            scene_sent: 0.0,
            settings: None,
            linked: HashMap::new(),
            pen: Vec::new(),
            shared: HashMap::new(),
            pad_url: None,
        }
    }
}

fn distance(from: [f64; 3], to: Pos) -> f64 {
    (0..3).map(|i| (f64::from(to[i]) + 0.5 - from[i]).powi(2)).sum::<f64>().sqrt()
}

impl Client {
    /// The player's settings, read from where the game keeps them the first time.
    pub fn settings(&mut self, host: &dyn Host) -> &mut Settings {
        self.settings.get_or_insert_with(|| Settings::read(|key| host.setting(ID, key)))
    }

    /// The game is drawing the core at `pos`: it is near and in view.
    pub fn seen(&mut self, pos: Pos) {
        self.cores.entry(pos).or_insert_with(|| Seen { read_from: None, screens: Vec::new(), shapes: HashMap::new(), sheet: None, traced: None, corners: Vec::new() });
    }

    /// Once a frame: reads the cores again, keeps hcef-host's browsers in line with what is
    /// near, and hands the renderer what to draw.
    pub fn frame(&mut self, host: &mut dyn Host, renderer: &Renderer) {
        self.frame = self.frame.wrapping_add(1);
        let Some(camera) = host.camera() else {
            // No world: nothing to show, and hcef-host has nothing to do.
            self.leave(renderer);
            return;
        };
        let eye = camera.position;
        let reshape = self.frame % RESHAPE_EVERY == 0;

        // Read what changed, forget what is gone or far.
        self.cores.retain(|pos, seen| {
            if distance(eye, *pos) > UNLOAD {
                return false;
            }
            let Some(entity) = host.mod_block_entity(*pos) else {
                return false;
            };
            let changed = seen.read_from.as_ref().is_none_or(|before| before.revision() != entity.revision());
            if changed {
                seen.screens = tags::screens(&entity.nbt);
                // A traced screen's master keeps its outline beside its screen.
                let outline = polysave::read_outline(&tags::from_compound(&entity.nbt));
                seen.traced = outline.as_ref().map(|outline| {
                    // The radius as it counts: one saved above what the walls leave room for
                    // bends the sheet no more than the widest.
                    let widest = outline.widest_radius().max(crate::model::poly::MIN_CURVE_RADIUS);
                    let radius = outline.curve_radius.clamp(crate::model::poly::MIN_CURVE_RADIUS, widest);
                    Traced { curved: outline.curved, flipped: outline.flipped, radius, widest }
                });
                seen.corners = outline.as_ref().map_or_else(Vec::new, |outline| outline.points().to_vec());
                seen.sheet = outline.as_ref().and_then(Surface::of);
                seen.read_from = Some(entity);
            }
            if seen.sheet.is_some() {
                seen.shapes.clear();
            } else if changed || reshape || seen.shapes.len() != seen.screens.len() {
                let world = |pos: Pos| ids::is_screen(&host.block(pos));
                seen.shapes = seen
                    .screens
                    .iter()
                    .map(|screen| {
                        let origin = connected_origin(&world, *pos, screen.side, MAX_SIZE);
                        let shape = Shape::compute(&world, origin, screen.side, screen.size, screen.shape_mode, Some(*pos));
                        let placement = Placement { origin, side: screen.side, rotation: screen.rotation, size: screen.size, page: NO_PAGE, tile: None, fit: None };
                        (screen.side, (placement, shape))
                    })
                    .collect();
            }
            true
        });

        // The screens that share a picture: those of one link on one face. One of them
        // owns the browser, chosen by where they are so that every client chooses alike.
        let mut keys: Vec<Key> = Vec::new();
        let mut members: Vec<Member> = Vec::new();
        for (pos, seen) in &self.cores {
            for screen in seen.screens.iter().filter(|screen| screen.is_linked()) {
                let Some((placement, _)) = seen.shapes.get(&screen.side) else { continue };
                keys.push((*pos, screen.side));
                members.push(Member {
                    pos: placement.origin,
                    dimension: camera.dimension.clone(),
                    side: screen.side,
                    rotation: screen.rotation,
                    size: screen.size,
                    resolution: screen.resolution,
                    link_id: screen.link_id.clone(),
                    mode: screen.link_mode,
                    origin: screen.link_origin,
                    browser: 0,
                });
            }
        }
        self.linked.clear();
        for group in groups::build(&members, MAX_RESOLUTION, 100) {
            let origin = keys[group.origin];
            for placed in &group.members {
                let linked = Linked { origin, tile: placed.tile, resolution: group.resolution, effective: group.effective, browser_size: group.browser_size() };
                self.linked.insert(keys[placed.index], linked);
            }
        }

        // A browser for each screen, near enough (or near enough to keep). A linked screen
        // has none of its own: being near it keeps its group's.
        let mut near_groups: HashSet<Key> = HashSet::new();
        for (key, linked) in &self.linked {
            if distance(eye, key.0) <= LOAD {
                near_groups.insert(linked.origin);
            }
        }
        let mut wants = Vec::new();
        // The shared browsers some screen near enough shows: one browser each, however
        // many screens show it.
        let mut shown: HashSet<u128> = HashSet::new();
        for (pos, seen) in &self.cores {
            let away = distance(eye, *pos);
            for screen in &seen.screens {
                let key = (*pos, screen.side);
                if let Some(id) = screen.browser.filter(|id| self.shared.contains_key(id)) {
                    if away <= LOAD || self.browsers.id(&shared_key(id)).is_some() {
                        shown.insert(id);
                    }
                    continue;
                }
                let linked = self.linked.get(&key);
                if linked.is_some_and(|linked| linked.origin != key) {
                    continue;
                }
                let near = away <= LOAD || near_groups.contains(&key);
                if !near && self.browsers.id(&key).is_none() {
                    continue;
                }
                let size = match linked {
                    Some(linked) => linked.browser_size,
                    None => resolution::browser_size(resolution::effective(screen.resolution, 100, MAX_RESOLUTION), screen.rotation),
                };
                wants.push(Want { key, size, fps: FPS, url: if screen.url.is_empty() { home() } else { sent_to(&screen.url) }, shown: true });
            }
        }
        // The minePad's browser lives while the pad is in the hand or open.
        let pad_open = matches!(self.ui.open, Some(Window::Pad(_)));
        let pad_held = Self::held(host).as_deref() == Some(MINEPAD);
        if pad_open || pad_held {
            let address = self.pad_url.get_or_insert_with(|| host.setting(ID, PAD_SETTING).filter(|address| !address.is_empty()).unwrap_or_else(home));
            wants.push(Want { key: PAD_KEY, size: pad::PAGE, fps: FPS, url: sent_to(address), shown: true });
        }
        for id in shown {
            let Some(shared) = self.shared.get(&id) else { continue };
            let size = resolution::browser_size(resolution::effective(shared.resolution, 100, MAX_RESOLUTION), Rotation::R0);
            wants.push(Want { key: shared_key(id), size, fps: FPS, url: sent_to(&shared.url), shown: true });
        }
        self.sync(host, renderer, &wants);

        // What to draw: every screen, with its browser's picture once it has one, and the
        // light each picture throws around it.
        let mut all: Vec<Quad> = Vec::new();
        let mut lights: Vec<Glow> = Vec::new();
        let mut see_through: HashSet<u32> = HashSet::new();
        for (pos, seen) in &self.cores {
            for (side, (placement, shape)) in &seen.shapes {
                let Some(screen) = seen.screens.iter().find(|screen| screen.side == *side) else { continue };
                // A shared browser's picture is fitted whole inside the screen; a linked
                // screen shows its part of the picture of its group's browser.
                let its_shared = screen.browser.and_then(|id| self.shared.get(&id));
                let linked = self.linked.get(&(*pos, *side)).filter(|_| its_shared.is_none());
                let browser = self.browsers.id(&page_key(&self.shared, &self.linked, *pos, screen));
                let fit = its_shared.map(|shared| Fit::for_screen(screen.size, screen.rotation.is_vertical(), shared.resolution));
                let placed = Placement { page: browser.unwrap_or(NO_PAGE), tile: linked.map(|linked| linked.tile), fit, ..*placement };
                all.extend(quads(&placed, shape));
                let Some(page) = browser else { continue };
                lights.extend(glows(&placed));
                if seen.screens.iter().any(|screen| screen.side == *side && screen.oled) {
                    see_through.insert(page);
                }
            }
        }
        // The traced screens: a sheet each, the page on its front.
        for (pos, seen) in &self.cores {
            let (Some(sheet), Some(screen)) = (&seen.sheet, seen.screens.first()) else { continue };
            let browser = self.browsers.id(&page_key(&self.shared, &self.linked, *pos, screen));
            let fit = screen.browser.and_then(|id| self.shared.get(&id)).map(|shared| traced::fit(sheet, shared.resolution));
            all.extend(traced::quads(sheet, browser.unwrap_or(NO_PAGE), PANEL_PAGE, fit.as_ref()));
            if let Some(page) = browser {
                lights.extend(traced::glows(sheet, page));
            }
        }
        all.extend(traced::trace(&self.pen, MARK_PAGE));
        // With the pen in hand, the outlines of the screens already traced show as well:
        // where each one's corners are, to trace the next one beside it.
        if Self::held(host).as_deref() == Some(PEN) {
            for seen in self.cores.values() {
                all.extend(traced::built(&seen.corners, MARK_PAGE));
            }
        }
        renderer.set_quads(all);
        // The pad, flat over the picture: in the hand, or large in the middle once opened.
        let pad_page = self.browsers.id(&PAD_KEY).unwrap_or(NO_PAGE);
        renderer.set_overlay(match (pad_open, pad_held) {
            (true, _) => pad::quads(pad::opened(), pad_page, PANEL_PAGE),
            (false, true) => pad::quads(pad::held(), pad_page, PANEL_PAGE),
            (false, false) => Vec::new(),
        });
        let glow = self.settings(host).glow.factor();
        renderer.set_look(lights, glow, see_through);

        self.sounds(host, eye, &camera.view);
        self.point(host);
        self.windows(host);
    }

    /// A few times a second: tells hcef-host what is heard, from where, and how the place
    /// colours it.
    fn sounds(&mut self, host: &mut dyn Host, eye: [f64; 3], view: &[f32; 16]) {
        let now = self.now();
        if now - self.scene_sent < SCENE_EVERY || !self.wd_host.as_ref().is_some_and(WdHost::is_running) {
            return;
        }
        self.scene_sent = now;
        let settings = *self.settings(host);
        let echo = settings.echo.factor();
        let answers = self.acoustics.as_ref().map(Worker::answers).unwrap_or_default();
        let mut emitters = Vec::new();
        let mut jobs = Vec::new();
        for (pos, seen) in &self.cores {
            for screen in &seen.screens {
                let Some(browser) = self.browsers.id(&page_key(&self.shared, &self.linked, *pos, screen)) else {
                    continue;
                };
                // A number for this screen's two voices, its own whatever browser it shows:
                // two screens showing one shared browser are two places it is heard from.
                let [x, y, z] = pos.map(|part| part as u32);
                let voice = 0x4000_0000 | ((x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663) ^ z.wrapping_mul(83_492_791) ^ u32::from(screen.side.index())) << 1 & 0x3FFF_FFFE);
                // Where its two channels come from: the two ends of a traced sheet, or the
                // two halves of a screen of blocks.
                let ends = match (&seen.sheet, seen.shapes.get(&screen.side)) {
                    (Some(sheet), _) => traced::sound_points(sheet),
                    (None, Some((placement, _))) => sound::screen_points(placement.origin, screen.side, screen.rotation, screen.size, eye),
                    (None, None) => continue,
                };
                // What plays: the screen's speakers when it has some, else the screen itself,
                // the page's left channel from its left half and its right from the right.
                // Each is (id, where, channel, band, level, how fast walls take its treble).
                let mut voices: Vec<(u32, [f64; 3], Source, Band, f32, f32)> = Vec::new();
                if screen.speakers.is_empty() {
                    for (index, (point, source)) in [(ends[0], Source::Left), (ends[1], Source::Right)].into_iter().enumerate() {
                        voices.push((voice | index as u32, point, source, Band::Full, 1.0, 1.0));
                    }
                }
                for speaker in &screen.speakers {
                    let facing = host.block_property(speaker.pos, "facing");
                    let (point, way) = sound::speaker_point(speaker.pos, facing.as_deref());
                    let toward = [0, 1, 2].map(|i| eye[i] - point[i]);
                    // A tweeter beams and is soon lost behind a wall; a bass speaker plays
                    // all round and through.
                    let (band, focus, shadow) = match speaker.kind {
                        Kind::Tweeter => (Band::High, 0.8, 2.0),
                        Kind::Bass => (Band::Low, 0.0, 0.3),
                        Kind::Full => (Band::Full, 0.35, 1.0),
                    };
                    let source = match speaker.channel {
                        Channel::Both => Source::Mono,
                        Channel::Left => Source::Left,
                        Channel::Right => Source::Right,
                    };
                    let [x, y, z] = speaker.pos.map(|part| part as u32);
                    // A number of its own, apart from the screens' (which are small).
                    let id = 0x8000_0000 | (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663) ^ z.wrapping_mul(83_492_791));
                    let level = f32::from(speaker.volume) / 100.0 * sound::beam(way, toward, focus);
                    voices.push((id, point, source, band, level, shadow));
                }
                for (id, point, source, band, level, shadow) in voices {
                    jobs.push(Job { id, source: point });
                    let heard = self.heard.entry(id).or_insert(Heard::DRY);
                    if let Some(answer) = answers.get(&id) {
                        *heard = heard.toward(answer, GLIDE);
                    }
                    let loud = sound::loudness(sound::distance(eye, point), screen.auto_volume);
                    // The echo of a place does not fall off as the straight path does: far
                    // from the screen, more of what is heard is the room.
                    let room = echo * (1.0 / loud.max(0.05)).sqrt().min(3.0);
                    emitters.push(Emitter {
                        id,
                        browser,
                        source,
                        band,
                        gain: screen.volume / MAX_VOLUME * level * loud * heard.gain.powf(shadow),
                        direction: sound::direction(eye, view, point),
                        treble: heard.treble,
                        sends: heard.sends.map(|send| (send * room).clamp(0.0, 1.0)),
                    });
                }
            }
        }
        // The pad plays to both ears as it is: it is in the hand, not in the place.
        if let Some(browser) = self.browsers.id(&PAD_KEY) {
            emitters.push(Emitter { id: 0x2000_0000, browser, source: Source::Stereo, band: Band::Full, gain: 1.0, direction: [0.0; 3], treble: 1.0, sends: [0.0; 4] });
        }
        self.heard.retain(|id, _| emitters.iter().any(|emitter| emitter.id == *id));
        if !jobs.is_empty() {
            let table = self.materials.get_or_insert_with(|| worker::table(host));
            self.acoustics.get_or_insert_with(Worker::new).ask(host, table, eye, jobs);
        }
        if let Some(wd_host) = self.wd_host.as_mut() {
            let master = f32::from(settings.volume) / 100.0;
            wd_host.send(&ToHost::Audio(Scene { master, headphones: settings.headphones, emitters }));
        }
    }

    /// Whether a screen shows a page that can be clicked: an address of its own, or a
    /// shared browser.
    fn has_page(&self, screen: &Screen) -> bool {
        !screen.url.is_empty() || screen.browser.is_some_and(|id| self.shared.contains_key(&id))
    }

    /// The screen at `target`, when this client knows of it.
    fn screen(&self, target: &Target) -> Option<&Screen> {
        self.cores.get(&target.core)?.screens.iter().find(|screen| screen.side == target.side)
    }

    /// How the sheet of the screen at `target` is bent and turned; None for a screen of blocks.
    fn traced(&self, target: &Target) -> Option<Traced> {
        self.cores.get(&target.core)?.traced
    }

    /// Whether `pos` is a block of a screen this client draws: the game's outline of the
    /// block aimed at would be drawn across the picture.
    pub fn is_showing(&self, pos: Pos) -> bool {
        self.cores.values().any(|seen| {
            seen.shapes.iter().any(|(side, (placement, shape))| {
                let cell = shape::cell_of(placement.origin, *side, pos);
                shape::block_at(placement.origin, *side, cell) == pos && cell[0] >= 0 && cell[1] >= 0 && shape.has_block(cell[0] as u32, cell[1] as u32)
            })
        })
    }

    /// Where the crosshair is on a screen, and how far that is from the eye. None off every
    /// screen, on a bezel, and where a screen's shape has no block.
    fn aim(&self, host: &dyn Host) -> Option<(At, f64)> {
        let looked = host.raycast(REACH as f32)?;
        let pos = [looked.block_x, looked.block_y, looked.block_z];
        let point = [looked.x, looked.y, looked.z];
        let eye = host.camera()?.position;
        let away = (0..3).map(|i| (point[i] - eye[i]).powi(2)).sum::<f64>().sqrt();
        if ids::is_poly(&looked.block) {
            // One of a traced screen's invisible blocks: the look goes on to the sheet.
            let master = match looked.block.as_str() {
                ids::POLY_CORE => pos,
                _ => polysave::read_master(&tags::from_compound(&host.mod_block_entity(pos)?.nbt))?,
            };
            let seen = self.cores.get(&master)?;
            let (sheet, screen) = (seen.sheet.as_ref()?, seen.screens.first()?);
            let [u, v] = traced::aim(sheet, eye, point)?;
            let target = Target { core: master, side: screen.side };
            // A shared picture sits inside the sheet with its own shape: the pointer is on
            // it, or on a bar, which is nobody's.
            if let Some(shared) = screen.browser.and_then(|id| self.shared.get(&id)) {
                let [across, down] = traced::fit(sheet, shared.resolution).to_content(sheet.page_uv(u, v))?;
                let pixel = [(across * shared.resolution[0] as f32) as u32, (down * shared.resolution[1] as f32) as u32];
                return Some(((target, [pixel[0].min(shared.resolution[0] - 1), pixel[1].min(shared.resolution[1] - 1)]), away));
            }
            return Some(((target, sheet.to_pixels(u, v, screen.resolution)), away));
        }
        if !ids::is_screen(&looked.block) {
            return None;
        }
        let side = hit::hit_side(pos, point)?;
        self.cores.iter().find_map(|(core, seen)| {
            let (placement, shape) = seen.shapes.get(&side)?;
            let screen = seen.screens.iter().find(|screen| screen.side == side)?;
            // The block must be one of this screen's: in its plane, and in its shape.
            let cell = shape::cell_of(placement.origin, side, pos);
            let on_screen = shape::block_at(placement.origin, side, cell) == pos
                && cell[0] >= 0
                && cell[1] >= 0
                && shape.has_block(cell[0] as u32, cell[1] as u32);
            if !on_screen {
                return None;
            }
            let local = [0, 1, 2].map(|i| (point[i] - f64::from(placement.origin[i])) as f32);
            let face = Face { side, rotation: screen.rotation, size: screen.size };
            let [width, height] = screen.size.map(|blocks| blocks as f32);
            let pixel = hit::hit_to_pixel(&face, screen.resolution, local, |x, y| shape.is_inside(x * width, y * height))?;
            // On a screen that shows a shared browser the pointer is on that browser's
            // picture, counted in its size; on a bar it is nowhere.
            if let Some(shared) = screen.browser.and_then(|id| self.shared.get(&id)) {
                let on_shared = hit::to_shared(&face, screen.resolution, shared.resolution, pixel)?;
                return Some(((Target { core: *core, side }, on_shared), away));
            }
            // On a linked screen the pointer is on the group's picture, whose browser is
            // its origin's.
            if let Some(linked) = self.linked.get(&(*core, side)) {
                let on_group = hit::to_group(&face, screen.resolution, &linked.tile, linked.resolution, pixel);
                return Some(((Target { core: linked.origin.0, side: linked.origin.1 }, on_group), away));
            }
            Some(((Target { core: *core, side }, pixel), away))
        })
    }

    /// What the player holds in their main hand; None for an empty hand.
    fn held(host: &dyn Host) -> Option<String> {
        host.held().filter(|(item, count, _)| *count > 0 && item != "minecraft:air").map(|(item, _, _)| item)
    }

    /// How far the player's pointer reaches on the screen `target` with `held` in hand: a
    /// laser pointer works a screen with the laser upgrade from afar.
    fn reach(&self, target: &Target, held: Option<&str>) -> f64 {
        let laser = held == Some(LASER_POINTER) && self.screen(target).is_some_and(|screen| screen.has_upgrade(UPGRADE_LASER));
        if laser { REACH } else { HAND }
    }

    /// Whether the player is working pages now (`pointer::working`), with `held` in hand.
    fn working(host: &dyn Host, held: Option<&str>) -> bool {
        super::pointer::working(host.sneaking(), held.is_none(), held == Some(LASER_POINTER))
    }

    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// Once a frame: the pointer follows the crosshair.
    fn point(&mut self, host: &mut dyn Host) {
        if !host.playing() {
            // A window opened over the world: whatever was held is let go.
            if let Some(up) = self.pointer.release() {
                let _ = host.net_send_to_server(CHANNEL, &up.encode());
            }
            return;
        }
        // The use key stopped saying it is held, and nothing said it came up: it is up.
        if self.pointer.is_stale(self.now())
            && let Some(up) = self.pointer.release()
        {
            let sent = host.net_send_to_server(CHANNEL, &up.encode());
            crate::trace::say(|| format!("release (the key went quiet): {up:?} -> {sent:?}"));
        }
        let aimed = self.aim(host);
        let held = Self::held(host);
        // The crosshair is a mouse only for a player who means it: a page does not react
        // to someone walking past. Without a button it shows only as far as the hand, or
        // the laser, reaches.
        let working = Self::working(host, held.as_deref());
        let at = aimed.filter(|((target, _), away)| self.pointer.is_down() || (working && *away <= self.reach(target, held.as_deref()))).map(|(at, _)| at);
        let hover = at.is_some_and(|(target, _)| self.screen(&target).is_some_and(|screen| self.has_page(screen)));
        if let Some(moved) = self.pointer.aim(self.now(), at, hover) {
            let _ = host.net_send_to_server(CHANNEL, &moved.encode());
        }
    }

    /// The use key went down while playing. True when a screen took it: the game then does
    /// nothing more with the click.
    pub fn use_key(&mut self, host: &mut dyn Host) -> bool {
        let held = Self::held(host);
        // The minePad opens, whatever is aimed at.
        if held.as_deref() == Some(MINEPAD) {
            self.wanted = Some(Wanted::Pad);
            return true;
        }
        // The pen marks the block corner nearest to where the player aims; sneaking, it
        // wipes the outline. The server keeps the outline and builds the screen.
        if held.as_deref() == Some(PEN) {
            let corner = if host.sneaking() {
                None
            } else {
                match host.raycast(8.0) {
                    Some(looked) => Some(nearest_corner([looked.x, looked.y, looked.z])),
                    None => return false,
                }
            };
            let _ = host.net_send_to_server(CHANNEL, &ToServer::Pen { corner }.encode());
            return true;
        }
        let Some((at, away)) = self.aim(host) else {
            return false;
        };
        let (target, _) = at;
        // The mod's tools act on the screen itself, not on its page: the server's business.
        if matches!(held.as_deref(), Some(LINKER | OWNER_THIEF | CONFIGURATOR | UPGRADE_LASER)) {
            return false;
        }
        if away > self.reach(&target, held.as_deref()) {
            return false;
        }
        let empty_hand = held.is_none();
        let blank = self.screen(&target).is_none_or(|screen| !self.has_page(screen));
        if blank || !Self::working(host, held.as_deref()) {
            // A plain click with an empty hand opens the address window. With something in
            // hand the click is the game's: a block is placed against the screen.
            if empty_hand {
                self.wanted = Some(Wanted::Url(target, None));
            }
            return empty_hand;
        }
        for said in self.pointer.press(self.now(), at) {
            let sent = host.net_send_to_server(CHANNEL, &said.encode());
            crate::trace::say(|| format!("press: {said:?} -> {sent:?}"));
        }
        true
    }

    /// A mouse button came up while playing.
    pub fn button_released(&mut self, host: &mut dyn Host) {
        if let Some(up) = self.pointer.release() {
            let sent = host.net_send_to_server(CHANNEL, &up.encode());
            crate::trace::say(|| format!("release: {up:?} -> {sent:?}"));
        }
    }

    /// The wheel turned by `steps` while playing. True when a page took it: sneaking with
    /// the crosshair on a screen, and an empty hand or the laser pointer, scrolls the page
    /// instead of the hotbar.
    pub fn wheel(&mut self, host: &mut dyn Host, steps: f32) -> bool {
        if !host.sneaking() || !Self::working(host, Self::held(host).as_deref()) {
            return false;
        }
        let Some(((target, pixel), away)) = self.aim(host) else {
            return false;
        };
        if away > HAND {
            return false;
        }
        let scroll = ToServer::Input { target, input: Input::Scroll { pixel, delta: [0, (steps * WHEEL_STEP) as i32] } };
        let _ = host.net_send_to_server(CHANNEL, &scroll.encode());
        true
    }

    /// The server passes on what a player near a screen did to its page: this client's
    /// browser is told the same.
    pub fn from_server(&mut self, data: &[u8]) {
        let (target, input) = match ToClient::decode(data) {
            Ok(ToClient::Input { target, input }) => (target, input),
            Ok(ToClient::OpenKeyboard { target }) => {
                self.wanted = Some(Wanted::Keyboard(target));
                return;
            }
            Ok(ToClient::OpenUrl { target, url }) => {
                self.wanted = Some(Wanted::Url(target, Some(url)));
                return;
            }
            Ok(ToClient::OpenConfig { target, player }) => {
                self.wanted = Some(Wanted::Config(target, player));
                return;
            }
            Ok(ToClient::OpenRedstone { pos, rising, falling }) => {
                self.wanted = Some(Wanted::Redstone(pos, rising, falling));
                return;
            }
            Ok(ToClient::Pen { points }) => {
                self.pen = points;
                return;
            }
            Ok(ToClient::Browser(shared)) => {
                self.shared.insert(shared.id, shared);
                return;
            }
            Ok(ToClient::BrowserGone { id }) => {
                self.shared.remove(&id);
                return;
            }
            Ok(ToClient::Screens { page, pages, entries }) => {
                if let Some(Window::Screens(window)) = self.ui.open.as_mut() {
                    window.received(usize::from(page), usize::from(pages), entries);
                }
                return;
            }
            Err(_) => return,
        };
        let Some(key) = self.screen(&target).map(|screen| page_key(&self.shared, &self.linked, target.core, screen)) else {
            return;
        };
        let (Some(browser), Some(screen)) = (self.browsers.id(&key), self.screen(&target)) else {
            crate::trace::say(|| format!("input for a screen this client has no browser for: {target:?}"));
            return;
        };
        if !matches!(input, Input::Pointer { what: What::Move, .. }) {
            crate::trace::say(|| format!("input for browser {browser}: {input:?}"));
        }
        // The pixel travels in the screen's saved resolution; this browser may render less.
        // A group's picture has the group's size, not its origin screen's.
        // A shared browser's has its own, and is never turned: the pixel was counted on it.
        let its_shared = screen.browser.and_then(|id| self.shared.get(&id));
        let (saved, effective, rotation) = match (its_shared, self.linked.get(&(target.core, target.side))) {
            (Some(shared), _) => (shared.resolution, resolution::effective(shared.resolution, 100, MAX_RESOLUTION), Rotation::R0),
            (None, Some(linked)) => (linked.resolution, linked.effective, screen.rotation),
            (None, None) => (screen.resolution, resolution::effective(screen.resolution, 100, MAX_RESOLUTION), screen.rotation),
        };
        let on_page = |pixel: [u32; 2]| hit::to_effective(rotation, saved, effective, pixel).map(|part| part as i32);
        let held = self.held.entry(key).or_insert(0);
        let commands = to_host(browser, held, &on_page, input);
        if let Some(wd_host) = self.wd_host.as_mut() {
            for command in &commands {
                wd_host.send(command);
            }
        }
    }

    /// Once a frame: opens the window that was asked for, and does what the open one asked.
    fn windows(&mut self, host: &mut dyn Host) {
        // Escape, or another screen, closed it.
        if self.ui.open.is_some() && host.mod_screen() != Some(ID) {
            self.ui.open = None;
            self.ui.actions.clear();
        }
        if let Some(wanted) = self.wanted.take()
            && self.ui.open.is_none()
        {
            let window = match wanted {
                Wanted::Url(target, known) => {
                    // The address in the window is the one of what the screen shows.
                    let shown = |screen: &Screen| screen.browser.and_then(|id| self.shared.get(&id)).map_or(screen.url.clone(), |shared| shared.url.clone());
                    let address = known.or_else(|| self.screen(&target).map(shown)).unwrap_or_default();
                    Some(Window::Url(UrlWindow::new(target, &address)))
                }
                Wanted::Keyboard(target) => Some(Window::Keyboard(KeyboardWindow::new(target))),
                Wanted::Redstone(pos, rising, falling) => Some(Window::Redstone(RedstoneWindow::new(pos, &rising, &falling))),
                Wanted::Pad => Some(Window::Pad(PadWindow::new(self.pad_url.as_deref().filter(|address| !address.starts_with("data:")).unwrap_or_default()))),
                // The settings are those of a screen this client sees.
                Wanted::Config(target, me) => self.screen(&target).map(|screen| {
                    let mut window = ConfigWindow::new(target, me, screen.clone());
                    window.traced = self.traced(&target);
                    Window::Config(Box::new(window))
                }),
            };
            if window.is_some() && host.mod_screen_open(ID) {
                self.ui.open = window;
                // The keys held now were pressed for the game, not for this window.
                self.ui.keys = Keys::default();
            }
        }
        // The pad's address bar follows its page.
        if let Some(address) = self.browsers.address(&PAD_KEY).map(str::to_owned)
            && let Some(Window::Pad(window)) = self.ui.open.as_mut()
        {
            window.went_to(&address);
        }
        // The shared browsers' window follows the server's list and the screen's choice.
        if let Some(Window::Browsers(window)) = self.ui.open.as_ref() {
            let target = window.target;
            let mut list: Vec<Shared> = self.shared.values().cloned().collect();
            list.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
            let current = self.screen(&target).and_then(|screen| screen.browser);
            if let Some(Window::Browsers(window)) = self.ui.open.as_mut() {
                window.sync(list, current);
            }
        }
        // The settings window shows the screen as it is now; it closes with the screen.
        if let Some(Window::Config(config)) = self.ui.open.as_ref() {
            let target = config.target;
            let traced = self.traced(&target);
            match self.screen(&target).cloned() {
                Some(screen) => {
                    if let Some(Window::Config(config)) = self.ui.open.as_mut() {
                        config.sync(&screen);
                        config.traced = traced;
                    }
                }
                None => self.ui.actions.push(Action::Close),
            }
        }
        // What was typed on a keyboard since the last frame leaves now.
        if let Some(Window::Keyboard(keyboard)) = self.ui.open.as_mut()
            && let Some(typed) = keyboard.take()
        {
            let _ = host.net_send_to_server(CHANNEL, &typed.encode());
        }
        for action in std::mem::take(&mut self.ui.actions) {
            match action {
                Action::Close => {
                    host.mod_screen_close(ID);
                    self.ui.open = None;
                }
                Action::Screens(target) => {
                    let (window, ask) = ScreensWindow::open(target);
                    self.ui.open = Some(Window::Screens(window));
                    self.ui.actions.push(ask);
                }
                // The pad's page is the player's own: what is done to it goes to its browser.
                Action::Pad(input) => {
                    if let Some(browser) = self.browsers.id(&PAD_KEY) {
                        let held = self.held.entry(PAD_KEY).or_insert(0);
                        let commands = to_host(browser, held, &|pixel: [u32; 2]| pixel.map(|part| part as i32), input);
                        if let Some(wd_host) = self.wd_host.as_mut() {
                            for command in &commands {
                                wd_host.send(command);
                            }
                        }
                    }
                }
                Action::PadNavigate(address) => {
                    host.set_setting(ID, PAD_SETTING, &address);
                    self.pad_url = Some(address);
                }
                // One window in place of another: the game's screen stays open under them.
                Action::Browsers(target) => {
                    self.ui.open = Some(Window::Browsers(BrowsersWindow::new(target, Vec::new(), None)));
                }
                Action::Send(message) => {
                    let _ = host.net_send_to_server(CHANNEL, &message.encode());
                }
                Action::Copy(text) => {
                    if !text.is_empty()
                        && let Ok(mut clipboard) = arboard::Clipboard::new()
                    {
                        let _ = clipboard.set_text(text);
                    }
                }
                Action::Paste => {
                    let pasted = arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text());
                    if let (Ok(text), Some(window)) = (pasted, self.ui.open.as_mut()) {
                        // What the paste asks for in turn is done on the next frame.
                        let asked = window.paste(&text);
                        self.ui.actions.extend(asked);
                    }
                }
            }
        }
    }

    /// Brings hcef-host in line with `wants` and takes in what it reports.
    fn sync(&mut self, host: &mut dyn Host, renderer: &Renderer, wants: &[Want<Key>]) {
        let wd_host = self.wd_host.get_or_insert_with(|| WdHost::new(hcef::host_exe(host)));
        let was_running = wd_host.is_running();
        let reports = wd_host.poll(!wants.is_empty());
        if was_running && !wd_host.is_running() {
            // It died: its browsers went with it, and are asked for again once it is back.
            for page in self.browsers.host_lost() {
                renderer.detach(page);
            }
        }
        for report in &reports {
            match self.browsers.report(report) {
                Some(Change::Frames { id, mapping, capacity }) => {
                    let _ = renderer.attach(id, &mapping, capacity);
                }
                Some(Change::Gone { id }) => renderer.detach(id),
                // The page followed a link by itself: nothing to do until screens share
                // their address with the other players (the interaction phase).
                Some(Change::Address { .. }) | None => {}
            }
        }
        if wd_host.is_running() {
            for command in self.browsers.sync(wants) {
                wd_host.send(&command);
            }
        }
        // Said once in the player's chat when it changes: a screen that stays on its test
        // card is otherwise a riddle.
        let status = wd_host.status();
        if self.said.as_ref() != Some(&status) {
            if let Status::Failed(reason) = &status {
                eprintln!("[webdisplays] hcef-host: {}", reason.english());
                let line = Phrase::new("webdisplays.h.host.failed", "WebDisplays: %s").with(reason.clone());
                host.client_action_bar(lang::text(&line), false);
            }
            self.said = Some(status);
        }
    }

    /// The player left the world: nothing is shown any more and hcef-host is let go.
    fn leave(&mut self, renderer: &Renderer) {
        if self.cores.is_empty() && self.wd_host.is_none() {
            return;
        }
        self.cores.clear();
        self.held.clear();
        self.pointer = Pointer::default();
        // Another world has other block states, and its own places.
        self.acoustics = None;
        self.materials = None;
        self.linked.clear();
        self.pen.clear();
        self.shared.clear();
        self.heard.clear();
        for page in self.browsers.host_lost() {
            renderer.detach(page);
        }
        renderer.set_quads(Vec::new());
        renderer.set_look(Vec::new(), 0.0, HashSet::new());
        // Dropping the link tells hcef-host to quit.
        self.wd_host = None;
        self.said = None;
    }
}

/// Where a browser is sent for the address a screen was given: the site's own player
/// alone in the page for a video or a live this mod knows (YouTube, Twitch), the address
/// itself otherwise. The screen keeps what the player typed.
fn sent_to(address: &str) -> String {
    url::player(address).unwrap_or_else(|| address.to_owned())
}
