//! How a game is going, read from its recording: the margin over time and
//! the runs in it, the four factors, who is hot and cold, and who guards
//! whom. Everything is counted from the events up to `Read::time`, so the
//! same numbers can be had for a live game and for a replay at any moment.

use serde::Serialize;

use crate::engine::LivePlayer;
use crate::player::PlayerId;
use crate::replay::{EventKind, Recording};

/// What to read and where: a recording up to a moment of it.
pub struct Read<'a> {
    pub rec: &'a Recording,
    /// Box-score players of each side, which the frames refer to by index.
    pub ids: &'a [Vec<PlayerId>; 2],
    /// The side to read for (the user's), 0 = home.
    pub me: usize,
    /// Replay time to read up to.
    pub time: f32,
    /// Energy, fouls and who is on court, when the game is live.
    pub live: Option<&'a [Vec<LivePlayer>; 2]>,
}

impl Read<'_> {
    /// How many events have happened.
    pub fn upto(&self) -> usize {
        self.rec.events.partition_point(|e| e.time <= self.time)
    }

    fn events(&self) -> &[crate::replay::Event] {
        &self.rec.events[..self.upto()]
    }

    pub fn side_of(&self, p: PlayerId) -> Option<usize> {
        (0..2).find(|&s| self.ids[s].contains(&p))
    }
}

/// Something that happened on the margin chart.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
pub enum MarkKind {
    Timeout,
    Substitution,
    /// A change of system by the bench.
    Adjust,
}

#[derive(Clone, Debug, Serialize)]
pub struct Mark {
    pub time: f32,
    /// 0 = home.
    pub side: u8,
    pub kind: MarkKind,
}

/// A stretch in which one side outscored the other by a lot.
#[derive(Clone, Debug, Serialize)]
pub struct Run {
    pub side: u8,
    pub from: f32,
    pub to: f32,
    pub period: u8,
    /// Seconds left in the period when it began.
    pub clock: f32,
    pub points: u16,
    /// What the other side scored in it.
    pub against: u16,
    /// Still going at the moment read.
    pub on: bool,
}

impl Run {
    pub fn label(&self) -> String {
        format!("{}-{} run", self.points, self.against)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Flow {
    /// `(time, margin)` at every change of the score, the user's side minus
    /// the other, starting at 0-0.
    pub margin: Vec<(f32, i16)>,
    /// When each period ended.
    pub period_ends: Vec<f32>,
    pub marks: Vec<Mark>,
    pub runs: Vec<Run>,
}

/// A run of at least this many points without an answer is worth marking.
pub const RUN_POINTS: u16 = 8;

impl Read<'_> {
    pub fn flow(&self) -> Flow {
        let mut margin = vec![(0.0, 0i16)];
        let mut period_ends = Vec::new();
        let mut marks = Vec::new();
        let mut runs = Vec::new();
        let mut last = [0u16; 2];
        // The run in progress: side, start, points for, against, period,
        // clock at its start.
        let mut cur: Option<(u8, f32, u16, u16, u8, f32)> = None;
        let close = |cur: &mut Option<(u8, f32, u16, u16, u8, f32)>,
                     runs: &mut Vec<Run>,
                     to: f32,
                     on: bool| {
            if let Some((side, from, points, against, period, clock)) = cur.take()
                && points >= RUN_POINTS
            {
                runs.push(Run {
                    side,
                    from,
                    to,
                    period,
                    clock,
                    points,
                    against,
                    on,
                });
            }
        };
        for e in self.events() {
            match &e.kind {
                EventKind::PeriodEnd => period_ends.push(e.time),
                EventKind::Timeout { side } => marks.push(Mark {
                    time: e.time,
                    side: *side,
                    kind: MarkKind::Timeout,
                }),
                EventKind::Substitution { on, .. } => {
                    if let Some(side) = self.side_of(*on) {
                        marks.push(Mark {
                            time: e.time,
                            side: side as u8,
                            kind: MarkKind::Substitution,
                        });
                    }
                }
                EventKind::Adjust { side, .. } => marks.push(Mark {
                    time: e.time,
                    side: *side,
                    kind: MarkKind::Adjust,
                }),
                _ => {}
            }
            if e.score != last {
                let d = [e.score[0] - last[0].min(e.score[0]), e.score[1] - last[1].min(e.score[1])];
                last = e.score;
                let m = if self.me == 0 {
                    e.score[0] as i16 - e.score[1] as i16
                } else {
                    e.score[1] as i16 - e.score[0] as i16
                };
                margin.push((e.time, m));
                // Which side scored; both in one event is not a thing here.
                let side = if d[0] >= d[1] { 0 } else { 1 };
                let (mine, theirs) = (d[side], d[1 - side]);
                match &mut cur {
                    Some(c) if c.0 as usize == side => c.2 += mine + theirs,
                    Some(c) => {
                        // The other side scores: the run is over unless it
                        // is answered by a stray point.
                        let _ = c;
                        close(&mut cur, &mut runs, e.time, false);
                        cur = Some((side as u8, e.time, mine, 0, e.period, e.clock));
                    }
                    None => cur = Some((side as u8, e.time, mine, 0, e.period, e.clock)),
                }
            }
        }
        close(&mut cur, &mut runs, self.time, true);
        Flow {
            margin,
            period_ends,
            marks,
            runs,
        }
    }
}

/// One side's four factors and what they are made of.
#[derive(Copy, Clone, Debug, Default, Serialize)]
pub struct Factors {
    pub fga: u16,
    pub fgm: u16,
    pub tpa: u16,
    pub tpm: u16,
    pub fta: u16,
    pub ftm: u16,
    pub tov: u16,
    pub orb: u16,
    pub drb: u16,
    pub pts: u16,
}

impl Factors {
    /// Shooting efficiency: field goals with threes worth half as much
    /// again.
    pub fn efg(&self) -> Option<f32> {
        (self.fga > 0).then(|| (self.fgm as f32 + 0.5 * self.tpm as f32) / self.fga as f32)
    }

    /// Turnovers per hundred plays.
    pub fn tov_rate(&self) -> Option<f32> {
        let plays = self.fga as f32 + 0.44 * self.fta as f32 + self.tov as f32;
        (plays > 0.0).then(|| self.tov as f32 / plays)
    }

    /// Free throws made per field goal attempted.
    pub fn ft_rate(&self) -> Option<f32> {
        (self.fga > 0).then(|| self.ftm as f32 / self.fga as f32)
    }
}

/// Offensive rebounding: the share of the misses a side got back.
pub fn orb_rate(own: &Factors, other: &Factors) -> Option<f32> {
    let chances = own.orb + other.drb;
    (chances > 0).then(|| own.orb as f32 / chances as f32)
}

impl Read<'_> {
    /// The four factors of both sides as they stand.
    pub fn factors(&self) -> [Factors; 2] {
        let mut f = [Factors::default(); 2];
        for e in self.events() {
            match &e.kind {
                EventKind::Shot {
                    who,
                    kind,
                    made,
                    fouled,
                    ..
                } => {
                    let Some(s) = self.side_of(*who) else { continue };
                    if *fouled && !made {
                        continue;
                    }
                    let x = &mut f[s];
                    x.fga += 1;
                    let three = *kind == crate::court::ShotKind::Three;
                    x.tpa += three as u16;
                    if *made {
                        x.fgm += 1;
                        x.tpm += three as u16;
                        x.pts += kind.points();
                    }
                }
                EventKind::FreeThrows { who, made, of } => {
                    if let Some(s) = self.side_of(*who) {
                        f[s].fta += *of as u16;
                        f[s].ftm += *made as u16;
                        f[s].pts += *made as u16;
                    }
                }
                EventKind::Turnover { who, .. } => {
                    if let Some(s) = self.side_of(*who) {
                        f[s].tov += 1;
                    }
                }
                EventKind::Rebound { who, offensive } => {
                    if let Some(s) = self.side_of(*who) {
                        if *offensive {
                            f[s].orb += 1;
                        } else {
                            f[s].drb += 1;
                        }
                    }
                }
                _ => {}
            }
        }
        f
    }
}

/// Streaks of a shooter.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Heat {
    Hot,
    Cold,
}

/// A player's last shots, energy and fouls.
#[derive(Clone, Debug, Serialize)]
pub struct Form {
    pub id: PlayerId,
    pub side: u8,
    pub on_court: Option<bool>,
    /// His last five attempts, oldest first: made or not.
    pub last: Vec<bool>,
    pub heat: Option<Heat>,
    pub points: u16,
    pub fouls: u16,
    /// 1 = fresh. Only for a live game.
    pub energy: Option<f32>,
}

impl Read<'_> {
    pub fn forms(&self) -> Vec<Form> {
        let mut out: Vec<Form> = Vec::new();
        for s in 0..2 {
            for &id in &self.ids[s] {
                let live = self.live.and_then(|l| l[s].iter().find(|p| p.id == id));
                out.push(Form {
                    id,
                    side: s as u8,
                    on_court: live.map(|p| p.on_court),
                    last: Vec::new(),
                    heat: None,
                    points: 0,
                    fouls: 0,
                    energy: live.map(|p| p.energy),
                });
            }
        }
        let at = |out: &mut Vec<Form>, p: PlayerId| out.iter().position(|f| f.id == p);
        for e in self.events() {
            match &e.kind {
                EventKind::Shot {
                    who,
                    kind,
                    made,
                    fouled,
                    ..
                } => {
                    let Some(i) = at(&mut out, *who) else { continue };
                    if *fouled && !made {
                        continue;
                    }
                    out[i].last.push(*made);
                    if *made {
                        out[i].points += kind.points();
                    }
                }
                EventKind::FreeThrows { who, made, .. } => {
                    if let Some(i) = at(&mut out, *who) {
                        out[i].points += *made as u16;
                    }
                }
                EventKind::Foul { who, .. } => {
                    if let Some(i) = at(&mut out, *who) {
                        out[i].fouls += 1;
                    }
                }
                _ => {}
            }
        }
        for f in &mut out {
            if f.last.len() > 5 {
                f.last.drain(..f.last.len() - 5);
            }
            let n = f.last.len();
            let made = f.last.iter().filter(|&&m| m).count();
            f.heat = if n >= 3 && made >= 3 && made * 4 >= n * 3 {
                Some(Heat::Hot)
            } else if (n >= 4 && made == 0) || (n == 5 && made <= 1) {
                Some(Heat::Cold)
            } else {
                None
            };
        }
        out
    }
}

/// How one of the user's players is doing against the man he guards.
#[derive(Clone, Debug, Serialize)]
pub struct Matchup {
    pub mine: PlayerId,
    /// The opponent he has been closest to while the other side had the
    /// ball, over the last half minute or so. Positional, not assigned: it
    /// is where the bodies were.
    pub guards: Option<PlayerId>,
    /// Shots the other side took with him as the nearest defender.
    pub shots_against: u16,
    pub made_against: u16,
    pub points_against: u16,
    /// Times he was beaten off the dribble.
    pub beaten: u16,
    /// Points the man he guards has scored in all.
    pub his_points: u16,
}

impl Matchup {
    /// A short verdict on how it is going, if there is enough to say.
    pub fn verdict(&self) -> Option<&'static str> {
        if self.shots_against >= 3 && self.made_against * 4 <= self.shots_against {
            Some("winning it")
        } else if (self.shots_against >= 3 && self.made_against * 2 >= self.shots_against + 2)
            || self.beaten >= 3
        {
            Some("getting hurt")
        } else {
            None
        }
    }
}

/// Seconds of play looked back over to see who guards whom.
const GUARD_WINDOW: f32 = 40.0;

impl Read<'_> {
    /// The user's five on the floor and the men they have been guarding.
    pub fn matchups(&self) -> Vec<Matchup> {
        let rec = self.rec;
        if rec.frames.is_empty() {
            return Vec::new();
        }
        let end = rec.frame_at(self.time);
        let (me, them) = (self.me, 1 - self.me);
        let now = &rec.frames[end];
        // Votes: (my box index, their box index) -> frames spent closest.
        let mut votes: Vec<(u8, u8, u16)> = Vec::new();
        let mut n = end;
        let mut samples = 0;
        loop {
            let f = &rec.frames[n];
            if self.time - f.time > GUARD_WINDOW {
                break;
            }
            if f.offense as usize == them && n % 4 == 0 {
                samples += 1;
                for a in 0..5 {
                    let mut best = (f32::MAX, 0usize);
                    for b in 0..5 {
                        let d = f.pos[me][a].dist(f.pos[them][b]);
                        if d < best.0 {
                            best = (d, b);
                        }
                    }
                    let key = (f.who[me][a], f.who[them][best.1]);
                    match votes.iter_mut().find(|v| (v.0, v.1) == key) {
                        Some(v) => v.2 += 1,
                        None => votes.push((key.0, key.1, 1)),
                    }
                }
            }
            if n == 0 {
                break;
            }
            n -= 1;
        }
        let events = self.events();
        (0..5)
            .filter_map(|a| {
                let my_idx = now.who[me][a];
                let mine = *self.ids[me].get(my_idx as usize)?;
                let guards = votes
                    .iter()
                    .filter(|v| v.0 == my_idx)
                    .max_by_key(|v| (v.2, std::cmp::Reverse(v.1)))
                    .filter(|v| samples > 0 && v.2 as usize * 100 >= samples * 30)
                    .and_then(|v| self.ids[them].get(v.1 as usize).copied());
                let mut m = Matchup {
                    mine,
                    guards,
                    shots_against: 0,
                    made_against: 0,
                    points_against: 0,
                    beaten: 0,
                    his_points: 0,
                };
                for e in events {
                    match &e.kind {
                        EventKind::Shot {
                            who,
                            kind,
                            made,
                            defender,
                            fouled,
                            ..
                        } => {
                            if *defender == Some(mine) && !(*fouled && !made) {
                                m.shots_against += 1;
                                if *made {
                                    m.made_against += 1;
                                    m.points_against += kind.points();
                                }
                            }
                            if Some(*who) == guards && *made {
                                m.his_points += kind.points();
                            }
                        }
                        EventKind::FreeThrows { who, made, .. } if Some(*who) == guards => {
                            m.his_points += *made as u16;
                        }
                        EventKind::Beaten { defender, .. } if *defender == mine => m.beaten += 1,
                        _ => {}
                    }
                }
                Some(m)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::league::World;

    fn game() -> (Recording, [Vec<PlayerId>; 2], [u16; 2]) {
        let mut w = World::new_career_tiers(5, 8, 3);
        w.apply(Command::TakeCharge(1)).unwrap();
        w.apply(Command::PlayRound).unwrap();
        let (i, rec) = w.replay.clone().unwrap();
        let r = w.fixtures[i].result.clone().unwrap();
        let ids = [0, 1].map(|s| r.boxes[s].iter().map(|b| b.0).collect::<Vec<_>>());
        (rec, ids, r.score)
    }

    #[test]
    fn the_counts_from_the_events_are_the_box_score() {
        let mut w = World::new_career_tiers(5, 8, 3);
        w.apply(Command::TakeCharge(1)).unwrap();
        w.apply(Command::PlayRound).unwrap();
        let (i, rec) = w.replay.clone().unwrap();
        let r = w.fixtures[i].result.clone().unwrap();
        let ids = [0, 1].map(|s| r.boxes[s].iter().map(|b| b.0).collect::<Vec<_>>());
        let read = Read {
            rec: &rec,
            ids: &ids,
            me: 0,
            time: f32::MAX,
            live: None,
        };
        let f = read.factors();
        for s in 0..2 {
            let sum = |g: fn(&crate::engine::BoxLine) -> u16| {
                r.boxes[s].iter().map(|b| g(&b.1)).sum::<u16>()
            };
            assert_eq!(f[s].fga, sum(|b| b.fga), "fga side {s}");
            assert_eq!(f[s].fgm, sum(|b| b.fgm));
            assert_eq!(f[s].tpm, sum(|b| b.tpm));
            assert_eq!(f[s].fta, sum(|b| b.fta));
            assert_eq!(f[s].ftm, sum(|b| b.ftm));
            assert_eq!(f[s].tov, sum(|b| b.tov), "tov side {s}");
            assert_eq!(f[s].pts, r.score[s]);
            assert_eq!(f[s].orb, sum(|b| b.orb));
        }
        let _ = i;
    }

    #[test]
    fn the_flow_ends_on_the_final_margin_and_finds_runs_in_order() {
        let (rec, ids, score) = game();
        for me in 0..2 {
            let read = Read {
                rec: &rec,
                ids: &ids,
                me,
                time: f32::MAX,
                live: None,
            };
            let flow = read.flow();
            let want = score[me] as i16 - score[1 - me] as i16;
            assert_eq!(flow.margin.last().unwrap().1, want);
            assert!(flow.margin.windows(2).all(|w| w[0].0 <= w[1].0));
            assert!(!flow.period_ends.is_empty());
            for r in &flow.runs {
                assert!(r.points >= RUN_POINTS && r.from <= r.to);
            }
            // Half-way through there is less of everything.
            let half = Read { time: rec.duration() / 2.0, ..read };
            assert!(half.flow().margin.len() < flow.margin.len());
        }
    }

    #[test]
    fn the_guarded_man_is_one_of_theirs_and_matchups_are_for_the_five() {
        let (rec, ids, _) = game();
        let read = Read {
            rec: &rec,
            ids: &ids,
            me: 0,
            time: rec.duration() * 0.6,
            live: None,
        };
        let m = read.matchups();
        assert_eq!(m.len(), 5);
        assert!(m.iter().filter(|m| m.guards.is_some()).count() >= 3);
        for x in &m {
            assert!(ids[0].contains(&x.mine));
            assert!(x.guards.is_none_or(|g| ids[1].contains(&g)));
        }
        let forms = read.forms();
        assert_eq!(forms.len(), ids[0].len() + ids[1].len());
    }
}

#[cfg(test)]
mod cost {
    use super::*;
    use crate::command::Command;
    use crate::league::World;

    /// Not an assertion about speed: prints what a refresh of the data tabs
    /// costs on a whole game, for the record.
    #[test]
    fn what_a_refresh_costs() {
        let mut w = World::new_career_tiers(5, 8, 3);
        w.apply(Command::TakeCharge(1)).unwrap();
        w.apply(Command::PlayRound).unwrap();
        let (i, rec) = w.replay.clone().unwrap();
        let r = w.fixtures[i].result.clone().unwrap();
        let ids = [0, 1].map(|s| r.boxes[s].iter().map(|b| b.0).collect::<Vec<_>>());
        let read = Read {
            rec: &rec,
            ids: &ids,
            me: 0,
            time: f32::MAX,
            live: None,
        };
        let t = std::time::Instant::now();
        let n = 200;
        for _ in 0..n {
            std::hint::black_box((read.flow(), read.factors(), read.forms(), read.matchups(), crate::shotchart::shots(&rec, &ids)));
        }
        eprintln!(
            "refresh of the data tabs on a whole game: {:.3} ms ({} events, {} frames)",
            t.elapsed().as_secs_f64() * 1000.0 / n as f64,
            rec.events.len(),
            rec.frames.len()
        );
    }
}
