//! The calendar: real dates, a preseason, and what is on each day.
//!
//! `World::day` counts days from the Monday the season's calendar opens
//! (the first day of training camp in a career, the first league week in a
//! bare league). The date arithmetic is integer only. A season starts on the
//! first Monday on or after 1 September of `START_YEAR + season`.
//!
//! A league week: reserve games on Wednesday, league games on Friday, Saturday
//! and Sunday. Preseason friendlies are on set days of the camp.

use serde::{Deserialize, Serialize};

use crate::league::{Fixture, TeamId, World};

/// Length of the training camp in a career, in days (four weeks).
pub const PRESEASON_DAYS: u32 = 28;
/// Days of the camp the friendlies are played on (Saturday, Wednesday,
/// Saturday, Saturday).
pub const FRIENDLY_DAYS: [u32; 4] = [12, 16, 19, 26];
/// Year of the first season.
pub const START_YEAR: i64 = 2026;
/// Day of the week (Monday = 0) the reserves play on.
pub const RESERVE_DAY: u32 = 2;
/// Offsets from Monday of a league week's game days, by the game's place in
/// the round. The first game of a round, always the same club's in a
/// round-robin, is on Saturday.
const GAME_DAYS: [u32; 4] = [5, 4, 6, 5];

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAYS_LONG: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Days since 1970-01-01 of a calendar date (proleptic Gregorian).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date `z` days after 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    (y, m, d)
}

/// Weekday of an absolute day, Monday = 0.
pub fn weekday_of(abs: i64) -> u32 {
    (abs + 3).rem_euclid(7) as u32
}

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Date {
    pub year: i32,
    /// 1..=12.
    pub month: u8,
    /// 1..=31.
    pub day: u8,
}

impl Date {
    pub fn from_abs(abs: i64) -> Date {
        let (y, m, d) = civil_from_days(abs);
        Date {
            year: y as i32,
            month: m as u8,
            day: d as u8,
        }
    }

    pub fn abs(self) -> i64 {
        days_from_civil(self.year as i64, self.month as i64, self.day as i64)
    }

    pub fn month_name(self) -> &'static str {
        MONTHS[self.month as usize - 1]
    }
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    let next = if month == 12 {
        days_from_civil(year as i64 + 1, 1, 1)
    } else {
        days_from_civil(year as i64, month as i64 + 1, 1)
    };
    (next - days_from_civil(year as i64, month as i64, 1)) as u8
}

/// What kind of entry a day holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalKind {
    League,
    Playoff,
    Friendly,
    Reserve,
    /// Training camp opens, opening day, the end of the regular season.
    Marker,
    Draft,
}

/// One thing on the calendar.
#[derive(Clone, Debug, Serialize)]
pub struct CalEvent {
    pub day: u32,
    pub kind: CalKind,
    /// Short words: "Friendly", "Draft night", "Opening day".
    pub title: String,
    pub home: Option<TeamId>,
    pub away: Option<TeamId>,
    pub played: bool,
    pub score: Option<[u16; 2]>,
    /// Index into `World::fixtures` for league and playoff games.
    pub fixture: Option<usize>,
    pub division: usize,
}

impl World {
    /// Absolute day (since 1970-01-01) of the season's day 0.
    pub fn season_start_abs(&self) -> i64 {
        let sept = days_from_civil(START_YEAR + self.season as i64, 9, 1);
        sept + (7 - weekday_of(sept) as i64) % 7
    }

    pub fn date_of(&self, day: u32) -> Date {
        Date::from_abs(self.season_start_abs() + day as i64)
    }

    /// The day number of a date in this season's calendar, if it is not
    /// before day 0.
    pub fn day_of(&self, date: Date) -> Option<u32> {
        let d = date.abs() - self.season_start_abs();
        (d >= 0).then_some(d as u32)
    }

    /// "Fri 9 Oct".
    pub fn day_label(&self, day: u32) -> String {
        let d = self.date_of(day);
        format!(
            "{} {} {}",
            WEEKDAYS[(day % 7) as usize],
            d.day,
            &d.month_name()[..3]
        )
    }

    /// "Friday 9 October 2026".
    pub fn day_long(&self, day: u32) -> String {
        let d = self.date_of(day);
        format!(
            "{} {} {} {}",
            WEEKDAYS_LONG[(day % 7) as usize],
            d.day,
            d.month_name(),
            d.year
        )
    }

    pub fn weekday(&self, day: u32) -> &'static str {
        WEEKDAYS[(day % 7) as usize]
    }

    /// Monday of the league week of `round`.
    pub fn round_base(&self, round: usize) -> u32 {
        self.preseason_days + 7 * round as u32
    }

    /// True while the training camp is on.
    pub fn in_preseason(&self) -> bool {
        self.day < self.preseason_days
    }

    /// Put the days on a fresh schedule: the round's place decides the day.
    pub(crate) fn date_fixtures(&mut self) {
        let base = self.preseason_days;
        let mut count: Vec<(usize, usize)> = Vec::new();
        for f in self.fixtures.iter_mut().filter(|f| !f.playoff) {
            let k = match count.iter_mut().find(|c| c.0 == f.round) {
                Some(c) => {
                    c.1 += 1;
                    c.1 - 1
                }
                None => {
                    count.push((f.round, 1));
                    0
                }
            };
            // A bare league plays its round on one day.
            let offset = if base == 0 { 5 } else { GAME_DAYS[k % 4] };
            f.day = base + 7 * f.round as u32 + offset;
        }
    }

    /// Arrange the preseason friendlies: four rounds of pairings, each club
    /// meeting somebody from its own division.
    pub(crate) fn arrange_friendlies(&mut self) {
        self.friendlies.clear();
        let divisions = self.division_count();
        for (n, &day) in FRIENDLY_DAYS.iter().enumerate() {
            for div in 0..divisions {
                let mut ids = self.division_teams(div);
                // A shuffle from the world's stream.
                for i in (1..ids.len()).rev() {
                    let j = self.rng.range(0, i as u32 + 1) as usize;
                    ids.swap(i, j);
                }
                for pair in ids.chunks_exact(2) {
                    let (home, away) = if (pair[0] + n) % 2 == 0 {
                        (pair[0], pair[1])
                    } else {
                        (pair[1], pair[0])
                    };
                    self.friendlies.push(Fixture {
                        round: n,
                        home,
                        away,
                        result: None,
                        playoff: false,
                        day,
                        division: div,
                    });
                }
            }
        }
    }

    /// Open the world in the training camp, a few weeks before the first
    /// league game. A new career does this; the league's fixtures and the
    /// friendlies are dated.
    pub fn begin_preseason(&mut self) {
        self.preseason_days = PRESEASON_DAYS;
        self.day = 0;
        self.date_fixtures();
        self.arrange_friendlies();
    }

    /// The day of the first game still to play this round, if any.
    pub(crate) fn next_game_day(&self) -> Option<u32> {
        self.fixtures
            .iter()
            .filter(|f| f.round == self.round && f.result.is_none())
            .map(|f| f.day)
            .min()
    }

    /// Where the draft is held: the first Thursday two weeks after the
    /// season's last game.
    pub(crate) fn default_draft_day(&self) -> u32 {
        let last = self.day.max(
            self.fixtures
                .iter()
                .map(|f| f.day)
                .max()
                .unwrap_or(self.day),
        );
        let mut d = last + 14;
        while d % 7 != 3 {
            d += 1;
        }
        d
    }

    /// Everything on the calendar from `from` to `to`, the whole league's.
    pub fn calendar_events(&self, from: u32, to: u32) -> Vec<CalEvent> {
        let mut out = Vec::new();
        let marker = |day: u32, title: &str| CalEvent {
            day,
            kind: CalKind::Marker,
            title: title.to_string(),
            home: None,
            away: None,
            played: day < self.day,
            score: None,
            fixture: None,
            division: 0,
        };
        let inside = |d: u32| d >= from && d <= to;
        if self.preseason_days > 0 && inside(0) {
            out.push(marker(0, "Training camp opens"));
        }
        let opening = self
            .fixtures
            .iter()
            .filter(|f| f.round == 0)
            .map(|f| f.day)
            .min();
        if let Some(d) = opening
            && self.preseason_days > 0
            && inside(d)
        {
            out.push(marker(d, "Opening day"));
        }
        let regular_end = self
            .fixtures
            .iter()
            .filter(|f| !f.playoff)
            .map(|f| f.day)
            .max();
        if let Some(d) = regular_end
            && inside(d)
        {
            out.push(marker(d, "Last day of the regular season"));
        }
        if self.draft_day != 0 && inside(self.draft_day) {
            let mut e = marker(self.draft_day, "Draft night");
            e.kind = CalKind::Draft;
            e.played = self.draft_day < self.day || self.draft.is_some();
            out.push(e);
        }
        for (i, f) in self.fixtures.iter().enumerate() {
            if inside(f.day) {
                out.push(CalEvent {
                    day: f.day,
                    kind: if f.playoff {
                        CalKind::Playoff
                    } else {
                        CalKind::League
                    },
                    title: if f.playoff { "Playoff" } else { "League" }.to_string(),
                    home: Some(f.home),
                    away: Some(f.away),
                    played: f.result.is_some(),
                    score: f.result.as_ref().map(|r| r.score),
                    fixture: Some(i),
                    division: f.division,
                });
            }
            // The reserves play the same pairing midweek.
            let rday = self.round_base(f.round) + RESERVE_DAY;
            if !f.playoff && inside(rday) {
                let game = self
                    .reserve_log
                    .iter()
                    .find(|g| g.round == f.round && g.home == f.home && g.away == f.away);
                out.push(CalEvent {
                    day: rday,
                    kind: CalKind::Reserve,
                    title: "Reserves".to_string(),
                    home: Some(f.home),
                    away: Some(f.away),
                    played: game.is_some() || f.round < self.reserve_rounds_done,
                    score: game.map(|g| g.score),
                    fixture: None,
                    division: f.division,
                });
            }
        }
        for f in &self.friendlies {
            if inside(f.day) {
                out.push(CalEvent {
                    day: f.day,
                    kind: CalKind::Friendly,
                    title: "Friendly".to_string(),
                    home: Some(f.home),
                    away: Some(f.away),
                    played: f.result.is_some(),
                    score: f.result.as_ref().map(|r| r.score),
                    fixture: None,
                    division: f.division,
                });
            }
        }
        out.sort_by_key(|e| (e.day, e.division, e.kind as u8));
        out
    }

    /// What is on one day.
    pub fn events_on(&self, day: u32) -> Vec<CalEvent> {
        self.calendar_events(day, day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_date_arithmetic_is_right() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(weekday_of(days_from_civil(2026, 9, 1)), 1);
        for z in [-800, -1, 0, 59, 11_000, 20_000, 20_500] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    #[test]
    fn seasons_open_on_a_monday_in_september() {
        let mut w = World::generate(1, 8);
        for s in 0..4 {
            w.season = s;
            let d = w.date_of(0);
            assert_eq!(d.month, 9);
            assert!(d.day <= 7);
            assert_eq!(weekday_of(d.abs()), 0);
        }
        w.season = 0;
        assert_eq!(w.day_label(4), "Fri 11 Sep");
    }

    use crate::command::Command;
    use crate::inbox::Stop;

    #[test]
    fn a_career_opens_in_the_camp_and_friendlies_stay_out_of_the_table() {
        let mut w = World::new_career(3, 8);
        w.apply(Command::TakeCharge(2)).unwrap();
        assert_eq!(w.day, 0);
        assert!(w.in_preseason() && w.round == 0);
        assert_eq!(w.friendlies.len(), 4 * 4, "four friendlies for each of eight clubs");
        // Every club plays once on each friendly day.
        for day in crate::calendar::FRIENDLY_DAYS {
            let mut seen: Vec<usize> = w
                .friendlies
                .iter()
                .filter(|f| f.day == day)
                .flat_map(|f| [f.home, f.away])
                .collect();
            seen.sort();
            assert_eq!(seen, (0..8).collect::<Vec<_>>());
        }
        let sharp = w.players[w.teams[2].roster[0] as usize].sharp;
        let mut days = 0;
        while w.day < crate::calendar::PRESEASON_DAYS {
            let out = w.apply(Command::Continue).unwrap();
            assert_eq!(out.day, Some(w.day));
            days += 1;
            assert!(days < 60, "opening day never came");
        }
        // The camp is over: the friendlies are in, the table is empty.
        assert_eq!(w.round, 0);
        assert!(w.friendlies.iter().all(|f| f.result.is_some()));
        assert!(w.standings.iter().all(|s| s.wins + s.losses == 0));
        assert!(w.team_stats.iter().all(|s| s.games == 0));
        assert!(w.games_played.iter().all(|&g| g == 0));
        assert!(w.season_stats.iter().all(|l| l.secs == 0));
        // Opening day arrives.
        while !w.match_today() {
            w.apply(Command::Continue).unwrap();
        }
        assert_eq!(w.round, 0);
        assert!(w.day >= crate::calendar::PRESEASON_DAYS + 4);
        // They did count for something: the squad knows its system better
        // and the manager was told about each of his.
        let mail = w
            .desk
            .messages
            .iter()
            .filter(|m| m.subject.starts_with("Friendly"))
            .count();
        assert_eq!(mail, 4);
        assert!(w.players[w.teams[2].roster[0] as usize].sharp != sharp || w.players[w.teams[2].roster[0] as usize].sharp == 1.0);
        let slot = &w.teams[2].playbook[0];
        assert!(slot.familiarity() > 0.6, "camp and friendlies drilled the system");
    }

    #[test]
    fn games_have_days_and_the_weekend_is_spread() {
        let w = World::new_career(5, 8);
        let mut days = std::collections::BTreeSet::new();
        for f in &w.fixtures {
            let week = (f.day - w.preseason_days) / 7;
            assert_eq!(week as usize, f.round);
            days.insert((f.day - w.preseason_days) % 7);
        }
        assert_eq!(days.into_iter().collect::<Vec<_>>(), vec![4, 5, 6]);
        // Club 0 always plays on Saturday.
        assert!(w
            .fixtures
            .iter()
            .filter(|f| f.home == 0 || f.away == 0)
            .all(|f| (f.day - w.preseason_days) % 7 == 5));
        // Day 0 is a Monday in September, and the opening game a Friday to
        // Sunday in October.
        assert_eq!(w.weekday(0), "Mon");
        assert_eq!(w.date_of(0).month, 9);
        assert_eq!(w.date_of(w.preseason_days + 5).month, 10);
        // The calendar lists every game once, plus reserves and friendlies.
        let events = w.calendar_events(0, 400);
        let league = events.iter().filter(|e| e.kind == CalKind::League).count();
        assert_eq!(league, w.fixtures.len());
        assert_eq!(events.iter().filter(|e| e.kind == CalKind::Friendly).count(), 16);
        let reserves: Vec<_> = events.iter().filter(|e| e.kind == CalKind::Reserve).collect();
        assert_eq!(reserves.len(), w.fixtures.len());
        assert!(reserves.iter().all(|e| w.weekday(e.day) == "Wed"));
        assert!(events.iter().any(|e| e.title == "Opening day"));
    }

    #[test]
    fn time_passes_a_day_at_a_time_and_stops_for_the_manager_s_game() {
        let mut w = World::new_career(4, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        // Through the camp to the day before the first game.
        let first = w.fixtures.iter().filter(|f| f.round == 0).map(|f| f.day).min().unwrap();
        while w.day < first + 1 {
            let out = w.apply(Command::Continue).unwrap();
            if out.stop == Some(Stop::MatchDay) {
                break;
            }
        }
        // Club 0 plays on Saturday: Friday's games were played on Friday
        // by the days passing, Saturday's and Sunday's were not.
        assert_eq!(w.weekday(w.day), "Sat");
        let played: Vec<_> = w.fixtures.iter().filter(|f| f.round == 0 && f.result.is_some()).map(|f| f.day).collect();
        assert!(!played.is_empty() && played.iter().all(|&d| d == first));
        assert!(w.match_today());
        // Continue does not pass the game.
        let day = w.day;
        assert_eq!(w.apply(Command::Continue).unwrap().stop, Some(Stop::MatchDay));
        assert_eq!(w.day, day);
        // Playing today leaves Sunday's games for Sunday.
        let out = w.apply(Command::PlayToday).unwrap();
        assert!(out.played.iter().all(|&i| w.fixtures[i].day <= day));
        assert_eq!(w.round, 0, "Sunday is still to come");
        w.apply(Command::Continue).unwrap();
        assert_eq!(w.round, 0);
        w.apply(Command::Continue).unwrap();
        assert_eq!(w.round, 1, "the round closed when the last game was in");
        assert_eq!(w.weekday(w.day), "Mon");
        assert!(w.standings.iter().all(|s| s.wins + s.losses == 1));
        // And Advance still goes to the next game.
        assert_eq!(w.advance(), Stop::Mail);
        loop {
            match w.advance() {
                Stop::MatchDay => break,
                Stop::Mail => {}
                Stop::Draft => unreachable!(),
            }
        }
        assert!(w.match_today());
        // Reserves played on the Wednesday.
        assert_eq!(w.reserve_rounds_done, 2);
    }

    #[test]
    fn replaying_the_log_rebuilds_a_career_that_began_in_the_camp() {
        let mut a = World::new_career(9, 8);
        a.apply(Command::TakeCharge(1)).unwrap();
        for _ in 0..9 {
            a.apply(Command::Continue).unwrap();
        }
        let mut games = 0;
        let mut n = 0;
        while games < 6 && n < 400 {
            n += 1;
            match a.apply(Command::Advance).unwrap().stop {
                Some(Stop::MatchDay) => {
                    games += 1;
                    match games % 3 {
                        0 => {
                            a.apply(Command::PlayRound).unwrap();
                        }
                        1 => {
                            a.apply(Command::PlayToday).unwrap();
                        }
                        _ => {
                            let me = a.watch.unwrap();
                            let mut live = a.kickoff_today(me).unwrap();
                            live.input(crate::league::LiveInput::Timeout);
                            while !live.game.is_over() {
                                live.step();
                            }
                            a.full_time(live);
                        }
                    }
                }
                Some(Stop::Draft) => {
                    a.apply(Command::DraftAuto { all: true }).unwrap();
                }
                _ => {}
            }
        }
        assert!(games == 6 && a.round >= 3);
        assert!(a.log.iter().any(|c| matches!(c, Command::PlayLiveToday { .. })));
        assert!(a.log.iter().any(|c| matches!(c, Command::Continue)));
        let mut b = World::new_career(9, 8);
        for cmd in a.log.clone() {
            b.apply(cmd).unwrap();
        }
        assert_eq!(a.day, b.day);
        assert!(format!("{a:?}") == format!("{b:?}"), "worlds differ after replay");
    }

    #[test]
    fn the_offseason_has_a_draft_day_and_the_next_season_begins_in_the_camp() {
        let mut w = World::new_career(6, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        let mut guard = 0;
        while !w.season_over() {
            guard += 1;
            assert!(guard < 2000);
            if w.match_today() {
                w.apply(Command::PlayRound).unwrap();
            } else {
                w.apply(Command::Advance).unwrap();
            }
        }
        assert!(w.draft_day > w.day);
        assert_eq!(w.weekday(w.draft_day), "Thu");
        assert!(w.events_on(w.draft_day).iter().any(|e| e.kind == CalKind::Draft));
        loop {
            match w.apply(Command::Advance).unwrap().stop {
                Some(Stop::Draft) => {
                    assert!(w.day >= w.draft_day);
                    w.apply(Command::DraftAuto { all: true }).unwrap();
                    break;
                }
                Some(Stop::Mail) if w.season == 1 => break,
                _ => {}
            }
        }
        assert_eq!(w.season, 1);
        assert_eq!(w.day, 0);
        assert!(w.in_preseason());
        assert_eq!(w.friendlies.len(), 16);
        assert!(w.friendlies.iter().all(|f| f.result.is_none()));
        assert_eq!(w.date_of(0).year, 2027);
    }
}
