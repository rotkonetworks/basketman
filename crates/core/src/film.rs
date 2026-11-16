//! The video room: a recorded game cut into the clips a coach wants to
//! watch. Pure functions over a `Recording`; nothing here touches the
//! simulation or the world.
//!
//! The recording has no marker for where a possession begins, so it is
//! found from the frames: walking back from an event while the same side
//! has the ball and the shot clock only runs down. A clip starts there, or
//! `MAX_LEAD` seconds before the event if that is later, and ends `TAIL`
//! seconds after it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::court::ShotKind;
use crate::player::PlayerId;
use crate::replay::{Event, EventKind, Mistake, Recording, Source, TurnoverKind};

/// Longest lead-up shown before the event, in seconds.
pub const MAX_LEAD: f32 = 8.0;
/// Time shown after the event, in seconds.
pub const TAIL: f32 = 1.5;

/// What went wrong, when something did.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Breakdown {
    /// One of our defenders failed; `conceded` is true if they scored on it.
    Defense { mistake: Mistake, conceded: bool },
    /// A player was in the wrong place and our own play broke down.
    Set,
    /// We ran the clock out without a shot.
    ShotClock,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipKind {
    /// A basket of ours.
    Score {
        source: Source,
        assisted: bool,
    },
    Turnover(TurnoverKind),
    Breakdown(Breakdown),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    /// Replay time, same clock as `Frame::time`.
    pub start: f32,
    pub end: f32,
    pub kind: ClipKind,
    /// The player it is about: the scorer, the man who lost the ball, the
    /// defender at fault.
    pub who: Option<PlayerId>,
    /// One line of play-by-play.
    pub text: String,
    /// Replay time of the event itself.
    pub time: f32,
    pub period: u8,
    /// Game clock at the event, in seconds left in the period.
    pub clock: f32,
    /// Points scored (scores only).
    pub points: u8,
    /// How good an attack it was, for ranking. Zero for everything else.
    pub quality: f32,
}

/// What one way of scoring produced.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceLine {
    pub source: Source,
    pub attempts: u16,
    pub made: u16,
    pub points: u16,
    pub assisted: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub turnovers: u16,
    /// Turnovers by kind, most common first.
    pub turnover_kinds: Vec<(TurnoverKind, u16)>,
    /// Turnovers by player, most first.
    pub turnover_players: Vec<(PlayerId, u16)>,
    pub breakdowns: u16,
    pub breakdown_players: Vec<(PlayerId, u16)>,
    /// Field goals made and tried, and how many of the makes were assisted.
    pub made: u16,
    pub attempts: u16,
    pub assisted: u16,
    pub points: u16,
    /// By source, most points first.
    pub sources: Vec<SourceLine>,
}

/// A game seen from one side.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Film {
    pub side: u8,
    /// Every clip, in the order they happen.
    pub clips: Vec<Clip>,
    pub summary: Summary,
}

impl Film {
    pub fn of<'a>(rec: &Recording, side: usize, name: impl Fn(PlayerId) -> &'a str) -> Film {
        let clips = clips(rec, side, name);
        Film {
            side: side as u8,
            summary: summary(rec, side),
            clips,
        }
    }

    /// Indices of the scoring clips, best attack first.
    pub fn best_looks(&self) -> Vec<usize> {
        best_looks(&self.clips)
    }

    pub fn turnovers(&self) -> Vec<usize> {
        self.pick(|k| matches!(k, ClipKind::Turnover(_)))
    }

    pub fn breakdowns(&self) -> Vec<usize> {
        self.pick(|k| matches!(k, ClipKind::Breakdown(_)))
    }

    fn pick(&self, f: impl Fn(&ClipKind) -> bool) -> Vec<usize> {
        (0..self.clips.len())
            .filter(|&i| f(&self.clips[i].kind))
            .collect()
    }
}

/// When the possession `side` has at `time` began.
pub fn possession_start(rec: &Recording, time: f32, side: u8) -> f32 {
    if rec.frames.is_empty() {
        return time;
    }
    let mut j = rec.frame_at(time);
    while j > 0 {
        let (a, b) = (&rec.frames[j - 1], &rec.frames[j]);
        if a.offense != side || a.period != b.period || a.shot_clock < b.shot_clock - 0.01 {
            break;
        }
        j -= 1;
    }
    rec.frames[j].time
}

/// How good a made shot looks as an attack: the offense working beats a
/// forced shot, a pass beats a lone effort, an open man beats a contested
/// one.
fn quality(source: Source, assisted: bool, contest: f32, points: u8) -> f32 {
    let source = match source {
        Source::Cut | Source::Roll => 3.0,
        Source::CatchAndShoot => 2.5,
        Source::Post => 1.5,
        Source::Transition => 1.5,
        Source::Drive => 1.0,
        Source::Putback => 0.5,
        Source::PullUp => 0.0,
        Source::Forced => -2.0,
    };
    let pass = if assisted { 3.0 } else { 0.0 };
    points as f32 * 0.5 + source + pass + (1.0 - contest.clamp(0.0, 1.0)) * 5.0
}

fn clip(rec: &Recording, e: &Event, kind: ClipKind, who: Option<PlayerId>, text: String) -> Clip {
    let duration = rec.duration();
    let start = possession_start(rec, e.time, e.offense).max(e.time - MAX_LEAD);
    Clip {
        start: start.clamp(0.0, duration),
        end: (e.time + TAIL).min(duration),
        kind,
        who,
        text,
        time: e.time,
        period: e.period,
        clock: e.clock,
        points: 0,
        quality: 0.0,
    }
}

/// The clips of one side (0 = home), in the order they happen.
///
/// Scores are the side's made field goals; turnovers are every turnover the
/// side committed; breakdowns are mistakes charged to the side's players
/// (a defender losing his man, or a player out of place in our own set) and
/// shot-clock violations. A defensive mistake is only a clip of ours when
/// the other side's possession is the one it happened in, and carries on
/// until their shot if they scored.
pub fn clips<'a>(rec: &Recording, side: usize, name: impl Fn(PlayerId) -> &'a str) -> Vec<Clip> {
    let side = side as u8;
    let mut out = Vec::new();
    for (i, e) in rec.events.iter().enumerate() {
        let text = |e: &Event| e.text(&name);
        match &e.kind {
            EventKind::Shot {
                who: shooter,
                kind,
                source,
                made: true,
                contest,
                assist,
                ..
            } if e.offense == side => {
                let points = kind.points() as u8;
                let assisted = assist.is_some();
                let mut c = clip(
                    rec,
                    e,
                    ClipKind::Score {
                        source: *source,
                        assisted,
                    },
                    Some(*shooter),
                    text(e),
                );
                c.points = points;
                c.quality = quality(*source, assisted, *contest, points);
                out.push(c);
            }
            EventKind::Turnover { who: w, kind, .. } if e.offense == side => {
                out.push(clip(rec, e, ClipKind::Turnover(*kind), Some(*w), text(e)));
                if *kind == TurnoverKind::ShotClock {
                    out.push(clip(
                        rec,
                        e,
                        ClipKind::Breakdown(Breakdown::ShotClock),
                        Some(*w),
                        "The clock ran out with no shot.".to_string(),
                    ));
                }
            }
            EventKind::Mistake {
                who: w,
                kind: Mistake::OutOfPosition,
                ..
            } if e.offense == side => {
                out.push(clip(
                    rec,
                    e,
                    ClipKind::Breakdown(Breakdown::Set),
                    Some(*w),
                    text(e),
                ));
            }
            EventKind::Mistake { who: w, kind, .. }
                if *kind != Mistake::OutOfPosition && e.offense != side =>
            {
                // Did they score on it? Their shot follows within a moment.
                let shot = rec.events[i + 1..]
                    .iter()
                    .take_while(|n| n.time <= e.time + 4.0)
                    .find(|n| n.offense == e.offense && matches!(n.kind, EventKind::Shot { .. }));
                let conceded =
                    shot.is_some_and(|n| matches!(n.kind, EventKind::Shot { made: true, .. }));
                let mut line = text(e);
                if conceded {
                    line.push_str(" They score.");
                }
                let mut c = clip(
                    rec,
                    e,
                    ClipKind::Breakdown(Breakdown::Defense {
                        mistake: *kind,
                        conceded,
                    }),
                    Some(*w),
                    line,
                );
                if let Some(n) = shot {
                    c.end = c.end.max((n.time + TAIL).min(rec.duration()));
                }
                out.push(c);
            }
            _ => {}
        }
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.time.total_cmp(&b.time)));
    out
}

/// Indices of the scoring clips, best attack first; ties go to the later one
/// in the game being listed last.
pub fn best_looks(clips: &[Clip]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..clips.len())
        .filter(|&i| matches!(clips[i].kind, ClipKind::Score { .. }))
        .collect();
    idx.sort_by(|&a, &b| {
        clips[b]
            .quality
            .total_cmp(&clips[a].quality)
            .then(clips[a].time.total_cmp(&clips[b].time))
    });
    idx
}

fn ranked<K: Ord + Copy>(m: BTreeMap<K, u16>) -> Vec<(K, u16)> {
    let mut v: Vec<(K, u16)> = m.into_iter().collect();
    // Stable, so equal counts keep the map's order.
    v.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    v
}

/// Counts for one side: turnovers by kind and player, breakdowns by player,
/// points by source.
pub fn summary(rec: &Recording, side: usize) -> Summary {
    let side = side as u8;
    let mut s = Summary::default();
    let mut kinds: BTreeMap<u8, (TurnoverKind, u16)> = BTreeMap::new();
    let mut tov_by: BTreeMap<PlayerId, u16> = BTreeMap::new();
    let mut brk_by: BTreeMap<PlayerId, u16> = BTreeMap::new();
    let mut sources: BTreeMap<u8, SourceLine> = BTreeMap::new();
    for e in &rec.events {
        match &e.kind {
            EventKind::Shot {
                who,
                kind,
                source,
                made,
                assist,
                ..
            } if e.offense == side => {
                let _ = who;
                let line = sources
                    .entry(Source::ALL.iter().position(|x| x == source).unwrap_or(0) as u8)
                    .or_insert(SourceLine {
                        source: *source,
                        attempts: 0,
                        made: 0,
                        points: 0,
                        assisted: 0,
                    });
                line.attempts += 1;
                s.attempts += 1;
                if *made {
                    let pts = kind_points(*kind);
                    line.made += 1;
                    line.points += pts;
                    s.made += 1;
                    s.points += pts;
                    if assist.is_some() {
                        line.assisted += 1;
                        s.assisted += 1;
                    }
                }
            }
            EventKind::Turnover { who, kind, .. } if e.offense == side => {
                s.turnovers += 1;
                kinds.entry(*kind as u8).or_insert((*kind, 0)).1 += 1;
                *tov_by.entry(*who).or_default() += 1;
                if *kind == TurnoverKind::ShotClock {
                    s.breakdowns += 1;
                    *brk_by.entry(*who).or_default() += 1;
                }
            }
            EventKind::Mistake { who, kind, .. }
                if (*kind == Mistake::OutOfPosition) == (e.offense == side) =>
            {
                s.breakdowns += 1;
                *brk_by.entry(*who).or_default() += 1;
            }
            _ => {}
        }
    }
    let mut kinds: Vec<(TurnoverKind, u16)> = kinds.into_values().collect();
    kinds.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    s.turnover_kinds = kinds;
    s.turnover_players = ranked(tov_by);
    s.breakdown_players = ranked(brk_by);
    let mut lines: Vec<SourceLine> = sources.into_values().collect();
    lines.sort_by_key(|l| std::cmp::Reverse(l.points));
    s.sources = lines;
    s
}

fn kind_points(kind: ShotKind) -> u16 {
    kind.points()
}

impl TurnoverKind {
    /// How a count of these reads in a sentence: "6 bad passes".
    pub fn plural(self) -> &'static str {
        match self {
            TurnoverKind::BadPass => "bad passes",
            TurnoverKind::Stripped => "stripped on the dribble",
            TurnoverKind::OffensiveFoul => "offensive fouls",
            TurnoverKind::ShotClock => "shot clock violations",
            TurnoverKind::Pressed => "lost to the press",
            TurnoverKind::ThreeSeconds => "three-second violations",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::league::{World, lineup};
    use crate::rng::Rng;

    fn names() -> Vec<String> {
        World::generate(9, 2)
            .players
            .iter()
            .map(|p| p.name.clone())
            .collect()
    }

    fn games() -> Vec<(crate::engine::GameResult, Recording)> {
        let w = World::generate(9, 2);
        let home = lineup(&w.teams[0], &w.players);
        let away = lineup(&w.teams[1], &w.players);
        (0..6)
            .map(|seed| {
                crate::engine::simulate_recorded(&home, &away, &w.rules, &mut Rng::new(seed))
            })
            .collect()
    }

    #[test]
    fn clips_cover_the_game_in_order() {
        let names = names();
        for (result, rec) in games() {
            let end = rec.duration();
            for (side, lines) in result.boxes.iter().enumerate() {
                let film = Film::of(&rec, side, |p| names[p as usize].as_str());
                assert!(!film.clips.is_empty());
                for c in &film.clips {
                    assert!(0.0 <= c.start && c.start <= c.time, "{c:?}");
                    assert!(c.time <= c.end && c.end <= end, "{c:?}");
                    assert!(c.time - c.start <= MAX_LEAD + 0.001, "{c:?}");
                    assert!(c.end - c.time <= 4.0 + TAIL + 0.001, "{c:?}");
                    assert!(!c.text.is_empty());
                }
                assert!(film.clips.windows(2).all(|c| c[0].start <= c[1].start));

                // Every turnover of the side has a clip, and they add up to
                // the box score.
                let logged = rec
                    .events
                    .iter()
                    .filter(|e| {
                        e.offense == side as u8 && matches!(e.kind, EventKind::Turnover { .. })
                    })
                    .count();
                let clipped = film.turnovers().len();
                let boxed: u16 = lines.iter().map(|(_, l)| l.tov).sum();
                assert_eq!(logged, clipped);
                assert_eq!(clipped as u16, boxed);
                let s = &film.summary;
                assert_eq!(s.turnovers, boxed);
                assert_eq!(s.turnover_kinds.iter().map(|k| k.1).sum::<u16>(), boxed);
                assert_eq!(s.turnover_players.iter().map(|k| k.1).sum::<u16>(), boxed);

                // Points and shots by source add up to the box score's field goals.
                let fgm: u16 = lines.iter().map(|(_, l)| l.fgm).sum();
                let fga: u16 = lines.iter().map(|(_, l)| l.fga).sum();
                assert_eq!(s.made, fgm);
                assert_eq!(s.sources.iter().map(|l| l.made).sum::<u16>(), fgm);
                assert_eq!(s.sources.iter().map(|l| l.points).sum::<u16>(), s.points);
                assert_eq!(
                    film.clips
                        .iter()
                        .filter(|c| matches!(c.kind, ClipKind::Score { .. }))
                        .count() as u16,
                    fgm
                );
                // Shots the engine charged as attempts are all in the log
                // (a missed shot that drew a foul is not an attempt).
                assert!(s.attempts >= fga);

                // Breakdowns of the film are the breakdowns of the summary.
                assert_eq!(film.breakdowns().len() as u16, s.breakdowns);
                assert_eq!(
                    s.breakdown_players.iter().map(|k| k.1).sum::<u16>(),
                    s.breakdowns
                );
            }
        }
    }

    #[test]
    fn best_looks_prefer_the_offense_working() {
        let names = names();
        let (_, rec) = games().remove(0);
        let film = Film::of(&rec, 0, |p| names[p as usize].as_str());
        let best = film.best_looks();
        assert_eq!(best.len(), film.summary.made as usize);
        assert!(
            best.windows(2)
                .all(|w| film.clips[w[0]].quality >= film.clips[w[1]].quality)
        );
        // Over several games the top of the list is assisted more often
        // than the bottom is.
        let (mut top, mut bottom, mut n) = (0, 0, 0);
        for (_, rec) in games() {
            let film = Film::of(&rec, 0, |p| names[p as usize].as_str());
            let best = film.best_looks();
            let assisted = |i: usize| {
                matches!(film.clips[i].kind, ClipKind::Score { assisted: true, .. }) as u32
            };
            let h = best.len() / 3;
            top += best[..h].iter().map(|&i| assisted(i)).sum::<u32>();
            bottom += best[best.len() - h..]
                .iter()
                .map(|&i| assisted(i))
                .sum::<u32>();
            n += h;
        }
        assert!(n > 0 && top > bottom, "{top} vs {bottom} of {n}");
    }

    #[test]
    fn a_possession_starts_before_its_event() {
        let (_, rec) = games().remove(1);
        let t = rec
            .events
            .iter()
            .find(|e| matches!(e.kind, EventKind::Shot { made: true, .. }))
            .unwrap();
        let start = possession_start(&rec, t.time, t.offense);
        assert!(start <= t.time && t.time - start < 30.0);
    }
}
