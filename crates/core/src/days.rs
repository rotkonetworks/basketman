//! Time passing, a day at a time.
//!
//! The manager stands at the morning of `World::day`. Whatever is scheduled
//! for the day happens when the day is passed: other clubs' games, reserve
//! games, preseason friendlies, the camp's weekly work. His own league game
//! is the one thing that does not happen by itself: time stops until he
//! plays it (`PlayRound`, `PlayToday` or a live game).
//!
//! `advance` passes days until something needs him; `continue_day` passes
//! one.

use crate::calendar::{CalKind, RESERVE_DAY};
use crate::inbox::{Kind, Stop};
use crate::league::{TeamId, World};
use crate::player::PlayerId;

/// How much a friendly rehearses the system the squad plays.
const FRIENDLY_FAMILIARITY: f32 = 0.015;
/// A friendly takes less out of the legs than a league game.
const FRIENDLY_LOAD: f32 = 0.6;

impl World {
    /// The user's game still to play this round.
    pub fn user_fixture(&self) -> Option<usize> {
        let me = self.watch?;
        (0..self.fixtures.len()).find(|&i| {
            let f = &self.fixtures[i];
            f.round == self.round && f.result.is_none() && (f.home == me || f.away == me)
        })
    }

    /// Day of the manager's next game; with none this round, of the next
    /// game anywhere, or the round's Saturday.
    pub fn match_day(&self) -> u32 {
        if let Some(i) = self.user_fixture() {
            self.fixtures[i].day
        } else {
            self.next_game_day()
                .unwrap_or(self.round_base(self.round) + 5)
        }
    }

    /// True if there is a game for the manager to play today (or one that
    /// should have been). With nobody in charge, any game due.
    pub fn match_today(&self) -> bool {
        if self.season_over() {
            return false;
        }
        match self.watch {
            Some(_) => self
                .user_fixture()
                .is_some_and(|i| self.fixtures[i].day <= self.day),
            None => self
                .fixtures
                .iter()
                .any(|f| f.round == self.round && f.result.is_none() && f.day <= self.day),
        }
    }

    /// Play the games up to the day of the manager's own, with the
    /// assistant in charge of his.
    pub(crate) fn play_today(&mut self) -> Vec<usize> {
        let Some(i) = self.user_fixture() else {
            return Vec::new();
        };
        let day = self.fixtures[i].day;
        self.skip_to(day);
        let (played, seeds) = self.draw_block(day);
        let last = self.last_day_of(&played);
        let out = self.complete_block(played, seeds, Vec::new());
        self.settle_to(last);
        out
    }

    /// Pass the days, letting everything happen, until `day`.
    pub(crate) fn settle_to(&mut self, day: u32) {
        self.day = self.day.max(day);
    }

    /// Get to `day` for a game that is about to be played: a career's days
    /// pass one by one, with everything on them; a bare league just goes.
    pub(crate) fn skip_to(&mut self, day: u32) {
        if self.preseason_days == 0 {
            self.settle_to(day);
        }
        while self.day < day {
            self.pass_day();
        }
    }

    /// Everything of today happens and the day ends, whoever is waiting.
    pub(crate) fn pass_day(&mut self) {
        self.run_today();
        self.day += 1;
        self.daily();
    }

    /// Let the days before the round's first game pass: a preseason runs
    /// its course.
    pub(crate) fn run_up_to_round(&mut self) {
        if let Some(first) = self.next_game_day() {
            self.skip_to(first);
        }
    }

    /// What is scheduled for today, apart from the manager's own game.
    fn run_today(&mut self) {
        let day = self.day;
        self.play_friendlies_on(day);
        if self.preseason_days > 0
            && self.round < self.rounds()
            && self.reserve_rounds_done <= self.round
        {
            if day >= self.round_base(self.round) + RESERVE_DAY {
                self.reserves_for(self.round);
            }
        }
        let (played, seeds) = self.draw_block(day);
        if !played.is_empty() {
            self.complete_block(played, seeds, Vec::new());
        }
        if day < self.preseason_days && day % 7 == 6 {
            self.camp_week(day / 7 + 1);
        }
    }

    /// The camp's week ends: the squad has trained and healed.
    fn camp_week(&mut self, week: u32) {
        self.health_week();
        self.dressing_week();
        let before = self.weekly_training();
        let Some(me) = self.watch else { return };
        let mut moved: Vec<(PlayerId, f32)> = before
            .iter()
            .map(|&(p, was)| (p, self.players[p as usize].overall() - was))
            .collect();
        moved.sort_by(|a, b| b.1.total_cmp(&a.1));
        let up: Vec<String> = moved
            .iter()
            .filter(|m| m.1 > 0.015)
            .take(3)
            .map(|m| format!("{} ({:+.2})", self.players[m.0 as usize].name, m.1))
            .collect();
        let slot = &self.teams[me].playbook[self.teams[me].active];
        let body = format!(
            "Week {week} of camp is done.\n\n{}\nThe squad knows {} {:.0}%. Friendlies and \
             training both build that, and sharpness comes back only with games.\n\n\
             Opening day is {}.",
            if up.is_empty() {
                "Nobody made a visible step this week.\n".to_string()
            } else {
                format!("Coming on: {}.\n", up.join(", "))
            },
            slot.name,
            slot.familiarity() * 100.0,
            self.day_long(self.match_day()),
        );
        self.post(
            self.day,
            Kind::Training,
            "Assistant coach",
            format!("Training camp, week {week}"),
            body,
            vec![],
        );
    }

    /// Play the friendlies due by `day`. They do not count in the table or
    /// the averages; they tire, sharpen, hurt and cheer the people in them,
    /// and the system they play gets a rehearsal.
    fn play_friendlies_on(&mut self, day: u32) {
        let due: Vec<usize> = (0..self.friendlies.len())
            .filter(|&i| self.friendlies[i].result.is_none() && self.friendlies[i].day <= day)
            .collect();
        if due.is_empty() {
            return;
        }
        let jobs: Vec<(TeamId, TeamId, u64, bool)> = due
            .iter()
            .map(|&i| {
                let f = &self.friendlies[i];
                (f.home, f.away, self.rng.next_u64(), false)
            })
            .collect();
        let results = self.simulate_batch(&jobs);
        let regulation = (self.rules.periods as u32 * self.rules.period_secs) as f32;
        for (&i, (result, _)) in due.iter().zip(results) {
            let (home, away) = (self.friendlies[i].home, self.friendlies[i].away);
            for (side, team) in [home, away].into_iter().enumerate() {
                for (pid, line) in &result.boxes[side] {
                    if line.secs == 0 {
                        continue;
                    }
                    let minutes = line.secs as f32 / 60.0;
                    self.wear_and_tear(
                        team,
                        *pid,
                        minutes,
                        line.secs as f32 / regulation,
                        FRIENDLY_LOAD,
                    );
                    // A run in a friendly lifts a man a little.
                    if minutes >= 8.0 {
                        let p = &mut self.players[*pid as usize];
                        p.morale = (p.morale + 0.01).min(1.0);
                    }
                }
                let t = &mut self.teams[team];
                let active = t.active;
                t.playbook[active].rehearse(FRIENDLY_FAMILIARITY);
                if result.score[side] > result.score[1 - side] {
                    t.belief = (t.belief + 0.004).min(1.0);
                }
            }
            if let Some(me) = self.watch
                && (home == me || away == me)
            {
                self.friendly_mail(me, i, &result);
            }
            self.friendlies[i].result = Some(result);
        }
    }

    fn friendly_mail(&mut self, me: TeamId, i: usize, result: &crate::engine::GameResult) {
        let f = &self.friendlies[i];
        let side = if f.home == me { 0 } else { 1 };
        let opp = self.teams[[f.away, f.home][side]].name.clone();
        let (us, them) = (result.score[side], result.score[1 - side]);
        let hurt: Vec<String> = result.boxes[side]
            .iter()
            .filter(|(p, _)| self.players[*p as usize].injury.is_some())
            .map(|(p, _)| self.players[*p as usize].name.clone())
            .collect();
        let mut body = format!(
            "{} {} - {} {}. It does not count in the table.\n\nFriendlies are for sharpness \
             and for the squad to learn the system, and everyone who played got some of both.",
            self.teams[f.home].name, result.score[0], result.score[1], self.teams[f.away].name,
        );
        if !hurt.is_empty() {
            body += &format!("\n\nHurt in the game: {}.", hurt.join(", "));
        }
        let subject = format!(
            "Friendly: {} {us}-{them} {} {opp}",
            if us > them { "won" } else { "lost" },
            if side == 0 { "at home to" } else { "away at" },
        );
        let day = f.day;
        self.post(day, Kind::Match, "Assistant coach", subject, body, vec![]);
    }

    /// Something that stops the calendar for the manager, or `None` if the
    /// day can pass. Nothing has happened if it returns `Some`.
    fn blocker(&mut self) -> Option<Stop> {
        if self.draft.is_some() {
            return Some(Stop::Draft);
        }
        if self.match_today() {
            return Some(Stop::MatchDay);
        }
        if self.season_over() {
            if self.draft_day == 0 {
                self.draft_day = self.default_draft_day();
            }
            if self.day >= self.draft_day {
                self.begin_draft();
                // With nobody to wait for, the draft is already over.
                return Some(if self.draft.is_some() {
                    Stop::Draft
                } else {
                    Stop::Mail
                });
            }
        }
        None
    }

    /// Pass one day unless the manager is needed first.
    fn step_day(&mut self) -> Option<Stop> {
        if let Some(stop) = self.blocker() {
            return Some(stop);
        }
        self.pass_day();
        None
    }

    fn mail_today(&self) -> bool {
        self.desk.messages.iter().any(|m| m.day == self.day)
    }

    /// Move through the calendar until something wants the manager: a game,
    /// new mail, or the draft.
    pub fn advance(&mut self) -> Stop {
        loop {
            let before = self.desk.messages.len();
            if let Some(stop) = self.step_day() {
                return stop;
            }
            if self.new_mail(before) {
                return Stop::Mail;
            }
        }
    }

    /// Mail that arrived while the day passed. A manager out of work gets
    /// calls from clubs on the day a round closes, which is dated the day
    /// before the new one, so for him new mail is counted, not dated.
    fn new_mail(&self, before: usize) -> bool {
        self.mail_today() || (self.watch.is_none() && self.desk.messages.len() > before)
    }

    /// Pass a single day. Returns what the new day holds for the manager,
    /// if anything: `MatchDay` when a game waits (also when it was already
    /// waiting and the day could not pass), `Mail`, `Draft`.
    pub fn continue_day(&mut self) -> Option<Stop> {
        let before = self.desk.messages.len();
        if let Some(stop) = self.step_day() {
            // A manager out of work has no game to wait for on a day of
            // play: the league plays it, and the day passes.
            if stop == Stop::MatchDay && self.jobless() {
                self.pass_day();
            } else {
                return Some(stop);
            }
        }
        if self.new_mail(before) {
            Some(Stop::Mail)
        } else if self.match_today() && !self.jobless() {
            Some(Stop::MatchDay)
        } else {
            None
        }
    }

    /// Whether the manager has a friendly today (played when the day
    /// passes).
    pub fn friendly_today(&self) -> Option<usize> {
        let me = self.watch?;
        self.friendlies.iter().position(|f| {
            f.day == self.day && f.result.is_none() && (f.home == me || f.away == me)
        })
    }

    /// Every kind of thing on today's calendar, for the agents.
    pub fn today_kinds(&self) -> Vec<CalKind> {
        let mut kinds: Vec<CalKind> = self.events_on(self.day).iter().map(|e| e.kind).collect();
        kinds.dedup();
        kinds
    }
}
