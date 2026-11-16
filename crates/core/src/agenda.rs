//! The agenda: what is coming up for the manager, day by day.
//!
//! One view for the screens and the agents: our games with where they are,
//! how far, the opponent's record and form, what the scout can still watch
//! of them before we meet, the report we hold; the week's training and its
//! preparation sessions; the scout's assignments; deadlines and mail that
//! needs an answer; and why the calendar is waiting, if it is.

use serde::Serialize;

use crate::calendar::CalKind;
use crate::inbox::OfferState;
use crate::league::{TeamId, World};
use crate::legal::{Waiting, waiting_all};
use crate::training::Session;
use crate::travel::TripKind;

/// A trip to an away game.
#[derive(Clone, Debug, Serialize)]
pub struct TripInfo {
    pub km: u32,
    /// "short trip", "bus on the morning of the game", "long trip, we
    /// leave the day before".
    pub kind: &'static str,
    pub long: bool,
    /// The day training is lost to the trip, when it is a long one.
    pub leave_day: Option<u32>,
}

/// One of an opponent's games the scout could watch.
#[derive(Clone, Debug, Serialize)]
pub struct WatchGame {
    pub day: u32,
    pub label: String,
    /// Who they play, and whether they are at home.
    pub vs: TeamId,
    pub vs_name: String,
    pub home: bool,
    /// The scout is already sent to this game.
    pub assigned: bool,
    /// He is sent to another club's game that day.
    pub busy_with: Option<String>,
    /// Before we meet them (otherwise it is for later).
    pub before_meeting: bool,
}

/// The report we hold on a club, for the agenda.
#[derive(Clone, Debug, Serialize)]
pub struct ReportBrief {
    pub id: u32,
    pub day: u32,
    pub date: String,
    /// "high", "fair", "low".
    pub confidence: &'static str,
    pub offense: Option<&'static str>,
    pub defense: Option<&'static str>,
    pub press: Option<&'static str>,
    pub games_watched: u8,
    /// The message that holds it, to open.
    pub message: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgendaGame {
    /// "league", "playoff", "friendly".
    pub competition: &'static str,
    pub fixture: Option<usize>,
    pub home: bool,
    pub opponent: TeamId,
    pub opponent_name: String,
    pub city: &'static str,
    /// "5-2" and the last games, newest last: "WWLWL".
    pub record: String,
    pub form: String,
    /// Place in their division's table.
    pub rank: usize,
    pub trip: Option<TripInfo>,
    pub played: bool,
    /// Ours first.
    pub score: Option<[u16; 2]>,
    pub report: Option<ReportBrief>,
    /// Preparation sessions that count before this game.
    pub prep_sessions: usize,
    /// Games of theirs the scout could watch (those before we meet first).
    pub watch: Vec<WatchGame>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TrainingDay {
    /// Morning and afternoon.
    pub sessions: [&'static str; 2],
    pub prep: [bool; 2],
    /// Lost to a long trip.
    pub lost_to_travel: bool,
    /// Weekly slots (0..10) of the two sessions, for changing them.
    pub slots: [usize; 2],
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgendaItem {
    Game(AgendaGame),
    Training(TrainingDay),
    /// The scout is sent to watch `team` play `vs`.
    Scout {
        day: u32,
        team: TeamId,
        team_name: String,
        vs_name: String,
        home: bool,
    },
    /// Something with a date: an offer that lapses, the draft.
    Deadline {
        text: String,
        message: Option<u32>,
    },
    /// Mail that wants an answer.
    Answer {
        message: u32,
        subject: String,
    },
    Marker {
        text: String,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct AgendaDay {
    pub day: u32,
    pub label: String,
    pub today: bool,
    pub items: Vec<AgendaItem>,
}

/// Why the calendar is waiting and what can be done.
#[derive(Clone, Debug, Serialize)]
pub struct HoldInfo {
    pub waiting: Waiting,
    pub reason: String,
    pub can: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Agenda {
    pub today: u32,
    pub date: String,
    pub hold: HoldInfo,
    pub days: Vec<AgendaDay>,
}

impl World {
    fn form_of(&self, team: TeamId) -> String {
        let mut played: Vec<(u32, bool)> = self
            .fixtures
            .iter()
            .filter(|f| !f.playoff && (f.home == team || f.away == team))
            .filter_map(|f| {
                let r = f.result.as_ref()?;
                let side = usize::from(f.away == team);
                Some((f.day, r.score[side] > r.score[1 - side]))
            })
            .collect();
        played.sort();
        let tail = played.len().saturating_sub(5);
        played[tail..]
            .iter()
            .map(|&(_, w)| if w { 'W' } else { 'L' })
            .collect()
    }

    fn report_brief(&self, team: TeamId) -> Option<ReportBrief> {
        let r = self.latest_report(team)?;
        let subject = format!(
            "Scout report: {} ({})",
            self.teams[team].name,
            self.day_label(r.day)
        );
        let message = self
            .desk
            .messages
            .iter()
            .find(|m| m.subject == subject && m.day == r.day)
            .map(|m| m.id);
        Some(ReportBrief {
            id: r.id,
            day: r.day,
            date: self.day_label(r.day),
            confidence: r.confidence_label(),
            offense: r.offense.map(|o| o.label()),
            defense: r.defense.map(|d| d.label()),
            press: r.press.map(|p| p.label()),
            games_watched: r.watched,
            message,
        })
    }

    /// Our game on `day`, if one is on the calendar.
    fn agenda_game(&self, me: TeamId, i: Option<usize>, friendly: Option<usize>) -> Option<AgendaGame> {
        let (f, kind, fixture) = match (i, friendly) {
            (Some(i), _) => {
                let f = &self.fixtures[i];
                (f, if f.playoff { "playoff" } else { "league" }, Some(i))
            }
            (_, Some(k)) => (&self.friendlies[k], "friendly", None),
            _ => return None,
        };
        let home = f.home == me;
        let opp = if home { f.away } else { f.home };
        let s = &self.standings[opp];
        let rank = self.table_of(self.teams[opp].division).iter().position(|&t| t == opp).unwrap_or(0) + 1;
        let trip = (!home && kind != "friendly").then(|| {
            let km = self.trip_km(me, opp);
            let kind = TripKind::of(km);
            TripInfo {
                km,
                kind: kind.label(),
                long: kind == TripKind::Long,
                leave_day: self.travel_day_lost(me, f.day).map(|_| f.day - 1),
            }
        });
        let before: Vec<WatchGame> = if kind == "friendly" || f.result.is_some() {
            Vec::new()
        } else {
            self.watchable_games(opp)
                .into_iter()
                .map(|(day, vs, at_home)| WatchGame {
                    day,
                    label: self.day_label(day),
                    vs,
                    vs_name: self.teams[vs].name.clone(),
                    home: at_home,
                    assigned: self.assignment_on(day).is_some_and(|a| a.team == opp),
                    busy_with: self
                        .assignment_on(day)
                        .filter(|a| a.team != opp)
                        .map(|a| self.teams[a.team].name.clone()),
                    before_meeting: day < f.day,
                })
                .collect()
        };
        Some(AgendaGame {
            competition: kind,
            fixture,
            home,
            opponent: opp,
            opponent_name: self.teams[opp].name.clone(),
            city: self.city_of(opp),
            record: format!("{}-{}", s.wins, s.losses),
            form: self.form_of(opp),
            rank,
            trip,
            played: f.result.is_some(),
            score: f.result.as_ref().map(|r| {
                let side = usize::from(!home);
                [r.score[side], r.score[1 - side]]
            }),
            report: self.report_brief(opp),
            prep_sessions: if kind == "friendly" { 0 } else { self.prep_sessions(me, f.day) },
            watch: before,
        })
    }

    /// Why the calendar waits, if it does.
    pub fn hold_info(&self) -> HoldInfo {
        // A game outranks the questions that do not hold the calendar.
        let all = waiting_all(self);
        let w = if all.contains(&Waiting::MatchDay) && matches!(all[0], Waiting::Press { .. } | Waiting::SponsorOffer { .. } | Waiting::Appearance { .. }) {
            Waiting::MatchDay
        } else {
            all[0].clone()
        };
        let (reason, can): (String, Vec<&'static str>) = match &w {
            Waiting::MatchDay => {
                let opp = self.user_fixture().map(|i| {
                    let f = &self.fixtures[i];
                    let me = self.watch.unwrap_or(f.home);
                    (if f.home == me { f.away } else { f.home }, f.home == me)
                });
                let who = opp.map_or("the league's games".to_string(), |(t, home)| {
                    format!("{} {}", if home { "at home to" } else { "away at" }, self.teams[t].name)
                });
                (
                    format!("A game is waiting: {who}. Time stops until it is played."),
                    vec!["Play match", "Simulate", "Pick the lineup", "Open tactics"],
                )
            }
            Waiting::DraftPick => ("You are on the clock in the draft.".into(), vec!["Draft"]),
            Waiting::Offer { .. } => (
                "A transfer offer for one of your players needs an answer before it lapses.".into(),
                vec!["Answer the offer", "Continue"],
            ),
            Waiting::Press { .. } => (
                "The press has a question. It lapses after the next round.".into(),
                vec!["Answer the press", "Continue"],
            ),
            Waiting::JobOffer { .. } => ("Another club has offered you its job.".into(), vec!["Answer", "Continue"]),
            Waiting::SponsorOffer { .. } => ("A sponsor's proposal is open.".into(), vec!["Answer", "Continue"]),
            Waiting::Appearance { .. } => ("A sponsor wants a player for an event.".into(), vec!["Answer", "Continue"]),
            Waiting::Nothing => {
                let today = self.desk.messages.iter().filter(|m| m.day == self.day && !m.read).count();
                (
                    if today > 0 {
                        format!("Nothing holds the calendar. {today} new message{} today.", if today == 1 { "" } else { "s" })
                    } else {
                        "Nothing holds the calendar: Continue passes the day.".to_string()
                    },
                    vec!["Continue", "Next event"],
                )
            }
        };
        HoldInfo { waiting: w, reason, can }
    }

    /// The agenda from today for `days` days.
    pub fn agenda(&self, days: u32) -> Agenda {
        let me = self.watch;
        let today = self.day;
        let mut out = Vec::new();
        for day in today..today + days {
            let mut items = Vec::new();
            if let Some(me) = me {
                // Our games.
                for (i, f) in self.fixtures.iter().enumerate() {
                    if f.day == day && (f.home == me || f.away == me) {
                        items.extend(self.agenda_game(me, Some(i), None).map(AgendaItem::Game));
                    }
                }
                for (k, f) in self.friendlies.iter().enumerate() {
                    if f.day == day && (f.home == me || f.away == me) {
                        items.extend(self.agenda_game(me, None, Some(k)).map(AgendaItem::Game));
                    }
                }
                // Training, Monday to Friday.
                let wd = (day % 7) as usize;
                if wd < 5 && !self.season_over() {
                    let s = &self.teams[me].schedule.sessions;
                    let lost = self.fixtures.iter().any(|f| f.away == me && self.travel_day_lost(me, f.day).is_some_and(|w| f.day - 1 == day && w == wd));
                    items.push(AgendaItem::Training(TrainingDay {
                        sessions: [s[wd * 2].label(), s[wd * 2 + 1].label()],
                        prep: [s[wd * 2] == Session::MatchPrep, s[wd * 2 + 1] == Session::MatchPrep],
                        lost_to_travel: lost,
                        slots: [wd * 2, wd * 2 + 1],
                    }));
                }
                if let Some(a) = self.assignment_on(day)
                    && let Some((_, home, away)) = self.game_of(a.team, day)
                {
                    let vs = if home == a.team { away } else { home };
                    items.push(AgendaItem::Scout {
                        day,
                        team: a.team,
                        team_name: self.teams[a.team].name.clone(),
                        vs_name: self.teams[vs].name.clone(),
                        home: home == a.team,
                    });
                }
                // Deadlines.
                for (i, o) in self.desk.offers.iter().enumerate() {
                    if o.state == OfferState::Open && o.expires == day {
                        let m = self.desk.messages.iter().find(|m| m.offer == Some(i)).map(|m| m.id);
                        items.push(AgendaItem::Deadline {
                            text: format!("Offer of {} for {} lapses today", crate::inbox::money(o.fee), self.players[o.player as usize].name),
                            message: m,
                        });
                    }
                }
                for (i, o) in self.econ.offers.iter().enumerate() {
                    if o.club == me && o.open_on(today) && o.expires == day {
                        let m = self.desk.messages.iter().find(|m| m.sponsor == Some(i)).map(|m| m.id);
                        items.push(AgendaItem::Deadline { text: "A sponsor's proposal lapses today".into(), message: m });
                    }
                }
                for (i, a) in self.fame.asks.iter().enumerate() {
                    if self.appearance_open(i) && a.expires == day {
                        let m = self.desk.messages.iter().find(|m| m.appearance == Some(i)).map(|m| m.id);
                        items.push(AgendaItem::Deadline { text: "A sponsor's request for a player lapses today".into(), message: m });
                    }
                }
                if day == today {
                    for m in self.inbox() {
                        let open = m.offer.is_some_and(|i| self.desk.offers[i].state == OfferState::Open)
                            || m.press.is_some_and(|i| self.press_open(i))
                            || m.sponsor.is_some_and(|i| self.econ.offers.get(i).is_some_and(|o| o.open_on(today)))
                            || m.appearance.is_some_and(|i| self.appearance_open(i))
                            || m.job.is_some_and(|i| self.open_job_offers().contains(&i));
                        if open {
                            items.push(AgendaItem::Answer { message: m.id, subject: m.subject.clone() });
                        }
                    }
                }
            }
            for e in self.events_on(day) {
                if matches!(e.kind, CalKind::Marker | CalKind::Draft) {
                    items.push(AgendaItem::Marker { text: e.title.clone() });
                }
            }
            out.push(AgendaDay {
                day,
                label: self.day_label(day),
                today: day == today,
                items,
            });
        }
        Agenda {
            today,
            date: self.day_long(today),
            hold: self.hold_info(),
            days: out,
        }
    }
}
