//! The operations an agent can perform, shared by the line protocol
//! (`basketman play`) and the MCP server (`basketman mcp`).
//!
//! A `Session` owns one world and, while a game is being coached, one live
//! round. `call(op, args)` takes a JSON object and returns JSON or an
//! `AgentError`. It never panics outward: a panic inside the game is caught
//! and reported as an `internal` error. Responses depend only on the
//! requests so far: no clock, no randomness, objects with sorted keys.

use std::panic::{AssertUnwindSafe, catch_unwind};

use basketman_core::command::{Command, CommandError, Outcome};
use basketman_core::engine::{Orders, Pause};
use basketman_core::inbox::{Deal, Stop};
use basketman_core::instructions::{Coach, Instructions};
use basketman_core::league::{LiveInput, LiveRound, Refusal, Stoppage, TeamId, World};
use basketman_core::player::PlayerId;
use basketman_core::staff::Job;
use basketman_core::save;
use serde::Serialize;
use serde_json::{Map, Value, json};

/// Why an operation did not happen.
#[derive(Debug)]
pub struct AgentError {
    /// Machine-readable: `no_game`, `no_club`, `bad_request`, `refused`, ...
    pub kind: &'static str,
    pub message: String,
}

impl AgentError {
    pub fn new(kind: &'static str, message: impl Into<String>) -> Self {
        AgentError {
            kind,
            message: message.into(),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({"kind": self.kind, "message": self.message})
    }
}

fn bad(message: impl Into<String>) -> AgentError {
    AgentError::new("bad_request", message)
}

impl From<CommandError> for AgentError {
    fn from(e: CommandError) -> Self {
        let kind = match &e {
            CommandError::NoClub => "no_club",
            CommandError::NoSuchPlayer(_) => "no_such_player",
            CommandError::NotYourPlayer(_) => "not_your_player",
            CommandError::NoSuchTeam(_) => "no_such_team",
            CommandError::OutOfRange(_) => "out_of_range",
            CommandError::NothingToDo => "nothing_to_do",
            CommandError::Refused(_) => "refused",
        };
        AgentError::new(kind, e.to_string())
    }
}

type Res = Result<Value, AgentError>;

fn to_json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// A game being coached: the round, and the standing-rules coach if the
/// manager wants his rules applied while stepping.
struct Live {
    round: LiveRound,
    coach: Option<Coach>,
    team: TeamId,
    /// What the assistant has noticed (see `feeds`).
    notes: crate::feeds::Notes,
}

#[derive(Default)]
pub struct Session {
    world: Option<World>,
    live: Option<Live>,
}

/// `{"type": "Advance"}` or `{"type": "Bid", "player": 3, "fee": 0}` as
/// well as serde's own `"Advance"` / `{"Bid": {"player": 3, "fee": 0}}`.
fn flat(v: &Value) -> Value {
    let Some(o) = v.as_object() else {
        return v.clone();
    };
    let Some(Value::String(t)) = o.get("type") else {
        return v.clone();
    };
    let rest: Map<String, Value> = o
        .iter()
        .filter(|(k, _)| *k != "type")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if rest.is_empty() {
        Value::String(t.clone())
    } else if rest.len() == 1 && o.contains_key("value") {
        // Tuple variants: {"type": "MarkRead", "value": 3}.
        json!({ t.clone(): rest["value"].clone() })
    } else {
        json!({ t.clone(): Value::Object(rest) })
    }
}

fn parse_command(v: &Value) -> Result<Command, AgentError> {
    serde_json::from_value(flat(v)).map_err(|e| bad(format!("not a command: {e}")))
}

fn get_u64(a: &Value, key: &str) -> Result<Option<u64>, AgentError> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| bad(format!("{key} must be a non-negative integer"))),
    }
}

fn need_u64(a: &Value, key: &str) -> Result<u64, AgentError> {
    get_u64(a, key)?.ok_or_else(|| bad(format!("{key} is required")))
}

fn get_str<'a>(a: &'a Value, key: &str) -> Result<Option<&'a str>, AgentError> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| bad(format!("{key} must be a string"))),
    }
}

fn get_bool(a: &Value, key: &str) -> Result<Option<bool>, AgentError> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_bool()
            .map(Some)
            .ok_or_else(|| bad(format!("{key} must be true or false"))),
    }
}

fn stop_name(s: Stop) -> &'static str {
    match s {
        Stop::MatchDay => "match_day",
        Stop::Mail => "mail",
        Stop::Draft => "draft",
    }
}

fn pause_name(p: Pause, me: usize) -> Value {
    match p {
        Pause::Timeout(s) => json!({"timeout": if s == me { "you" } else { "opponent" }}),
        Pause::Review(_) => json!("review"),
        Pause::PeriodEnd => json!("period_end"),
        Pause::Final => json!("final"),
    }
}

/// Whether play is live or stopped, why, and what the coach may do now.
fn play_json(live: &LiveRound) -> Value {
    let me = live.side;
    let stop = live.stoppage();
    let (state, why) = match stop {
        None => ("live", Value::Null),
        Some(Stoppage::Tipoff) => ("stopped", json!("tipoff")),
        Some(Stoppage::Timeout(s)) => (
            "stopped",
            json!(if s == me { "timeout_yours" } else { "timeout_theirs" }),
        ),
        Some(Stoppage::Break { half: true, .. }) => ("stopped", json!("half_time")),
        Some(Stoppage::Break { .. }) => ("stopped", json!("period_break")),
        Some(Stoppage::Review) => ("stopped", json!("review")),
        Some(Stoppage::Final) => ("over", json!("final")),
    };
    let (team, words) = live.talks_left();
    let mut may = vec!["Timeout", "AutoSubs", "Swap", "CancelSwap"];
    if matches!(stop, Some(s) if s != Stoppage::Final && s != Stoppage::Review) {
        may.push("Orders");
        may.push("Brief");
        if team > 0 || words > 0 {
            may.push("Shout");
        }
    }
    if stop == Some(Stoppage::Final) {
        may.clear();
    }
    if stop == Some(Stoppage::Review) {
        may = vec!["Challenge"];
    }
    json!({
        "state": state,
        "stopped_for": why,
        "may": may,
        "stoppage_seconds": stop.map(Stoppage::seconds),
        "talks_left": {"team": team, "players": words},
        "note": if stop.is_none() {
            json!("Play is live: tactics, instructions and talk are for a timeout, a break or before the tip. Call a Timeout (taken at the next dead ball), then step to it.")
        } else { Value::Null },
    })
}

/// What a command's outcome looks like to the agent.
fn outcome_json(w: &World, o: &Outcome) -> Value {
    let me = w.watch;
    let mut out = Map::new();
    if let Some(n) = &o.note {
        out.insert("note".into(), json!(n));
    }
    if let Some(s) = o.stop {
        out.insert("stop".into(), json!(stop_name(s)));
    }
    if let Some(s) = o.slot {
        out.insert("slot".into(), json!(s));
    }
    if let Some(d) = o.day {
        out.insert("date".into(), json!(w.day_label(d)));
        out.insert("weekday".into(), json!(w.weekday(d)));
        // What is on the new day for the manager's own club.
        let mine: Vec<Value> = w
            .events_on(d)
            .iter()
            .filter(|e| me.is_some() && (e.home == me || e.away == me) || e.home.is_none())
            .map(|e| to_json(e))
            .collect();
        out.insert("today".into(), Value::Array(mine));
    }
    if !o.played.is_empty() {
        out.insert("played".into(), json!(o.played));
        let games: Vec<Value> = o
            .played
            .iter()
            .filter_map(|&i| {
                let f = &w.fixtures[i];
                let r = f.result.as_ref()?;
                Some(json!({
                    "fixture": i,
                    "home": w.teams[f.home].name,
                    "away": w.teams[f.away].name,
                    "score": r.score,
                    "yours": Some(f.home) == me || Some(f.away) == me,
                }))
            })
            .collect();
        out.insert("games".into(), Value::Array(games));
    }
    Value::Object(out)
}

impl Session {
    pub fn new() -> Self {
        Session::default()
    }

    /// Run `op`; a panic inside is an `internal` error, not a crash.
    pub fn call(&mut self, op: &str, args: &Value) -> Res {
        match catch_unwind(AssertUnwindSafe(|| self.dispatch(op, args))) {
            Ok(r) => r,
            Err(_) => Err(AgentError::new(
                "internal",
                "the game hit an internal error on this request; the world may be unchanged or half-changed, save and reload to be safe",
            )),
        }
    }

    fn world(&self) -> Result<&World, AgentError> {
        self.world
            .as_ref()
            .ok_or_else(|| AgentError::new("no_game", "start with new_game or load"))
    }

    fn club(&self) -> Result<TeamId, AgentError> {
        self.world()?
            .watch
            .ok_or_else(|| AgentError::new("no_club", "nobody is in charge of a club"))
    }

    fn no_live(&self) -> Result<(), AgentError> {
        if self.live.is_some() {
            Err(AgentError::new(
                "live_match_open",
                "a live match is open: step it and call live_finish first",
            ))
        } else {
            Ok(())
        }
    }

    fn dispatch(&mut self, op: &str, a: &Value) -> Res {
        match op {
            "new_game" => self.new_game(a),
            "load" => self.load(a),
            "save" => self.save(a),
            "view" => self.view(a),
            "legal" => {
                let w = self.world()?;
                if w.watch.is_none() {
                    return Ok(to_json(
                        &w.legal_jobless().ok_or_else(|| bad("nobody is in charge of a club"))?,
                    ));
                }
                let me = self.club()?;
                Ok(to_json(&w.legal(me).ok_or_else(|| bad("no club"))?))
            }
            "suggest_tactics" => {
                let w = self.world()?;
                let me = self.club()?;
                Ok(to_json(
                    &w.suggest_tactics(me).ok_or_else(|| bad("no club"))?,
                ))
            }
            "trade_check" => self.trade_check(a),
            "apply" => self.apply(a.get("command").unwrap_or(a)),
            "advance" => self.apply(&json!("Advance")),
            "continue" => self.apply(&json!("Continue")),
            "play_round" => {
                let today = get_bool(a, "today")?.unwrap_or(false);
                if today {
                    // Standing instructions, if there are any, coach it.
                    self.apply(&json!("PlayToday"))
                } else if get_bool(a, "instructions")?.unwrap_or(false) {
                    self.apply(&json!("PlayWithInstructions"))
                } else {
                    self.apply(&json!("PlayRound"))
                }
            }
            "set_instructions" => {
                let ins: Instructions = serde_json::from_value(
                    a.get("instructions").cloned().unwrap_or_else(|| a.clone()),
                )
                .map_err(|e| bad(format!("not instructions: {e}")))?;
                self.apply(&json!({"SetInstructions": ins}))
            }
            "get_instructions" => {
                let w = self.world()?;
                Ok(to_json(&w.teams[self.club()?].instructions))
            }
            "log" => {
                let w = self.world()?;
                let from = get_u64(a, "from")?.unwrap_or(0) as usize;
                Ok(
                    json!({"length": w.log.len(), "commands": to_json(&w.log.get(from..).unwrap_or(&[]))}),
                )
            }
            "kickoff" => self.kickoff(a),
            "live_state" => Ok(self.live_state()?),
            "live_step" => self.live_step(a),
            "live_input" => self.live_input(a),
            "live_feeds" => {
                let w = self.world()?;
                let live = self.live.as_ref().ok_or_else(|| {
                    AgentError::new("no_live_match", "no live match: use kickoff")
                })?;
                crate::feeds::report(w, &live.round, live.team, &live.notes, a)
            }
            "live_finish" => self.live_finish(),
            "ping" => Ok(json!("pong")),
            other => Err(AgentError::new(
                "unknown_op",
                format!("no operation called {other}"),
            )),
        }
    }

    fn new_game(&mut self, a: &Value) -> Res {
        self.no_live()?;
        let seed = get_u64(a, "seed")?.unwrap_or(1);
        let clubs = get_u64(a, "clubs")?.unwrap_or(12) as usize;
        let club = get_u64(a, "club")?.unwrap_or(0) as usize;
        let divisions = get_u64(a, "divisions")?.unwrap_or(3) as usize;
        if !(2..=20).contains(&clubs) || !clubs.is_multiple_of(2) {
            return Err(bad("clubs must be an even number from 2 to 20"));
        }
        if !(1..=3).contains(&divisions) {
            return Err(bad("divisions must be 1, 2 or 3"));
        }
        if club >= clubs * divisions {
            return Err(bad(format!("club must be 0..={}", clubs * divisions - 1)));
        }
        let mut w = World::new_career_tiers(seed, clubs, divisions);
        w.apply(Command::TakeCharge(club))?;
        self.world = Some(w);
        self.view(&json!({"what": "summary"}))
    }

    fn load(&mut self, a: &Value) -> Res {
        self.no_live()?;
        let path = get_str(a, "path")?.ok_or_else(|| bad("path is required"))?;
        let bytes = std::fs::read(path)
            .map_err(|e| AgentError::new("io", format!("cannot read {path}: {e}")))?;
        let w = save::decode(&bytes).map_err(|e| AgentError::new("bad_save", e.to_string()))?;
        self.world = Some(w);
        if self.world()?.watch.is_some() {
            self.view(&json!({"what": "summary"}))
        } else if self.world()?.jobless() {
            let career = to_json(&self.world()?.view_career());
            Ok(json!({"loaded": true, "jobless": true, "career": career,
                      "note": "you are out of work: advance, play_round, and answer the offers that come"}))
        } else {
            Ok(json!({"loaded": true, "note": "no club is managed in this save"}))
        }
    }

    fn save(&mut self, a: &Value) -> Res {
        self.no_live()?;
        let path = get_str(a, "path")?.ok_or_else(|| bad("path is required"))?;
        let bytes = save::encode(self.world()?);
        std::fs::write(path, &bytes)
            .map_err(|e| AgentError::new("io", format!("cannot write {path}: {e}")))?;
        Ok(json!({"saved": path, "bytes": bytes.len()}))
    }

    fn view(&self, a: &Value) -> Res {
        let w = self.world()?;
        let what = get_str(a, "what")?.unwrap_or("summary");
        let none = || AgentError::new("view_unavailable", "no such thing to show");
        // These work without a club too: an out-of-work manager reads his
        // career and the offers.
        match what {
            "career" => return Ok(to_json(&w.view_career())),
            "inbox" if w.jobless() => {
                return Ok(to_json(&w.view_inbox(0).ok_or_else(none)?));
            }
            "job_offer" => {
                let offer = need_u64(a, "offer")? as usize;
                let jobs = |key: &str| -> Result<Vec<Job>, AgentError> {
                    match a.get(key) {
                        None | Some(Value::Null) => Ok(Vec::new()),
                        Some(v) => serde_json::from_value(v.clone())
                            .map_err(|e| bad(format!("{key} must be a list of jobs: {e}"))),
                    }
                };
                let bring = jobs("bring")?;
                // Without `keep`, everybody at the new club stays.
                let keep = if a.get("keep").is_some_and(|k| !k.is_null()) {
                    jobs("keep")?
                } else {
                    Job::HIRED.to_vec()
                };
                return w
                    .view_job_offer(offer, &bring, &keep)
                    .map(|v| to_json(&v))
                    .map_err(|e| AgentError::new("refused", e));
            }
            "officials" => {
                let me = w.watch;
                let person = |r: &basketman_core::officials::Referee| {
                    json!({"id": r.id, "name": r.name, "age": r.age, "known_for": r.tags(),
                           "accuracy": r.accuracy, "strictness": r.strictness, "consistency": r.consistency,
                           "home_lean": r.home_lean, "composure": r.composure,
                           "game_management": r.game_mgmt,
                           "season": {"games": r.season.games, "fouls_per_game": r.season.fouls_per_game(),
                                      "home_balance": r.season.home_balance(), "overturned": r.season.overturned,
                                      "fouls_on_leading": r.season.fouls_lead, "fouls_on_trailing": r.season.fouls_trail,
                                      "leading_minus_trailing_per_game": r.season.lead_balance(),
                                      "reviewed": r.season.reviewed},
                           "warned": r.warned})
                };
                let next = me.and_then(|me| {
                    let i = w.fixtures.iter().position(|f| {
                        f.round == w.round && f.result.is_none() && (f.home == me || f.away == me)
                    })?;
                    let ids = w.crew_of(basketman_core::officials::GameKey::League(i))?;
                    Some(json!({"fixture": i, "crew": w.referees_of(ids).iter().map(|r| person(r)).collect::<Vec<_>>()}))
                });
                let pool: Vec<Value> = w.officials.pool.iter().filter(|r| r.active).map(person).collect();
                return Ok(json!({"next_game": next, "referees": pool, "office": w.officials.notices}));
            }
            "club_profile" => {
                let t = match get_u64(a, "club")? {
                    Some(t) => t as usize,
                    None => w.watch.ok_or_else(|| bad("club is required when you have no job"))?,
                };
                if t >= w.teams.len() {
                    return Err(AgentError::new("out_of_range", format!("there is no club {t}")));
                }
                return Ok(to_json(&w.club_summary(t)));
            }
            _ => {}
        }
        let me = self.club()?;
        Ok(match what {
            "summary" => to_json(&w.view_summary(me).ok_or_else(none)?),
            "full" => to_json(&w.view(me).ok_or_else(none)?),
            "own" => to_json(&w.view(me).ok_or_else(none)?.own),
            "squad" => {
                let (squad, reserves) = w.view_squad(me).ok_or_else(none)?;
                json!({"first_team": to_json(&squad), "reserves": to_json(&reserves)})
            }
            "player" => {
                let p = need_u64(a, "player")? as PlayerId;
                to_json(&w.view_player(me, p).ok_or_else(|| {
                    AgentError::new("out_of_range", format!("there is no player {p}"))
                })?)
            }
            "club" => {
                let t = need_u64(a, "club")? as usize;
                to_json(&w.view_club(me, t).ok_or_else(|| {
                    AgentError::new("out_of_range", format!("there is no club {t}"))
                })?)
            }
            "inbox" => {
                let mut v = to_json(&w.view_inbox(me).ok_or_else(none)?);
                if get_bool(a, "unread_only")?.unwrap_or(false)
                    && let Some(m) = v.get_mut("messages").and_then(Value::as_array_mut)
                {
                    m.retain(|x| x["read"] == json!(false));
                }
                v
            }
            "table" => match get_u64(a, "division")? {
                Some(d) if (d as usize) < w.division_count() => to_json(&w.view_table_of(d as usize)),
                Some(_) => return Err(bad("no such division")),
                None => to_json(&w.view_table()),
            },
            "fixtures" => {
                let mut f = w.view_fixtures();
                if let Some(r) = get_u64(a, "round")? {
                    f.retain(|x| x.round == r as usize);
                }
                if let Some(c) = get_u64(a, "club")? {
                    f.retain(|x| x.home == c as usize || x.away == c as usize);
                }
                if let Some(d) = get_u64(a, "division")? {
                    f.retain(|x| x.division == d as usize);
                }
                to_json(&f)
            }
            "box_score" => {
                let i = need_u64(a, "fixture")? as usize;
                to_json(
                    &w.view_box_score(i)
                        .ok_or_else(|| AgentError::new("out_of_range", "no such played game"))?,
                )
            }
            "market" => to_json(&w.view_market(me).ok_or_else(none)?),
            "draft" => to_json(&w.view_draft(me)),
            "calendar" => {
                let mut v = to_json(&w.view_calendar());
                // `day`, or `from` and `to`: what is on, across the league
                // (or for one `club`).
                let (from, to) = match get_u64(a, "day")? {
                    Some(d) => (Some(d as u32), Some(d as u32)),
                    None => (
                        get_u64(a, "from")?.map(|x| x as u32),
                        get_u64(a, "to")?.map(|x| x as u32),
                    ),
                };
                if from.is_some() || to.is_some() {
                    let from = from.unwrap_or(0);
                    let to = to.unwrap_or(from + 30);
                    let club = get_u64(a, "club")?.map(|c| c as usize);
                    let events: Vec<Value> = w
                        .calendar_events(from, to)
                        .iter()
                        .filter(|e| club.is_none() || e.home == club || e.away == club)
                        .map(|e| {
                            let mut j = to_json(e);
                            j["date"] = json!(w.day_label(e.day));
                            j["weekday"] = json!(w.weekday(e.day));
                            j
                        })
                        .collect();
                    v["events"] = Value::Array(events);
                }
                v
            }
            "agenda" => {
                let days = get_u64(a, "days")?.unwrap_or(21).clamp(1, 120) as u32;
                to_json(&w.agenda(days))
            }
            "reports" => {
                // The scout's assignments and his reports, newest first,
                // each with its text; `club` narrows them to one opponent
                // and adds the games of theirs he could still watch.
                let club = get_u64(a, "club")?.map(|c| c as usize);
                if club.is_some_and(|c| c >= w.teams.len()) {
                    return Err(AgentError::new("out_of_range", "there is no such club"));
                }
                let mut all: Vec<&basketman_core::opponent::OpponentReport> = match club {
                    Some(c) => w.reports_on(c),
                    None => w.scouting.opponent_reports.iter().rev().collect(),
                };
                all.truncate(20);
                let reports: Vec<Value> = all
                    .iter()
                    .map(|r| {
                        let mut j = to_json(r);
                        j["date"] = json!(w.day_label(r.day));
                        j["club_name"] = json!(w.teams[r.team].name);
                        j["confidence_label"] = json!(r.confidence_label());
                        j["text"] = json!(w.report_text(r));
                        j
                    })
                    .collect();
                let assignments: Vec<Value> = w
                    .scouting
                    .assignments
                    .iter()
                    .map(|s| {
                        json!({"day": s.day, "date": w.day_label(s.day), "club": s.team,
                               "club_name": w.teams[s.team].name})
                    })
                    .collect();
                let watchable: Vec<Value> = club
                    .map(|c| {
                        w.watchable_games(c)
                            .into_iter()
                            .map(|(day, vs, home)| {
                                json!({"day": day, "date": w.day_label(day), "vs": vs,
                                       "vs_name": w.teams[vs].name, "home": home})
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                json!({"has_scout": w.teams[me].office.staffer(Job::Scout).is_some(),
                       "assignments": assignments, "reports": reports, "watchable": watchable})
            }
            "leaders" => {
                let n = get_u64(a, "n")?.unwrap_or(15) as usize;
                match get_u64(a, "division")? {
                    Some(d) if (d as usize) < w.division_count() => {
                        to_json(&w.view_leaders_in(d as usize, n))
                    }
                    Some(_) => return Err(bad("no such division")),
                    None => to_json(&w.view_leaders(n)),
                }
            }
            "league" => to_json(&w.view_league(me).ok_or_else(none)?),
            "finance" => to_json(&w.view_finance(me).ok_or_else(none)?),
            "fans" => to_json(&w.view_finance(me).ok_or_else(none)?.fans),
            "names" => to_json(&w.view_names(get_u64(a, "n")?.unwrap_or(10) as usize)),
            "appearances" => to_json(&w.view_appearances(me).ok_or_else(none)?),
            other => {
                return Err(bad(format!(
                    "unknown view {other}; try summary, full, own, squad, player, club, club_profile, career, job_offer, inbox, table, fixtures, box_score, market, draft, calendar, agenda, reports, leaders, league, finance, fans, names, appearances"
                )));
            }
        })
    }

    fn trade_check(&self, a: &Value) -> Res {
        let w = self.world()?;
        let deal: Deal =
            serde_json::from_value(a.get("deal").cloned().unwrap_or_else(|| a.clone()))
                .map_err(|e| bad(format!("not a deal: {e}")))?;
        if deal.club >= w.teams.len() {
            return Err(AgentError::new("no_such_team", "there is no such club"));
        }
        Ok(match w.trade_verdict(&deal) {
            Ok(text) => json!({"acceptable": true, "verdict": text}),
            Err(text) => json!({"acceptable": false, "verdict": text}),
        })
    }

    fn apply(&mut self, cmd: &Value) -> Res {
        self.no_live()?;
        let cmd = parse_command(cmd)?;
        let w = self
            .world
            .as_mut()
            .ok_or_else(|| AgentError::new("no_game", "start with new_game or load"))?;
        if matches!(cmd, Command::PlayLive { .. }) {
            return Err(AgentError::new(
                "refused",
                "PlayLive is written by the game when a live match finishes; use kickoff",
            ));
        }
        let out = w.apply(cmd)?;
        let w = self.world()?;
        let mut v = outcome_json(w, &out);
        if let Some(me) = w.watch {
            v["waiting"] = to_json(&basketman_core::legal::waiting(w));
            v["day"] = json!(w.day);
            v["round"] = json!(w.round);
            v["season"] = json!(w.season + 1);
            let _ = me;
        } else if w.jobless() {
            v["waiting"] = to_json(&basketman_core::legal::waiting(w));
            v["jobless"] = json!(true);
            v["season"] = json!(w.season + 1);
        }
        Ok(v)
    }

    fn kickoff(&mut self, a: &Value) -> Res {
        self.no_live()?;
        let use_rules = get_bool(a, "instructions")?.unwrap_or(false);
        let me = self.club()?;
        let w = self
            .world
            .as_mut()
            .ok_or_else(|| AgentError::new("no_game", "start with new_game or load"))?;
        if w.season_over() {
            return Err(AgentError::new("refused", "The season is over."));
        }
        if !w.match_today() {
            return Err(AgentError::new(
                "refused",
                "there is no game today: advance to match day first",
            ));
        }
        let round = if get_bool(a, "today")?.unwrap_or(false) {
            w.kickoff_today(me)
        } else {
            w.kickoff(me)
        }
        .ok_or_else(|| AgentError::new("no_game", "There is no game for you this round."))?;
        let coach = use_rules.then(|| Coach::new(&w.teams[me].instructions, &w.rules));
        self.live = Some(Live {
            round,
            coach,
            team: me,
            notes: Default::default(),
        });
        self.live_state()
    }

    fn live_state(&self) -> Res {
        let live = self
            .live
            .as_ref()
            .ok_or_else(|| AgentError::new("no_live_match", "no live match: use kickoff"))?;
        let w = self.world()?;
        let g = &live.round.game;
        let side = live.round.side;
        let f = &w.fixtures[live.round.fixture];
        let res = g.result();
        let club = [f.home, f.away];
        let row = |s: usize| {
            json!({"club": club[s], "name": w.teams[club[s]].name, "score": res.score[s],
                   "timeouts_left": g.timeouts_left(s), "team_fouls": g.team_fouls(s)})
        };
        let mood = |p: &basketman_core::engine::LivePlayer| -> &'static str {
            if p.fouled_out {
                "fouled out"
            } else if p.focus < -0.15 {
                "rattled"
            } else if p.drive < -0.15 {
                "flat"
            } else if p.drive > 0.25 {
                "fired up"
            } else if p.focus > 0.25 {
                "locked in"
            } else {
                ""
            }
        };
        let yours: Vec<Value> = g
            .players(side)
            .iter()
            .map(|p| {
                json!({"id": p.id, "name": w.players[p.id as usize].name, "on_court": p.on_court,
                       "energy_pct": (p.energy * 100.0).round(), "fouls": p.line.pf, "fouled_out": p.fouled_out,
                       "minutes": p.line.secs / 60, "pts": p.line.pts, "reb": p.line.reb(), "ast": p.line.ast,
                       "mood": mood(p)})
            })
            .collect();
        let theirs: Vec<Value> = g
            .players(1 - side)
            .iter()
            .map(|p| {
                json!({"id": p.id, "name": w.players[p.id as usize].name, "on_court": p.on_court,
                       "fouls": p.line.pf, "pts": p.line.pts, "reb": p.line.reb(), "ast": p.line.ast})
            })
            .collect();
        let theirs_o = g.orders(1 - side);
        let o = g.orders(side);
        let rules_fired = live.coach.as_ref().map(|c| c.fired.len());
        Ok(json!({
            "time": g.time(),
            "over": g.is_over(),
            "you_are": if side == 0 { "home" } else { "away" },
            "home": row(0),
            "away": row(1),
            "periods": res.periods,
            "players": {"yours": yours, "opponent": theirs},
            "play": play_json(&live.round),
            "timeout_wanted": g.timeout_wanted(side),
            "auto_subs": g.auto_subs(side),
            "queued_swaps": g.queued(side),
            "voice_worn": g.voice(side) > 0.55,
            "orders": {"yours": to_json(&o), "opponent": {
                "offense": theirs_o.offense, "defense": theirs_o.defense, "press": theirs_o.press, "pace": theirs_o.pace},
                "opponent_changes": g.adjustments(1 - side)},
            "foul_bonus_after": w.rules.bonus_after,
            "standing_rules": live.coach.is_some(),
            "rules_fired": rules_fired,
            "assistant_notes": crate::feeds::recent(w, &live.round, live.team, &live.notes, 3),
            "team": live.team,
            "officials": self.officials_json(w, live),
        }))
    }

    /// The crew, what each side has left to challenge with, and a call
    /// waiting to be challenged.
    fn officials_json(&self, w: &World, live: &Live) -> Value {
        let g = &live.round.game;
        let side = live.round.side;
        let crew: Vec<Value> = g
            .crew()
            .map(|c| {
                c.ids()
                    .iter()
                    .filter_map(|&id| w.officials.get(id))
                    .map(|r| json!({"name": r.name, "tags": r.tags()}))
                    .collect()
            })
            .unwrap_or_default();
        let review = g.review().map(|r| {
            json!({
                "call": r.call_text(|p| w.players[p as usize].name.as_str()),
                "period": r.period + 1,
                "seconds_left": r.clock.round(),
                "assistant": r.opinion.words(),
                "challenges_left": r.left,
                "timeouts_left": r.timeouts,
                "how": "send the live input \"Challenge\" now, or step on to let it stand; a lost challenge costs a timeout",
            })
        });
        json!({
            "crew": crew,
            "challenges_left": g.challenges_left(side),
            "challenged": g.calls().iter().any(|c| c.review.is_some_and(|r| r.by as usize == side)),
            "review": review,
        })
    }

    fn live_step(&mut self, a: &Value) -> Res {
        let seconds = match a.get("seconds") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                v.as_f64()
                    .filter(|s| *s >= 0.0)
                    .ok_or_else(|| bad("seconds must be a non-negative number"))?,
            ),
        };
        let max_steps = get_u64(a, "max_steps")?.unwrap_or(1_000_000);
        let w = self
            .world
            .as_ref()
            .ok_or_else(|| AgentError::new("no_game", "start with new_game or load"))?;
        let live = self
            .live
            .as_mut()
            .ok_or_else(|| AgentError::new("no_live_match", "no live match: use kickoff"))?;
        let start = live.round.game.time() as f64;
        let me = live.round.side;
        let team = &w.teams[live.team];
        let mut paused = Value::Null;
        let mut n = 0u64;
        while !live.round.game.is_over() && n < max_steps {
            let p = match live.coach.as_mut() {
                Some(c) => c.step(team, &mut live.round),
                None => live.round.step(),
            };
            n += 1;
            live.notes.watch(w, &live.round);
            if let Some(p) = p {
                paused = pause_name(p, me);
                break;
            }
            if let Some(s) = seconds
                && live.round.game.time() as f64 - start >= s
            {
                break;
            }
        }
        let mut st = self.live_state()?;
        st["paused"] = paused;
        st["steps"] = json!(n);
        Ok(st)
    }

    fn live_input(&mut self, a: &Value) -> Res {
        let w = self
            .world
            .as_ref()
            .ok_or_else(|| AgentError::new("no_game", "start with new_game or load"))?;
        let live = self
            .live
            .as_mut()
            .ok_or_else(|| AgentError::new("no_live_match", "no live match: use kickoff"))?;
        let v = flat(a.get("input").unwrap_or(a));
        // How well the squad knows a new system is the game's business: the
        // client's number, if any, is replaced.
        let input = if let Some(o) = v.get("Orders") {
            let orders: Orders =
                serde_json::from_value(o.get("orders").cloned().unwrap_or_else(|| o.clone()))
                    .map_err(|e| bad(format!("not orders: {e}")))?;
            let fam =
                w.teams[live.team].familiarity_with(orders.offense, orders.defense, orders.press);
            LiveInput::Orders {
                orders,
                familiarity: fam,
            }
        } else {
            serde_json::from_value::<LiveInput>(v)
                .map_err(|e| bad(format!("not a live input: {e}")))?
        };
        if live.round.game.is_over() {
            return Err(AgentError::new(
                "refused",
                "the game is over: call live_finish",
            ));
        }
        let side = live.round.side;
        let players = live.round.game.players(side);
        let known = |id: PlayerId| players.iter().any(|p| p.id == id);
        match &input {
            LiveInput::Swap { off, on } => {
                let ok = players.iter().any(|p| p.id == *off && p.on_court)
                    && players
                        .iter()
                        .any(|p| p.id == *on && !p.on_court && !p.fouled_out);
                if !ok {
                    return Err(AgentError::new(
                        "refused",
                        "swap needs one of your players on the floor and one on the bench who has not fouled out",
                    ));
                }
            }
            LiveInput::Shout {
                target: Some(t), ..
            } if !known(*t) => {
                return Err(AgentError::new(
                    "not_your_player",
                    "that player is not on your team",
                ));
            }
            LiveInput::Orders { orders, .. }
                if [
                    orders.pace,
                    orders.three_bias,
                    orders.crash_boards,
                    orders.aggression,
                ]
                .iter()
                .any(|v| !(1..=20).contains(v)) =>
            {
                return Err(AgentError::new("out_of_range", "sliders run from 1 to 20"));
            }
            _ => {}
        }
        let said = live.round.try_input(input).map_err(|r| {
            let kind = match r {
                Refusal::PlayIsLive => "play_is_live",
                Refusal::GameOver | Refusal::NoTimeouts | Refusal::NothingToChallenge => "refused",
                Refusal::ReviewOnly => "review_only",
                Refusal::TeamTalkUsed | Refusal::WordsUsed => "talk_used",
                Refusal::NotOnTheTeam => "not_your_player",
            };
            AgentError::new(kind, r.to_string())
        })?;
        let mut st = self.live_state()?;
        st["reactions"] = Value::Array(
            said.iter()
                .map(|(p, r)| json!({"player": p, "reaction": r.label(), "bad": r.is_bad()}))
                .collect(),
        );
        Ok(st)
    }

    fn live_finish(&mut self) -> Res {
        let mut live = self
            .live
            .take()
            .ok_or_else(|| AgentError::new("no_live_match", "no live match: use kickoff"))?;
        let w = self
            .world
            .as_mut()
            .ok_or_else(|| AgentError::new("no_game", "start with new_game or load"))?;
        match live.coach.as_mut() {
            Some(c) => c.play_out(&w.teams[live.team], &mut live.round),
            None => {
                let mut n = 0u32;
                while !live.round.game.is_over() && n < 2_000_000 {
                    live.round.step();
                    n += 1;
                }
            }
        }
        let played = w.full_time(live.round);
        let out = Outcome {
            played,
            ..Outcome::default()
        };
        let w = self.world()?;
        let mut v = outcome_json(w, &out);
        v["waiting"] = to_json(&basketman_core::legal::waiting(w));
        Ok(v)
    }
}
