//! WebDisplays for Horizon: the code half of the mod (the blocks, items and assets are the
//! content mod's, `../content`). The launcher links this crate into the game and calls
//! `hooks()` (docs/native-mods.md, "Trait mods").
//!
//! The browser itself (CEF) is another mod's, HCEF, which this one depends on: it runs it
//! in a process of its own (hcef-host), keeps the link to it and downloads it. This crate
//! says which browsers it wants, draws the frames they share and sends them input.
//!
//! One instance of the mod serves both the window's game and the server thread's, so the
//! two halves keep apart: `server` is what is true of the world, `client` what one player
//! sees of it. This file only routes hooks to them; nothing here decides anything.
pub mod acoustics;
pub mod client;
pub mod ids;
pub mod lang;
pub mod model;
pub mod net;
pub mod render;
pub mod server;
pub mod tags;
pub mod trace;

// The browser's side, by the names it had when it lived here.
pub use hcef::{ipc, proto};

use horizon_client::mod_api::{
    BlockEntityEvent, BlockEntitySubmitEvent, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent,
    BlockHighlightEvent,
    Compound, DestroyBlockEvent, Flow, FrameEvent,
    Hooks, Host, InteractionKeyEvent, KeyEvent, ModsEvent, MouseButtonPostEvent, NativeCommand, NativeMod, NetFromClientEvent,
    NetFromServerEvent, PerformCommandEvent, PlaceBlockEvent, PlayerLoggedInEvent, PlayerLoggedOutEvent,
    RegisterCommandsEvent, RenderHooks, RightClickBlockEvent, ServerStoppingEvent, ServerTickEvent,
    ScreenCharTypedPreEvent, ScreenKeyPressedPreEvent, ScreenKeyReleasedPreEvent, ScreenMouseButtonPressedPreEvent,
    RenderHandEvent, ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent,
    ScreenRenderPostEvent, SettingsClickEvent,
    SettingsEvent, WheelEvent,
};
use ids::{CORE, ID};
use model::phrase::Phrase;
use net::msg::{CHANNEL, Reshape};
use render::gpu::Renderer;
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::Duration;

#[derive(Default)]
struct WebDisplays {
    renderer: Renderer,
    client: Mutex<client::world::Client>,
    server: Mutex<server::State>,
}

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(WebDisplays::default())
}

impl WebDisplays {
    /// The client's state, from a hook that must not wait long for it. The frame hook
    /// holds it for most of its run, on a thread that may not be the hook's; waiting for
    /// ever could hang the game if a hook were ever called from inside the frame's own
    /// calls, and not waiting at all loses a click now and then. So: a short wait.
    fn client_soon(&self) -> Option<MutexGuard<'_, client::world::Client>> {
        for _ in 0..60 {
            match self.client.try_lock() {
                Ok(client) => return Some(client),
                Err(TryLockError::Poisoned(_)) => return None,
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_micros(50)),
            }
        }
        None
    }

    /// Runs `change` on the mod's open window, when this game's window shows one. The
    /// screen hooks fire for every screen of the game: only ours is answered.
    fn on_window(&self, host: &dyn Host, change: impl FnOnce(&mut client::gui::Window) -> Vec<client::gui::Action>) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        let taken = self.client_soon().is_some_and(|mut client| client.ui.on(change));
        if taken { Flow::Cancel } else { Flow::Continue }
    }

    /// The same for the keyboard, which the windows' keeper sorts first (`gui::Keys`).
    fn on_ui(&self, host: &dyn Host, change: impl FnOnce(&mut client::gui::Ui) -> bool) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        let taken = self.client_soon().is_some_and(|mut client| change(&mut client.ui));
        if taken { Flow::Cancel } else { Flow::Continue }
    }
}

impl Hooks for WebDisplays {
    fn mods(&self, _host: &mut dyn Host, event: &mut ModsEvent<'_>) {
        event.mods.push(NativeMod {
            id: ID.to_owned(),
            title: concat!("WebDisplays ", env!("CARGO_PKG_VERSION")).to_owned(),
            description: lang::plain(&Phrase::new("webdisplays.h.description", "Web browsers on screens in the world.")),
            keys: Vec::new(),
        });
    }

    /// The mod's settings page: one button a setting, showing its value.
    fn settings(&self, host: &mut dyn Host, event: &mut SettingsEvent<'_>) {
        if event.mod_id == ID
            && let Ok(mut client) = self.client.lock()
        {
            event.buttons.extend(client.settings(host).buttons().iter().map(lang::plain));
        }
    }

    /// A click on a setting's button steps it to its next value, and keeps it.
    fn settings_click(&self, host: &mut dyn Host, event: &mut SettingsClickEvent<'_>) {
        if event.mod_id != ID {
            return;
        }
        let kept = self.client.lock().ok().and_then(|mut client| client.settings(host).click(event.button));
        if let Some((key, value)) = kept {
            host.set_setting(ID, key, &value);
        }
    }

    /// Once a frame, for the window only: everything that needs a screen or a browser
    /// starts here, never on the server thread.
    fn frame(&self, host: &mut dyn Host, _event: &mut FrameEvent) {
        if !host.window() {
            return;
        }
        if let Ok(mut client) = self.client.lock() {
            client.frame(host, &self.renderer);
        }
    }

    fn render(&self) -> Option<&dyn RenderHooks> {
        Some(&self.renderer)
    }

    // ---- blocks

    /// The core of a screen has a block entity: it holds the screens' data. So has a
    /// peripheral: it holds its link to a screen.
    fn block_entity(&self, _host: &mut dyn Host, event: &mut BlockEntityEvent<'_>) -> Option<(String, Compound)> {
        (event.block == CORE || ids::has_link(event.block) || ids::is_poly(event.block)).then(|| (event.block.to_owned(), Compound::new()))
    }

    /// The redstone controller looks at its signal every tick.
    fn block_entity_ticks(&self, _host: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        // A core ticks to be put on the world's list of screens when its chunk is loaded.
        if event.block == ids::REDSTONE || event.block == CORE || event.block == ids::POLY_CORE {
            event.ticks = true;
        }
    }

    fn block_entity_tick(&self, host: &mut dyn Host, event: &mut BlockEntityTickEvent<'_>) {
        if event.entity.id == ids::REDSTONE {
            server::redstone::tick(host, event.pos, event.entity);
        } else if event.entity.id == CORE || event.entity.id == ids::POLY_CORE {
            let traced = event.entity.id == ids::POLY_CORE;
            server::index::met(host, event.pos, || tags::screens(&event.entity.nbt), traced);
        }
    }

    /// Clients are sent all of it: what a screen shows, and who may change it.
    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        (event.entity.id == CORE || ids::is_poly(&event.entity.id)).then(|| event.entity.nbt.clone())
    }

    /// The game is about to draw a core: the client now knows of its screens.
    fn block_entity_submit(&self, _host: &dyn Host, event: &mut BlockEntitySubmitEvent<'_>) {
        if (event.block == CORE || event.block == ids::POLY_CORE)
            && let Ok(mut client) = self.client.lock()
        {
            client.seen(event.pos);
        }
    }

    fn right_click_block(&self, host: &mut dyn Host, event: &mut RightClickBlockEvent) -> Flow {
        match self.server.lock() {
            Ok(mut state) => server::click::right_click(host, &mut state, event.pos, event.face),
            Err(_) => Flow::Continue,
        }
    }

    fn destroy_block(&self, host: &mut dyn Host, event: &mut DestroyBlockEvent) -> Flow {
        let block = host.block(event.pos);
        if ids::is_screen(&block) {
            server::screens::breaking(host, event.pos);
        } else if ids::is_peripheral(&block) {
            server::keyboard::breaking(host, event.pos, &block);
        } else if ids::is_poly(&block) {
            server::poly::breaking(host, event.pos);
        }
        Flow::Continue
    }

    /// The keyboard's item places one half, the other is placed here; a screen block placed
    /// against a screen makes it larger.
    fn place_block(&self, host: &mut dyn Host, event: &mut PlaceBlockEvent<'_>) {
        server::keyboard::placed(host, event.pos);
        server::screens::placed(host, event.pos);
    }

    // ---- what the player does to a screen

    /// The use key on a screen opens its address window; sneaking with an empty hand, or
    /// with the laser pointer, it clicks its page.
    fn interaction_key(&self, host: &mut dyn Host, event: &mut InteractionKeyEvent<'_>) -> Flow {
        if event.button != 1 || !host.window() {
            return Flow::Continue;
        }
        if !self.client_soon().is_some_and(|mut client| client.use_key(host)) {
            return Flow::Continue;
        }
        // The hand does not swing at a page.
        event.swing_hand = false;
        Flow::Cancel
    }

    /// The outline of the block aimed at is not drawn across a screen's picture.
    fn block_highlight(&self, host: &dyn Host, event: &mut BlockHighlightEvent) -> Flow {
        // A traced screen's blocks are invisible: their outline would give them away.
        let on_screen = ids::is_poly(&host.block(event.pos)) || self.client_soon().is_some_and(|client| client.is_showing(event.pos));
        if on_screen { Flow::Cancel } else { Flow::Continue }
    }

    /// A mouse button coming up ends a press on a page.
    fn key(&self, host: &mut dyn Host, event: &mut KeyEvent<'_>) {
        if !event.pressed
            && event.key.starts_with("key.mouse.")
            && host.window()
            && let Some(mut client) = self.client_soon()
        {
            client.button_released(host);
        }
    }

    /// The same from the raw mouse, which says a release the game's key names sometimes
    /// do not.
    fn mouse_button_post(&self, host: &mut dyn Host, event: &mut MouseButtonPostEvent) {
        if !event.pressed
            && host.window()
            && let Some(mut client) = self.client_soon()
        {
            client.button_released(host);
        }
    }

    fn wheel(&self, host: &mut dyn Host, event: &mut WheelEvent) -> Flow {
        let taken = host.window() && self.client_soon().is_some_and(|mut client| client.wheel(host, event.steps));
        if taken { Flow::Cancel } else { Flow::Continue }
    }

    /// A player who joins is sent the world's shared browsers.
    fn player_logged_in(&self, host: &mut dyn Host, event: &mut PlayerLoggedInEvent<'_>) {
        if let Ok(mut state) = self.server.lock() {
            server::browsers::greet(host, &mut state, event.player);
        }
    }

    fn player_logged_out(&self, _host: &mut dyn Host, event: &mut PlayerLoggedOutEvent<'_>) {
        if let Ok(mut state) = self.server.lock() {
            server::browsers::left(&mut state, event.player);
        }
    }

    /// The server stops: what it remembered was this world's.
    fn server_stopping(&self, _host: &mut dyn Host, _event: &mut ServerStoppingEvent) {
        server::index::forget();
        if let Ok(mut state) = self.server.lock() {
            *state = server::State::default();
        }
    }

    /// What waits to be sent leaves a little each tick.
    fn server_tick(&self, host: &mut dyn Host, _event: &mut ServerTickEvent<'_>) {
        if let Ok(mut state) = self.server.lock() {
            server::browsers::tick(host, &mut state);
        }
    }

    fn net_from_client(&self, host: &mut dyn Host, event: &mut NetFromClientEvent<'_>) {
        if event.channel == CHANNEL
            && let Ok(mut state) = self.server.lock()
        {
            server::interact::from_client(host, &mut state, event.player, event.data);
        }
    }

    fn net_from_server(&self, _host: &mut dyn Host, event: &mut NetFromServerEvent<'_>) {
        if event.channel == CHANNEL
            && let Ok(mut client) = self.client.lock()
        {
            client.from_server(event.data);
        }
    }

    // ---- the mod's windows

    fn screen_render_post(&self, host: &dyn Host, event: &mut ScreenRenderPostEvent<'_>) {
        if host.mod_screen() == Some(ID)
            && let Some(mut client) = self.client_soon()
        {
            // Where the mouse is, said once a frame: a page under it shows its hover.
            let (at, area) = (event.screen.mouse, event.screen.rect);
            client.ui.on(|window| window.moved(at, area));
            if let Some(window) = client.ui.open.as_mut() {
                client::paint::window(host, event.batch, window, area, at);
            }
        }
    }

    fn screen_mouse_scrolled_pre(&self, host: &dyn Host, event: &mut ScreenMouseScrolledPreEvent<'_>) -> Flow {
        let (at, area, steps) = (event.screen.mouse, event.screen.rect, event.scroll[1]);
        self.on_window(host, |window| window.scroll(at, area, steps))
    }

    /// The minePad is drawn by the mod, over the picture: the game's own drawing of the
    /// item in the hand would be a second pad.
    fn render_hand(&self, host: &dyn Host, _event: &mut RenderHandEvent<'_>) -> Flow {
        let pad = host.held().is_some_and(|(item, count, _)| count > 0 && item == ids::MINEPAD);
        if pad { Flow::Cancel } else { Flow::Continue }
    }

    fn screen_mouse_button_pressed_pre(&self, host: &dyn Host, event: &mut ScreenMouseButtonPressedPreEvent<'_>) -> Flow {
        let (at, area, left) = (event.screen.mouse, event.screen.rect, event.button == 0);
        let measure = |text: &str| host.text_width(text);
        self.on_window(host, |window| if left { window.click(at, area, &measure) } else { Vec::new() })
    }

    fn screen_mouse_dragged_pre(&self, host: &dyn Host, event: &mut ScreenMouseDraggedPreEvent<'_>) -> Flow {
        let (at, area) = (event.screen.mouse, event.screen.rect);
        self.on_window(host, |window| window.drag(at, area))
    }

    fn screen_mouse_button_released_pre(&self, host: &dyn Host, _event: &mut ScreenMouseButtonReleasedPreEvent<'_>) -> Flow {
        self.on_window(host, |window| window.release())
    }

    fn screen_key_pressed_pre(&self, host: &dyn Host, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
        // Escape stays the game's: it closes the window.
        if event.key.ends_with("escape") {
            return Flow::Continue;
        }
        let (key, repeat, held) = (event.key, event.repeat, event.modifiers);
        self.on_ui(host, |ui| ui.key(key, repeat, held))
    }

    fn screen_key_released_pre(&self, host: &dyn Host, event: &mut ScreenKeyReleasedPreEvent<'_>) -> Flow {
        let (key, held) = (event.key, event.modifiers);
        self.on_ui(host, |ui| ui.released(key, held))
    }

    fn screen_char_typed_pre(&self, host: &dyn Host, event: &mut ScreenCharTypedPreEvent<'_>) -> Flow {
        let (c, held) = (event.c, event.modifiers);
        self.on_ui(host, |ui| ui.typed(c, held))
    }

    // ---- commands

    fn register_commands(&self, _host: &mut dyn Host, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.push(NativeCommand::new("webdisplays url <address:text>"));
        event.commands.push(NativeCommand::new("webdisplays curve"));
        event.commands.push(NativeCommand::new("webdisplays flip"));
    }

    fn perform_command(&self, host: &mut dyn Host, event: &mut PerformCommandEvent<'_>) -> Flow {
        let command = event.command.trim_start_matches('/');
        let Some(rest) = command.strip_prefix("webdisplays ") else {
            return Flow::Continue;
        };
        let player = event.player.uuid;
        let answer = match rest.trim().split_once(' ').unwrap_or((rest.trim(), "")) {
            ("url", address) if !address.is_empty() => server::screens::set_url(host, player, address),
            // A traced screen's corners rounded or sharp, and which way it faces.
            ("curve", "") => server::poly::reshape(host, player, |outline| {
                let rounded = !outline.curved;
                server::poly::apply(outline, Reshape::Curved(rounded))
            }),
            ("flip", "") => server::poly::reshape(host, player, |outline| {
                let flipped = !outline.flipped;
                server::poly::apply(outline, Reshape::Flipped(flipped))
            }),
            _ => Err(Phrase::new("webdisplays.h.command.usage", "Usage: /webdisplays url <address>, /webdisplays curve, /webdisplays flip")),
        };
        // Done or refused, the answer is a line of chat, read by the client in its language.
        host.tell(event.player.uuid, lang::text(&answer.unwrap_or_else(|refused| refused)));
        Flow::Cancel
    }
}
