//! What the manager can do right now, described as data.
//!
//! `World::legal(club)` lists every kind of `Command` with whether it is
//! available, why not if it is not, the parameters and their valid choices
//! or ranges, and a concrete `example` command that `World::apply` accepts
//! when `available` is true. The checks are the ones `Command` execution
//! makes, so a listed example is accepted and a refused kind says why.
//!
//! It also says what the game is waiting for (`Waiting`): the draft pick
//! that holds the calendar, or the mail that wants an answer before it
//! lapses.

use serde::Serialize;
use serde_json::{Value, json};

use crate::command::Command;
use crate::inbox::{Deal, MAX_ROSTER, MIN_ROSTER, OfferState};
use crate::league::{TeamId, World};
use crate::player::PlayerId;
use crate::reserves::RESERVE_MAX;
use crate::scouting::SHORTLIST_MAX;
use crate::staff::Job;
use crate::tactics::{
    Brief, DefenseScheme, MAX_TACTICS, OffenseSet, Press, Role, RotationSlot, Tactics,
};
use crate::training::{Intensity, Preset, Schedule, Session, TrainingFocus};

/// What the game needs from the manager next.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Waiting {
    /// You are on the clock in the draft. `Advance` will not move the
    /// calendar until you pick (`DraftPick` or `DraftAuto`).
    DraftPick,
    /// A transfer offer for one of your players is open. It lapses if left.
    Offer { message: u32, offer: usize },
    /// A question from the press is open. It lapses after the next round.
    Press { question: usize },
    /// Another club has offered you its job: `AcceptJob` or `DeclineJob`.
    JobOffer { offer: usize },
    /// A sponsor's proposal is open.
    SponsorOffer { offer: usize },
    /// A sponsor wants one of your players at an event: `AnswerAppearance`.
    Appearance { request: usize },
    /// A game is due today: `PlayRound`, `PlayWithInstructions` or a live
    /// match.
    MatchDay,
    /// Nothing is waiting: `Advance` moves the calendar on.
    Nothing,
}

/// Everything waiting, most urgent first. Never empty: `MatchDay` or
/// `Nothing` comes after everything that holds the calendar, and only job
/// offers follow it.
pub fn waiting_all(w: &World) -> Vec<Waiting> {
    let mut out = Vec::new();
    if w.draft.as_ref().is_some_and(|d| d.on_clock() == w.watch) && w.watch.is_some() {
        out.push(Waiting::DraftPick);
    }
    for m in w.inbox() {
        if let Some(i) = m.offer
            && w.desk
                .offers
                .get(i)
                .is_some_and(|o| o.state == OfferState::Open)
        {
            out.push(Waiting::Offer {
                message: m.id,
                offer: i,
            });
        }
    }
    for i in 0..w.desk.press.len() {
        if w.press_open(i) {
            out.push(Waiting::Press { question: i });
        }
    }
    if let Some(me) = w.watch {
        for (i, o) in w.econ.offers.iter().enumerate() {
            if o.club == me && o.open_on(w.day) {
                out.push(Waiting::SponsorOffer { offer: i });
            }
        }
    }
    for i in 0..w.fame.asks.len() {
        if w.appearance_open(i) {
            out.push(Waiting::Appearance { request: i });
        }
    }
    out.push(if w.match_today() {
        Waiting::MatchDay
    } else {
        Waiting::Nothing
    });
    // A club's offer of its job waits for nobody and blocks nothing, so it
    // comes after the day's business and is never the first thing.
    for offer in w.open_job_offers() {
        out.push(Waiting::JobOffer { offer });
    }
    out
}

/// The first thing the game is waiting for.
pub fn waiting(w: &World) -> Waiting {
    waiting_all(w).remove(0)
}

/// One kind of command and whether it can be given now.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ActionSpec {
    /// The `Command` variant's name, as serde writes it.
    pub kind: &'static str,
    pub available: bool,
    /// Why not, when it is not available.
    pub why_not: Option<String>,
    pub describe: &'static str,
    /// Parameters and the valid choices or ranges.
    pub params: Value,
    /// A command of this kind that `apply` accepts (present when available).
    pub example: Option<Command>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Legal {
    /// `None` when you are out of work.
    pub club: Option<TeamId>,
    pub waiting: Waiting,
    /// Everything waiting, most urgent first.
    pub pending: Vec<Waiting>,
    pub actions: Vec<ActionSpec>,
}

fn spec(
    kind: &'static str,
    describe: &'static str,
    blocked: Option<String>,
    params: Value,
    example: Option<Command>,
) -> ActionSpec {
    let available = blocked.is_none() && example.is_some();
    ActionSpec {
        kind,
        available,
        why_not: blocked.or_else(|| (!available).then(|| "Nothing to choose from.".to_string())),
        describe,
        params,
        example: if available { example } else { None },
    }
}

fn names<T: Serialize>(all: &[T]) -> Value {
    serde_json::to_value(all).unwrap_or(Value::Null)
}

impl World {
    /// What the manager of `club` can do now. `None` unless `club` is the
    /// managed club.
    pub fn legal(&self, club: TeamId) -> Option<Legal> {
        if self.watch != Some(club) {
            return None;
        }
        let pending = waiting_all(self);
        Some(Legal {
            club: Some(club),
            waiting: pending[0].clone(),
            pending,
            actions: self.action_specs(club),
        })
    }

    /// What an out-of-work manager can do: let time pass, read his mail and
    /// answer the offers that come.
    pub fn legal_jobless(&self) -> Option<Legal> {
        if !self.jobless() {
            return None;
        }
        let pending = waiting_all(self);
        let mut actions = Vec::new();
        actions.push(spec(
            "Advance",
            "Let the calendar run until a game, new mail or the draft. Returns why it stopped.",
            None,
            json!({}),
            Some(Command::Advance),
        ));
        actions.push(spec(
            "Continue",
            "Pass one day. Every club's games are played on their days, by the assistants.",
            None,
            json!({}),
            Some(Command::Continue),
        ));
        let over = self.season_over().then(|| "The season is over.".to_string());
        actions.push(spec(
            "PlayRound",
            "Play this round's games. You manage nobody, so all are played by the assistants.",
            over,
            json!({}),
            Some(Command::PlayRound),
        ));
        let unread: Vec<u32> = self.inbox().iter().filter(|m| !m.read).map(|m| m.id).collect();
        actions.push(spec(
            "MarkRead",
            "Mark a message read.",
            None,
            json!({"message": {"choices": unread}}),
            unread.first().map(|&id| Command::MarkRead(id)),
        ));
        actions.extend(self.job_specs());
        Some(Legal {
            club: None,
            waiting: pending[0].clone(),
            pending,
            actions,
        })
    }

    /// The career commands: answering a job offer, resigning.
    fn job_specs(&self) -> Vec<ActionSpec> {
        let open = self.open_job_offers();
        let list: Vec<Value> = open
            .iter()
            .map(|&i| {
                let o = &self.career.offers[i];
                json!({"offer": i, "club": o.club, "club_name": self.teams[o.club].name,
                       "stature": self.stature(o.club),
                       "division": self.division_name(self.teams[o.club].division), "why": o.why})
            })
            .collect();
        let draft = self
            .draft
            .is_some()
            .then(|| "The draft is on. Finish it first.".to_string());
        let mut out = Vec::new();
        out.push(spec(
            "AcceptJob",
            "Take another club's job. `bring`: jobs on your staff whose holders you ask to come (those who are willing do, the rest stay); `keep`: jobs at the new club whose holders stay (the others are paid off; a loyal owner limits how many). See view job_offer for what each says before you sign.",
            draft.clone(),
            json!({"offer": {"choices": list},
                   "bring": {"jobs": ["Assistant", "Skills", "Fitness", "Scout", "Press"]},
                   "keep": {"jobs": ["Assistant", "Skills", "Fitness", "Scout", "Press"]}}),
            open.first().map(|&offer| Command::AcceptJob {
                offer,
                bring: Vec::new(),
                keep: Job::HIRED.to_vec(),
            }),
        ));
        out.push(spec(
            "DeclineJob",
            "Turn down a job offer.",
            None,
            json!({"offer": {"choices": list}}),
            open.first().map(|&i| Command::DeclineJob(i)),
        ));
        out.push(spec(
            "Resign",
            "Leave your club. You are out of work until somebody calls, and your reputation is untouched.",
            if self.watch.is_none() {
                Some("You have no job to leave.".to_string())
            } else {
                draft
            },
            json!({}),
            Some(Command::Resign),
        ));
        out
    }

    /// A rotation `SetTactics` accepts: the assistant's own choice of the
    /// squad, with the sliders as they are now. Use it as the starting
    /// point when the squad has changed.
    pub fn suggest_tactics(&self, club: TeamId) -> Option<Tactics> {
        if self.watch != Some(club) {
            return None;
        }
        let team = &self.teams[club];
        let mut fit: Vec<&crate::player::Player> = team
            .roster
            .iter()
            .map(|&p| &self.players[p as usize])
            .filter(|p| p.fit())
            .collect();
        if fit.len() < 8 {
            fit = team
                .roster
                .iter()
                .map(|&p| &self.players[p as usize])
                .collect();
        }
        let auto = Tactics::auto(&fit);
        let mut t = team.tactics().clone();
        t.rotation = auto.rotation;
        Some(t)
    }

    /// A one-for-one deal (with a little cash if needed) that the other
    /// club's general manager would accept now, if one is easy to find.
    fn sketch_trade(&self, me: TeamId) -> Option<Deal> {
        let mine = &self.teams[me];
        let mut give: Vec<PlayerId> = mine.roster.clone();
        give.sort_by_key(|&p| std::cmp::Reverse(self.value(p)));
        for club in (0..self.teams.len()).filter(|&t| t != me) {
            let mut get: Vec<PlayerId> = self.teams[club].roster.clone();
            get.sort_by_key(|&p| self.value(p));
            for &g in get.iter().take(4) {
                for &o in give.iter().rev().take(5) {
                    for cash in [0, mine.budget.clamp(0, 60_000)] {
                        let deal = Deal {
                            club,
                            give: vec![o],
                            get: vec![g],
                            cash,
                            ..Deal::default()
                        };
                        if self.trade_verdict(&deal).is_ok() {
                            return Some(deal);
                        }
                    }
                }
            }
        }
        None
    }

    fn action_specs(&self, me: TeamId) -> Vec<ActionSpec> {
        let team = &self.teams[me];
        let frozen = self.spending_frozen(me);
        let frozen_why = || {
            frozen.then(|| {
                "The board has frozen spending until the budget is above zero.".to_string()
            })
        };
        let mut out = Vec::new();

        out.push(spec(
            "TakeCharge",
            "Take the job at a club. Only once per world; already done here.",
            Some("A club is already in your charge.".into()),
            json!({"team": {"type": "club id", "range": [0, self.teams.len() - 1]}}),
            None,
        ));
        out.push(spec(
            "Advance",
            "Let the calendar run until a game, new mail or the draft. Returns why it stopped.",
            None,
            json!({}),
            Some(Command::Advance),
        ));
        out.push(spec(
            "Continue",
            "Pass one day: other clubs' games, reserve games and friendlies are played on their days. Does nothing while you have a game to play today.",
            None,
            json!({}),
            Some(Command::Continue),
        ));
        let over = self
            .season_over()
            .then(|| "The season is over.".to_string());
        let no_game = if over.is_some() {
            over.clone()
        } else if self.user_fixture().is_none() {
            Some("You have no game this round.".to_string())
        } else {
            None
        };
        out.push(spec(
            "PlayToday",
            "Play only today's games, with the assistant coaching yours; the rest of the round is played on its days.",
            no_game,
            json!({}),
            Some(Command::PlayToday),
        ));
        out.push(spec(
            "PlayRound",
            "Play this round's games with the assistant coaching yours.",
            over.clone(),
            json!({}),
            Some(Command::PlayRound),
        ));
        out.push(spec(
            "PlayWithInstructions",
            "Play this round with your game coached by your standing instructions (the assistant's game if you left none).",
            over,
            json!({"uses": "the club's stored instructions"}),
            Some(Command::PlayWithInstructions),
        ));
        out.push(spec(
            "PlayLive",
            "Not for direct use: a live game you coach is played with the live match requests of the protocol, and logged as this.",
            Some("Use kickoff and live inputs.".into()),
            json!({}),
            None,
        ));
        out.push(spec(
            "PlayLiveToday",
            "Not for direct use: a live game of today's games only, logged as this.",
            Some("Use kickoff and live inputs.".into()),
            json!({}),
            None,
        ));

        // Mail.
        let unread: Vec<u32> = self
            .inbox()
            .iter()
            .filter(|m| !m.read)
            .map(|m| m.id)
            .collect();
        out.push(spec(
            "MarkRead",
            "Mark a message read.",
            None,
            json!({"message": {"choices": unread}}),
            unread.first().map(|&id| Command::MarkRead(id)),
        ));
        let open_offers: Vec<Value> = (0..self.desk.offers.len())
            .filter(|&i| {
                self.desk.offers[i].state == OfferState::Open
                    && self.inbox().iter().any(|m| m.offer == Some(i))
            })
            .map(|i| {
                let o = &self.desk.offers[i];
                json!({"offer": i, "player": o.player, "player_name": self.players[o.player as usize].name,
                       "club": o.club, "fee": o.fee, "expires_day": o.expires})
            })
            .collect();
        out.push(spec(
            "AnswerOffer",
            "Accept or refuse another club's offer for one of your players. Accepting sells him; it lapses if the squad is at its minimum.",
            None,
            json!({"offer": {"choices": open_offers}, "accept": "bool"}),
            open_offers
                .first()
                .map(|o| Command::AnswerOffer { offer: o["offer"].as_u64().unwrap_or(0) as usize, accept: false }),
        ));
        let open_press: Vec<Value> = (0..self.desk.press.len())
            .filter(|&i| self.press_open(i))
            .map(|i| json!({"question": i, "topic": format!("{:?}", self.desk.press[i].topic),
                            "about": self.desk.press[i].about, "options": self.desk.press[i].options}))
            .collect();
        out.push(spec(
            "AnswerPress",
            "Answer a press question with option 0, 1 or 2.",
            None,
            json!({"question": {"choices": open_press}, "choice": {"range": [0, 2]}}),
            open_press.first().map(|q| Command::AnswerPress {
                question: q["question"].as_u64().unwrap_or(0) as usize,
                choice: 1,
            }),
        ));
        let open_sponsors: Vec<Value> = (0..self.econ.offers.len())
            .filter(|&i| self.econ.offers[i].club == me && self.econ.offers[i].open_on(self.day))
            .map(|i| {
                let o = &self.econ.offers[i];
                json!({"offer": i, "sponsor": self.econ.sponsors[o.sponsor].name, "years": o.years,
                       "fee_per_season": o.fee, "boards": o.slots.len(), "bonus": o.bonus.describe()})
            })
            .collect();
        out.push(spec(
            "AnswerSponsor",
            "Accept or refuse a sponsor's proposal for your boards.",
            None,
            json!({"offer": {"choices": open_sponsors}, "accept": "bool"}),
            open_sponsors.first().map(|o| Command::AnswerSponsor {
                offer: o["offer"].as_u64().unwrap_or(0) as usize,
                accept: false,
            }),
        ));

        // Market.
        let in_draft = |p: PlayerId| self.draft.as_ref().is_some_and(|d| d.pool.contains(&p));
        let bid_why = frozen_why().or_else(|| {
            (team.roster.len() >= MAX_ROSTER).then(|| "The squad is full.".to_string())
        });
        let can_bid = |p: PlayerId| {
            !self.players[p as usize].is_retired()
                && !team.roster.contains(&p)
                && !team.reserves.contains(&p)
                && self.bid_pending(p).is_none()
                && !in_draft(p)
        };
        let free: Vec<PlayerId> = (0..self.players.len() as PlayerId)
            .filter(|&p| self.team_of(p).is_none() && can_bid(p))
            .collect();
        let shortlisted: Vec<Value> = self
            .scouting
            .shortlist
            .iter()
            .filter(|&&p| can_bid(p))
            .map(|&p| json!({"player": p, "name": self.players[p as usize].name, "club": self.team_of(p), "market_value": self.value(p)}))
            .collect();
        // A bid the board's budgets would let through: the example must be
        // one `apply` accepts.
        let fits = |p: PlayerId, fee: i64| {
            let salary = self.players[p as usize].contract.salary as i64;
            fee <= team.budget
                && self.wage_check(me, salary).is_ok()
                && (self.team_of(p).is_none() || self.fee_check(me, fee).is_ok())
        };
        let example_bid = free
            .iter()
            .copied()
            .find(|&p| fits(p, 0))
            .map(|p| Command::Bid { player: p, fee: 0 })
            .or_else(|| {
                self.scouting
                    .shortlist
                    .iter()
                    .copied()
                    .find(|&p| {
                        can_bid(p)
                            && self.team_of(p).is_some_and(|t| t != me)
                            && fits(p, self.value(p).min(team.budget.max(0)))
                    })
                    .map(|p| Command::Bid {
                        player: p,
                        fee: self.value(p).min(team.budget.max(0)),
                    })
            })
            .or_else(|| {
                (0..self.players.len() as PlayerId)
                    .filter(|&p| {
                        can_bid(p)
                            && self.team_of(p).is_some()
                            && fits(p, self.value(p).min(team.budget.max(0)))
                    })
                    .min_by_key(|&p| self.value(p))
                    .map(|p| Command::Bid {
                        player: p,
                        fee: self.value(p).min(team.budget.max(0)),
                    })
            });
        out.push(spec(
            "Bid",
            "Offer a fee for a player of another club (answer comes by mail in two days), or sign a free agent with fee 0 on the spot. Any player of another club may be bid for; view_player shows his market value.",
            bid_why,
            json!({"player": {"free_agents": free, "shortlisted": shortlisted, "or": "any player id of another club not in the draft"},
                   "fee": {"range": [0, team.budget], "note": "0 for a free agent"}, "budget": team.budget,
                   "squad_size": team.roster.len(), "squad_max": MAX_ROSTER}),
            example_bid,
        ));
        let others: Vec<TeamId> = (0..self.teams.len()).filter(|&t| t != me).collect();
        let own_ids: Vec<PlayerId> = team.roster.clone();
        let trade_block = self
            .draft
            .as_ref()
            .map(|_| "No trades while the draft is being held.".to_string());
        // Look for a deal their general manager would take, so the example
        // is a real one.
        let sketch = if trade_block.is_none() {
            self.sketch_trade(me)
        } else {
            None
        };
        out.push(ActionSpec {
            kind: "ProposeTrade",
            available: trade_block.is_none(),
            why_not: trade_block,
            describe: "Propose a deal: players and picks each way plus cash. Check it first with the trade_check query, which answers as their general manager would. No trades while the draft is held. The example, when present, is a deal they would accept right now.",
            params: json!({"deal": {"club": others, "give": own_ids, "get": "players of that club", "give_picks": "indices of your picks",
                            "get_picks": "indices of their picks", "cash": "i64, negative if they pay"},
                   "payroll": self.payroll(me), "salary_cap": crate::season::SALARY_CAP,
                   "roster_limits": [MIN_ROSTER, MAX_ROSTER]}),
            example: sketch.map(Command::ProposeTrade),
        });
        let sl_full = self.scouting.shortlist.len() >= SHORTLIST_MAX;
        let sl_example = (0..self.players.len() as PlayerId).find(|&p| {
            !team.roster.contains(&p)
                && !team.reserves.contains(&p)
                && !self.scouting.is_shortlisted(p)
                && !in_draft(p)
                && self.team_of(p).is_some()
        });
        out.push(spec(
            "Shortlist",
            "Put a player on, or take him off, the shortlist: scouts watch him every round and the ranges narrow.",
            None,
            json!({"player": "any player id", "on": "bool", "shortlist_size": self.scouting.shortlist.len(), "shortlist_max": SHORTLIST_MAX,
                   "shortlisted": self.scouting.shortlist}),
            if sl_full {
                self.scouting.shortlist.first().map(|&p| Command::Shortlist { player: p, on: false })
            } else {
                sl_example.map(|p| Command::Shortlist { player: p, on: true })
            },
        ));

        // The scout's game, preparation and training sessions.
        let scout_example = (|| {
            let first = self.agenda(30).days.into_iter().flat_map(|d| d.items).find_map(|i| match i {
                crate::agenda::AgendaItem::Game(g) if g.competition != "friendly" => Some(g),
                _ => None,
            })?;
            let pick = first.watch.iter().find(|x| x.before_meeting && x.busy_with.is_none() && !x.assigned)?;
            Some(Command::AssignScout { team: first.opponent, day: pick.day })
        })();
        let no_scout = team
            .office
            .staffer(Job::Scout)
            .is_none()
            .then(|| "You have no chief scout.".to_string());
        out.push(spec(
            "AssignScout",
            "Send the chief scout to watch a club play on a day: he files an opponent report after the game (view reports). One game a day; replaces what he was to do that day. While at a game he does no player scouting elsewhere. The agenda lists, for each of our games, the opponent's games he can still watch before we meet.",
            no_scout,
            json!({"team": "the club to watch (not yours)", "day": "day number of a game of that club still to play, today or later"}),
            scout_example,
        ));
        out.push(spec(
            "UnassignScout",
            "Take the chief scout off his assignment on a day.",
            None,
            json!({"day": {"choices": self.scouting.assignments.iter().map(|a| a.day).collect::<Vec<_>>()}}),
            self.scouting.assignments.first().map(|a| Command::UnassignScout { day: a.day }),
        ));
        out.push(spec(
            "SetSession",
            "Set one session of the weekly training plan; slot 0..9 is Monday morning, Monday afternoon ... Friday afternoon. Use MatchPrep for match preparation: in the week of a league game these sessions prepare for the next opponent and help only as far as the scout's report on them is right (an edge of up to a quarter of a point of belief for four sessions and a fully right report), and they build nothing else. Sessions on a day lost to a long trip do not count.",
            None,
            json!({"slot": {"range": [0, 9]}, "session": names(&Session::ALL), "now": team.schedule.sessions}),
            Some(Command::SetSession {
                slot: 7,
                session: if team.schedule.sessions[7] == Session::MatchPrep { Session::Tactics } else { Session::MatchPrep },
            }),
        ));

        // Draft.
        let ours = self
            .draft
            .as_ref()
            .is_some_and(|d| d.on_clock() == Some(me));
        let pool: Vec<PlayerId> = self
            .draft
            .as_ref()
            .map(|d| d.pool.clone())
            .unwrap_or_default();
        let not_on_clock = (!ours).then(|| "It is not your pick.".to_string());
        out.push(spec(
            "DraftPick",
            "Take a prospect with your pick. Prospects are in the draft view with scouted ranges.",
            not_on_clock.clone(),
            json!({"player": {"choices": pool}}),
            pool.first().map(|&p| Command::DraftPick(p)),
        ));
        out.push(spec(
            "DraftAuto",
            "Let the assistant make this pick, or all of yours.",
            not_on_clock,
            json!({"all": "bool"}),
            Some(Command::DraftAuto { all: true }),
        ));

        // Staff.
        let candidates: Vec<Value> = self
            .staff_pool
            .iter()
            .enumerate()
            .map(|(i, s)| json!({"candidate": i, "name": s.name, "wage": s.wage,
                "rating_as": {"Assistant": s.rating(Job::Assistant), "Skills": s.rating(Job::Skills),
                              "Fitness": s.rating(Job::Fitness), "Scout": s.rating(Job::Scout),
                              "Press": s.rating(Job::Press)}}))
            .collect();
        let hire_example = (!self.staff_pool.is_empty()).then(|| {
            let job = Job::HIRED
                .into_iter()
                .find(|&j| team.office.staffer(j).is_none())
                .unwrap_or(Job::Assistant);
            Command::Hire { job, candidate: 0 }
        });
        out.push(spec(
            "Hire",
            "Give a job to a person on the staff market. The man in the job is paid a quarter of his wage and leaves. It can be refused for money; the refusal says so.",
            frozen_why(),
            json!({"job": names(&Job::HIRED), "candidate": {"choices": candidates}}),
            hire_example,
        ));
        let held: Vec<Job> = Job::HIRED
            .into_iter()
            .filter(|&j| team.office.staffer(j).is_some())
            .collect();
        out.push(spec(
            "Dismiss",
            "Let a staff member go (costs a quarter of his wage).",
            None,
            json!({"job": names(&held)}),
            held.first().map(|&j| Command::Dismiss(j)),
        ));

        // Squad.
        out.push(spec(
            "Promote",
            "Bring a reserve into the first team.",
            (team.roster.len() >= MAX_ROSTER)
                .then(|| format!("The first team is full at {MAX_ROSTER}.")),
            json!({"player": {"choices": team.reserves}}),
            team.reserves.first().map(|&p| Command::Promote(p)),
        ));
        let demote_why = if team.roster.len() <= MIN_ROSTER {
            Some(format!("The first team cannot go below {MIN_ROSTER}."))
        } else if team.reserves.len() >= RESERVE_MAX {
            Some("The reserve squad is full.".to_string())
        } else {
            None
        };
        // Not someone the rotation needs: keep at least five in it.
        let rotation: Vec<PlayerId> = team.tactics().rotation.iter().map(|s| s.player).collect();
        let spare: Vec<PlayerId> = team
            .roster
            .iter()
            .copied()
            .filter(|p| !rotation.contains(p))
            .collect();
        out.push(spec(
            "Demote",
            "Move a first-team player to the reserves for good (he will not like it). Players outside your rotation are the safe choice.",
            demote_why,
            json!({"player": {"choices": team.roster, "not_in_rotation": spare}}),
            spare.first().or(team.roster.first()).map(|&p| Command::Demote(p)),
        ));
        out.push(spec(
            "SetDown",
            "Whether a first-team player turns out for the reserves when he is not in the rotation.",
            None,
            json!({"player": {"choices": team.roster}, "on": "bool"}),
            team.roster.first().map(|&p| Command::SetDown { player: p, on: !team.down.contains(&p) }),
        ));

        // Training.
        let tr_example = team.roster.first().map(|&p| {
            let pl = &self.players[p as usize];
            let focus = TrainingFocus::ALL
                .into_iter()
                .find(|&f| (f, pl.intensity) != (pl.training, pl.intensity))
                .unwrap_or(TrainingFocus::ALL[0]);
            Command::SetTraining {
                player: p,
                focus,
                intensity: pl.intensity,
            }
        });
        out.push(spec(
            "SetTraining",
            "Set a player's training focus and intensity (heavy tires him).",
            None,
            json!({"player": {"choices": team.roster}, "focus": names(&TrainingFocus::ALL), "intensity": names(&Intensity::ALL)}),
            tr_example,
        ));
        let preset_example = Preset::ALL
            .into_iter()
            .map(Schedule::preset)
            .find(|s| *s != team.schedule)
            .map(Command::SetSchedule);
        out.push(spec(
            "SetSchedule",
            "Replace the team's weekly training plan: ten sessions (Monday morning, Monday afternoon, ... Friday afternoon). Build one from a preset or choose sessions freely.",
            None,
            json!({"schedule": {"sessions": {"length": 10, "choices": names(&Session::ALL)}},
                   "presets": names(&Preset::ALL), "preset_note": "the example is a preset; ask the training view for the sessions"}),
            preset_example,
        ));
        out.push(spec(
            "DelegateTraining",
            "Let the assistant redo the weekly plan and everyone's training each week.",
            None,
            json!({"on": "bool", "now": team.delegate_training}),
            Some(Command::DelegateTraining(!team.delegate_training)),
        ));
        out.push(spec(
            "AskAssistantToPlan",
            "The assistant plans the week and every player's training once, now.",
            None,
            json!({}),
            Some(Command::AskAssistantToPlan),
        ));

        // Tactics.
        let slots = team.playbook.len();
        let sug = self.suggest_tactics(me);
        let set_example = sug.map(|mut t| {
            let slot = team.active;
            if t == team.playbook[slot].tactics {
                t.pace = if t.pace > 10 { t.pace - 1 } else { t.pace + 1 };
            }
            Command::SetTactics { slot, tactics: t }
        });
        out.push(spec(
            "SetTactics",
            "Replace the system, sliders and rotation of playbook entry `slot`. The rotation lists only your own players, no repeats, at least five, with planned minutes, role and brief. The squad's familiarity with the entry is not changed, so a big change to a drilled system costs familiarity in games. The suggest_tactics query returns a valid rotation.",
            None,
            json!({"slot": {"range": [0, slots - 1]},
                   "tactics": {"offense": names(&OffenseSet::ALL), "defense": names(&DefenseScheme::ALL), "press": names(&Press::ALL),
                               "pace": {"range": [1, 20]}, "three_bias": {"range": [1, 20]},
                               "crash_boards": {"range": [1, 20]}, "aggression": {"range": [1, 20]},
                               "rotation": {"of": RotationSlot::example_shape(),
                                            "role": names(&Role::ALL), "brief": names(&Brief::ALL),
                                            "players": team.roster, "minutes": {"range": [1, 48]}},
                               "board": "optional drawn plays; leave as in the playbook view"}}),
            set_example,
        ));
        out.push(spec(
            "RenameTactic",
            "Rename a playbook entry.",
            None,
            json!({"slot": {"range": [0, slots - 1]}, "name": "string"}),
            Some(Command::RenameTactic {
                slot: 0,
                name: format!("{} II", team.playbook[0].name),
            }),
        ));
        out.push(spec(
            "UseTactic",
            "Use playbook entry `slot` in matches.",
            (slots <= 1).then(|| "There is only one entry.".to_string()),
            json!({"slot": {"range": [0, slots - 1], "active": team.active}}),
            (0..slots)
                .find(|&s| s != team.active)
                .map(Command::UseTactic),
        ));
        out.push(spec(
            "CopyTactic",
            "Copy a playbook entry into a new one (the squad starts with half the familiarity).",
            (slots >= MAX_TACTICS).then(|| "The playbook is full.".to_string()),
            json!({"from": {"range": [0, slots - 1]}, "playbook_max": MAX_TACTICS}),
            Some(Command::CopyTactic { from: team.active }),
        ));
        out.push(spec(
            "DeleteTactic",
            "Delete a playbook entry (at least one must stay).",
            (slots <= 1).then(|| "The playbook needs at least one tactic.".to_string()),
            json!({"slot": {"range": [0, slots - 1]}}),
            (slots > 1).then_some(Command::DeleteTactic(slots - 1)),
        ));

        // The hall.
        out.push(spec(
            "Renovate",
            "Refit the hall (+40 points of condition): better condition fills more seats.",
            self.works_blocked(me, false),
            json!({"cost": self.arena(me).renovation_cost()}),
            Some(Command::Renovate),
        ));
        out.push(spec(
            "Expand",
            "Build 1,000 more seats, open next season.",
            self.works_blocked(me, true),
            json!({"cost": self.arena(me).expansion_cost()}),
            Some(Command::Expand),
        ));

        // The books.
        let (lo, hi) = self.price_range(me);
        let now = self.ticket_price(me);
        let new_price = if now < hi { now + 1 } else { now - 1 };
        out.push(spec(
            "SetTicketPrice",
            "Charge this much for a ticket at home games, within the range. A higher price brings more per seat and fewer people; a lower one builds support and the fan base. The finance view's price.preview shows crowd and gate one step either side for the next home game, and the price that would take most.",
            None,
            json!({"price": {"range": [lo, hi], "now": now, "reference": self.fair_price(me)}}),
            (lo..=hi).contains(&new_price).then_some(Command::SetTicketPrice(new_price)),
        ));
        let to_wages = self.budget_move_limit(me, true);
        let to_fees = self.budget_move_limit(me, false);
        let move_example = if to_wages >= 1_000 {
            Some(Command::MoveBudget {
                to_wages: true,
                amount: 1_000,
            })
        } else if to_fees >= 1_000 {
            Some(Command::MoveBudget {
                to_wages: false,
                amount: 1_000,
            })
        } else {
            None
        };
        out.push(spec(
            "MoveBudget",
            "Move money between the board's wage budget and transfer budget. The donor keeps at least half of what the board first gave and whatever is committed.",
            None,
            json!({"to_wages": "true moves transfer money to wages, false the other way", "amount": "i64 above zero",
                   "most_to_wages": to_wages, "most_to_transfers": to_fees}),
            move_example,
        ));
        let facility_choices: Vec<Value> = crate::books::FacilityKind::ALL
            .iter()
            .map(|&k| {
                json!({"kind": format!("{k:?}"), "label": k.label(), "level": self.facilities(me).level(k),
                       "blocked": self.works_refused(me, k)})
            })
            .collect();
        let facility_example = crate::books::FacilityKind::ALL
            .into_iter()
            .find(|&k| self.works_refused(me, k).is_none())
            .map(Command::UpgradeFacility);
        out.push(spec(
            "UpgradeFacility",
            "Start building the training ground (Training), medical centre (Medical) or youth setup (Youth) up one level: a lump sum now, six rounds to build, a higher running cost after. One build at a time.",
            frozen_why(),
            json!({"kind": {"choices": facility_choices}}),
            facility_example,
        ));

        // Media work and appearances.
        let media_example = team.roster.iter().find_map(|&p| {
            let now = self.players[p as usize].renown.media;
            [
                crate::media::MediaLevel::Some,
                crate::media::MediaLevel::ALot,
                crate::media::MediaLevel::None,
            ]
            .into_iter()
            .find(|&l| l != now)
            .map(|level| Command::SetMedia { player: p, level })
        });
        let media_now: Vec<Value> = team
            .roster
            .iter()
            .map(|&p| json!({"player": p, "level": self.players[p as usize].renown.media}))
            .collect();
        out.push(spec(
            "SetMedia",
            "Give a first-team player a weekly level of media and community work: None, Some or ALot. It raises his fame a little and the club's fan base, teaches him to talk (faster with a good press officer), and costs training gain and freshness in proportion. A poor talker may say the wrong thing; a good one may earn a glowing story.",
            None,
            json!({"player": {"choices": team.roster}, "level": names(&crate::media::MediaLevel::ALL), "now": media_now}),
            media_example,
        ));
        let open_asks: Vec<Value> = (0..self.fame.asks.len())
            .filter(|&i| self.appearance_open(i))
            .map(|i| {
                let a = &self.fame.asks[i];
                json!({"request": i, "sponsor": self.econ.sponsors[a.sponsor].name, "player": a.player,
                       "player_name": self.players[a.player as usize].name, "fee": a.fee,
                       "event_day": a.event_day, "expires_day": a.expires})
            })
            .collect();
        out.push(spec(
            "AnswerAppearance",
            "Accept or decline a sponsor's request for a player at an event. The fee is paid on the day if he is fit; he is tired and loses some of the week's training.",
            None,
            json!({"request": {"choices": open_asks}, "accept": "bool"}),
            open_asks.first().map(|r| Command::AnswerAppearance {
                request: r["request"].as_u64().unwrap_or(0) as usize,
                accept: true,
            }),
        ));

        // Instructions.
        out.push(spec(
            "SetInstructions",
            "Leave standing match instructions (if-then rules) for games you do not coach live. An empty list leaves the game to the assistant. Limits: 16 rules, 8 conditions and 4 actions per rule, max_fires 1 to 10.",
            None,
            json!({"instructions": {"rules": {"max": crate::instructions::MAX_RULES,
                "rule": {"name": "string", "when": "list of conditions, all must hold", "then": "list of actions",
                         "max_fires": [1, crate::instructions::MAX_FIRES], "cooldown_secs": "game seconds between firings"}}},
                   "example": "see the docs: {\"rules\":[{\"when\":[{\"OpponentRunAtLeast\":8}],\"then\":[\"Timeout\"],\"max_fires\":2}]}"}),
            Some(Command::SetInstructions(crate::instructions::Instructions {
                rules: if team.instructions.rules.is_empty() {
                    vec![crate::instructions::Rule {
                        name: "stop the run".into(),
                        when: vec![crate::instructions::Condition::OpponentRunAtLeast(8)],
                        then: vec![crate::instructions::Action::Timeout],
                        max_fires: 2,
                        cooldown_secs: 60,
                    }]
                } else {
                    Vec::new()
                },
            })),
        ));
        out.extend(self.job_specs());
        out
    }
}

impl RotationSlot {
    /// The shape of a rotation slot, for the parameter descriptions.
    fn example_shape() -> Value {
        json!({"player": "player id", "minutes": "u8", "role": "Role", "brief": "Brief", "position": "optional Position or null"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::tests::tidy;
    use crate::inbox::Stop;

    /// Does the command succeed on a copy of the world?
    fn accepted(w: &World, cmd: &Command) -> Result<(), String> {
        let mut copy = w.clone();
        copy.apply(cmd.clone())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn check(w: &World) -> usize {
        let legal = w.legal(w.watch.unwrap()).unwrap();
        let mut ok = 0;
        for a in &legal.actions {
            match (&a.example, a.available) {
                (Some(cmd), true) => {
                    // The kind name matches the serialised command.
                    let json = serde_json::to_value(cmd).unwrap();
                    let kind = json
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| json.as_object().and_then(|o| o.keys().next().cloned()));
                    assert_eq!(kind.as_deref(), Some(a.kind));
                    accepted(w, cmd).unwrap_or_else(|e| panic!("{} refused: {e}", a.kind));
                    ok += 1;
                }
                (None, false) => assert!(a.why_not.is_some(), "{} gives no reason", a.kind),
                (e, av) => panic!("{}: example {e:?} but available {av}", a.kind),
            }
        }
        ok
    }

    #[test]
    fn what_it_lists_is_accepted_and_what_it_refuses_is_refused() {
        let mut w = World::generate(31, 8);
        w.apply(Command::TakeCharge(2)).unwrap();
        // The board's patience is not what this test is about: this club
        // finishes last without a manager's hand on it.
        w.club.clubs[2].board.owner.patience = 20;
        w.club.clubs[2].board.chairman = None;
        assert_eq!(w.legal(0), None);
        let n = check(&w);
        assert!(n >= 15, "only {n} examples");
        // Marked impossible, and refused when tried.
        let legal = w.legal(2).unwrap();
        let by = |k: &str| legal.actions.iter().find(|a| a.kind == k).unwrap().clone();
        assert!(!by("TakeCharge").available);
        for k in [
            "SetTicketPrice",
            "MoveBudget",
            "UpgradeFacility",
            "SetMedia",
        ] {
            assert!(by(k).available, "{k} is not available");
        }
        assert!(legal.actions.iter().any(|a| a.kind == "AnswerAppearance"));
        assert!(w.clone().apply(Command::TakeCharge(1)).is_err());
        assert!(!by("DraftPick").available);
        assert!(w.clone().apply(Command::DraftPick(0)).is_err());
        assert!(!by("DeleteTactic").available);
        assert!(w.clone().apply(Command::DeleteTactic(0)).is_err());
        assert!(!by("Promote").available || !w.teams[2].reserves.is_empty());

        // Through the season, at several points, including the draft.
        let mut n = 0;
        let mut seen_draft = false;
        let mut seen_offer = false;
        for _ in 0..4000 {
            tidy_except_offers(&mut w, &mut n);
            let wait = waiting(&w);
            if matches!(wait, Waiting::Offer { .. }) {
                seen_offer = true;
                check(&w);
                let legal = w.legal(2).unwrap();
                let a = legal
                    .actions
                    .iter()
                    .find(|a| a.kind == "AnswerOffer")
                    .unwrap();
                assert!(a.available);
            }
            if wait == Waiting::DraftPick {
                seen_draft = true;
                check(&w);
                let legal = w.legal(2).unwrap();
                assert!(
                    legal
                        .actions
                        .iter()
                        .find(|a| a.kind == "DraftPick")
                        .unwrap()
                        .available
                );
                assert!(
                    legal
                        .actions
                        .iter()
                        .find(|a| a.kind == "ProposeTrade")
                        .unwrap()
                        .why_not
                        .is_some()
                );
                w.apply(Command::DraftAuto { all: true }).unwrap();
                break;
            }
            if w.round.is_multiple_of(3) {
                check(&w);
            }
            match w.apply(Command::Advance).unwrap().stop.unwrap() {
                Stop::MatchDay => {
                    w.apply(Command::PlayRound).unwrap();
                }
                Stop::Mail | Stop::Draft => {}
            }
        }
        assert!(seen_draft, "never reached the draft");
        let _ = seen_offer;
        check(&w);
    }

    /// Answer press and sponsors, but leave offers for the test to see.
    fn tidy_except_offers(w: &mut World, n: &mut usize) {
        for i in 0..w.desk.press.len() {
            if w.press_open(i) {
                w.apply(Command::AnswerPress {
                    question: i,
                    choice: (i % 3) as u8,
                })
                .unwrap();
            }
        }
        if w.needs_answer().is_some() {
            // Answer it anyway so the calendar goes on; the check above ran.
            tidy(w, n);
        }
    }

    #[test]
    fn suggested_tactics_are_valid() {
        let mut w = World::generate(4, 8);
        w.apply(Command::TakeCharge(5)).unwrap();
        let t = w.suggest_tactics(5).unwrap();
        // The suggestion is for the current sliders with a fresh rotation.
        let mut t2 = t.clone();
        t2.pace = if t.pace > 10 { 6 } else { 15 };
        w.apply(Command::SetTactics {
            slot: 0,
            tactics: t2,
        })
        .unwrap();
        assert!(!t.rotation.is_empty());
    }
}
