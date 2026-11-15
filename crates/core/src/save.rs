//! Saving a career: a snapshot of the `World`, not a replay of its log.
//!
//! The sim changes between builds, so replaying `World::log` on a newer
//! build would not give the same career. A snapshot does survive an update,
//! as long as the fields it holds can still be read: it is JSON, so a field
//! added later with `#[serde(default)]` leaves older saves loadable.
//!
//! Layout: the 4 bytes `BKMN`, one version byte, then the JSON compressed
//! with deflate. Pure functions; reading and writing the bytes somewhere is
//! the caller's business.
//!
//! Left out: `World::tapes` and `World::replay`, the recordings of games,
//! which are by far the biggest part of a world (a season's tapes are tens
//! of megabytes of JSON; the latest game alone is about 1.3 MB compressed).
//! After loading, those games simply have no tape (`World::has_tape`).

use std::fmt;

use miniz_oxide::deflate::compress_to_vec;
use miniz_oxide::inflate::decompress_to_vec_with_limit;

use crate::league::World;

const MAGIC: [u8; 4] = *b"BKMN";
/// Bumped only when the layout above changes, not when `World` grows a
/// field.
const VERSION: u8 = 1;
const HEADER: usize = MAGIC.len() + 1;
/// What a save may inflate to; a guard against corrupt input, far above a
/// real career.
const LIMIT: usize = 512 * 1024 * 1024;
const LEVEL: u8 = 6;

/// Why bytes are not a save this build can open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// Not a save at all, or cut short before the data.
    BadHeader,
    /// A save written by a build with another layout.
    UnsupportedVersion(u8),
    /// A save whose data cannot be read: damaged, or from a build whose
    /// world no longer fits this one.
    Corrupt(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::BadHeader => f.write_str("not a saved career"),
            SaveError::UnsupportedVersion(v) => {
                write!(f, "saved by a different version of the game (format {v})")
            }
            SaveError::Corrupt(why) => write!(f, "the saved career is damaged ({why})"),
        }
    }
}

impl std::error::Error for SaveError {}

/// The bytes to keep for `world`.
pub fn encode(world: &World) -> Vec<u8> {
    let mut slim = world.clone();
    slim.tapes.clear();
    slim.replay = None;
    let json = serde_json::to_vec(&slim).expect("a world is always serializable");
    let mut out = Vec::with_capacity(HEADER + json.len() / 8);
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.extend(compress_to_vec(&json, LEVEL));
    out
}

/// The world in `bytes`. Never panics, whatever the input.
pub fn decode(bytes: &[u8]) -> Result<World, SaveError> {
    if bytes.len() < HEADER || bytes[..MAGIC.len()] != MAGIC {
        return Err(SaveError::BadHeader);
    }
    let version = bytes[MAGIC.len()];
    if version != VERSION {
        return Err(SaveError::UnsupportedVersion(version));
    }
    let json = decompress_to_vec_with_limit(&bytes[HEADER..], LIMIT)
        .map_err(|e| SaveError::Corrupt(format!("{:?}", e.status)))?;
    let mut world: World =
        serde_json::from_slice(&json).map_err(|e| SaveError::Corrupt(e.to_string()))?;
    // Saves from before footwork and signature moves get them here.
    world.repair_new_fields();
    // Saves from before arenas and sponsors existed get theirs here.
    world.ensure_economy();
    world.ensure_fame();
    // Saves from before the calendar get their game days.
    world.ensure_calendar();
    Ok(world)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::command::tests::tidy;
    use crate::inbox::Stop;
    use crate::player::Attr;

    /// Play on as the user's club until `seasons` seasons are complete.
    fn play(w: &mut World, seasons: u16) {
        let target = w.season + seasons;
        let mut n = 0;
        for _ in 0..200_000 {
            if w.season >= target {
                return;
            }
            tidy(w, &mut n);
            match w.apply(Command::Advance).unwrap().stop.unwrap() {
                Stop::MatchDay => {
                    w.apply(Command::PlayRound).unwrap();
                }
                Stop::Mail => {}
                Stop::Draft => {
                    let _ = w.apply(Command::DraftAuto { all: true });
                }
            }
        }
        panic!("the seasons never ended");
    }

    /// Play `rounds` more rounds.
    fn rounds(w: &mut World, rounds: usize) {
        let mut n = 1000;
        let mut played = 0;
        for _ in 0..100_000 {
            if played >= rounds {
                return;
            }
            tidy(w, &mut n);
            match w.apply(Command::Advance).unwrap().stop.unwrap() {
                Stop::MatchDay => {
                    w.apply(Command::PlayRound).unwrap();
                    played += 1;
                }
                Stop::Mail => {}
                Stop::Draft => {
                    let _ = w.apply(Command::DraftAuto { all: true });
                }
            }
        }
        panic!("no rounds were played");
    }

    fn career(seasons: u16) -> World {
        let mut w = World::generate(11, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        play(&mut w, seasons);
        w
    }

    #[test]
    fn a_save_loads_as_the_world_it_was() {
        let mut w = career(2);
        assert_eq!(w.season, 2);
        rounds(&mut w, 6);
        assert!(!w.tapes.is_empty(), "the test needs tapes to drop");
        let bytes = encode(&w);
        let loaded = decode(&bytes).unwrap();
        // Only the old tapes are gone.
        let mut want = w.clone();
        want.tapes.clear();
        want.replay = None;
        assert!(
            format!("{want:?}") == format!("{loaded:?}"),
            "the loaded world differs"
        );
        assert!(loaded.tapes.is_empty() && loaded.replay.is_none());
        assert_eq!(loaded.log, w.log);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn a_loaded_world_plays_on_exactly_like_the_original() {
        let mut a = career(1);
        let mut b = decode(&encode(&a)).unwrap();
        rounds(&mut a, 30);
        rounds(&mut b, 30);
        // Recordings are not saved, so the two hold different old ones.
        (a.tapes, b.tapes) = (Vec::new(), Vec::new());
        assert!(
            format!("{a:?}") == format!("{b:?}"),
            "the loaded world drifted from the original"
        );
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert_eq!(decode(&[]).err(), Some(SaveError::BadHeader));
        assert_eq!(decode(b"BKM").err(), Some(SaveError::BadHeader));
        assert_eq!(decode(b"{\"rng\":1}").err(), Some(SaveError::BadHeader));
        assert_eq!(
            decode(b"BKMN\x09abc").err(),
            Some(SaveError::UnsupportedVersion(9))
        );
        assert!(matches!(decode(b"BKMN\x01"), Err(SaveError::Corrupt(_))));
        assert!(matches!(
            decode(b"BKMN\x01garbage, not deflate"),
            Err(SaveError::Corrupt(_))
        ));
        // Valid deflate of valid JSON that is not a world.
        let mut not_a_world = b"BKMN\x01".to_vec();
        not_a_world.extend(compress_to_vec(b"{\"rng\":1}", LEVEL));
        assert!(matches!(decode(&not_a_world), Err(SaveError::Corrupt(_))));

        let bytes = encode(&career(0));
        // Every truncation, and damage anywhere, is an error or (for a flip
        // that leaves valid data) a world, never a panic.
        for cut in (0..bytes.len()).step_by(bytes.len() / 200 + 1) {
            assert!(decode(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        for at in (HEADER..bytes.len()).step_by(bytes.len() / 200 + 1) {
            let mut damaged = bytes.clone();
            damaged[at] ^= 0x55;
            let _ = decode(&damaged);
        }
        assert!(SaveError::BadHeader.to_string().len() > 5);
    }

    #[test]
    fn saves_stay_small() {
        // The size of the league the game ships with.
        let mut w = World::generate(11, 12);
        w.apply(Command::TakeCharge(0)).unwrap();
        let mut sizes = Vec::new();
        for _ in 0..3 {
            play(&mut w, 1);
            sizes.push(encode(&w).len());
        }
        // Mid-season, with a season's tapes in the world but not the save.
        rounds(&mut w, 10);
        let json = serde_json::to_vec(&w).unwrap().len();
        let tapes = serde_json::to_vec(&w.tapes).unwrap().len();
        let replay = serde_json::to_vec(&w.replay).unwrap().len();
        eprintln!(
            "save bytes after 1, 2, 3 seasons: {sizes:?}; mid-season {}; json {json}, of which tapes {tapes} ({} of them), latest replay {replay}",
            encode(&w).len(),
            w.tapes.len()
        );
        assert!(sizes[2] < 1_500_000, "{sizes:?}");
    }

    #[test]
    fn a_save_from_before_the_calendar_keeps_time_passing() {
        let mut w = World::generate(6, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        for _ in 0..3 {
            while w.apply(Command::Advance).unwrap().stop != Some(Stop::MatchDay) {}
            w.apply(Command::PlayRound).unwrap();
        }
        assert_eq!(w.round, 3);
        let bytes = encode(&w);
        let json = decompress_to_vec_with_limit(&bytes[HEADER..], LIMIT).unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&json).unwrap();
        let obj = v.as_object_mut().unwrap();
        for k in [
            "friendlies",
            "preseason_days",
            "reserve_rounds_done",
            "draft_day",
            "round_seeds",
            "tiers",
        ] {
            obj.remove(k);
        }
        for f in obj["fixtures"].as_array_mut().unwrap() {
            let f = f.as_object_mut().unwrap();
            f.remove("day");
            f.remove("division");
        }
        for t in obj["teams"].as_array_mut().unwrap() {
            t.as_object_mut().unwrap().remove("division");
        }
        let mut old = MAGIC.to_vec();
        old.push(VERSION);
        old.extend(compress_to_vec(&serde_json::to_vec(&v).unwrap(), LEVEL));
        let mut loaded = decode(&old).expect("an old save opens");
        // The old calendar: a round a week, the game on Saturday.
        assert_eq!(loaded.match_day(), 3 * 7 + 5);
        assert!(loaded.fixtures.iter().all(|f| f.day == f.round as u32 * 7 + 5));
        assert!(!loaded.match_today());
        // Time passes before the game: mail first, then the match day.
        let mut stops = Vec::new();
        for _ in 0..6 {
            let s = loaded.apply(Command::Advance).unwrap().stop.unwrap();
            stops.push(s);
            if s == Stop::MatchDay {
                break;
            }
        }
        assert_eq!(stops.last(), Some(&Stop::MatchDay));
        assert!(stops.contains(&Stop::Mail), "{stops:?}");
        assert_eq!(loaded.day, 26);
        loaded.apply(Command::PlayRound).unwrap();
        assert_eq!(loaded.round, 4);
        assert_eq!(loaded.match_day(), 33);
    }

    #[test]
    fn saves_from_before_arenas_still_open() {
        let mut w = World::generate(2, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        play(&mut w, 1);
        let ages: Vec<u8> = w.players.iter().map(|p| p.age).collect();
        let pay: Vec<u32> = w.players.iter().map(|p| p.contract.salary).collect();
        let bytes = encode(&w);
        let json = decompress_to_vec_with_limit(&bytes[HEADER..], LIMIT).unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&json).unwrap();
        let obj = v.as_object_mut().unwrap();
        assert!(obj.remove("econ").is_some());
        for m in obj["desk"]["messages"].as_array_mut().unwrap() {
            let m = m.as_object_mut().unwrap();
            m.remove("sponsor");
            m.remove("appearance");
        }
        // And from before fame, fans, media work, presence and the press
        // officer.
        assert!(obj.remove("fame").is_some());
        for p in obj["players"].as_array_mut().unwrap() {
            let p = p.as_object_mut().unwrap();
            assert!(p.remove("renown").is_some());
            p["personality"].as_object_mut().unwrap().remove("presence");
            // And from before footwork and signature moves.
            assert!(p.remove("signatures").is_some());
            assert!(p.remove("signatures_rolled").is_some());
            let attrs = p["attrs"].as_array_mut().unwrap();
            assert_eq!(attrs.len(), Attr::COUNT);
            attrs.pop();
        }
        obj["scouting"]
            .as_object_mut()
            .unwrap()
            .remove("seen_signatures");
        for t in obj["teams"].as_array_mut().unwrap() {
            let office = t["office"].as_object_mut().unwrap();
            assert!(office.remove("press").is_some());
            for s in office["staff"].as_array_mut().unwrap() {
                if let Some(s) = s.as_object_mut() {
                    s.remove("media");
                }
            }
        }
        for s in obj["staff_pool"].as_array_mut().unwrap() {
            s.as_object_mut().unwrap().remove("media");
        }
        let mut old = MAGIC.to_vec();
        old.push(VERSION);
        old.extend(compress_to_vec(&serde_json::to_vec(&v).unwrap(), LEVEL));
        let loaded = decode(&old).expect("an old save opens");
        assert_eq!(loaded.econ.arenas.len(), 8);
        assert_eq!(loaded.econ.ledgers.len(), 8);
        assert!(!loaded.econ.sponsors.is_empty());
        assert!(loaded.arena(0).capacity >= 2_500);
        // Opening a career does not rewrite its players or their pay.
        assert_eq!(
            ages,
            loaded.players.iter().map(|p| p.age).collect::<Vec<_>>()
        );
        assert_eq!(
            pay,
            loaded
                .players
                .iter()
                .map(|p| p.contract.salary)
                .collect::<Vec<_>>()
        );
        // Fame and fans are set up, and nobody is a liability by default.
        assert!(loaded.fame.seeded);
        assert_eq!(loaded.fame.fans.len(), 8);
        assert!(loaded.biggest_names(1)[0].1 > 30.0);
        assert!(loaded.players.iter().all(|p| p.personality.presence == 10));
        assert!(loaded.teams.iter().all(|t| t.office.press.is_none()));
        assert!(loaded.staff_pool.iter().all(|s| s.media == 8));
        // Footwork was filled in from body and speed, and signatures rolled
        // for everyone from id and name, as a new player would get them.
        assert!(
            loaded
                .players
                .iter()
                .all(|p| (1.0..=20.0).contains(&p.attrs[Attr::Footwork]) && p.signatures_rolled)
        );
        let with = loaded
            .players
            .iter()
            .filter(|p| !p.signatures.is_empty())
            .count();
        assert!(with * 20 > loaded.players.len() && with * 3 < loaded.players.len());
        assert!(loaded.players.iter().all(|p| p.signatures.len() <= 3));
        let fast: f32 = loaded.players.iter().map(|p| p.attrs[Attr::Footwork]).sum();
        assert!(fast / loaded.players.len() as f32 > 5.0);
        // And it plays on, through a summer of retirements.
        let mut loaded = loaded;
        play(&mut loaded, 2);
        assert!(loaded.fame.counts.retired < 100);
    }
}
