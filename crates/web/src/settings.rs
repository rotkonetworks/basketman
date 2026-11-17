//! What the player can set: how the game looks and how it saves.
//!
//! The match view and the match HUD (level, opacity) have their own keys,
//! `basketman.view` and `basketman.hud`, written from the live state in
//! `App::save`; the settings screen edits that state, so there is one copy
//! of each. Everything else is here, under `basketman.settings`.

use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) const KEY: &str = "basketman.settings";

static REDUCED: AtomicBool = AtomicBool::new(false);

/// The player asked for no fades and nothing that moves by itself.
pub(crate) fn reduced() -> bool {
    REDUCED.load(Ordering::Relaxed)
}

pub(crate) fn set_reduced(on: bool) {
    REDUCED.store(on, Ordering::Relaxed);
}

/// When the career is saved by itself.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Autosave {
    Off,
    /// After every match day.
    Round,
    /// After every day of the calendar.
    Day,
}

impl Autosave {
    pub const ALL: [Autosave; 3] = [Autosave::Day, Autosave::Round, Autosave::Off];

    pub fn label(self) -> &'static str {
        match self {
            Autosave::Off => "Off",
            Autosave::Round => "Every match day",
            Autosave::Day => "Every day",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Autosave::Off => "off",
            Autosave::Round => "round",
            Autosave::Day => "day",
        }
    }

    fn from_key(k: &str) -> Option<Autosave> {
        Autosave::ALL.into_iter().find(|a| a.key() == k)
    }
}

pub(crate) const SPEEDS: [f32; 4] = [1.0, 2.0, 4.0, 8.0];
/// Interface scale steps, in percent.
pub(crate) const SCALES: [u32; 6] = [80, 90, 100, 115, 130, 150];

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    /// Speed a match starts at: one of `SPEEDS`.
    pub speed: f32,
    /// Interface scale in percent: one of `SCALES`.
    pub scale: u32,
    /// No fades and no moving backdrops.
    pub reduced_motion: bool,
    pub autosave: Autosave,
    /// Ask before leaving a match that is being coached.
    pub confirm_leave: bool,
    /// Start in full screen (desktop).
    pub fullscreen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            speed: 4.0,
            scale: 100,
            reduced_motion: false,
            autosave: Autosave::Day,
            confirm_leave: true,
            fullscreen: false,
        }
    }
}

impl Settings {
    pub fn encode(&self) -> String {
        let b = |v: bool| u8::from(v);
        format!(
            "speed={};scale={};reduced={};autosave={};confirm={};fullscreen={}",
            self.speed,
            self.scale,
            b(self.reduced_motion),
            self.autosave.key(),
            b(self.confirm_leave),
            b(self.fullscreen),
        )
    }

    /// Unknown or missing keys keep their defaults; values are put back in range.
    pub fn decode(text: &str) -> Settings {
        let mut s = Settings::default();
        for part in text.split(';') {
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            let flag = v == "1";
            match k {
                "speed" => {
                    if let Some(x) = v.parse::<f32>().ok().and_then(nearest_speed) {
                        s.speed = x;
                    }
                }
                "scale" => {
                    if let Some(x) = v.parse::<u32>().ok().map(nearest_scale) {
                        s.scale = x;
                    }
                }
                "reduced" => s.reduced_motion = flag,
                "autosave" => {
                    if let Some(a) = Autosave::from_key(v) {
                        s.autosave = a;
                    }
                }
                "confirm" => s.confirm_leave = flag,
                "fullscreen" => s.fullscreen = flag,
                _ => {}
            }
        }
        s
    }

    pub fn zoom(&self) -> f32 {
        self.scale as f32 / 100.0
    }
}

fn nearest_speed(v: f32) -> Option<f32> {
    SPEEDS.into_iter().min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
}

fn nearest_scale(v: u32) -> u32 {
    SCALES.into_iter().min_by_key(|s| s.abs_diff(v)).unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let s = Settings {
            speed: 8.0,
            scale: 130,
            reduced_motion: true,
            autosave: Autosave::Round,
            confirm_leave: false,
            fullscreen: true,
        };
        assert_eq!(Settings::decode(&s.encode()), s);
        assert_eq!(Settings::decode(&Settings::default().encode()), Settings::default());
    }

    #[test]
    fn odd_settings_are_put_right() {
        assert_eq!(Settings::decode(""), Settings::default());
        assert_eq!(Settings::decode("nonsense;;=;scale=abc"), Settings::default());
        let s = Settings::decode("scale=500;speed=3.1;autosave=sometimes;future=1");
        assert_eq!((s.scale, s.speed, s.autosave), (150, 4.0, Autosave::Day));
        assert_eq!(Settings::decode("scale=10").scale, 80);
    }
}
