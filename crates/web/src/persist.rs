//! Keeping the career between runs: the encoded save (see
//! `basketman_core::save`) as base64 text under one key of eframe's storage,
//! which is a file on the desktop and `localStorage` in the browser.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use basketman_core::league::World;
use basketman_core::save;

const KEY: &str = "basketman.career";

/// What was found in storage.
pub(crate) enum Loaded {
    /// Nothing saved.
    Nothing,
    Career(Box<World>),
    /// Something is saved but cannot be opened; the reason, for the title
    /// screen. The stored text is left alone.
    Unreadable(String),
}

/// A line of plain text about the saved career, for the web page around
/// the game to show next to its Continue button without decoding the save.
const SUMMARY_KEY: &str = "basketman.summary";

/// Club, season and round of a career, in words.
fn summary(world: &World) -> String {
    let club = world
        .watch
        .and_then(|t| world.teams.get(t))
        .map_or("", |t| t.name.as_str());
    if world.season_over() {
        format!("{club}, season {}, off-season", world.season + 1)
    } else {
        format!(
            "{club}, season {}, round {} of {}",
            world.season + 1,
            world.round + 1,
            world.rounds()
        )
    }
}

/// Write `world` under the career key and flush.
pub(crate) fn write(storage: &mut dyn eframe::Storage, world: &World) {
    storage.set_string(KEY, STANDARD.encode(save::encode(world)));
    storage.set_string(SUMMARY_KEY, summary(world));
    storage.flush();
}

/// Read the career back.
pub(crate) fn read(storage: &dyn eframe::Storage) -> Loaded {
    let Some(text) = storage.get_string(KEY) else {
        return Loaded::Nothing;
    };
    let bytes = match STANDARD.decode(text.trim()) {
        Ok(b) => b,
        Err(_) => return Loaded::Unreadable("the stored text is damaged".into()),
    };
    match save::decode(&bytes) {
        Ok(w) if w.watch.is_some_and(|t| t < w.teams.len()) => Loaded::Career(Box::new(w)),
        Ok(_) => Loaded::Unreadable("it has no club in it".into()),
        Err(e) => Loaded::Unreadable(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use basketman_core::command::Command;
    use eframe::Storage as _;

    use super::*;

    #[derive(Default)]
    struct Memory {
        values: HashMap<String, String>,
        flushes: usize,
    }

    impl eframe::Storage for Memory {
        fn get_string(&self, key: &str) -> Option<String> {
            self.values.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.values.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.values.remove(key);
        }
        fn flush(&mut self) {
            self.flushes += 1;
        }
    }

    #[test]
    fn a_career_survives_storage() {
        let mut store = Memory::default();
        assert!(matches!(read(&store), Loaded::Nothing));

        let mut w = World::generate(5, 12);
        w.apply(Command::TakeCharge(3)).unwrap();
        w.apply(Command::Advance).unwrap();
        write(&mut store, &w);
        assert_eq!(store.flushes, 1);

        let Loaded::Career(back) = read(&store) else {
            panic!("the career did not come back");
        };
        assert_eq!(back.watch, Some(3));
        assert_eq!(back.day, w.day);
        assert!(format!("{w:?}") == format!("{back:?}"));
    }

    #[test]
    fn an_unreadable_save_is_reported_and_kept() {
        let mut store = Memory::default();
        for junk in ["not base64 !!", "Zm9vYmFy", "QktNTgE="] {
            store.set_string(KEY, junk.into());
            assert!(matches!(read(&store), Loaded::Unreadable(_)), "{junk}");
            assert_eq!(store.get_string(KEY).as_deref(), Some(junk));
        }
        // A world nobody is in charge of is not a career.
        let w = World::generate(5, 12);
        write(&mut store, &w);
        assert!(matches!(read(&store), Loaded::Unreadable(_)));
    }
}
