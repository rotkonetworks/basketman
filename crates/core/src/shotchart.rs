//! Where the shots of a game were taken, read from a recording: the place
//! of every attempt on a half court, and the zones they fall in.
//!
//! The recording logs a shot as an event without a position. The position
//! is the shooter's, read from the frames around the event: the frame where
//! the ball leaves his hand when it is there, else the frame of the event.

use serde::Serialize;

use crate::court::{CORNER_X, CORNER_Y, HOOP, ShotKind, Vec2, v};
use crate::league::World;
use crate::player::PlayerId;
use crate::replay::{EventKind, Recording, ShotStage, Source};

/// The areas of the floor a shot is counted in.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Zone {
    Rim,
    Mid,
    CornerThree,
    AboveBreakThree,
}

impl Zone {
    pub const ALL: [Zone; 4] = [Zone::Rim, Zone::Mid, Zone::CornerThree, Zone::AboveBreakThree];

    pub fn label(self) -> &'static str {
        match self {
            Zone::Rim => "At the rim",
            Zone::Mid => "Mid-range",
            Zone::CornerThree => "Corner three",
            Zone::AboveBreakThree => "Above the break",
        }
    }

    pub fn of(kind: ShotKind, at: Vec2) -> Zone {
        match kind {
            ShotKind::Rim => Zone::Rim,
            ShotKind::Mid => Zone::Mid,
            ShotKind::Three if at.x.abs() >= CORNER_X - 0.3 && at.y <= CORNER_Y + 0.3 => {
                Zone::CornerThree
            }
            ShotKind::Three => Zone::AboveBreakThree,
        }
    }
}

/// One field-goal attempt.
#[derive(Clone, Debug, Serialize)]
pub struct Shot {
    /// Index into `Recording::events`.
    pub event: usize,
    /// Replay time, for jumping to it.
    pub time: f32,
    pub period: u8,
    /// Seconds left in the period.
    pub clock: f32,
    /// The side that shot, 0 = home.
    pub side: u8,
    pub who: PlayerId,
    pub kind: ShotKind,
    pub zone: Zone,
    pub made: bool,
    /// Where it was taken, in the attack frame of the shooting side: x
    /// across (0 under the basket, negative left of it), y from the
    /// baseline. The basket is at `court::HOOP`.
    pub x: f32,
    pub y: f32,
    /// 0 = wide open, 1 = smothered.
    pub contest: f32,
    pub assisted: bool,
    pub blocked: bool,
    pub source: Source,
    pub defender: Option<PlayerId>,
}

/// Full-court coordinates to the attack frame of the side with the ball.
pub fn to_attack(p: Vec2, offense: usize) -> Vec2 {
    if offense == 0 {
        v(p.y - 7.5, crate::court::LENGTH - p.x)
    } else {
        v(7.5 - p.y, p.x)
    }
}

/// Every attempt in the recording so far, in order. `ids[side]` are the
/// box-score players of each side (`GameResult::boxes`), which the frames
/// refer to by index. A shot that drew a foul and missed is not an attempt.
pub fn shots(rec: &Recording, ids: &[Vec<PlayerId>; 2]) -> Vec<Shot> {
    let mut out = Vec::new();
    for (n, e) in rec.events.iter().enumerate() {
        let EventKind::Shot {
            who,
            kind,
            source,
            made,
            contest,
            defender,
            assist,
            blocked_by,
            fouled,
        } = &e.kind
        else {
            continue;
        };
        if *fouled && !made {
            continue;
        }
        let side = e.offense as usize;
        let Some(idx) = ids[side].iter().position(|p| p == who) else {
            continue;
        };
        let Some(at) = shooter_at(rec, e.time, side, idx) else {
            continue;
        };
        let at = to_attack(at, side);
        out.push(Shot {
            event: n,
            time: e.time,
            period: e.period,
            clock: e.clock,
            side: e.offense,
            who: *who,
            kind: *kind,
            zone: Zone::of(*kind, at),
            made: *made,
            x: at.x,
            y: at.y,
            contest: *contest,
            assisted: assist.is_some(),
            blocked: blocked_by.is_some(),
            source: *source,
            defender: *defender,
        });
    }
    out
}

/// Where player `idx` of `side` was when he shot at `time`.
fn shooter_at(rec: &Recording, time: f32, side: usize, idx: usize) -> Option<Vec2> {
    let end = rec.frame_at(time);
    let frame = rec.frames.get(end)?;
    let k = frame.who[side].iter().position(|&w| w as usize == idx)?;
    // Back up to two seconds for the release.
    let mut n = end;
    while n > 0 && time - rec.frames[n - 1].time <= 2.0 {
        n -= 1;
        let f = &rec.frames[n];
        if let Some(k) = f.who[side].iter().position(|&w| w as usize == idx)
            && f.pose[side][k].stage == ShotStage::Release
        {
            return Some(f.pos[side][k]);
        }
    }
    Some(frame.pos[side][k])
}

/// Made and attempted in one zone.
#[derive(Copy, Clone, Debug, Default, Serialize)]
pub struct ZoneLine {
    pub made: u16,
    pub att: u16,
}

impl ZoneLine {
    pub fn pct(&self) -> Option<f32> {
        (self.att > 0).then(|| self.made as f32 / self.att as f32)
    }
}

/// The zones of a set of shots, in `Zone::ALL` order.
pub fn zones<'a>(shots: impl IntoIterator<Item = &'a Shot>) -> [ZoneLine; 4] {
    let mut out = [ZoneLine::default(); 4];
    for s in shots {
        let z = &mut out[Zone::ALL.iter().position(|&z| z == s.zone).unwrap_or(0)];
        z.att += 1;
        z.made += s.made as u16;
    }
    out
}

impl World {
    /// The league's shooting so far this season by what a shot is worth:
    /// `(two-point %, three-point %)`. The recordings do not keep the
    /// others' shots, so zones inside the arc share the two-point average
    /// and both threes the three-point one. `None` before any game.
    pub fn league_shooting(&self) -> Option<(f32, f32)> {
        let t = self.league_totals();
        let (own, g) = (&t.own, t.games);
        if g == 0 || own.tpa == 0 || own.fga <= own.tpa {
            return None;
        }
        Some((
            (own.fgm - own.tpm) as f32 / (own.fga - own.tpa) as f32,
            own.tpm as f32 / own.tpa as f32,
        ))
    }
}

/// The league's percentage for a zone, from `World::league_shooting`.
pub fn zone_average(avg: (f32, f32), z: Zone) -> f32 {
    match z {
        Zone::Rim | Zone::Mid => avg.0,
        _ => avg.1,
    }
}

impl Shot {
    /// Distance from the basket in metres.
    pub fn distance(&self) -> f32 {
        v(self.x, self.y).dist(HOOP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;

    #[test]
    fn the_places_agree_with_the_kind_of_shot_the_engine_logged() {
        let mut w = World::new_career_tiers(5, 8, 3);
        w.apply(Command::TakeCharge(1)).unwrap();
        w.apply(Command::PlayRound).unwrap();
        let (i, rec) = w.replay.clone().expect("the user's game is recorded");
        let r = w.fixtures[i].result.clone().unwrap();
        let ids = [0, 1].map(|s| r.boxes[s].iter().map(|b| b.0).collect::<Vec<_>>());
        let list = shots(&rec, &ids);
        assert!(list.len() > 60, "{} shots", list.len());
        let agree = list
            .iter()
            .filter(|s| ShotKind::at(v(s.x, s.y)) == s.kind)
            .count();
        let share = agree as f32 / list.len() as f32;
        eprintln!("{} shots, {:.1}% where the engine put them", list.len(), share * 100.0);
        assert!(share > 0.9, "only {share} of shots sit where their kind says");
        let z = zones(&list);
        assert_eq!(z.iter().map(|l| l.att as usize).sum::<usize>(), list.len());
        assert!(list.iter().all(|s| s.x.abs() < 8.0 && s.y > -1.0 && s.y < 15.0));
    }
}
