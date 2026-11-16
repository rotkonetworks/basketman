//! The assistant coach: reads the team's numbers against the league and
//! proposes a training week and individual plans. The user can take the
//! proposal as a starting point or leave training delegated.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::league::{TeamId, TeamStats, World};
use crate::player::{Attr, Player, Position};
use crate::training::{Intensity, Preset, Schedule, Session, TrainingFocus};

/// Why the assistant changed the default week. Percentages are 0..=100.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Reason {
    TooEarly,
    NothingStandsOut,
    PoorThreePoint {
        team: f32,
        league: f32,
    },
    PoorFinishing {
        team: f32,
        league: f32,
    },
    Turnovers {
        team: f32,
        league: f32,
    },
    OutRebounded {
        team: f32,
        league: f32,
    },
    PerimeterLeaks {
        allowed: f32,
        league: f32,
    },
    InteriorLeaks {
        allowed: f32,
        league: f32,
    },
    /// The rotation's feet are well below the league's: footwork is
    /// attribute points, so 1..=20.
    SlowFeet {
        team: f32,
        league: f32,
    },
    Tired {
        fitness: f32,
    },
    LearningTactic {
        familiarity: f32,
    },
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reason::TooEarly => write!(
                f,
                "Too few games to read anything into the numbers, so a balanced week."
            ),
            Reason::NothingStandsOut => {
                write!(f, "No clear weakness in the numbers, so a balanced week.")
            }
            Reason::PoorThreePoint { team, league } => {
                write!(
                    f,
                    "We shoot {team:.1}% from three against a league {league:.1}%: more shooting."
                )
            }
            Reason::PoorFinishing { team, league } => {
                write!(
                    f,
                    "We make {team:.1}% of our twos against a league {league:.1}%: more finishing work."
                )
            }
            Reason::Turnovers { team, league } => {
                write!(
                    f,
                    "We turn it over {team:.1} times a game, the league {league:.1}: more playmaking."
                )
            }
            Reason::OutRebounded { team, league } => {
                write!(
                    f,
                    "We get {team:.1} rebounds a game, the league {league:.1}: more rebounding."
                )
            }
            Reason::PerimeterLeaks { allowed, league } => {
                write!(
                    f,
                    "Opponents hit {allowed:.1}% of their threes on us, league {league:.1}%: perimeter defense."
                )
            }
            Reason::InteriorLeaks { allowed, league } => {
                write!(
                    f,
                    "Opponents make {allowed:.1}% of their twos on us, league {league:.1}%: interior defense."
                )
            }
            Reason::SlowFeet { team, league } => {
                write!(
                    f,
                    "Our rotation's footwork averages {team:.1} against a league {league:.1}: more perimeter defense work, which trains the feet."
                )
            }
            Reason::Tired { fitness } => {
                write!(
                    f,
                    "The rotation is at {fitness:.0}% fitness: conditioning swapped for recovery."
                )
            }
            Reason::LearningTactic { familiarity } => {
                write!(
                    f,
                    "The match tactic is only {familiarity:.0}% familiar: an extra tactics session."
                )
            }
        }
    }
}

pub struct Advice {
    pub schedule: Schedule,
    pub reasons: Vec<Reason>,
}

fn pct(made: u16, attempts: u16) -> f32 {
    if attempts == 0 {
        0.0
    } else {
        100.0 * made as f32 / attempts as f32
    }
}

fn two_point_pct(s: &crate::engine::BoxLine) -> f32 {
    pct(s.fgm - s.tpm, s.fga - s.tpa)
}

/// Games needed before the stats are worth acting on.
const MIN_GAMES: u16 = 3;

pub fn plan_schedule(world: &World, team: TeamId) -> Advice {
    let mine = &world.team_stats[team];
    let league = world.league_totals();
    let mut schedule = Schedule::preset(Preset::Balanced);
    let mut reasons = Vec::new();

    // (severity, session, reason): severity 1.0 is "worth one session".
    let mut needs: Vec<(f32, Session, Reason)> = Vec::new();
    if mine.games >= MIN_GAMES && league.games > 0 {
        let per_game = |s: &TeamStats, v: u16| v as f32 / s.games.max(1) as f32;
        let (t3, l3) = (
            pct(mine.own.tpm, mine.own.tpa),
            pct(league.own.tpm, league.own.tpa),
        );
        needs.push((
            (l3 - t3) / 2.0,
            Session::Shooting,
            Reason::PoorThreePoint {
                team: t3,
                league: l3,
            },
        ));
        let (t2, l2) = (two_point_pct(&mine.own), two_point_pct(&league.own));
        needs.push((
            (l2 - t2) / 2.0,
            Session::Finishing,
            Reason::PoorFinishing {
                team: t2,
                league: l2,
            },
        ));
        let (tt, lt) = (
            per_game(mine, mine.own.tov),
            per_game(&league, league.own.tov),
        );
        needs.push((
            (tt - lt) / 1.0,
            Session::Playmaking,
            Reason::Turnovers {
                team: tt,
                league: lt,
            },
        ));
        let (tr, lr) = (
            per_game(mine, mine.own.reb()),
            per_game(&league, league.own.reb()),
        );
        needs.push((
            (lr - tr) / 1.5,
            Session::Rebounding,
            Reason::OutRebounded {
                team: tr,
                league: lr,
            },
        ));
        let o3 = pct(mine.opp.tpm, mine.opp.tpa);
        needs.push((
            (o3 - l3) / 2.0,
            Session::PerimeterDefense,
            Reason::PerimeterLeaks {
                allowed: o3,
                league: l3,
            },
        ));
        let o2 = two_point_pct(&mine.opp);
        needs.push((
            (o2 - l2) / 2.0,
            Session::InteriorDefense,
            Reason::InteriorLeaks {
                allowed: o2,
                league: l2,
            },
        ));
        // Footwork: the rotation's feet against everyone's. A point short
        // is worth one session.
        let rotation = &world.teams[team].tactics().rotation;
        let feet = |ids: &mut dyn Iterator<Item = &crate::player::PlayerId>| {
            let (sum, n) = ids.fold((0.0, 0u32), |(s, n), &p| {
                (s + world.players[p as usize].attrs[Attr::Footwork], n + 1)
            });
            sum / n.max(1) as f32
        };
        let ours = feet(&mut rotation.iter().map(|s| &s.player));
        let all = feet(&mut world.teams.iter().flat_map(|t| t.roster.iter()));
        needs.push((
            all - ours,
            Session::PerimeterDefense,
            Reason::SlowFeet {
                team: ours,
                league: all,
            },
        ));
        needs.retain(|n| n.0 >= 1.0);
        needs.sort_by(|a, b| b.0.total_cmp(&a.0));
        if needs.is_empty() {
            reasons.push(Reason::NothingStandsOut);
        }
    } else {
        reasons.push(Reason::TooEarly);
    }

    // The four skill slots of the balanced week are the ones to reassign.
    const FLEX: [usize; 4] = [0, 2, 3, 6];
    let mut flex = FLEX.iter();
    'fill: for (severity, session, reason) in needs {
        for _ in 0..if severity >= 2.0 { 2 } else { 1 } {
            let Some(&slot) = flex.next() else {
                break 'fill;
            };
            schedule.sessions[slot] = session;
        }
        reasons.push(reason);
    }

    let t = &world.teams[team];
    let rotation = &t.playbook[t.active].tactics.rotation;
    let fitness = rotation
        .iter()
        .map(|s| world.players[s.player as usize].condition)
        .sum::<f32>()
        / rotation.len().max(1) as f32;
    if fitness < 0.90 {
        schedule.sessions[4] = Session::Recovery;
        if fitness < 0.82 {
            schedule.sessions[8] = Session::Rest;
        }
        reasons.push(Reason::Tired {
            fitness: fitness * 100.0,
        });
    }
    let familiarity = t.playbook[t.active].familiarity();
    if familiarity < 0.85 {
        schedule.sessions[9] = Session::Tactics;
        reasons.push(Reason::LearningTactic {
            familiarity: familiarity * 100.0,
        });
    }

    Advice { schedule, reasons }
}

/// Individual plan: veterans hold on to their bodies, everyone else works
/// on the weakest part of the job their position asks of them.
pub fn plan_individual(p: &Player) -> (TrainingFocus, Intensity) {
    use TrainingFocus::*;
    let relevant: &[TrainingFocus] = match p.position {
        Position::PG | Position::SG => &[Shooting, Playmaking, PerimeterDefense],
        Position::SF => &[Shooting, Finishing, PerimeterDefense, Rebounding],
        Position::PF | Position::C => &[Finishing, Rebounding, InteriorDefense],
    };
    let level = |f: &TrainingFocus| {
        let attrs = f.attrs();
        attrs.iter().map(|&a| p.attrs[a]).sum::<f32>() / attrs.len() as f32
    };
    let focus = if p.age >= 31 || p.attrs[Attr::Stamina] < 7.0 {
        Conditioning
    } else {
        *relevant
            .iter()
            .min_by(|a, b| level(a).total_cmp(&level(b)))
            .unwrap_or(&Balanced)
    };
    let intensity = if p.condition < 0.85 {
        Intensity::Light
    } else if p.age <= 24 && p.personality.professionalism >= 12 && p.condition >= 0.95 {
        Intensity::Heavy
    } else {
        Intensity::Normal
    };
    (focus, intensity)
}

/// Let the assistant set the team's week and every player's plan.
pub fn run(world: &mut World, team: TeamId) {
    let advice = plan_schedule(world, team);
    for pid in world.teams[team].roster.clone() {
        let p = &mut world.players[pid as usize];
        (p.training, p.intensity) = plan_individual(p);
    }
    let t = &mut world.teams[team];
    let old = t.schedule.clone();
    t.schedule = advice.schedule;
    // Preparation for the next game is the manager's call: keep it.
    for (slot, s) in old.sessions.iter().enumerate() {
        if *s == Session::MatchPrep {
            t.schedule.sessions[slot] = Session::MatchPrep;
        }
    }
    t.assistant_notes = advice.reasons;
}

/// How one way of playing did in the assistant's rehearsal.
#[derive(Clone, Debug)]
pub struct Trial {
    pub label: &'static str,
    /// Share of the rehearsal games won, 0..=1, and mean margin.
    pub wins: f32,
    pub margin: f32,
    /// How well the squad knows the system today, 0..=1.
    pub familiarity: f32,
    /// This is what the active tactic already does.
    pub current: bool,
}

/// The next opponent played many times over on paper.
#[derive(Clone, Debug)]
pub struct Rehearsal {
    pub opponent: TeamId,
    pub round: usize,
    pub games: u32,
    pub offense: Vec<Trial>,
    pub defense: Vec<Trial>,
}

/// Play the next game `games` times with each offense and each defense the
/// user could send out, everything else as the active tactic has it, at the
/// familiarity the squad would really have. The opponent plays what it
/// played last and adjusts as its coach would. Does not touch the world.
pub fn rehearse(world: &World, team: TeamId, games: u32) -> Option<Rehearsal> {
    use crate::engine::{Lineup, simulate_game};
    use crate::league::lineup;
    use crate::tactics::{DefenseScheme, OffenseSet};

    let f = world
        .fixtures
        .iter()
        .find(|f| f.result.is_none() && (f.home == team || f.away == team))?;
    let home = f.home == team;
    let opponent = if home { f.away } else { f.home };
    let mine = &world.teams[team];
    let base = mine.tactics().clone();
    let other = lineup(&world.teams[opponent], &world.players);
    let trial = |label, offense: OffenseSet, defense: DefenseScheme| {
        let mut t = base.clone();
        (t.offense, t.defense) = (offense, defense);
        let familiarity = mine.familiarity_with(offense, defense, t.press);
        let us = Lineup {
            tactics: &t,
            players: t
                .rotation
                .iter()
                .map(|s| &world.players[s.player as usize])
                .collect(),
            familiarity,
            belief: mine.belief,
            coach: 0,
        };
        let (mut wins, mut margin) = (0u32, 0i32);
        for g in 0..games {
            let rng = &mut crate::rng::Rng::new(0x5EED_0000 + g as u64);
            let r = if home {
                simulate_game(&us, &other, &world.rules, rng)
            } else {
                simulate_game(&other, &us, &world.rules, rng)
            };
            let side = if home { 0 } else { 1 };
            let d = r.score[side] as i32 - r.score[1 - side] as i32;
            wins += (d > 0) as u32;
            margin += d;
        }
        Trial {
            label,
            wins: wins as f32 / games.max(1) as f32,
            margin: margin as f32 / games.max(1) as f32,
            familiarity,
            current: offense == base.offense && defense == base.defense,
        }
    };
    Some(Rehearsal {
        opponent,
        round: f.round,
        games,
        offense: OffenseSet::ALL
            .into_iter()
            .map(|o| trial(o.label(), o, base.defense))
            .collect(),
        defense: DefenseScheme::ALL
            .into_iter()
            .map(|d| trial(d.label(), base.offense, d))
            .collect(),
    })
}

#[cfg(test)]
mod rehearsal {
    use super::*;

    /// How much the choice of system is worth against one opponent.
    #[test]
    fn the_choice_of_system_moves_the_result() {
        let mut w = World::generate(3, 8);
        w.take_charge(0);
        let r = rehearse(&w, 0, 40).unwrap();
        let spread = |t: &[Trial]| {
            let (lo, hi) = t
                .iter()
                .fold((1.0f32, 0.0f32), |(a, b), x| (a.min(x.wins), b.max(x.wins)));
            hi - lo
        };
        for t in r.offense.iter().chain(&r.defense) {
            eprintln!(
                "{:18} {:>3.0}% {:+.1} fam {:.0}%{}",
                t.label,
                t.wins * 100.0,
                t.margin,
                t.familiarity * 100.0,
                if t.current { " (current)" } else { "" }
            );
        }
        assert!(r.offense.iter().filter(|t| t.current).count() == 1);
        assert!(spread(&r.offense) + spread(&r.defense) > 0.1);
    }
}
