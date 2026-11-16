//! What the live game shows a manager beyond the score: the other games of
//! the day and the table as it stands, where the shots were taken, how the
//! game is flowing, and what the assistant has noticed. The same numbers
//! as the match screen's Scores, Shots and Flow tabs and the assistant's
//! notes in its feed; all of it is computed by the game library.

use basketman_core::game_flow::{self, Read};
use basketman_core::league::{LiveRound, World};
use basketman_core::live_feed::{Now, Status};
use basketman_core::observations::{Note, Observer, Setting};
use basketman_core::player::PlayerId;
use basketman_core::shotchart;
use serde_json::{Value, json};

use crate::agent::AgentError;

/// The assistant's observer and what it has said in this game.
#[derive(Default)]
pub struct Notes {
    observer: Observer,
    pub said: Vec<Note>,
    at: f32,
}

/// Seconds of replay time between two looks at the game.
const LOOK: f32 = 2.0;

fn ids(round: &LiveRound) -> [Vec<PlayerId>; 2] {
    let r = round.game.result();
    [0, 1].map(|s| r.boxes[s].iter().map(|b| b.0).collect())
}

impl Notes {
    /// Let the assistant look at the game if it has moved on since the
    /// last look.
    pub fn watch(&mut self, w: &World, round: &LiveRound) {
        let time = round.game.time();
        if time - self.at < LOOK || round.game.recording().frames.is_empty() {
            return;
        }
        self.at = time;
        let ids = ids(round);
        let players = [round.game.players(0), round.game.players(1)];
        let read = Read {
            rec: round.game.recording(),
            ids: &ids,
            me: round.side,
            time,
            live: Some(&players),
        };
        let name = |p: PlayerId| w.players[p as usize].name.clone();
        let set = Setting {
            name: &name,
            orders: [round.game.orders(0), round.game.orders(1)],
            timeouts_left: round.game.timeouts_left(round.side),
            timeout_wanted: round.game.timeout_wanted(round.side),
            foul_limit: w.rules.foul_limit,
        };
        if let Some(note) = self.observer.observe(&read, &set) {
            self.said.push(note);
        }
    }
}

fn pct(x: Option<f32>) -> Value {
    x.map_or(Value::Null, |x| json!((x * 1000.0).round() / 10.0))
}

fn note_json(w: &World, round: &LiveRound, team: usize, n: &Note) -> Value {
    let suggestion = n.suggestion.as_ref().map(|s| {
        let orders = round.game.orders(round.side);
        let fam = w.teams[team].familiarity_with(orders.offense, orders.defense, orders.press);
        json!({"label": s.label, "input": s.input.input(orders, fam)})
    });
    json!({
        "time": n.time, "period": n.period + 1, "clock": n.clock,
        "topic": n.topic, "player": n.subject,
        "text": n.text, "suggestion": suggestion,
    })
}

/// The last few things the assistant said, for `live_state`.
pub fn recent(w: &World, round: &LiveRound, team: usize, notes: &Notes, n: usize) -> Value {
    let from = notes.said.len().saturating_sub(n);
    Value::Array(
        notes.said[from..]
            .iter()
            .map(|x| note_json(w, round, team, x))
            .collect(),
    )
}

fn now_of(round: &LiveRound) -> Now {
    let r = round.game.result();
    let f = round.game.recording().frames.last();
    Now {
        period: f.map_or(0, |f| f.period),
        clock: f.map_or(0.0, |f| f.clock),
        score: r.score,
        over: round.game.is_over(),
    }
}

fn scores(w: &World, round: &LiveRound, division: Option<usize>) -> Value {
    let now = now_of(round);
    let games = round.games_now(w, now);
    let mine = w.my_division();
    let d = division.unwrap_or(mine);
    let rows = round.table_now(w, now, d);
    let lines = w.table_lines(d);
    json!({
        "period": now.period + 1, "clock": now.clock,
        "games": games.iter().map(|g| json!({
            "fixture": g.fixture, "division": g.division,
            "home": g.home, "away": g.away,
            "home_name": g.home_name, "away_name": g.away_name,
            "score": g.score,
            "status": if g.status == Status::Final { "final" } else { "live" },
            "period": g.period + 1, "clock": g.clock,
            "scored_secs_ago": g.changed_ago, "scored_by": g.changed_side.map(|s| ["home", "away"][s]),
            "yours": g.ours,
        })).collect::<Vec<_>>(),
        "table": {
            "division": d,
            "note": "as it stands if every game on now ended at its score; `was` is the place before today's games",
            "lines": lines,
            "rows": rows,
        },
    })
}

fn shots(w: &World, round: &LiveRound, a: &Value) -> Result<Value, AgentError> {
    let ids = ids(round);
    let side = round.side;
    let rec = round.game.recording();
    let mut list = shotchart::shots(rec, &ids);
    let which = a.get("side").and_then(Value::as_str).unwrap_or("both");
    let player = a.get("player").and_then(Value::as_u64).map(|p| p as PlayerId);
    let period = a
        .get("period")
        .and_then(Value::as_u64)
        .map(|p| p.saturating_sub(1) as u8);
    list.retain(|s| match which {
        "yours" => s.side as usize == side,
        "theirs" => s.side as usize != side,
        _ => true,
    });
    list.retain(|s| player.is_none_or(|p| s.who == p) && period.is_none_or(|p| s.period == p));
    let zones = shotchart::zones(list.iter());
    let avg = w.league_shooting();
    Ok(json!({
        "frame": "x across the court in metres, 0 under the basket, negative to the shooter's left; y from the baseline; the basket is at (0, 1.575); the same frame for both teams",
        "count": list.len(),
        "shots": list.iter().map(|s| json!({
            "time": s.time, "period": s.period + 1, "clock": s.clock,
            "side": if s.side as usize == side { "yours" } else { "theirs" },
            "player": s.who, "name": w.players[s.who as usize].name,
            "kind": s.kind, "zone": s.zone, "made": s.made,
            "x": (s.x * 100.0).round() / 100.0, "y": (s.y * 100.0).round() / 100.0,
            "distance": (s.distance() * 10.0).round() / 10.0,
            "contest": (s.contest * 100.0).round() / 100.0,
            "assisted": s.assisted, "blocked": s.blocked, "source": s.source,
            "defender": s.defender,
        })).collect::<Vec<_>>(),
        "zones": shotchart::Zone::ALL.iter().zip(zones).map(|(z, l)| json!({
            "zone": z, "label": z.label(), "made": l.made, "attempted": l.att,
            "pct": pct(l.pct()),
            "league_pct": pct(avg.map(|a| shotchart::zone_average(a, *z))),
        })).collect::<Vec<_>>(),
        "league_note": "the league figure is the season's two-point average for the rim and mid-range and its three-point average for both threes",
    }))
}

fn flow(w: &World, round: &LiveRound) -> Value {
    let ids = ids(round);
    let side = round.side;
    let players = [round.game.players(0), round.game.players(1)];
    let read = Read {
        rec: round.game.recording(),
        ids: &ids,
        me: side,
        time: round.game.time(),
        live: Some(&players),
    };
    let flow = read.flow();
    let f = read.factors();
    let (a, b) = (&f[side], &f[1 - side]);
    let factors = |x: &game_flow::Factors, y: &game_flow::Factors| {
        json!({
            "efg_pct": pct(x.efg()), "turnover_pct": pct(x.tov_rate()),
            "offensive_rebound_pct": pct(game_flow::orb_rate(x, y)),
            "free_throws_per_shot": pct(x.ft_rate()),
            "counts": x,
        })
    };
    let name = |p: PlayerId| w.players[p as usize].name.clone();
    json!({
        "margin": flow.margin,
        "margin_note": "(replay time, your points minus theirs) at every change of the score",
        "period_ends": flow.period_ends,
        "runs": flow.runs.iter().map(|r| json!({
            "by": if r.side as usize == side { "you" } else { "them" },
            "label": r.label(), "from": r.from, "to": r.to, "period": r.period + 1,
            "clock": r.clock, "points": r.points, "against": r.against, "going": r.on,
        })).collect::<Vec<_>>(),
        "events": flow.marks.iter().map(|m| json!({
            "time": m.time, "by": if m.side as usize == side { "you" } else { "them" }, "kind": m.kind,
        })).collect::<Vec<_>>(),
        "four_factors": {"yours": factors(a, b), "theirs": factors(b, a)},
        "form": read.forms().iter().filter(|f| f.on_court == Some(true) || f.heat.is_some() || f.fouls >= 3).map(|f| json!({
            "player": f.id, "name": name(f.id), "side": if f.side as usize == side { "yours" } else { "theirs" },
            "on_court": f.on_court, "last_shots": f.last, "streak": f.heat,
            "points": f.points, "fouls": f.fouls,
            "energy_pct": if f.side as usize == side { f.energy.map(|e| (e * 100.0).round()) } else { None },
        })).collect::<Vec<_>>(),
        "matchups": read.matchups().iter().map(|m| json!({
            "yours": m.mine, "name": name(m.mine),
            "guarding": m.guards, "guarding_name": m.guards.map(name),
            "shots_against": m.shots_against, "made_against": m.made_against,
            "points_against": m.points_against, "beaten": m.beaten,
            "his_points": m.his_points, "verdict": m.verdict(),
        })).collect::<Vec<_>>(),
        "matchup_note": "who he guards is the opponent nearest to him while they had the ball over the last forty seconds (positions, not assignments)",
    })
}

/// `live_feeds`: the live game's data feeds. `part` is `scores`, `shots`,
/// `flow` or `notes`; without it you get scores, flow and notes (the shot
/// list is long, ask for it).
pub fn report(
    w: &World,
    round: &LiveRound,
    team: usize,
    notes: &Notes,
    a: &Value,
) -> Result<Value, AgentError> {
    if round.game.recording().frames.is_empty() {
        return Err(AgentError::new(
            "refused",
            "the game has not started: live_step first",
        ));
    }
    let part = a.get("part").and_then(Value::as_str);
    let division = a.get("division").and_then(Value::as_u64).map(|d| d as usize);
    if let Some(d) = division
        && d >= w.division_count()
    {
        return Err(AgentError::new("out_of_range", "there is no such division"));
    }
    let all = notes
        .said
        .iter()
        .map(|n| note_json(w, round, team, n))
        .collect::<Vec<_>>();
    Ok(match part {
        Some("scores") => scores(w, round, division),
        Some("shots") => shots(w, round, a)?,
        Some("flow") => flow(w, round),
        Some("notes") => json!({"notes": all}),
        None => json!({
            "scores": scores(w, round, division),
            "flow": flow(w, round),
            "notes": all,
        }),
        Some(other) => {
            return Err(AgentError::new(
                "bad_request",
                format!("part must be scores, shots, flow or notes, not {other}"),
            ));
        }
    })
}
