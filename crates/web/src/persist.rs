//! Keeping careers between runs: named save slots.
//!
//! Each slot is the encoded save (see `basketman_core::save`) as base64
//! text under its own key of eframe's storage (a file on the desktop,
//! `localStorage` in the browser), and a small index under one more key
//! holds what the menus show about every slot without opening it: manager,
//! club, division, date, record, season, when it was last played.
//!
//! Older versions kept a single career under `basketman.career`. The first
//! run of this version finds it and files it as slot 1 (`Index::read` and
//! `Index::commit_migration`); nothing is lost.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use basketman_core::league::World;
use basketman_core::save;
use serde::{Deserialize, Serialize};

const INDEX_KEY: &str = "basketman.slots";
/// The single save of the versions before slots.
const OLD_KEY: &str = "basketman.career";
const OLD_SUMMARY: &str = "basketman.summary";

fn slot_key(id: u32) -> String {
    format!("basketman.slot.{id}")
}

/// How many careers there may be. The browser keeps about five million
/// characters in all; a career after a few seasons is a good deal under a
/// million (see the numbers in `docs/`/the tests), so five fit with room.
pub(crate) fn max_slots() -> usize {
    if cfg!(target_arch = "wasm32") { 5 } else { 20 }
}

/// The most characters the slots may take together. The browser keeps about
/// five million in all for the page; a career takes about a quarter of a
/// million to begin with and 120 thousand more for every season played
/// (measured: see `storage_sizes`), so a long career can be too big for
/// five slots, and saving into it is refused rather than lost. The desktop
/// has no such limit.
fn budget() -> usize {
    if cfg!(target_arch = "wasm32") { 4_500_000 } else { usize::MAX }
}

/// Seconds since 1970. The browser has no `SystemTime`.
pub(crate) fn now_secs() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
    #[cfg(target_arch = "wasm32")]
    {
        (eframe::web_sys::js_sys::Date::now() / 1000.0) as u64
    }
}

/// What the menus say about a slot.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct SlotMeta {
    pub id: u32,
    /// The name of the slot, which the player may change.
    pub name: String,
    #[serde(default)]
    pub manager: String,
    pub club: String,
    pub division: String,
    /// The date in the game.
    pub date: String,
    pub record: String,
    /// Counting from 1.
    pub season: u16,
    /// Real time of the last save, seconds since 1970; 0 if not known.
    #[serde(default)]
    pub played: u64,
    /// Characters it takes in storage.
    #[serde(default)]
    pub chars: usize,
    /// The text in the slot cannot be opened.
    #[serde(default)]
    pub damaged: bool,
}

impl SlotMeta {
    /// "Helsinki Wolves, Third Division, season 1".
    pub fn line(&self) -> String {
        if self.damaged {
            return "Cannot be opened".into();
        }
        format!("{}, {}, season {}", self.club, self.division, self.season)
    }

    /// "17 Nov 2026 / 3-1" style second line.
    pub fn detail(&self) -> String {
        format!("{}   record {}", self.date, self.record)
    }
}

fn describe(world: &World) -> (String, String, String, String, u16) {
    let (club, division, record) = match world.watch.and_then(|t| world.teams.get(t).map(|x| (t, x))) {
        Some((t, team)) => {
            let s = &world.standings[t];
            (
                team.name.clone(),
                world.division_name(team.division),
                format!("{}-{}", s.wins, s.losses),
            )
        }
        None => ("No club".into(), "out of work".into(), "-".into()),
    };
    let d = world.date_of(world.day);
    let date = format!("{} {} {}", d.day, &d.month_name()[..3], d.year);
    (club, division, date, record, world.season + 1)
}

impl SlotMeta {
    pub fn of(id: u32, world: &World, manager: &str, name: Option<&str>, played: u64) -> SlotMeta {
        let (club, division, date, record, season) = describe(world);
        SlotMeta {
            id,
            name: name.map_or_else(|| club.clone(), str::to_string),
            manager: manager.to_string(),
            club,
            division,
            date,
            record,
            season,
            played,
            chars: 0,
            damaged: false,
        }
    }
}

/// The list of slots, as stored.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Index {
    /// The id the next slot gets. Ids are never reused.
    #[serde(default)]
    pub next: u32,
    pub slots: Vec<SlotMeta>,
    /// The old single save has been seen, but not yet moved to slot 1.
    #[serde(skip)]
    pub migrating: bool,
    /// Characters all slots may take together.
    #[serde(skip, default = "budget")]
    pub budget: usize,
}

impl Default for Index {
    fn default() -> Self {
        Index { next: 0, slots: Vec::new(), migrating: false, budget: budget() }
    }
}

/// What was found in a slot.
pub(crate) enum Loaded {
    Career(Box<World>),
    /// The reason, for the list.
    Unreadable(String),
}

pub(crate) fn encode_text(world: &World) -> String {
    STANDARD.encode(save::encode(world))
}

pub(crate) fn decode_text(text: &str) -> Loaded {
    let bytes = match STANDARD.decode(text.trim()) {
        Ok(b) => b,
        Err(_) => return Loaded::Unreadable("the stored text is damaged".into()),
    };
    decode_bytes(&bytes)
}

fn decode_bytes(bytes: &[u8]) -> Loaded {
    match save::decode(bytes) {
        Ok(w) if w.watch.is_some_and(|t| t < w.teams.len()) => Loaded::Career(Box::new(w)),
        Ok(w) if w.jobless() => Loaded::Career(Box::new(w)),
        Ok(_) => Loaded::Unreadable("it has no club in it".into()),
        Err(e) => Loaded::Unreadable(e.to_string()),
    }
}

impl Index {
    /// Read the index. Finds the single save of older versions and lists it
    /// as slot 1; call `commit_migration` to make that permanent.
    pub fn read(storage: &dyn eframe::Storage) -> Index {
        if let Some(text) = storage.get_string(INDEX_KEY)
            && let Ok(mut index) = serde_json::from_str::<Index>(&text)
        {
            index.next = index.next.max(index.slots.iter().map(|s| s.id).max().unwrap_or(0) + 1);
            return index;
        }
        let mut index = Index { next: 1, ..Default::default() };
        if let Some(text) = storage.get_string(OLD_KEY) {
            let mut meta = match decode_text(&text) {
                Loaded::Career(w) => SlotMeta::of(1, &w, "", None, 0),
                Loaded::Unreadable(why) => SlotMeta {
                    id: 1,
                    name: "Saved career".into(),
                    club: why,
                    damaged: true,
                    ..Default::default()
                },
            };
            meta.chars = text.len();
            meta.manager = "Manager".into();
            index.slots.push(meta);
            index.next = 2;
            index.migrating = true;
        }
        index
    }

    /// Move the old single save into slot 1's key.
    pub fn commit_migration(&mut self, storage: &mut dyn eframe::Storage) {
        if !self.migrating {
            return;
        }
        self.migrating = false;
        let Some(text) = storage.get_string(OLD_KEY) else {
            return;
        };
        storage.set_string(&slot_key(1), text.clone());
        // Only when it is certain to be there is the old copy removed.
        if storage.get_string(&slot_key(1)).as_deref() == Some(text.as_str()) {
            self.write(storage);
            storage.remove_string(OLD_KEY);
            storage.remove_string(OLD_SUMMARY);
        }
        storage.flush();
    }

    fn write(&self, storage: &mut dyn eframe::Storage) {
        if let Ok(text) = serde_json::to_string(self) {
            storage.set_string(INDEX_KEY, text);
        }
    }

    pub fn get(&self, id: u32) -> Option<&SlotMeta> {
        self.slots.iter().find(|s| s.id == id)
    }

    /// The slot played last.
    pub fn latest(&self) -> Option<&SlotMeta> {
        self.slots
            .iter()
            .filter(|s| !s.damaged)
            .max_by_key(|s| (s.played, s.id))
    }

    /// Characters the slots take, leaving out `except`.
    fn used_without(&self, except: u32) -> usize {
        self.slots.iter().filter(|s| s.id != except).map(|s| s.chars).sum()
    }

    pub fn full(&self) -> bool {
        self.slots.len() >= max_slots()
    }

    /// The slot to give up first when all are in use: the one played longest ago.
    pub fn oldest(&self) -> Option<u32> {
        self.slots.iter().min_by_key(|s| (s.played, s.id)).map(|s| s.id)
    }

    /// Save `world` into an existing slot; the slot keeps its name.
    pub fn save(
        &mut self,
        storage: &mut dyn eframe::Storage,
        id: u32,
        world: &World,
        manager: &str,
    ) -> Result<(), String> {
        let name = self.get(id).map(|s| s.name.clone());
        let text = encode_text(world);
        if self.used_without(id) + text.len() > self.budget {
            return Err("The browser has no more room for this career. Delete a saved career to make room.".into());
        }
        let mut meta = SlotMeta::of(id, world, manager, name.as_deref(), now_secs());
        meta.chars = text.len();
        storage.set_string(&slot_key(id), text);
        match self.slots.iter_mut().find(|s| s.id == id) {
            Some(s) => *s = meta,
            None => self.slots.push(meta),
        }
        self.write(storage);
        storage.flush();
        Ok(())
    }

    /// Put `world` into a new slot and return its id.
    pub fn create(
        &mut self,
        storage: &mut dyn eframe::Storage,
        world: &World,
        manager: &str,
        name: Option<&str>,
    ) -> Result<u32, String> {
        if self.full() {
            return Err(format!(
                "All {} slots are in use. Delete one first.",
                max_slots()
            ));
        }
        let text = encode_text(world);
        if self.used_without(0) + text.len() > self.budget {
            return Err("The browser has no more room for another career. Delete a saved career to make room.".into());
        }
        let id = self.next.max(1);
        self.next = id + 1;
        let text = encode_text(world);
        let mut meta = SlotMeta::of(id, world, manager, name, now_secs());
        meta.chars = text.len();
        storage.set_string(&slot_key(id), text);
        self.slots.push(meta);
        self.write(storage);
        storage.flush();
        Ok(id)
    }

    pub fn load(&self, storage: &dyn eframe::Storage, id: u32) -> Loaded {
        let key = if self.migrating && id == 1 { OLD_KEY.to_string() } else { slot_key(id) };
        match storage.get_string(&key) {
            Some(text) => decode_text(&text),
            None => Loaded::Unreadable("the save is missing".into()),
        }
    }

    pub fn delete(&mut self, storage: &mut dyn eframe::Storage, id: u32) {
        storage.remove_string(&slot_key(id));
        self.slots.retain(|s| s.id != id);
        self.write(storage);
        storage.flush();
    }

    pub fn rename(&mut self, storage: &mut dyn eframe::Storage, id: u32, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(s) = self.slots.iter_mut().find(|s| s.id == id) {
            s.name = name.chars().take(40).collect();
        }
        self.write(storage);
        storage.flush();
    }

    /// A slot as a file: JSON holding the names and the save.
    pub fn export(&self, storage: &dyn eframe::Storage, id: u32) -> Result<(String, Vec<u8>), String> {
        let meta = self.get(id).ok_or("no such slot")?;
        let text = storage.get_string(&slot_key(id)).ok_or("the save is missing")?;
        let file = Export {
            basketman: 1,
            name: meta.name.clone(),
            manager: meta.manager.clone(),
            save: text,
        };
        let name: String = meta
            .name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        Ok((
            format!("basketman-{}.bmsave", name.trim_matches('-').to_lowercase()),
            serde_json::to_vec(&file).map_err(|e| e.to_string())?,
        ))
    }

    /// Add a career from a file made by `export` (or the bare save).
    pub fn import(&mut self, storage: &mut dyn eframe::Storage, bytes: &[u8]) -> Result<u32, String> {
        let (name, manager, world) = match serde_json::from_slice::<Export>(bytes) {
            Ok(f) => match decode_text(&f.save) {
                Loaded::Career(w) => (Some(f.name), f.manager, *w),
                Loaded::Unreadable(why) => return Err(why),
            },
            Err(_) => match decode_bytes(bytes) {
                Loaded::Career(w) => (None, "Manager".to_string(), *w),
                Loaded::Unreadable(why) => return Err(why),
            },
        };
        self.create(storage, &world, &manager, name.as_deref())
    }
}

#[derive(Serialize, Deserialize)]
struct Export {
    basketman: u32,
    name: String,
    #[serde(default)]
    manager: String,
    save: String,
}

/// "just now", "5 minutes ago", "3 days ago", or the date for older.
pub(crate) fn ago(now: u64, then: u64) -> String {
    if then == 0 {
        return String::new();
    }
    let s = now.saturating_sub(then);
    match s {
        0..=89 => "just now".into(),
        90..=3599 => format!("{} minutes ago", (s + 30) / 60),
        3600..=86_399 => {
            let h = (s + 1800) / 3600;
            format!("{h} hour{} ago", if h == 1 { "" } else { "s" })
        }
        86_400..=2_591_999 => {
            let d = s / 86_400;
            format!("{d} day{} ago", if d == 1 { "" } else { "s" })
        }
        _ => {
            let d = basketman_core::calendar::Date::from_abs((then / 86_400) as i64);
            format!("{} {} {}", d.day, &d.month_name()[..3], d.year)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use basketman_core::command::Command;
    use eframe::Storage as _;

    use super::*;

    #[derive(Default)]
    pub(crate) struct Memory {
        pub values: HashMap<String, String>,
        pub flushes: usize,
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

    fn career(seed: u64, team: usize) -> World {
        let mut w = World::new_career_tiers(seed, 4, 2);
        w.apply(Command::TakeCharge(team)).unwrap();
        w
    }

    fn open(index: &Index, store: &Memory, id: u32) -> World {
        match index.load(store, id) {
            Loaded::Career(w) => *w,
            Loaded::Unreadable(why) => panic!("slot {id}: {why}"),
        }
    }

    #[test]
    fn a_career_survives_a_slot() {
        let mut store = Memory::default();
        let mut index = Index::read(&store);
        assert!(index.slots.is_empty());

        let mut w = career(5, 3);
        w.apply(Command::Advance).unwrap();
        let id = index.create(&mut store, &w, "Ada Lind", None).unwrap();
        assert!(store.flushes > 0);

        let back = open(&index, &store, id);
        assert_eq!(back.watch, Some(3));
        assert_eq!(back.day, w.day);
        assert!(format!("{w:?}") == format!("{back:?}"));
    }

    #[test]
    fn the_slot_list_round_trips_through_storage() {
        let mut store = Memory::default();
        let mut index = Index::read(&store);
        let a = index.create(&mut store, &career(1, 0), "Ada", Some("First")).unwrap();
        let b = index.create(&mut store, &career(2, 5), "Bo", None).unwrap();
        index.rename(&mut store, a, "  Renamed  ");
        assert_ne!(a, b);

        let again = Index::read(&store);
        assert_eq!(again.slots, index.slots);
        assert_eq!(again.get(a).unwrap().name, "Renamed");
        assert_eq!(again.get(a).unwrap().manager, "Ada");
        assert_eq!(again.get(b).unwrap().name, again.get(b).unwrap().club);
        assert!(again.get(b).unwrap().chars > 1000);
        assert_eq!(again.get(b).unwrap().season, 1);

        // Ids are not reused after a delete.
        let mut again = again;
        again.delete(&mut store, a);
        assert!(store.get_string(&slot_key(a)).is_none());
        let c = again.create(&mut store, &career(3, 1), "Cy", None).unwrap();
        assert!(c > b);
        assert_eq!(Index::read(&store).slots.len(), 2);
    }

    #[test]
    fn a_save_goes_back_into_its_own_slot() {
        let mut store = Memory::default();
        let mut index = Index::read(&store);
        let mut w = career(1, 0);
        let id = index.create(&mut store, &w, "Ada", Some("Mine")).unwrap();
        let _ = index.create(&mut store, &career(2, 1), "Bo", None).unwrap();
        w.apply(Command::Advance).unwrap();
        index.save(&mut store, id, &w, "Ada").unwrap();
        assert_eq!(index.slots.len(), 2);
        assert_eq!(index.get(id).unwrap().name, "Mine");
        assert_eq!(open(&index, &store, id).day, w.day);
    }

    #[test]
    fn slots_are_limited() {
        let mut store = Memory::default();
        let mut index = Index::read(&store);
        let w = career(1, 0);
        for _ in 0..max_slots() {
            index.create(&mut store, &w, "Ada", None).unwrap();
        }
        assert!(index.full());
        assert!(index.create(&mut store, &w, "Ada", None).is_err());
    }

    #[test]
    fn a_save_that_would_not_fit_is_refused_and_the_old_one_stays() {
        let mut store = Memory::default();
        let mut index = Index::read(&store);
        let mut w = career(1, 0);
        let id = index.create(&mut store, &w, "Ada", None).unwrap();
        let size = index.get(id).unwrap().chars;
        // Room for one career and a half.
        index.budget = size * 3 / 2;
        assert!(index.create(&mut store, &w, "Bo", None).is_err());
        assert_eq!(index.slots.len(), 1);
        // The career in the slot may still grow a little.
        w.apply(Command::Advance).unwrap();
        assert!(index.save(&mut store, id, &w, "Ada").is_ok());
        // Not past the budget: the slot keeps what it had.
        index.budget = 1000;
        let before = store.get_string(&slot_key(id));
        assert!(index.save(&mut store, id, &w, "Ada").is_err());
        assert_eq!(store.get_string(&slot_key(id)), before);
    }

    #[test]
    fn the_old_single_save_becomes_slot_one() {
        let mut store = Memory::default();
        let mut w = career(5, 3);
        w.apply(Command::Advance).unwrap();
        // As the version before slots wrote it.
        store.set_string(OLD_KEY, encode_text(&w));
        store.set_string(OLD_SUMMARY, "Somewhere, season 1, round 1 of 6".into());

        let mut index = Index::read(&store);
        assert!(index.migrating);
        assert_eq!(index.slots.len(), 1);
        assert_eq!(index.slots[0].id, 1);
        assert_eq!(index.slots[0].club, w.teams[3].name);
        // Even before it is moved, it opens.
        assert_eq!(open(&index, &store, 1).day, w.day);

        index.commit_migration(&mut store);
        assert!(store.get_string(OLD_KEY).is_none());
        assert!(store.get_string(OLD_SUMMARY).is_none());
        let again = Index::read(&store);
        assert!(!again.migrating);
        assert_eq!(again.slots.len(), 1);
        let back = open(&again, &store, 1);
        assert!(format!("{w:?}") == format!("{back:?}"));
        // The next career gets slot 2.
        let mut again = again;
        assert_eq!(again.create(&mut store, &w, "Ada", None).unwrap(), 2);
    }

    #[test]
    fn an_unreadable_old_save_is_kept_and_listed() {
        let mut store = Memory::default();
        store.set_string(OLD_KEY, "Zm9vYmFy".into());
        let mut index = Index::read(&store);
        assert!(index.slots[0].damaged);
        index.commit_migration(&mut store);
        assert_eq!(store.get_string(&slot_key(1)).as_deref(), Some("Zm9vYmFy"));
        assert!(matches!(
            Index::read(&store).load(&store, 1),
            Loaded::Unreadable(_)
        ));
        // A damaged slot is never the one to continue.
        assert!(index.latest().is_none());
    }

    #[test]
    fn unreadable_text_is_reported() {
        for junk in ["not base64 !!", "Zm9vYmFy", "QktNTgE="] {
            assert!(matches!(decode_text(junk), Loaded::Unreadable(_)), "{junk}");
        }
        // A world nobody is in charge of is not a career.
        let w = World::generate(5, 12);
        assert!(matches!(decode_text(&encode_text(&w)), Loaded::Unreadable(_)));
    }

    #[test]
    fn a_career_travels_as_a_file() {
        let mut store = Memory::default();
        let mut index = Index::read(&store);
        let w = career(4, 2);
        let id = index.create(&mut store, &w, "Ada", Some("Road trip")).unwrap();
        let (file, bytes) = index.export(&store, id).unwrap();
        assert_eq!(file, "basketman-road-trip.bmsave");

        let mut other = Memory::default();
        let mut index2 = Index::read(&other);
        let id2 = index2.import(&mut other, &bytes).unwrap();
        let meta = index2.get(id2).unwrap();
        assert_eq!((meta.name.as_str(), meta.manager.as_str()), ("Road trip", "Ada"));
        assert!(format!("{w:?}") == format!("{:?}", open(&index2, &other, id2)));
        assert!(index2.import(&mut other, b"nonsense").is_err());
        // The bare save works too.
        let bare = save::encode(&w);
        assert!(index2.import(&mut other, &bare).is_ok());
    }

    #[test]
    fn real_time_is_said_plainly() {
        assert_eq!(ago(1000, 1000), "just now");
        assert_eq!(ago(1000 + 600, 1000), "10 minutes ago");
        assert_eq!(ago(1000 + 3 * 3600, 1000), "3 hours ago");
        assert_eq!(ago(1000 + 86_400, 1000), "1 day ago");
        assert_eq!(ago(2_000_000_000, 1_700_000_000), "14 Nov 2023");
        assert_eq!(ago(5, 0), "");
    }

    /// `cargo test --release -p basketman-web storage_sizes -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn storage_sizes() {
        use basketman_core::inbox::Stop;
        let t = std::time::Instant::now();
        let mut w = World::new_career_tiers(7, 12, 3);
        eprintln!("generate 36 clubs: {:?}", t.elapsed());
        w.apply(Command::TakeCharge(30)).unwrap();
        let mut report = |label: &str, w: &World| {
            let t = std::time::Instant::now();
            let text = encode_text(w);
            let enc = t.elapsed();
            let t = std::time::Instant::now();
            let _ = decode_text(&text);
            eprintln!("{label}: {} chars, encode {:?}, decode {:?}", text.len(), enc, t.elapsed());
        };
        report("fresh", &w);
        for season in 1..=4 {
            loop {
                match w.advance() {
                    Stop::MatchDay => {
                        w.play_round();
                    }
                    Stop::Mail => {}
                    Stop::Draft => break,
                }
            }
            w.draft_auto(true);
            report(&format!("after {season} season(s)"), &w);
        }
    }
}
