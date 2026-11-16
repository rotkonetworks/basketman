//! Match preparation: training against the next opponent's system.
//!
//! A club sets `Session::MatchPrep` sessions in its weekly plan. In the week
//! of a league game they are spent on the next opponent, and they help in
//! that one game, by as much as the scout's report on that club is right:
//!
//! - the **edge** is `sessions / 4` (at most one) times the share of the
//!   opponent's system the report names correctly: half for the offense,
//!   half for the defense. No report, or one that is wrong or out of date
//!   (the club changed its system since), gives no edge at all;
//! - the edge goes into the game as the squad's `belief` in the `Lineup`
//!   the engine is handed, `PREP_BELIEF` for a full edge, capped at 1.
//!   Belief is the engine's existing "plays above itself" input: it scales
//!   every effective attribute by `1 + 0.07 * (belief - 0.5)`. Nothing
//!   inside the engine was touched; both places that build lineups for a
//!   game (`simulate_batch` and `kickoff_scoped`) go through `lineups_for`.
//!
//! The cost is the general training those sessions would have given: a
//! preparation session builds no attribute and no tactic familiarity.
//!
//! Preparation (like travel and the clubs' changes of system) belongs to a
//! career: a manager in charge and the dated calendar. A league nobody
//! manages, or a bare one that plays its rounds on one day, is left as it
//! was, which keeps the lab's statistics and the headless runs what they
//! were.
//!
//! Clubs the manager does not run prepare too, in a simple way: two sessions
//! and a report as good as their chief scout is (a chance of 50% plus 2.5%
//! per point of judgement of reading each half of the opponent's system).

use crate::engine::Lineup;
use crate::league::{TeamId, World, lineup};
use crate::scouting::skew;
use crate::staff::Job;
use crate::training::Session;

/// Preparation sessions that make a full edge.
pub const FULL_SESSIONS: usize = 4;
/// Belief a full edge adds to the squad.
pub const PREP_BELIEF: f32 = 0.25;
/// Sessions a club the manager does not run puts in.
const AI_SESSIONS: usize = 2;

/// How well a club is prepared for one game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prep {
    /// Sessions that count: those before the game, not on a day lost to
    /// travel.
    pub sessions: usize,
    /// The report relied on (`OpponentReport::id`), for the manager's club.
    pub report: Option<u32>,
    /// What the report got right about the opponent's system today.
    pub offense_ok: bool,
    pub defense_ok: bool,
    /// 0..=1.
    pub edge: f32,
}

impl World {
    /// Do the calendar's extras (preparation, travel, clubs changing
    /// system) apply? Only with a manager in charge and a dated calendar.
    pub fn calendar_game(&self) -> bool {
        self.watch.is_some() && self.preseason_days > 0
    }

    /// Preparation sessions in `team`'s week before a game on `day`:
    /// Monday to Friday, the days up to the game, less any lost to travel.
    pub fn prep_sessions(&self, team: TeamId, day: u32) -> usize {
        let weekday = (day % 7) as usize;
        let last = weekday.min(5);
        let lost = self.travel_day_lost(team, day);
        self.teams[team]
            .schedule
            .sessions
            .iter()
            .enumerate()
            .filter(|&(slot, &s)| s == Session::MatchPrep && slot / 2 < last && Some(slot / 2) != lost)
            .count()
    }

    /// How well `team` is prepared for `opp` on `day`.
    pub fn prep_for(&self, team: TeamId, opp: TeamId, day: u32) -> Prep {
        let actual = self.teams[opp].tactics();
        if Some(team) == self.watch {
            let sessions = self.prep_sessions(team, day);
            let Some(r) = self.latest_report(opp) else {
                return Prep {
                    sessions,
                    report: None,
                    offense_ok: false,
                    defense_ok: false,
                    edge: 0.0,
                };
            };
            let (o, d) = (r.offense == Some(actual.offense), r.defense == Some(actual.defense));
            let known = 0.5 * (o as u8 as f32) + 0.5 * (d as u8 as f32);
            Prep {
                sessions,
                report: Some(r.id),
                offense_ok: o,
                defense_ok: d,
                edge: sessions.min(FULL_SESSIONS) as f32 / FULL_SESSIONS as f32 * known,
            }
        } else {
            let p = (0.5 + 0.025 * self.teams[team].office.rating(Job::Scout) as f32).min(0.95);
            let id = (team * self.teams.len() + opp) as u32;
            let salt = self.season as u32 * 100_000 + day * 2;
            let reads = |k: u32| (skew(id, salt + k) + 1.0) / 2.0 < p;
            let (o, d) = (reads(0), reads(1));
            let known = 0.5 * (o as u8 as f32) + 0.5 * (d as u8 as f32);
            Prep {
                sessions: AI_SESSIONS,
                report: None,
                offense_ok: o,
                defense_ok: d,
                edge: AI_SESSIONS as f32 / FULL_SESSIONS as f32 * known,
            }
        }
    }

    /// The two lineups of a game. `league_day` is the day of a league or
    /// playoff game, whose sides prepare for each other; friendlies and
    /// reserve games pass `None`.
    pub(crate) fn lineups_for(
        &self,
        home: TeamId,
        away: TeamId,
        league_day: Option<u32>,
    ) -> (Lineup<'_>, Lineup<'_>) {
        let mut h = lineup(&self.teams[home], &self.players);
        let mut a = lineup(&self.teams[away], &self.players);
        // Only in a managed career's calendar; see the module notes.
        if let Some(day) = league_day.filter(|_| self.calendar_game()) {
            let boost = |l: &mut Lineup, edge: f32| l.belief = (l.belief + PREP_BELIEF * edge).min(1.0);
            boost(&mut h, self.prep_for(home, away, day).edge);
            boost(&mut a, self.prep_for(away, home, day).edge);
        }
        (h, a)
    }

    /// A line for the match report about how our preparation went.
    pub(crate) fn prep_line(&self, me: TeamId, opp: TeamId, day: u32) -> Option<String> {
        let p = self.prep_for(me, opp, day);
        let n = p.sessions;
        let right = |p: &Prep| match (p.offense_ok, p.defense_ok) {
            (true, true) => "their offense and their defense",
            (true, false) => "their offense, not their defense",
            (false, true) => "their defense, not their offense",
            (false, false) => "nothing about how they set up",
        };
        let r = self.latest_report(opp);
        Some(match (r, n) {
            (None, 0) => return None,
            (None, n) => format!(
                "\nWe had {n} preparation session{} but no scout's report on them, so they \
                 did no good. Send the scout to one of their games.\n",
                if n == 1 { "" } else { "s" }
            ),
            (Some(r), 0) => format!(
                "\nWe had the scout's report from {} but did not set any preparation sessions.\n",
                self.day_label(r.day)
            ),
            (Some(r), n) => format!(
                "\nPreparation: {n} session{} against the report from {}, which was right about \
                 {}. The edge it gave us: {:.0}% of the most it can.\n",
                if n == 1 { "" } else { "s" },
                self.day_label(r.day),
                right(&p),
                p.edge * 100.0,
            ),
        })
    }
}
