//! The player's own settings: how loud the mod is, how much of a place's echo is played,
//! how strongly screens light their surroundings, loudspeakers or headphones.
//!
//! Horizon gives a mod a settings page made of buttons, and keeps a value per key for it.
//! Each setting is one button that shows its value and steps to the next when clicked.
//! Plain data: the game's part (reading and keeping the values) is handed in, and what a
//! button says is a `Phrase` the game's side reads in the player's language.
use crate::model::phrase::Phrase;

/// How much of something: the steps a button goes through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Off,
    Soft,
    Normal,
    Strong,
}

impl Level {
    const ALL: [Level; 4] = [Level::Off, Level::Soft, Level::Normal, Level::Strong];

    fn name(self) -> &'static str {
        match self {
            Level::Off => "off",
            Level::Soft => "discreet",
            Level::Normal => "normal",
            Level::Strong => "strong",
        }
    }

    /// The level as its button shows it. `name` is what is kept, in every language.
    fn label(self) -> Phrase {
        match self {
            Level::Off => Phrase::new("webdisplays.h.settings.level.off", "off"),
            Level::Soft => Phrase::new("webdisplays.h.settings.level.soft", "discreet"),
            Level::Normal => Phrase::new("webdisplays.h.settings.level.normal", "normal"),
            Level::Strong => Phrase::new("webdisplays.h.settings.level.strong", "strong"),
        }
    }

    fn parse(text: &str) -> Option<Level> {
        Level::ALL.into_iter().find(|level| level.name() == text)
    }

    fn next(self) -> Level {
        Level::ALL[(Level::ALL.iter().position(|level| *level == self).unwrap_or(0) + 1) % Level::ALL.len()]
    }

    /// What the level multiplies: nothing, a little, as worked out, more.
    pub fn factor(self) -> f32 {
        match self {
            Level::Off => 0.0,
            Level::Soft => 0.6,
            Level::Normal => 1.0,
            Level::Strong => 1.6,
        }
    }
}

/// The volumes the volume button goes through, in percent.
const VOLUMES: [u8; 5] = [100, 75, 50, 25, 0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The mod's own volume, in percent: the game's volume sliders do not reach a mod.
    pub volume: u8,
    /// How much of a place's echo is played.
    pub echo: Level,
    /// How strongly a screen lights what is around it.
    pub glow: Level,
    /// Sound is placed for two ears on a head instead of two loudspeakers.
    pub headphones: bool,
}

impl Default for Settings {
    /// Loudspeakers, a discreet echo, screens that light their room.
    fn default() -> Self {
        Self { volume: 100, echo: Level::Soft, glow: Level::Normal, headphones: false }
    }
}

/// The keys the values are kept under, in the buttons' order.
pub const KEYS: [&str; 4] = ["volume", "echo", "glow", "headphones"];

impl Settings {
    /// The settings as kept: `get` gives the value kept under a key. What is missing or
    /// makes no sense is at its default.
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Settings {
        let default = Settings::default();
        Settings {
            volume: get(KEYS[0]).and_then(|value| value.parse::<u8>().ok()).filter(|volume| *volume <= 100).unwrap_or(default.volume),
            echo: get(KEYS[1]).and_then(|value| Level::parse(&value)).unwrap_or(default.echo),
            glow: get(KEYS[2]).and_then(|value| Level::parse(&value)).unwrap_or(default.glow),
            headphones: get(KEYS[3]).map_or(default.headphones, |value| value == "true"),
        }
    }

    /// The value kept under the key of button `index`.
    fn value(&self, index: usize) -> String {
        match index {
            0 => self.volume.to_string(),
            1 => self.echo.name().to_owned(),
            2 => self.glow.name().to_owned(),
            _ => self.headphones.to_string(),
        }
    }

    /// The buttons of the settings page, top to bottom.
    pub fn buttons(&self) -> Vec<Phrase> {
        let ears = if self.headphones {
            Phrase::new("webdisplays.h.settings.headphones", "headphones")
        } else {
            Phrase::new("webdisplays.h.settings.loudspeakers", "loudspeakers")
        };
        vec![
            Phrase::new("webdisplays.h.settings.volume", "Volume of screens: %s%%").with(self.volume),
            Phrase::new("webdisplays.h.settings.echo", "Echo of the place: %s").with(self.echo.label()),
            Phrase::new("webdisplays.h.settings.glow", "Light thrown by screens: %s").with(self.glow.label()),
            Phrase::new("webdisplays.h.settings.sound", "Sound for: %s").with(ears),
        ]
    }

    /// Button `index` was clicked: its setting steps to its next value. Gives the key and
    /// the value to keep; None for a button that is not one of these.
    pub fn click(&mut self, index: usize) -> Option<(&'static str, String)> {
        match index {
            0 => {
                let at = VOLUMES.iter().position(|volume| *volume == self.volume);
                self.volume = VOLUMES[at.map_or(0, |at| (at + 1) % VOLUMES.len())];
            }
            1 => self.echo = self.echo.next(),
            2 => self.glow = self.glow.next(),
            3 => self.headphones = !self.headphones,
            _ => return None,
        }
        Some((KEYS[index], self.value(index)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn what_was_never_set_is_at_its_default() {
        let settings = Settings::read(|_| None);
        assert_eq!(settings, Settings::default());
        assert_eq!((settings.volume, settings.echo, settings.glow, settings.headphones), (100, Level::Soft, Level::Normal, false));
    }

    #[test]
    fn every_click_is_kept_and_read_back() {
        let mut kept: HashMap<&str, String> = HashMap::new();
        let mut settings = Settings::default();
        for index in [0, 0, 1, 2, 2, 3] {
            let (key, value) = settings.click(index).unwrap();
            kept.insert(key, value);
        }
        assert_eq!((settings.volume, settings.echo, settings.glow, settings.headphones), (50, Level::Normal, Level::Off, true));
        assert_eq!(Settings::read(|key| kept.get(key).cloned()), settings);
        assert_eq!(settings.click(4), None, "no fifth button");
    }

    #[test]
    fn each_button_comes_back_round_to_where_it_started() {
        let mut settings = Settings::default();
        for (index, clicks) in [(0, 5), (1, 4), (2, 4), (3, 2)] {
            for _ in 0..clicks {
                settings.click(index);
            }
        }
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn a_kept_value_that_makes_no_sense_is_ignored() {
        let odd = |key: &str| Some(match key {
            "volume" => "250",
            "echo" => "LOUD",
            "glow" => "",
            _ => "yes",
        }.to_owned());
        let settings = Settings::read(odd);
        assert_eq!((settings.volume, settings.echo, settings.glow, settings.headphones), (100, Level::Soft, Level::Normal, false));
    }

    #[test]
    fn the_buttons_say_what_they_are_set_to() {
        let buttons: Vec<String> = Settings::default().buttons().iter().map(Phrase::english).collect();
        assert_eq!(buttons, ["Volume of screens: 100%", "Echo of the place: discreet", "Light thrown by screens: normal", "Sound for: loudspeakers"]);
        let mut settings = Settings::default();
        settings.click(0);
        settings.click(3);
        let buttons: Vec<String> = settings.buttons().iter().map(Phrase::english).collect();
        assert_eq!((buttons[0].as_str(), buttons[3].as_str()), ("Volume of screens: 75%", "Sound for: headphones"));
    }

    #[test]
    fn a_value_is_an_argument_of_its_button_and_is_kept_in_english() {
        let mut settings = Settings::default();
        let glow = &settings.buttons()[2];
        assert_eq!((glow.key, glow.args.len()), ("webdisplays.h.settings.glow", 1), "the level is translated on its own");
        assert_eq!(settings.click(2), Some(("glow", "strong".to_owned())), "what is kept does not follow the language");
    }
}
