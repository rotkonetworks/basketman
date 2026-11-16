//! A short scrimmage: our five against an opponent's, played by the real
//! match engine for a handful of possessions, so a coach can see what a
//! system does with his own players before he trusts it in a game.
//!
//! It is a game of its own. It reads the world, never writes it, and uses a
//! random stream seeded from `Setup::seed`, so the world's own stream is
//! untouched and the same setup always plays out the same. Both benches are
//! told to hold their systems (no coach on the bench to change them).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::court::{CORNER_X, LENGTH, ShotKind, Vec2, v};
use crate::engine::{Lineup, LiveMatch};
use crate::league::{TeamId, World, lineup};
use crate::player::PlayerId;
use crate::replay::{Event, EventKind, Source, TurnoverKind};
use crate::rng::Rng;
use crate::tactics::{DefenseScheme, OffenseSet, Tactics};

/// Which end of the floor is being watched.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Focus {
    /// Our offense against their defense.
    Attack,
    /// Their offense against our defense.
    Defend,
}

/// Everything a scrimmage depends on.
#[derive(Clone, Debug)]
pub struct Setup {
    pub team: TeamId,
    /// What we play: the slot being edited, with the drawn play if any.
    pub tactics: Tactics,
    /// How well the squad knows it, 0..=1.
    pub familiarity: f32,
    pub opponent: TeamId,
    /// Replace what the opponent plays, to test against a sample.
    pub their_offense: Option<OffenseSet>,
    pub their_defense: Option<DefenseScheme>,
    pub focus: Focus,
    /// Possessions of the watched side to play.
    pub possessions: usize,
    pub seed: u64,
}

impl Setup {
    /// The playbook slot `slot` against the next opponent, or a sample club
    /// when there is no game coming.
    pub fn for_slot(world: &World, team: TeamId, slot: usize, focus: Focus) -> Setup {
        let slot = slot.min(world.teams[team].playbook.len() - 1);
        let s = &world.teams[team].playbook[slot];
        Setup {
            team,
            tactics: s.tactics.clone(),
            familiarity: s.familiarity(),
            opponent: crate::fit::next_opponent(world, team)
                .unwrap_or_else(|| crate::fit::sample_opponent(world, team)),
            their_offense: None,
            their_defense: None,
            focus,
            possessions: 6,
            seed: 0x5C41_0000 + slot as u64,
        }
    }

    /// The same, but with this system and the familiarity the slot would
    /// really have in it.
    pub fn with_system(
        mut self,
        world: &World,
        slot: usize,
        offense: OffenseSet,
        defense: DefenseScheme,
    ) -> Setup {
        self.tactics.offense = offense;
        self.tactics.defense = defense;
        self.familiarity =
            world.teams[self.team].playbook[slot].familiarity_with(offense, defense, self.tactics.press);
        self
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Scored,
    Missed,
    Turnover,
    /// Fouled and sent to the line without a basket.
    Fouled,
}

/// One tenth of a second of a possession, in the attack frame of the side
/// with the ball: the hoop at `court::HOOP`, x across, y up the court.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Moment {
    pub t: f32,
    pub attackers: [Vec2; 5],
    pub defenders: [Vec2; 5],
    pub ball: Vec2,
    /// Which attacker has the ball, or none while it is in the air.
    pub holder: Option<usize>,
    pub ball_height: f32,
    pub shot_clock: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Possession {
    /// We have the ball (else they do and we defend).
    pub we_attack: bool,
    pub attackers: [PlayerId; 5],
    pub defenders: [PlayerId; 5],
    pub moments: Vec<Moment>,
    pub points: u8,
    pub outcome: Outcome,
    pub shooter: Option<PlayerId>,
    /// What happened and why, in plain words.
    pub caption: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scrimmage {
    pub focus: Focus,
    pub opponent: TeamId,
    pub possessions: Vec<Possession>,
}

impl Scrimmage {
    pub fn points(&self) -> u32 {
        self.possessions.iter().map(|p| p.points as u32).sum()
    }

    /// Points per possession of the watched side.
    pub fn ppp(&self) -> f32 {
        if self.possessions.is_empty() {
            0.0
        } else {
            self.points() as f32 / self.possessions.len() as f32
        }
    }

    pub fn turnovers(&self) -> usize {
        self.possessions
            .iter()
            .filter(|p| p.outcome == Outcome::Turnover)
            .count()
    }
}

/// A point on the full court, in the attack frame of the side `offense`
/// (the inverse of `replay::to_court`).
pub fn from_court(p: Vec2, offense: usize) -> Vec2 {
    if offense == 0 {
        v(p.y - 7.5, LENGTH - p.x)
    } else {
        v(7.5 - p.y, p.x)
    }
}

/// Surnames, or initial and surname where two of the ten share one.
fn short_names(world: &World, ids: &[PlayerId]) -> BTreeMap<PlayerId, String> {
    let last = |id: PlayerId| {
        let n = &world.players[id as usize].name;
        n.rsplit(' ').next().unwrap_or(n).to_string()
    };
    ids.iter()
        .map(|&id| {
            let l = last(id);
            let twin = ids.iter().any(|&o| o != id && last(o) == l);
            let n = &world.players[id as usize].name;
            let same = ids
                .iter()
                .any(|&o| o != id && world.players[o as usize].name == *n);
            let s = if same {
                format!(
                    "{}. {l} ({})",
                    n.chars().next().unwrap_or(' '),
                    world.players[id as usize].position.label()
                )
            } else if twin {
                format!("{}. {l}", n.chars().next().unwrap_or(' '))
            } else {
                l
            };
            (id, s)
        })
        .collect()
}

/// Play the scrimmage.
pub fn run(world: &World, s: &Setup) -> Scrimmage {
    let me = &world.teams[s.team];
    let mine = Lineup {
        tactics: &s.tactics,
        players: s
            .tactics
            .rotation
            .iter()
            .map(|r| &world.players[r.player as usize])
            .collect(),
        familiarity: s.familiarity,
        belief: me.belief,
        coach: 0,
    };
    let mut their_tactics = world.teams[s.opponent].tactics().clone();
    if let Some(o) = s.their_offense {
        their_tactics.offense = o;
        their_tactics.board = None;
    }
    if let Some(d) = s.their_defense {
        their_tactics.defense = d;
    }
    let mut theirs = lineup(&world.teams[s.opponent], &world.players);
    theirs.tactics = &their_tactics;
    theirs.coach = 0;

    // We are the home side, index 0.
    let watched: u8 = if s.focus == Focus::Attack { 0 } else { 1 };
    let mut game = LiveMatch::new(&mine, &theirs, &world.rules, Rng::new(s.seed), None);
    let (mut seen, mut done, mut cur, mut steps) = (0usize, 0usize, None::<u8>, 0u32);
    while !game.is_over() && steps < 60_000 && done < s.possessions {
        game.step();
        steps += 1;
        let frames = &game.recording().frames;
        for f in &frames[seen..] {
            if cur != Some(f.offense) {
                if cur == Some(watched) {
                    done += 1;
                }
                cur = Some(f.offense);
            }
        }
        seen = frames.len();
    }
    let (result, rec) = game.finish();
    let ids = |side: usize, who: &[u8; 5]| {
        let mut out = [0 as PlayerId; 5];
        for (k, &w) in who.iter().enumerate() {
            out[k] = result.boxes[side][w as usize].0;
        }
        out
    };
    let mut all: Vec<PlayerId> = Vec::new();
    if let Some(f) = rec.frames.first() {
        all.extend(ids(0, &f.who[0]));
        all.extend(ids(1, &f.who[1]));
    }
    let names = short_names(world, &all);

    // Cut the frames into possessions: runs of the same side with the ball.
    let mut out = Vec::new();
    let mut i = 0;
    while i < rec.frames.len() && out.len() < s.possessions {
        let side = rec.frames[i].offense;
        let mut j = i;
        while j < rec.frames.len() && rec.frames[j].offense == side {
            j += 1;
        }
        // The run still going when we stopped is not a possession.
        if side == watched && j < rec.frames.len() {
            let slice = &rec.frames[i..j];
            let (att, def) = (side as usize, 1 - side as usize);
            let first = &slice[0];
            let attackers = ids(att, &first.who[att]);
            let defenders = ids(def, &first.who[def]);
            let moments: Vec<Moment> = slice
                .iter()
                .map(|f| {
                    let conv = |p: Vec2| from_court(p, att);
                    let mut a = [Vec2::ZERO; 5];
                    let mut d = [Vec2::ZERO; 5];
                    for k in 0..5 {
                        a[k] = conv(f.pos[att][k]);
                        d[k] = conv(f.pos[def][k]);
                    }
                    Moment {
                        t: f.time,
                        attackers: a,
                        defenders: d,
                        ball: conv(f.ball),
                        holder: match f.holder {
                            Some((sd, k)) if sd as usize == att => Some(k as usize),
                            _ => None,
                        },
                        ball_height: f.ball_height,
                        shot_clock: f.shot_clock,
                    }
                })
                .collect();
            let t0 = slice[0].time;
            let t1 = slice[slice.len() - 1].time + 0.3;
            let events: Vec<&Event> = rec
                .events
                .iter()
                .filter(|e| e.time >= t0 && e.time <= t1)
                .collect();
            let points = {
                let before = first.score[att];
                let after = rec.frames[j].score[att];
                after.saturating_sub(before).min(4) as u8
            };
            let we_attack = s.focus == Focus::Attack;
            let scheme = if we_attack {
                their_tactics.defense
            } else {
                s.tactics.defense
            };
            let (caption, outcome, shooter) = describe(
                &events,
                &moments,
                we_attack,
                scheme,
                points,
                &|id| names.get(&id).map_or("somebody", |s| s.as_str()),
            );
            out.push(Possession {
                we_attack,
                attackers,
                defenders,
                moments,
                points,
                outcome,
                shooter,
                caption,
            });
        }
        i = j;
    }
    Scrimmage {
        focus: s.focus,
        opponent: s.opponent,
        possessions: out,
    }
}

/// What a possession was, from the events in it: the result in the
/// engine's own words, then the reason when the engine recorded one.
fn describe<'a>(
    events: &[&Event],
    moments: &[Moment],
    we_attack: bool,
    scheme: DefenseScheme,
    points: u8,
    name: &dyn Fn(PlayerId) -> &'a str,
) -> (String, Outcome, Option<PlayerId>) {
    let shot = events.iter().rev().find(|e| matches!(e.kind, EventKind::Shot { .. }));
    let tov = events
        .iter()
        .find(|e| matches!(e.kind, EventKind::Turnover { .. }));
    let foul = events
        .iter()
        .find(|e| matches!(e.kind, EventKind::Foul { shots, .. } if shots > 0));
    let owner = if we_attack { "Their" } else { "Our" };
    let zone = matches!(scheme, DefenseScheme::Zone23 | DefenseScheme::Zone32);

    let Some(shot) = shot else {
        if let Some(t) = tov {
            let mut text = t.text(name);
            if let EventKind::Turnover {
                kind: TurnoverKind::Pressed,
                ..
            } = t.kind
            {
                text.push_str(" The press did it.");
            }
            return (text, Outcome::Turnover, t.actor());
        }
        if let Some(f) = foul {
            return (f.text(name), Outcome::Fouled, f.actor());
        }
        return ("The possession ended.".into(), Outcome::Missed, None);
    };
    let EventKind::Shot {
        who,
        kind,
        source,
        made,
        contest,
        defender,
        assist,
        ..
    } = &shot.kind
    else {
        unreachable!()
    };
    let mut text = shot.text(name);
    let why: Option<String>;

    // The engine's own record of a breakdown by the defense.
    let mistake = events.iter().find(|e| {
        matches!(e.kind, EventKind::Mistake { beneficiary, kind, .. }
            if beneficiary == *who && kind != crate::replay::Mistake::OutOfPosition)
    });
    // A screen set in the possession, for the man who shot or the ball he got.
    let screen = events.iter().find_map(|e| match e.kind {
        EventKind::Screen { screener, handler } => Some((screener, handler)),
        _ => None,
    });
    let corner = moments
        .iter()
        .min_by(|a, b| {
            (a.t - shot.time)
                .abs()
                .total_cmp(&(b.t - shot.time).abs())
        })
        .is_some_and(|m| m.ball.x.abs() >= CORNER_X - 0.6 && m.ball.y <= 4.0);
    if let Some(m) = mistake {
        why = Some(m.text(name));
    } else if zone && *kind == ShotKind::Three && *contest < 0.3 {
        why = Some(format!(
            "{owner} {} left the {} open.",
            scheme.label(),
            if corner { "corner" } else { "arc" }
        ));
    } else if let Some((screener, handler)) = screen.filter(|_| *contest < 0.4) {
        why = Some(if handler == *who {
            format!("{}'s screen freed {} for it.", name(screener), name(*who))
        } else {
            format!(
                "{} screened for {}, and the ball found {}.",
                name(screener),
                name(handler),
                name(*who)
            )
        });
    } else {
        why = match source {
            Source::Roll => Some(format!("{} rolled to the rim after the screen.", name(*who))),
            Source::Cut => Some(format!("{} cut to the basket.", name(*who))),
            Source::Post => Some(format!("{} worked in the post.", name(*who))),
            Source::Drive => defender.map(|d| format!("{} drove at {}.", name(*who), name(d))),
            Source::Transition => Some("It came off the break.".to_string()),
            Source::Putback => Some("It came off the offensive rebound.".to_string()),
            Source::Forced => Some("The shot clock was almost gone.".to_string()),
            Source::CatchAndShoot if assist.is_some() => {
                Some(format!("{} found him in rhythm.", name(assist.unwrap())))
            }
            Source::CatchAndShoot | Source::PullUp => None,
        };
    }
    let why = why.or_else(|| {
        (!*made && *contest < 0.25 && !matches!(shot.kind, EventKind::Shot { blocked_by: Some(_), .. }))
            .then(|| "It was a good look that did not fall.".to_string())
    });
    if let Some(w) = why {
        text.push(' ');
        text.push_str(&w);
    }
    let outcome = if *made || points > 0 {
        Outcome::Scored
    } else if foul.is_some() {
        Outcome::Fouled
    } else {
        Outcome::Missed
    };
    (text, outcome, Some(*who))
}

/// One row of a comparison: a system and what it produced over the same
/// possessions.
#[derive(Clone, Debug, Serialize)]
pub struct Row {
    pub label: &'static str,
    pub offense: OffenseSet,
    pub defense: DefenseScheme,
    pub possessions: usize,
    pub points: u32,
    pub turnovers: usize,
    pub familiarity: f32,
    /// The system now in use.
    pub current: bool,
}

impl Row {
    pub fn ppp(&self) -> f32 {
        if self.possessions == 0 {
            0.0
        } else {
            self.points as f32 / self.possessions as f32
        }
    }
}

/// How each offense (when we attack) or each defense (when we defend) does
/// over the same seeds, everything else as the tactic has it.
pub fn compare(world: &World, base: &Setup, slot: usize, seeds: u32) -> Vec<Row> {
    let play = |o: OffenseSet, d: DefenseScheme, label: &'static str| {
        let (mut poss, mut pts, mut tov) = (0, 0, 0);
        let mut fam = 0.0;
        for k in 0..seeds {
            let mut s = base.clone().with_system(world, slot, o, d);
            s.seed = base.seed ^ (0x9E37 * (k as u64 + 1));
            fam = s.familiarity;
            let r = run(world, &s);
            poss += r.possessions.len();
            pts += r.points();
            tov += r.turnovers();
        }
        Row {
            label,
            offense: o,
            defense: d,
            possessions: poss,
            points: pts,
            turnovers: tov,
            familiarity: fam,
            current: o == base.tactics.offense && d == base.tactics.defense,
        }
    };
    match base.focus {
        Focus::Attack => OffenseSet::ALL
            .into_iter()
            .map(|o| play(o, base.tactics.defense, o.label()))
            .collect(),
        Focus::Defend => DefenseScheme::ALL
            .into_iter()
            .map(|d| play(base.tactics.offense, d, d.label()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = World::generate(3, 8);
        w.take_charge(0);
        w
    }

    #[test]
    fn it_is_deterministic_and_leaves_the_world_alone() {
        let w = world();
        let before = serde_json::to_vec(&w).unwrap();
        let s = Setup::for_slot(&w, 0, 0, Focus::Attack);
        let a = run(&w, &s);
        let b = run(&w, &s);
        assert_eq!(a.possessions.len(), s.possessions);
        assert_eq!(a.points(), b.points());
        for (x, y) in a.possessions.iter().zip(&b.possessions) {
            assert_eq!(x.caption, y.caption);
            assert_eq!(x.moments.len(), y.moments.len());
            assert_eq!(x.moments[0].ball, y.moments[0].ball);
        }
        // The world, its random stream included, is byte for byte the same.
        assert_eq!(before, serde_json::to_vec(&w).unwrap());
        for p in &a.possessions {
            assert!(!p.caption.is_empty());
            assert!(p.moments.len() > 10);
            assert!(p.attackers.iter().all(|id| w.teams[0].roster.contains(id)));
        }
    }

    #[test]
    fn it_watches_either_end() {
        let w = world();
        let d = run(&w, &Setup::for_slot(&w, 0, 0, Focus::Defend));
        assert!(!d.possessions.is_empty());
        assert!(d.possessions.iter().all(|p| !p.we_attack));
        assert!(
            d.possessions
                .iter()
                .all(|p| p.defenders.iter().all(|id| w.teams[0].roster.contains(id)))
        );
    }

    #[test]
    fn the_drawn_play_is_tested() {
        let w = world();
        let mut s = Setup::for_slot(&w, 0, 0, Focus::Attack);
        let plain = run(&w, &s);
        let mut b = s.tactics.layout();
        b.spots[0] = v(-6.0, 3.0);
        b.runs[1] = Some(v(0.0, 2.0));
        s.tactics.draw(b);
        let drawn = run(&w, &s);
        let track = |r: &Scrimmage| -> Vec<Vec2> {
            r.possessions[0].moments.iter().map(|m| m.attackers[0]).collect()
        };
        assert_ne!(track(&plain), track(&drawn));
    }

    #[test]
    fn a_comparison_covers_every_system() {
        let w = world();
        let rows = compare(&w, &Setup::for_slot(&w, 0, 0, Focus::Attack), 0, 2);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows.iter().filter(|r| r.current).count(), 1);
        assert!(rows.iter().all(|r| r.possessions > 0));
    }
}

