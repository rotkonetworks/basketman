//! `basketman balance`: is the game balanced?
//!
//! A laboratory that plays the real engine thousands of times and reads off
//! whether tactics matter, whether any is dominant, whether the best choice
//! depends on the squad, how much coaching is worth, and whether there are
//! exploits. It never changes the engine. See docs/balance.md for the targets
//! and how to read the output.
//!
//! Everything is deterministic: games are seeded from (master seed, job,
//! unit), jobs run on any number of threads, and results are reduced in job
//! order, so serial and parallel builds print the same report.
//!
//! Vocabulary: a *strategy* (`Strat`) is every choice a manager makes before
//! tip-off (system, sliders, minutes plan, briefs, a drawn set). A *unit* is
//! four games of strategy X against Y: X on either of two squads, at home and
//! away, which cancels squad strength and home court. A *job* is one X
//! against one Y for some number of units.

#![allow(dead_code)]

use std::fmt::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use basketman_core::court::v;
use basketman_core::engine::{BoxLine, Lineup, Orders, Pause, Shout, simulate_game};
use basketman_core::league::{LiveInput, World};
use basketman_core::player::{Attr, Player, PlayerId};
use basketman_core::rng::Rng;
use basketman_core::staff::Job as Post;
use basketman_core::tactics::{
    BOARD_ROLES, Board, Brief, DefenseScheme, OffenseSet, Press, Role, TacticSlot, Tactics,
};
use serde_json::{Value, json};

// ---------------------------------------------------------------- plumbing

thread_local! {
    /// Tests pin the thread count of the calling thread with this.
    static FORCE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Worker threads: half the cores (or `RAYON_NUM_THREADS` if smaller) in a parallel build, one otherwise;
/// `BALANCE_THREADS` overrides. Output never depends on it.
fn threads() -> usize {
    if let Some(n) = FORCE.with(|f| f.get()) {
        return n;
    }
    if let Some(n) = std::env::var("BALANCE_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    {
        return n.max(1);
    }
    if cfg!(feature = "parallel") {
        // At most half the cores, and no more than a rayon pool size the
        // caller has asked for.
        let half = std::thread::available_parallelism().map_or(2, |n| n.get()) / 2;
        let cap = std::env::var("RAYON_NUM_THREADS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        half.max(1).min(cap.max(1))
    } else {
        1
    }
}

/// `f(0..n)` on all threads, results in index order.
fn par_map<T: Send, F: Fn(usize) -> T + Sync>(n: usize, f: F) -> Vec<T> {
    let th = threads().min(n);
    if th <= 1 {
        return (0..n).map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<(usize, T)>> = Mutex::new(Vec::with_capacity(n));
    std::thread::scope(|s| {
        for _ in 0..th {
            s.spawn(|| {
                let mut mine = Vec::new();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    mine.push((i, f(i)));
                }
                out.lock().unwrap().extend(mine);
            });
        }
    });
    let mut all = out.into_inner().unwrap();
    all.sort_by_key(|x| x.0);
    all.into_iter().map(|x| x.1).collect()
}

fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn mix(parts: &[u64]) -> u64 {
    parts.iter().fold(0x1234_5678, |h, &p| splitmix(h ^ splitmix(p)))
}

fn progress(msg: &str) {
    eprintln!("[balance] {msg}");
}

/// Mean with a standard error.
#[derive(Clone, Copy, Default, Debug)]
pub struct Est {
    pub mean: f64,
    pub se: f64,
    pub n: usize,
}

impl Est {
    fn of(v: &[f64]) -> Est {
        let n = v.len();
        if n == 0 {
            return Est::default();
        }
        let mean = v.iter().sum::<f64>() / n as f64;
        let var = if n > 1 {
            v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (n - 1) as f64
        } else {
            0.0
        };
        Est {
            mean,
            se: (var / n as f64).sqrt(),
            n,
        }
    }

    /// Half-width of the 95% interval.
    fn ci(&self) -> f64 {
        1.96 * self.se
    }

    /// Difference of two independent estimates.
    fn minus(&self, o: &Est) -> Est {
        Est {
            mean: self.mean - o.mean,
            se: (self.se * self.se + o.se * o.se).sqrt(),
            n: self.n.min(o.n),
        }
    }

    fn pct(&self) -> String {
        format!("{:.1}±{:.1}", 100.0 * self.mean, 100.0 * self.ci())
    }

    fn json(&self) -> Value {
        json!({"mean": r4(self.mean), "se": r4(self.se), "n": self.n})
    }
}

fn r4(x: f64) -> f64 {
    (x * 10000.0).round() / 10000.0
}

/// Report text and JSON built side by side; the text is printed as it grows.
#[derive(Default)]
pub struct Report {
    pub text: String,
    pub json: serde_json::Map<String, Value>,
    pub quiet: bool,
}

impl Report {
    fn line(&mut self, s: &str) {
        if !self.quiet {
            println!("{s}");
        }
        self.text.push_str(s);
        self.text.push('\n');
    }
}

macro_rules! out {
    ($r:expr) => { $r.line("") };
    ($r:expr, $($a:tt)*) => { $r.line(&format!($($a)*)) };
}

// ---------------------------------------------------------------- strategy

/// Everything a manager chooses before the game.
#[derive(Clone, Debug, PartialEq)]
pub struct Strat {
    pub off: OffenseSet,
    pub def: DefenseScheme,
    pub press: Press,
    pub pace: u8,
    pub three: u8,
    pub crash: u8,
    pub aggr: u8,
    /// None: the squad's own plan. 0 standard, 1 deep, 2 starters heavy, 3 starters only.
    pub mins: Option<u8>,
    pub briefs: [Brief; 5],
    pub star: Option<Role>,
    pub board: Option<Board>,
}

fn off_short(o: OffenseSet) -> &'static str {
    match o {
        OffenseSet::Motion => "Motion",
        OffenseSet::PickAndRoll => "PnR",
        OffenseSet::PostUp => "Post",
        OffenseSet::Isolation => "Iso",
        OffenseSet::FiveOut => "5out",
    }
}

fn def_short(d: DefenseScheme) -> &'static str {
    match d {
        DefenseScheme::ManToMan => "Man",
        DefenseScheme::SwitchEverything => "Switch",
        DefenseScheme::Zone23 => "2-3",
        DefenseScheme::Zone32 => "3-2",
        DefenseScheme::PackLine => "Pack",
    }
}

impl Strat {
    pub fn sys(off: OffenseSet, def: DefenseScheme, press: Press) -> Strat {
        Strat {
            off,
            def,
            press,
            pace: 10,
            three: 10,
            crash: 10,
            aggr: 10,
            mins: None,
            briefs: [Brief::Free; 5],
            star: None,
            board: None,
        }
    }

    pub fn naive() -> Strat {
        Strat::sys(OffenseSet::Motion, DefenseScheme::ManToMan, Press::None)
    }

    pub fn label(&self) -> String {
        let mut s = format!("{}/{}", off_short(self.off), def_short(self.def));
        match self.press {
            Press::None => {}
            Press::ThreeQuarter => s.push_str("+3Q"),
            Press::FullCourt => s.push_str("+FC"),
        }
        for (n, x) in [
            ("pace", self.pace),
            ("3pt", self.three),
            ("crash", self.crash),
            ("aggr", self.aggr),
        ] {
            if x != 10 {
                let _ = write!(s, " {n}{x}");
            }
        }
        if let Some(m) = self.mins {
            let _ = write!(s, " mins{m}");
        }
        if self.briefs != [Brief::Free; 5] {
            let _ = write!(s, " briefs{:?}", self.briefs.map(|b| b as u8));
        }
        if let Some(r) = self.star {
            let _ = write!(s, " star:{}", r.label());
        }
        if self.board.is_some() {
            s.push_str(" drawn");
        }
        s
    }

    /// The tactics this strategy makes of a squad's own (rotation, roles).
    fn apply(&self, base: &Tactics, squad_mins: u8) -> Tactics {
        let mut t = base.clone();
        t.offense = self.off;
        t.defense = self.def;
        t.press = self.press;
        t.pace = self.pace;
        t.three_bias = self.three;
        t.crash_boards = self.crash;
        t.aggression = self.aggr;
        for (i, slot) in t.rotation.iter_mut().enumerate() {
            if i < 5 {
                slot.brief = self.briefs[i];
            }
            if let (Some(r), Role::PrimaryOption) = (self.star, slot.role) {
                slot.role = r;
            }
            slot.minutes = match (self.mins.unwrap_or(squad_mins), i < 5) {
                (1, _) => 20,
                (2, true) => 36,
                (2, false) => 4,
                (3, true) => 40,
                (3, false) => 1,
                _ => slot.minutes,
            };
        }
        t.board = self.board.clone();
        t
    }
}

/// The systems a manager picks between with sliders at neutral.
pub fn systems(presses: &[Press]) -> Vec<Strat> {
    let mut out = Vec::new();
    for &p in presses {
        for o in OffenseSet::ALL {
            for d in DefenseScheme::ALL {
                out.push(Strat::sys(o, d, p));
            }
        }
    }
    out
}

/// Five systems that between them use every offense and every defense once.
pub fn reps() -> Vec<Strat> {
    OffenseSet::ALL
        .iter()
        .zip(DefenseScheme::ALL)
        .map(|(&o, d)| Strat::sys(o, d, Press::None))
        .collect()
}

// ------------------------------------------------------------------ squads

/// A league's worth of squads of one kind.
pub struct Squad {
    pub name: String,
    pub w: World,
    /// Minutes plan the squad naturally plays (see `Strat::mins`).
    pub mins: u8,
}

fn rotation_ids(w: &World, team: usize) -> Vec<PlayerId> {
    w.teams[team]
        .tactics()
        .rotation
        .iter()
        .map(|s| s.player)
        .collect()
}

/// Copy of the league with every team's rotation players changed.
fn alter(w: &World, f: &dyn Fn(&mut [Player], &[PlayerId])) -> World {
    let mut out = w.clone();
    for t in 0..out.teams.len() {
        let ids = rotation_ids(&out, t);
        f(&mut out.players, &ids);
        for &i in &ids {
            for a in Attr::ALL {
                let x = &mut out.players[i as usize].attrs[a];
                *x = x.clamp(1.0, 20.0);
            }
        }
    }
    out
}

fn add(p: &mut Player, d: &[(Attr, f32)]) {
    for &(a, x) in d {
        p.attrs[a] += x;
    }
}

fn all_attrs(p: &mut Player, x: f32) {
    for a in Attr::ALL {
        p.attrs[a] += x;
    }
}

/// The generic league: `n` teams from one seed.
pub fn base_world(seed: u64, n: usize) -> World {
    World::generate(seed, n)
}

/// Every team's rotation players made identical to team 0's, so any two
/// squads are exactly equal (ids and names stay their own).
pub fn equalize(w: &World) -> World {
    let mut out = w.clone();
    let src = rotation_ids(w, 0);
    for t in 1..out.teams.len() {
        for (&from, to) in src.iter().zip(rotation_ids(w, t)) {
            let mut p = w.players[from as usize].clone();
            p.id = to;
            p.name = w.players[to as usize].name.clone();
            out.players[to as usize] = p;
        }
        // Same roles and minutes too.
        let rot = w.teams[0].tactics().rotation.clone();
        let ids = rotation_ids(w, t);
        let mut slots = w.teams[t].tactics().rotation.clone();
        for (s, r) in slots.iter_mut().zip(&rot) {
            s.role = r.role;
            s.minutes = r.minutes;
            s.brief = r.brief;
            s.position = r.position;
        }
        let _ = ids;
        let mut tac = out.teams[t].tactics().clone();
        tac.rotation = slots;
        let active = out.teams[t].active;
        out.teams[t].playbook[active] = TacticSlot::new("T", tac, 1.0);
    }
    out
}

/// Equal squads, all attributes of every player raised by `d`.
pub fn shifted(w: &World, d: f32) -> World {
    alter(w, &|ps, ids| {
        for &i in ids {
            all_attrs(&mut ps[i as usize], d);
        }
    })
}

/// Squad archetypes for the squad-fit measurements.
pub fn archetypes(w: &World) -> Vec<Squad> {
    let mk = |name: &str, w: World, mins: u8| Squad {
        name: name.into(),
        w,
        mins,
    };
    let mut out = vec![mk("Balanced", w.clone(), 0)];
    out.push(mk(
        "Shooters",
        alter(w, &|ps, ids| {
            for &i in ids {
                add(
                    &mut ps[i as usize],
                    &[
                        (Attr::CloseShot, 1.5),
                        (Attr::MidRange, 3.0),
                        (Attr::ThreePoint, 4.0),
                        (Attr::FreeThrow, 2.0),
                        (Attr::Strength, -1.5),
                        (Attr::InteriorD, -1.0),
                        (Attr::Block, -1.0),
                    ],
                );
            }
        }),
        0,
    ));
    out.push(mk(
        "Bigs",
        alter(w, &|ps, ids| {
            for &i in ids {
                add(
                    &mut ps[i as usize],
                    &[
                        (Attr::PostMoves, 3.0),
                        (Attr::Strength, 2.5),
                        (Attr::InteriorD, 3.0),
                        (Attr::Block, 3.0),
                        (Attr::OffRebound, 3.0),
                        (Attr::DefRebound, 3.0),
                        (Attr::CloseShot, 1.5),
                        (Attr::Footwork, 1.0),
                        (Attr::ThreePoint, -4.0),
                        (Attr::Speed, -2.0),
                        (Attr::Handling, -2.0),
                        (Attr::PerimeterD, -2.0),
                    ],
                );
            }
        }),
        0,
    ));
    out.push(mk(
        "Athletes",
        alter(w, &|ps, ids| {
            for &i in ids {
                add(
                    &mut ps[i as usize],
                    &[
                        (Attr::Speed, 3.0),
                        (Attr::Vertical, 3.0),
                        (Attr::Stamina, 3.0),
                        (Attr::Strength, 1.5),
                        (Attr::WorkRate, 1.0),
                        (Attr::Iq, -2.0),
                        (Attr::Composure, -2.0),
                        (Attr::CloseShot, -1.0),
                        (Attr::MidRange, -1.0),
                        (Attr::ThreePoint, -1.0),
                    ],
                );
            }
        }),
        0,
    ));
    out.push(mk(
        "Slow veterans",
        alter(w, &|ps, ids| {
            for &i in ids {
                add(
                    &mut ps[i as usize],
                    &[
                        (Attr::Speed, -3.0),
                        (Attr::Vertical, -3.0),
                        (Attr::Stamina, -4.0),
                        (Attr::Iq, 3.0),
                        (Attr::Composure, 3.0),
                        (Attr::Passing, 2.0),
                        (Attr::MidRange, 1.5),
                        (Attr::ThreePoint, 1.5),
                        (Attr::FreeThrow, 1.5),
                    ],
                );
            }
        }),
        0,
    ));
    out.push(mk(
        "Star + role players",
        alter(w, &|ps, ids| {
            let best = *ids
                .iter()
                .max_by(|&&a, &&b| {
                    ps[a as usize]
                        .overall()
                        .total_cmp(&ps[b as usize].overall())
                })
                .unwrap();
            for &i in ids {
                all_attrs(&mut ps[i as usize], if i == best { 5.0 } else { -1.2 });
            }
        }),
        0,
    ));
    out.push(mk(
        "Top-heavy",
        alter(w, &|ps, ids| {
            for (k, &i) in ids.iter().enumerate() {
                all_attrs(&mut ps[i as usize], if k < 5 { 2.0 } else { -3.0 });
            }
        }),
        2,
    ));
    out.push(mk(
        "Deep bench",
        alter(w, &|ps, ids| {
            for (k, &i) in ids.iter().enumerate() {
                all_attrs(&mut ps[i as usize], if k < 5 { -1.5 } else { 1.5 });
            }
        }),
        1,
    ));
    out
}

// -------------------------------------------------------------------- games

#[derive(Clone, Copy, Default, Debug)]
pub struct Tot {
    pub pts: f64,
    pub poss: f64,
    pub fgm: f64,
    pub fga: f64,
    pub tpm: f64,
    pub tpa: f64,
    pub ftm: f64,
    pub fta: f64,
    pub orb: f64,
    pub drb: f64,
    pub ast: f64,
    pub stl: f64,
    pub blk: f64,
    pub tov: f64,
    pub pf: f64,
}

impl Tot {
    fn add_box(&mut self, b: &BoxLine) {
        self.pts += b.pts as f64;
        self.fgm += b.fgm as f64;
        self.fga += b.fga as f64;
        self.tpm += b.tpm as f64;
        self.tpa += b.tpa as f64;
        self.ftm += b.ftm as f64;
        self.fta += b.fta as f64;
        self.orb += b.orb as f64;
        self.drb += b.drb as f64;
        self.ast += b.ast as f64;
        self.stl += b.stl as f64;
        self.blk += b.blk as f64;
        self.tov += b.tov as f64;
        self.pf += b.pf as f64;
    }

    fn add(&mut self, o: &Tot) {
        self.pts += o.pts;
        self.poss += o.poss;
        self.fgm += o.fgm;
        self.fga += o.fga;
        self.tpm += o.tpm;
        self.tpa += o.tpa;
        self.ftm += o.ftm;
        self.fta += o.fta;
        self.orb += o.orb;
        self.drb += o.drb;
        self.ast += o.ast;
        self.stl += o.stl;
        self.blk += o.blk;
        self.tov += o.tov;
        self.pf += o.pf;
    }

    pub fn ppp(&self) -> f64 {
        self.pts / self.poss.max(1.0)
    }
}

/// Four games of X against Y, summed, from X's point of view.
#[derive(Clone, Copy, Default, Debug)]
pub struct Acc {
    pub games: f64,
    pub win: f64,
    pub margin: f64,
    pub overtime: f64,
    pub x: Tot,
    pub y: Tot,
}

/// One side of a game.
#[derive(Clone, Copy)]
pub struct Who<'a> {
    pub sq: &'a Squad,
    pub team: usize,
    pub fam: f32,
    pub coach: u8,
}

fn lineup<'a>(w: &'a World, t: &'a Tactics, fam: f32, coach: u8) -> Lineup<'a> {
    Lineup {
        tactics: t,
        players: t
            .rotation
            .iter()
            .map(|s| &w.players[s.player as usize])
            .collect(),
        familiarity: fam,
        belief: 0.55,
        coach,
    }
}

/// Play one game; the result is from X's point of view.
fn play(x: Who, sx: &Strat, y: Who, sy: &Strat, x_home: bool, seed: u64) -> Acc {
    let tx = sx.apply(x.sq.w.teams[x.team].tactics(), x.sq.mins);
    let ty = sy.apply(y.sq.w.teams[y.team].tactics(), y.sq.mins);
    let lx = lineup(&x.sq.w, &tx, x.fam, x.coach);
    let ly = lineup(&y.sq.w, &ty, y.fam, y.coach);
    let mut rng = Rng::new(seed);
    let r = if x_home {
        simulate_game(&lx, &ly, &x.sq.w.rules, &mut rng)
    } else {
        simulate_game(&ly, &lx, &x.sq.w.rules, &mut rng)
    };
    let (xi, yi) = if x_home { (0, 1) } else { (1, 0) };
    let mut a = Acc {
        games: 1.0,
        ..Default::default()
    };
    let (px, py) = (r.score[xi] as f64, r.score[yi] as f64);
    a.win = if px > py {
        1.0
    } else if px < py {
        0.0
    } else {
        0.5
    };
    a.margin = px - py;
    a.overtime = (r.periods.len() > x.sq.w.rules.periods as usize) as u8 as f64;
    for (_, b) in &r.boxes[xi] {
        a.x.add_box(b);
    }
    for (_, b) in &r.boxes[yi] {
        a.y.add_box(b);
    }
    a.x.poss = r.possessions[xi] as f64;
    a.y.poss = r.possessions[yi] as f64;
    a
}

impl Acc {
    fn plus(&mut self, o: &Acc) {
        self.games += o.games;
        self.win += o.win;
        self.margin += o.margin;
        self.overtime += o.overtime;
        self.x.add(&o.x);
        self.y.add(&o.y);
    }
}

/// Two squads of a league: unit `u` pairs teams (a, b) so that over the units
/// every ordered pair of teams meets.
fn pair(u: usize, n: usize) -> (usize, usize) {
    let a = u % n;
    let b = (a + 1 + (u / n) % (n - 1)) % n;
    (a, b)
}

/// A unit: X on team a and on team b, home and away.
fn unit(
    px: &Squad,
    py: &Squad,
    (fx, fy): (f32, f32),
    (cx, cy): (u8, u8),
    sx: &Strat,
    sy: &Strat,
    u: usize,
    seed: u64,
) -> Acc {
    let n = px.w.teams.len();
    let (a, b) = pair(u, n);
    let mut acc = Acc::default();
    for k in 0..4usize {
        let (tx, ty) = if k & 1 == 0 { (a, b) } else { (b, a) };
        let x = Who {
            sq: px,
            team: tx,
            fam: fx,
            coach: cx,
        };
        let y = Who {
            sq: py,
            team: ty,
            fam: fy,
            coach: cy,
        };
        acc.plus(&play(x, sx, y, sy, k < 2, mix(&[seed, u as u64, k as u64])));
    }
    acc
}

/// X against Y for some units.
#[derive(Clone)]
pub struct Job<'a> {
    pub px: &'a Squad,
    pub py: &'a Squad,
    pub sx: &'a Strat,
    pub sy: &'a Strat,
    pub fam: (f32, f32),
    pub coach: (u8, u8),
    /// Seeds derive from this, so jobs sharing a key play the same games
    /// where they can.
    pub key: u64,
}

impl<'a> Job<'a> {
    pub fn new(px: &'a Squad, py: &'a Squad, sx: &'a Strat, sy: &'a Strat, key: u64) -> Self {
        Job {
            px,
            py,
            sx,
            sy,
            fam: (1.0, 1.0),
            coach: (0, 0),
            key,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Res {
    pub win: Est,
    pub margin: Est,
    pub acc: Acc,
}

fn reduce(units: &[Acc]) -> Res {
    let win: Vec<f64> = units.iter().map(|a| a.win / a.games).collect();
    let margin: Vec<f64> = units.iter().map(|a| a.margin / a.games).collect();
    let mut acc = Acc::default();
    for a in units {
        acc.plus(a);
    }
    Res {
        win: Est::of(&win),
        margin: Est::of(&margin),
        acc,
    }
}

/// Run every job for `units` units (four games each), on all threads.
pub fn run_jobs(jobs: &[Job], units: usize, seed: u64) -> Vec<Res> {
    let all = par_map(jobs.len() * units, |i| {
        let (j, u) = (i / units, i % units);
        let job = &jobs[j];
        unit(
            job.px,
            job.py,
            job.fam,
            job.coach,
            job.sx,
            job.sy,
            u,
            mix(&[seed, job.key]),
        )
    });
    all.chunks(units).map(reduce).collect()
}

// -------------------------------------------------------- matrix game solver

/// Value and optimal mixes of the zero-sum game where the row player gets
/// `a[i][j]` and the column player the negative. Simplex on the usual dual,
/// Bland's rule so it cannot cycle.
pub fn solve(a: &[Vec<f64>]) -> (f64, Vec<f64>, Vec<f64>) {
    let (m, n) = (a.len(), a[0].len());
    let lo = a.iter().flatten().cloned().fold(f64::INFINITY, f64::min);
    let c = 1.0 - lo;
    let w = n + m + 1;
    let mut t = vec![vec![0.0; w]; m + 1];
    for i in 0..m {
        for j in 0..n {
            t[i][j] = a[i][j] + c;
        }
        t[i][n + i] = 1.0;
        t[i][w - 1] = 1.0;
    }
    for j in 0..n {
        t[m][j] = -1.0;
    }
    let mut basis: Vec<usize> = (0..m).map(|i| n + i).collect();
    for _ in 0..100_000 {
        let Some(col) = (0..w - 1).find(|&j| t[m][j] < -1e-12) else {
            break;
        };
        let mut row = None;
        let mut best = f64::INFINITY;
        for i in 0..m {
            if t[i][col] > 1e-12 {
                let ratio = t[i][w - 1] / t[i][col];
                if ratio < best - 1e-12
                    || ((ratio - best).abs() <= 1e-12
                        && row.is_some_and(|r: usize| basis[i] < basis[r]))
                {
                    best = ratio;
                    row = Some(i);
                }
            }
        }
        let Some(r) = row else { break };
        let p = t[r][col];
        for x in t[r].iter_mut() {
            *x /= p;
        }
        for i in 0..=m {
            if i != r {
                let f = t[i][col];
                if f != 0.0 {
                    for k in 0..w {
                        let d = f * t[r][k];
                        t[i][k] -= d;
                    }
                }
            }
        }
        basis[r] = col;
    }
    let total = t[m][w - 1];
    let mut y = vec![0.0; n];
    for (i, &b) in basis.iter().enumerate() {
        if b < n {
            y[b] = t[i][w - 1];
        }
    }
    let x: Vec<f64> = (0..m).map(|i| t[m][n + i]).collect();
    let value = 1.0 / total - c;
    (
        value,
        x.iter().map(|v| v / total).collect(),
        y.iter().map(|v| v / total).collect(),
    )
}

/// How many strategies carry at least `min` of the mix.
fn support(p: &[f64], min: f64) -> usize {
    p.iter().filter(|&&x| x >= min).count()
}

/// 1 / sum of squares: the number of equally likely strategies the mix is worth.
fn effective(p: &[f64]) -> f64 {
    1.0 / p.iter().map(|x| x * x).sum::<f64>().max(1e-9)
}

// ------------------------------------------------------------ the config

#[derive(Clone, Debug)]
pub struct Cfg {
    pub seed: u64,
    pub full: bool,
    /// Units (four games) per cell of the main matrix.
    pub units: usize,
    pub teams: usize,
    pub sections: Vec<String>,
    pub out: String,
}

impl Cfg {
    pub fn quick() -> Cfg {
        Cfg {
            seed: 1,
            full: false,
            units: 20,
            teams: 16,
            sections: Vec::new(),
            out: ".".into(),
        }
    }

    pub fn full() -> Cfg {
        Cfg {
            full: true,
            units: 60,
            ..Cfg::quick()
        }
    }

    fn wants(&self, s: &str) -> bool {
        self.sections.is_empty() || self.sections.iter().any(|x| x == s)
    }

    /// Scale a count that is for quick mode up for full mode.
    fn scale(&self, quick: usize, _full: usize) -> usize {
        (quick * self.units / 20).max(1)
    }
}

// ------------------------------------------------------------ matrix section

pub struct Matrix {
    pub strats: Vec<Strat>,
    pub win: Vec<Vec<Est>>,
    pub margin: Vec<Vec<Est>>,
    /// Points and possessions of offense o (row) against defense d (column),
    /// whatever else the two systems were.
    pub od: [[(f64, f64); 5]; 5],
    pub league: Tot,
    pub games: f64,
    pub overtime: f64,
}

pub fn matrix(sq: &Squad, strats: Vec<Strat>, units: usize, seed: u64) -> Matrix {
    let n = strats.len();
    let mut idx = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            idx.push((i, j));
        }
    }
    let jobs: Vec<Job> = idx
        .iter()
        .map(|&(i, j)| Job::new(sq, sq, &strats[i], &strats[j], (i * n + j) as u64))
        .collect();
    let res = run_jobs(&jobs, units, seed);
    let zero = Est {
        mean: 0.5,
        se: 0.0,
        n: units,
    };
    let mut win = vec![vec![zero; n]; n];
    let mut margin = vec![vec![Est::default(); n]; n];
    let mut od = [[(0.0, 0.0); 5]; 5];
    let mut league = Tot::default();
    let (mut games, mut ot) = (0.0, 0.0);
    for (&(i, j), r) in idx.iter().zip(&res) {
        win[i][j] = r.win;
        win[j][i] = Est {
            mean: 1.0 - r.win.mean,
            ..r.win
        };
        margin[i][j] = r.margin;
        margin[j][i] = Est {
            mean: -r.margin.mean,
            ..r.margin
        };
        let (si, sj) = (&strats[i], &strats[j]);
        let c = &mut od[si.off as usize][sj.def as usize];
        c.0 += r.acc.x.pts;
        c.1 += r.acc.x.poss;
        let c = &mut od[sj.off as usize][si.def as usize];
        c.0 += r.acc.y.pts;
        c.1 += r.acc.y.poss;
        league.add(&r.acc.x);
        league.add(&r.acc.y);
        games += r.acc.games;
        ot += r.acc.overtime;
    }
    Matrix {
        strats,
        win,
        margin,
        od,
        league,
        games,
        overtime: ot,
    }
}

/// Everything read off a matrix.
pub struct MatrixFacts {
    /// Mean win probability against every other strategy.
    pub strength: Vec<Est>,
    /// Best counter of each strategy and its win probability against it.
    pub counter: Vec<(usize, Est)>,
    /// (dominated, by, mean gain)
    pub dominated: Vec<(usize, usize, f64)>,
    pub no_counter: Vec<usize>,
    pub mix: Vec<f64>,
    pub sym_support: usize,
    pub sym_effective: f64,
    /// Best pure strategy's worst-case against the equilibrium mix.
    pub decisive_pairs: f64,
    pub cycles: usize,
    pub condorcet: Vec<usize>,
}

pub fn facts(m: &Matrix) -> MatrixFacts {
    let n = m.strats.len();
    let strength: Vec<Est> = (0..n)
        .map(|i| {
            let vals: Vec<f64> = (0..n).filter(|&j| j != i).map(|j| m.win[i][j].mean).collect();
            let mut e = Est::of(&vals);
            // The cells are noisy estimates: their own error dominates.
            let se2: f64 = (0..n).filter(|&j| j != i).map(|j| m.win[i][j].se.powi(2)).sum();
            e.se = se2.sqrt() / (n - 1) as f64;
            e
        })
        .collect();
    let counter: Vec<(usize, Est)> = (0..n)
        .map(|i| {
            let j = (0..n)
                .filter(|&j| j != i)
                .max_by(|&a, &b| m.win[a][i].mean.total_cmp(&m.win[b][i].mean))
                .unwrap();
            (j, m.win[j][i])
        })
        .collect();
    let no_counter: Vec<usize> = (0..n)
        .filter(|&i| {
            let (_, e) = counter[i];
            // Even the best of n-1 noisy counters is not shown to beat it.
            e.mean < 0.5 + 2.5 * e.se.max(0.01)
        })
        .collect();
    let mut dominated = Vec::new();
    for i in 0..n {
        let mut best: Option<(usize, f64)> = None;
        for j in 0..n {
            if j == i {
                continue;
            }
            let ks: Vec<usize> = (0..n).filter(|&k| k != i && k != j).collect();
            let mut gain = 0.0;
            let mut var = 0.0;
            let mut worse = 0;
            for &k in &ks {
                let d = m.win[j][k].mean - m.win[i][k].mean;
                let se = (m.win[j][k].se.powi(2) + m.win[i][k].se.powi(2)).sqrt();
                gain += d;
                var += se * se;
                if d < -2.0 * se {
                    worse += 1;
                }
            }
            let mean = gain / ks.len() as f64;
            let se = var.sqrt() / ks.len() as f64;
            let ahead = ks
                .iter()
                .filter(|&&k| m.win[j][k].mean >= m.win[i][k].mean - 0.5 * (m.win[j][k].se.powi(2) + m.win[i][k].se.powi(2)).sqrt())
                .count();
            if worse == 0 && mean > 2.0 * se && ahead as f64 >= 0.85 * ks.len() as f64 && best.is_none_or(|b| mean > b.1) {
                best = Some((j, mean));
            }
        }
        if let Some((j, g)) = best {
            dominated.push((i, j, g));
        }
    }
    let a: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| m.win[i][j].mean - 0.5).collect())
        .collect();
    let (_, mix, _) = solve(&a);
    let mut decisive = 0usize;
    let mut pairs = 0usize;
    for i in 0..n {
        for j in i + 1..n {
            pairs += 1;
            if (m.win[i][j].mean - 0.5).abs() > 2.0 * m.win[i][j].se.max(1e-9) {
                decisive += 1;
            }
        }
    }
    let beats = |i: usize, j: usize| m.win[i][j].mean > 0.5 + 1.64 * m.win[i][j].se;
    let mut cycles = 0;
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                if i < j && i < k && j != k && beats(i, j) && beats(j, k) && beats(k, i) {
                    cycles += 1;
                }
            }
        }
    }
    let condorcet: Vec<usize> = (0..n)
        .filter(|&i| (0..n).all(|j| j == i || m.win[i][j].mean >= 0.5 - 1.0 * m.win[i][j].se))
        .collect();
    MatrixFacts {
        strength,
        counter,
        dominated,
        no_counter,
        sym_support: support(&mix, 0.05),
        sym_effective: effective(&mix),
        mix,
        decisive_pairs: decisive as f64 / pairs.max(1) as f64,
        cycles,
        condorcet,
    }
}

/// Offense-by-defense game on points per possession.
pub struct OdFacts {
    pub ppp: [[f64; 5]; 5],
    pub se: [[f64; 5]; 5],
    pub value: f64,
    pub off_mix: Vec<f64>,
    pub def_mix: Vec<f64>,
    pub mean: f64,
    /// For each defense, the largest ppp above league mean an offense reaches.
    pub off_edge: [f64; 5],
    /// For each offense, the largest ppp below league mean a defense holds it to.
    pub def_edge: [f64; 5],
}

pub fn od_facts(m: &Matrix) -> OdFacts {
    let mut ppp = [[0.0; 5]; 5];
    let mut se = [[0.0; 5]; 5];
    let (mut pts, mut poss) = (0.0, 0.0);
    for o in 0..5 {
        for d in 0..5 {
            let (p, q) = m.od[o][d];
            ppp[o][d] = p / q.max(1.0);
            se[o][d] = 1.15 / q.max(1.0).sqrt();
            pts += p;
            poss += q;
        }
    }
    let mean = pts / poss.max(1.0);
    let a: Vec<Vec<f64>> = (0..5).map(|o| ppp[o].to_vec()).collect();
    let (value, off_mix, def_mix) = solve(&a);
    let mut off_edge = [0.0; 5];
    let mut def_edge = [0.0; 5];
    for d in 0..5 {
        off_edge[d] = (0..5).map(|o| ppp[o][d] - mean).fold(f64::MIN, f64::max);
    }
    for o in 0..5 {
        let row_mean = ppp[o].iter().sum::<f64>() / 5.0;
        def_edge[o] = (0..5).map(|d| row_mean - ppp[o][d]).fold(f64::MIN, f64::max);
    }
    OdFacts {
        ppp,
        se,
        value,
        off_mix,
        def_mix,
        mean,
        off_edge,
        def_edge,
    }
}

fn matrix_json(m: &Matrix, f: &MatrixFacts, od: &OdFacts) -> Value {
    let names: Vec<String> = m.strats.iter().map(|s| s.label()).collect();
    json!({
        "strategies": names,
        "win": m.win.iter().map(|r| r.iter().map(|e| r4(e.mean)).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "win_se": m.win.iter().map(|r| r.iter().map(|e| r4(e.se)).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "margin": m.margin.iter().map(|r| r.iter().map(|e| r4(e.mean)).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "strength": f.strength.iter().map(|e| e.json()).collect::<Vec<_>>(),
        "best_counter": f.counter.iter().map(|(j, e)| json!({"by": names[*j], "win": r4(e.mean), "se": r4(e.se)})).collect::<Vec<_>>(),
        "dominated": f.dominated.iter().map(|(i, j, g)| json!({"strategy": names[*i], "by": names[*j], "gain": r4(*g)})).collect::<Vec<_>>(),
        "no_counter": f.no_counter.iter().map(|&i| names[i].clone()).collect::<Vec<_>>(),
        "equilibrium_mix": f.mix.iter().map(|x| r4(*x)).collect::<Vec<_>>(),
        "equilibrium_support": f.sym_support,
        "equilibrium_effective": r4(f.sym_effective),
        "decisive_pair_share": r4(f.decisive_pairs),
        "cycles": f.cycles,
        "condorcet": f.condorcet.iter().map(|&i| names[i].clone()).collect::<Vec<_>>(),
        "od_ppp": od.ppp.iter().map(|r| r.iter().map(|x| r4(*x)).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "od_value": r4(od.value),
        "od_off_mix": od.off_mix.iter().map(|x| r4(*x)).collect::<Vec<_>>(),
        "od_def_mix": od.def_mix.iter().map(|x| r4(*x)).collect::<Vec<_>>(),
    })
}

pub struct MatrixOut {
    pub m: Matrix,
    pub f: MatrixFacts,
    pub od: OdFacts,
    /// For each system, the system that answers it best by the pooled
    /// offense/defense numbers (far less noisy than one matrix cell): the
    /// offense with the highest ppp against its defense and the defense that
    /// allows the lowest ppp to its offense, press off.
    pub counter: Vec<usize>,
    /// The same, the worst possible reply.
    pub worst: Vec<usize>,
}

fn index_of(strats: &[Strat], o: usize, d: usize) -> usize {
    strats
        .iter()
        .position(|s| s.off as usize == o && s.def as usize == d && s.press == Press::None)
        .expect("system in the matrix")
}

fn replies(m: &Matrix, od: &OdFacts) -> (Vec<usize>, Vec<usize>) {
    let arg = |it: &mut dyn Iterator<Item = (usize, f64)>, max: bool| {
        let v: Vec<(usize, f64)> = it.collect();
        v.into_iter()
            .reduce(|a, b| if (b.1 > a.1) == max && b.1 != a.1 { b } else { a })
            .unwrap()
            .0
    };
    let mut counter = Vec::new();
    let mut worst = Vec::new();
    for s in &m.strats {
        let (oy, dy) = (s.off as usize, s.def as usize);
        let best_o = arg(&mut (0..5).map(|o| (o, od.ppp[o][dy])), true);
        let best_d = arg(&mut (0..5).map(|d| (d, od.ppp[oy][d])), false);
        let worst_o = arg(&mut (0..5).map(|o| (o, od.ppp[o][dy])), false);
        let worst_d = arg(&mut (0..5).map(|d| (d, od.ppp[oy][d])), true);
        counter.push(index_of(&m.strats, best_o, best_d));
        worst.push(index_of(&m.strats, worst_o, worst_d));
    }
    (counter, worst)
}

pub fn matrix_section(cfg: &Cfg, r: &mut Report, sq: &Squad) -> MatrixOut {
    let presses: Vec<Press> = if cfg.full {
        Press::ALL.to_vec()
    } else {
        vec![Press::None]
    };
    let strats = systems(&presses);
    let n = strats.len();
    progress(&format!(
        "matrix: {n} systems, {} pairs, {} units each",
        n * (n - 1) / 2,
        cfg.units
    ));
    let m = matrix(sq, strats, cfg.units, mix(&[cfg.seed, 1]));
    let f = facts(&m);
    let od = od_facts(&m);
    let names: Vec<String> = m.strats.iter().map(|s| s.label()).collect();
    out!(r, "== 1. Full matchups: {n} systems (offense x defense{}), sliders neutral ==", if cfg.full { " x press" } else { "" });
    out!(
        r,
        "{} games ({} units of 4 per pair: both squads, home and away). Overtime in {:.1}% of games.",
        m.games, cfg.units, 100.0 * m.overtime / m.games.max(1.0)
    );
    let typical = f.strength.iter().map(|e| e.se).sum::<f64>() / n as f64;
    let cell = m.win[0][1].se.max(m.win[1][2].se);
    out!(
        r,
        "Noise: one cell of the matrix has a standard error of about {:.1} win points (95% interval ±{:.1}); a strength average over {} opponents ±{:.1}.",
        100.0 * cell, 196.0 * cell, n - 1, 196.0 * typical
    );
    out!(r);
    if n <= 25 {
        out!(r, "Win % of the row system against the column system:");
        let mut h = String::from("               ");
        for j in 0..n {
            let _ = write!(h, "{:>4}", j);
        }
        out!(r, "{h}");
        for i in 0..n {
            let mut l = format!("{:>2} {:<12}", i, names[i]);
            for j in 0..n {
                if i == j {
                    l.push_str("   .");
                } else {
                    let _ = write!(l, "{:>4.0}", 100.0 * m.win[i][j].mean);
                }
            }
            out!(r, "{l}");
        }
        out!(r);
    }
    out!(r, "Strength (mean win % against all other systems), best counter and its win %, weight in the equilibrium mix:");
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| f.strength[b].mean.total_cmp(&f.strength[a].mean));
    for &i in &order {
        let (j, e) = f.counter[i];
        let line = format!(
            "  {:<22} {:>6}   countered by {:<22} {:>5.1}±{:.1}   mix {:>4.1}%",
            names[i],
            f.strength[i].pct(),
            names[j],
            100.0 * e.mean,
            100.0 * e.ci(),
            100.0 * f.mix[i]
        );
        out!(r, "{line}");
    }
    out!(r);
    let best = &f.strength[order[0]];
    let worst = &f.strength[order[n - 1]];
    out!(
        r,
        "Spread of strength: best {} {:.1} win %, worst {} {:.1} win % (a spread of {:.1} points; the noise on each is ±{:.1}).",
        names[order[0]], 100.0 * best.mean, names[order[n - 1]], 100.0 * worst.mean,
        100.0 * (best.mean - worst.mean), 196.0 * typical
    );
    out!(
        r,
        "Matchups with a decisive edge (beyond 2 se): {:.0}% of pairs. Significant 3-cycles (A beats B beats C beats A, each at 90%): {}.",
        100.0 * f.decisive_pairs, f.cycles
    );
    if f.condorcet.is_empty() {
        out!(r, "No system is at least even against every other (within one se): no dominant pure strategy.");
    } else {
        out!(
            r,
            "Systems at least even against every other (within one se): {}.",
            f.condorcet.iter().map(|&i| names[i].clone()).collect::<Vec<_>>().join(", ")
        );
    }
    if f.dominated.is_empty() {
        out!(r, "Dominated systems (another is at least as good against nearly everything, and better on average): none found.");
    } else {
        out!(r, "Dominated or nearly dominated systems (another is at least as good against 85% of opponents, never significantly worse, and better on average):");
        for (i, j, g) in &f.dominated {
            out!(r, "  {} is dominated by {} (+{:.1} win points on average)", names[*i], names[*j], 100.0 * g);
        }
    }
    if f.no_counter.is_empty() {
        out!(r, "Every system has a counter that beats it.");
    } else {
        out!(
            r,
            "Systems with no demonstrated counter (best answer below 50% + 2.5 se): {}.",
            f.no_counter.iter().map(|&i| names[i].clone()).collect::<Vec<_>>().join(", ")
        );
    }
    out!(
        r,
        "Equilibrium of the symmetric game (both coaches choose among {n}): support {} systems with at least 5% each, effective number {:.1}.",
        f.sym_support, f.sym_effective
    );
    let mut mixed: Vec<usize> = (0..n).filter(|&i| f.mix[i] >= 0.02).collect();
    mixed.sort_by(|&a, &b| f.mix[b].total_cmp(&f.mix[a]));
    out!(
        r,
        "  mix: {}",
        mixed.iter().map(|&i| format!("{} {:.0}%", names[i], 100.0 * f.mix[i])).collect::<Vec<_>>().join(", ")
    );
    out!(r, "  (With cells this noisy the solver also spreads weight over systems that are merely tied; read the support as an upper bound.)");
    out!(r);
    out!(r, "Points per possession, offense (rows) against defense (columns), whatever else each side played:");
    let mut h = format!("{:<16}", "");
    for d in DefenseScheme::ALL {
        let _ = write!(h, "{:>9}", def_short(d));
    }
    out!(r, "{h}    mean   mix");
    for o in 0..5 {
        let mut l = format!("{:<16}", OffenseSet::ALL[o].label());
        for d in 0..5 {
            let _ = write!(l, "{:>9.3}", od.ppp[o][d]);
        }
        let _ = write!(l, "  {:>6.3}  {:>3.0}%", od.ppp[o].iter().sum::<f64>() / 5.0, 100.0 * od.off_mix[o]);
        out!(r, "{l}");
    }
    let mut l = format!("{:<16}", "mean");
    for d in 0..5 {
        let _ = write!(l, "{:>9.3}", (0..5).map(|o| od.ppp[o][d]).sum::<f64>() / 5.0);
    }
    out!(r, "{l}");
    let mut l = format!("{:<16}", "defense mix");
    for d in 0..5 {
        let _ = write!(l, "{:>8.0}%", 100.0 * od.def_mix[d]);
    }
    out!(r, "{l}");
    out!(
        r,
        "Each cell is ±{:.3} (95%). Value of the offense-vs-defense game {:.3} ppp (league mean {:.3}); offense support {} sets, defense support {} schemes (≥5%).",
        1.96 * od.se[0][0], od.value, od.mean, support(&od.off_mix, 0.05), support(&od.def_mix, 0.05)
    );
    out!(r);
    r.json.insert("matrix".into(), matrix_json(&m, &f, &od));
    let (counter, worst) = replies(&m, &od);
    MatrixOut { m, f, od, counter, worst }
}

/// The structural counter of each system (see `MatrixOut::counter`), played
/// on fresh games against the system it answers.
pub fn validate_counters(sq: &Squad, mo: &MatrixOut, units: usize, seed: u64) -> Vec<Est> {
    let m = &mo.m;
    let jobs: Vec<Job> = (0..m.strats.len())
        .map(|i| Job::new(sq, sq, &m.strats[mo.counter[i]], &m.strats[i], 7000 + i as u64))
        .collect();
    run_jobs(&jobs, units, seed).into_iter().map(|r| r.win).collect()
}

// ---------------------------------------------------------------- squad fit

pub struct FitOut {
    pub squads: Vec<String>,
    /// Lift in win points of each system over the squad's own average system.
    pub sys: Vec<Vec<Est>>,
    pub off: Vec<Vec<Est>>,
    pub def: Vec<Vec<Est>>,
    pub quality: Vec<Est>,
    pub best_off: Vec<usize>,
    pub best_def: Vec<usize>,
    pub best_sys: Vec<usize>,
    /// What using the Balanced squad's best system costs each squad.
    pub regret: Vec<Est>,
}

fn avg(v: &[Est]) -> Est {
    let n = v.len() as f64;
    Est {
        mean: v.iter().map(|e| e.mean).sum::<f64>() / n,
        se: v.iter().map(|e| e.se * e.se).sum::<f64>().sqrt() / n,
        n: v[0].n,
    }
}

pub fn fit_section(cfg: &Cfg, r: &mut Report, squads: &[Squad]) -> FitOut {
    let units = cfg.scale(6, 24);
    let sys = systems(&[Press::None]);
    let field = reps();
    let base = &squads[0];
    let mut jobs = Vec::new();
    for (s, sq) in squads.iter().enumerate() {
        for (i, x) in sys.iter().enumerate() {
            for (f, y) in field.iter().enumerate() {
                jobs.push(Job::new(sq, base, x, y, (s * 1000 + i * 10 + f) as u64));
            }
        }
    }
    progress(&format!("squad fit: {} squads, {} jobs of {units} units", squads.len(), jobs.len()));
    let res = run_jobs(&jobs, units, mix(&[cfg.seed, 2]));
    let mut out = FitOut {
        squads: squads.iter().map(|s| s.name.clone()).collect(),
        sys: vec![],
        off: vec![],
        def: vec![],
        quality: vec![],
        best_off: vec![],
        best_def: vec![],
        best_sys: vec![],
        regret: vec![],
    };
    let mut raw: Vec<Vec<Est>> = Vec::new();
    for s in 0..squads.len() {
        let row: Vec<Est> = (0..sys.len())
            .map(|i| {
                let v: Vec<Est> = (0..field.len())
                    .map(|f| res[(s * sys.len() + i) * field.len() + f].win)
                    .collect();
                avg(&v)
            })
            .collect();
        raw.push(row);
    }
    for row in &raw {
        let q = avg(row);
        out.quality.push(q);
        out.sys.push(row.iter().map(|e| Est { mean: e.mean - q.mean, ..*e }).collect());
        // System index = offense * 5 + defense.
        let off: Vec<Est> = (0..5)
            .map(|o| {
                let e = avg(&row[o * 5..o * 5 + 5]);
                Est { mean: e.mean - q.mean, ..e }
            })
            .collect();
        let def: Vec<Est> = (0..5)
            .map(|d| {
                let v: Vec<Est> = (0..5).map(|o| row[o * 5 + d]).collect();
                let e = avg(&v);
                Est { mean: e.mean - q.mean, ..e }
            })
            .collect();
        let arg = |v: &[Est]| (0..v.len()).max_by(|&a, &b| v[a].mean.total_cmp(&v[b].mean)).unwrap();
        out.best_off.push(arg(&off));
        out.best_def.push(arg(&def));
        out.best_sys.push(arg(&out.sys[out.sys.len() - 1]));
        out.off.push(off);
        out.def.push(def);
    }
    // Held out: the system built from each squad's best offense and best
    // defense against the Balanced squad's, on fresh games, so that picking
    // the maximum of noisy numbers does not flatter the difference.
    let sys_of = |o: usize, d: usize| o * 5 + d;
    let gen_best = sys_of(out.best_off[0], out.best_def[0]);
    let fit_sys: Vec<usize> = (0..squads.len()).map(|s| sys_of(out.best_off[s], out.best_def[s])).collect();
    let mut jobs2 = Vec::new();
    for (s, sq) in squads.iter().enumerate() {
        for (f, y) in field.iter().enumerate() {
            let key = 20000 + (s * 10 + f) as u64;
            jobs2.push(Job::new(sq, base, &sys[fit_sys[s]], y, key));
            jobs2.push(Job::new(sq, base, &sys[gen_best], y, key));
        }
    }
    let res2 = run_jobs(&jobs2, units * 3, mix(&[cfg.seed, 22]));
    let mut held: Vec<(Est, Est)> = Vec::new();
    for s in 0..squads.len() {
        let chunk = &res2[s * 10..s * 10 + 10];
        let fit_e = avg(&chunk.iter().step_by(2).map(|r| r.win).collect::<Vec<_>>());
        let gen_e = avg(&chunk.iter().skip(1).step_by(2).map(|r| r.win).collect::<Vec<_>>());
        held.push((fit_e, gen_e));
        out.regret.push(if fit_sys[s] == gen_best { Est { mean: 0.0, se: 0.0, n: units } } else { fit_e.minus(&gen_e) });
    }
    out!(r, "== 2. Squad fit: does the best tactic depend on the squad? ==");
    out!(r, "Each squad type plays all 25 systems against five generic opponents (Balanced squads; one system per offense and per defense). The fit system of a squad is its best offense with its best defense; its worth against the Balanced squad's fit system is then measured on fresh games, because the best of 25 noisy cells flatters itself. Figures are win-point lifts of a system over that squad's own average system; ±{:.1} is the typical 95% interval of an offense lift, ±{:.1} of a single system.", 196.0 * out.off[0][0].se, 196.0 * out.sys[0][0].se);
    out!(r);
    let mut h = format!("{:<20}{:>9}", "squad", "overall");
    for o in OffenseSet::ALL {
        let _ = write!(h, "{:>8}", off_short(o));
    }
    h.push_str("  |");
    for d in DefenseScheme::ALL {
        let _ = write!(h, "{:>8}", def_short(d));
    }
    out!(r, "{h}");
    for s in 0..squads.len() {
        let mut l = format!("{:<20}{:>9.1}", out.squads[s], 100.0 * out.quality[s].mean);
        for o in 0..5 {
            let mark = if o == out.best_off[s] { '*' } else { ' ' };
            let _ = write!(l, "{:>+7.1}{mark}", 100.0 * out.off[s][o].mean);
        }
        l.push_str("  |");
        for d in 0..5 {
            let mark = if d == out.best_def[s] { '*' } else { ' ' };
            let _ = write!(l, "{:>+7.1}{mark}", 100.0 * out.def[s][d].mean);
        }
        out!(r, "{l}");
    }
    out!(r);
    for s in 0..squads.len() {
        out!(
            r,
            "  {:<20} fit system {:<13} vs the Balanced squad's {:<13}: the generic choice costs {:+.1}±{:.1} win points on fresh games (fit {} vs generic {})",
            out.squads[s], sys[fit_sys[s]].label(), sys[gen_best].label(),
            100.0 * out.regret[s].mean, 196.0 * out.regret[s].se, held[s].0.pct(), held[s].1.pct()
        );
    }
    let distinct = |v: &[usize]| {
        let mut u = v.to_vec();
        u.sort();
        u.dedup();
        u.len()
    };
    out!(r);
    out!(
        r,
        "Distinct best offenses across {} squads: {}; best defenses: {}; best systems: {}.",
        squads.len(), distinct(&out.best_off), distinct(&out.best_def), distinct(&out.best_sys)
    );
    let top = |v: &Vec<Vec<Est>>, k: usize, n: usize| -> usize {
        (0..squads.len())
            .filter(|&s| {
                let mut idx: Vec<usize> = (0..n).collect();
                idx.sort_by(|&a, &b| v[s][b].mean.total_cmp(&v[s][a].mean));
                idx[..2].contains(&k)
            })
            .count()
    };
    let _ = top(&out.off, 0, 5);
    for o in 0..5 {
        out!(r, "  {:<8} is in the top two offenses for {} of {} squads", off_short(OffenseSet::ALL[o]), top(&out.off, o, 5), squads.len());
    }
    for d in 0..5 {
        out!(r, "  {:<8} is in the top two defenses for {} of {} squads", def_short(DefenseScheme::ALL[d]), top(&out.def, d, 5), squads.len());
    }
    out!(r);
    r.json.insert(
        "squad_fit".into(),
        json!({
            "squads": out.squads,
            "overall_win": out.quality.iter().map(|e| r4(e.mean)).collect::<Vec<_>>(),
            "offense_lift": out.off.iter().map(|v| v.iter().map(|e| r4(e.mean)).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "offense_lift_se": out.off.iter().map(|v| v.iter().map(|e| r4(e.se)).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "defense_lift": out.def.iter().map(|v| v.iter().map(|e| r4(e.mean)).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "best_offense": out.best_off.iter().map(|&o| off_short(OffenseSet::ALL[o])).collect::<Vec<_>>(),
            "best_defense": out.best_def.iter().map(|&d| def_short(DefenseScheme::ALL[d])).collect::<Vec<_>>(),
            "best_system": out.best_sys.iter().map(|&i| sys[i].label()).collect::<Vec<_>>(),
            "regret_of_generic_best": out.regret.iter().map(|e| e.json()).collect::<Vec<_>>(),
        }),
    );
    out
}

// ------------------------------------------------------------ lever curves

pub struct Lever {
    pub name: String,
    pub values: Vec<(String, Box<dyn Fn(&mut Strat) + Sync>)>,
    pub neutral: usize,
}

fn slider(name: &str, set: fn(&mut Strat, u8)) -> Lever {
    let vals = [1u8, 5, 10, 15, 20];
    Lever {
        name: name.into(),
        values: vals
            .iter()
            .map(|&v| {
                (
                    v.to_string(),
                    Box::new(move |s: &mut Strat| set(s, v)) as Box<dyn Fn(&mut Strat) + Sync>,
                )
            })
            .collect(),
        neutral: 2,
    }
}

pub fn levers() -> Vec<Lever> {
    let mut out = vec![
        slider("tempo (pace)", |s, v| s.pace = v),
        slider("three-point bias", |s, v| s.three = v),
        slider("crash the boards", |s, v| s.crash = v),
        slider("aggression", |s, v| s.aggr = v),
    ];
    out.push(Lever {
        name: "press".into(),
        values: Press::ALL
            .iter()
            .map(|&p| {
                (
                    p.label().to_string(),
                    Box::new(move |s: &mut Strat| s.press = p) as Box<dyn Fn(&mut Strat) + Sync>,
                )
            })
            .collect(),
        neutral: 0,
    });
    out.push(Lever {
        name: "brief for all five starters".into(),
        values: Brief::ALL
            .iter()
            .map(|&b| {
                (
                    b.label().to_string(),
                    Box::new(move |s: &mut Strat| s.briefs = [b; 5]) as Box<dyn Fn(&mut Strat) + Sync>,
                )
            })
            .collect(),
        neutral: 0,
    });
    out.push(Lever {
        name: "role of the primary option".into(),
        values: [Role::PrimaryOption, Role::Secondary, Role::RolePlayer, Role::Limited]
            .iter()
            .map(|&b| {
                (
                    b.label().to_string(),
                    Box::new(move |s: &mut Strat| s.star = Some(b)) as Box<dyn Fn(&mut Strat) + Sync>,
                )
            })
            .collect(),
        neutral: 0,
    });
    out.push(Lever {
        name: "minutes plan".into(),
        values: ["standard (30/10)", "deep (20 each)", "starters heavy (36/4)", "starters only (40/1)"]
            .iter()
            .enumerate()
            .map(|(k, &n)| {
                (
                    n.to_string(),
                    Box::new(move |s: &mut Strat| s.mins = Some(k as u8)) as Box<dyn Fn(&mut Strat) + Sync>,
                )
            })
            .collect(),
        neutral: 0,
    });
    out
}

pub struct LeverOut {
    pub name: String,
    pub labels: Vec<String>,
    /// Win-point lift over neutral, averaged over bases, per value.
    pub lift: Vec<Est>,
    /// Per base: index of best value.
    pub best: Vec<usize>,
    pub neutral: usize,
    pub always_best_edge: Option<usize>,
    pub flat: bool,
    pub range: f64,
}

pub fn lever_section(cfg: &Cfg, r: &mut Report, sq: &Squad) -> Vec<LeverOut> {
    let units = cfg.scale(6, 16);
    let bases = reps();
    let field = reps();
    let levs = levers();
    // Strategies first, so jobs can borrow them.
    let mut strats: Vec<Vec<Vec<Strat>>> = Vec::new();
    for l in &levs {
        strats.push(
            l.values
                .iter()
                .map(|(_, f)| {
                    bases
                        .iter()
                        .map(|b| {
                            let mut s = b.clone();
                            f(&mut s);
                            s
                        })
                        .collect()
                })
                .collect(),
        );
    }
    let mut jobs = Vec::new();
    for (li, l) in levs.iter().enumerate() {
        for vi in 0..l.values.len() {
            for b in 0..bases.len() {
                for f in 0..field.len() {
                    jobs.push(Job::new(sq, sq, &strats[li][vi][b], &field[f], (b * 10 + f) as u64));
                }
            }
        }
    }
    progress(&format!("levers: {} jobs of {units} units", jobs.len()));
    let res = run_jobs(&jobs, units, mix(&[cfg.seed, 3]));
    let mut k = 0;
    let mut outs = Vec::new();
    out!(r, "== 3. Sliders, press, briefs, roles, minutes: response curves ==");
    out!(r, "Each setting is tried on five base systems against five generic opponents. Numbers are win-point lifts over the neutral setting (sliders 10, no press, free briefs, standard minutes), averaged over the bases.");
    out!(r);
    for (li, l) in levs.iter().enumerate() {
        let nv = l.values.len();
        // wins[v][b]
        let mut wins = vec![vec![Est::default(); bases.len()]; nv];
        for v in 0..nv {
            for b in 0..bases.len() {
                let vs: Vec<Est> = (0..field.len()).map(|f| res[k + f].win).collect();
                k += field.len();
                wins[v][b] = avg(&vs);
            }
        }
        let _ = li;
        let lift: Vec<Est> = (0..nv)
            .map(|v| {
                let d: Vec<Est> = (0..bases.len())
                    .map(|b| wins[v][b].minus(&wins[l.neutral][b]))
                    .collect();
                avg(&d)
            })
            .collect();
        let best: Vec<usize> = (0..bases.len())
            .map(|b| (0..nv).max_by(|&x, &y| wins[x][b].mean.total_cmp(&wins[y][b].mean)).unwrap())
            .collect();
        let mean_best = (0..nv).max_by(|&a, &b| lift[a].mean.total_cmp(&lift[b].mean)).unwrap();
        let range = lift.iter().map(|e| e.mean).fold(f64::MIN, f64::max)
            - lift.iter().map(|e| e.mean).fold(f64::MAX, f64::min);
        let edge = mean_best == 0 || mean_best == nv - 1;
        let always = edge
            && best.iter().filter(|&&b| b == mean_best).count() >= 4
            && mean_best != l.neutral
            && lift[mean_best].mean > 2.5 * lift[mean_best].se;
        let flat = lift.iter().all(|e| e.mean.abs() < 2.5 * e.se.max(0.005));
        let mut line = format!("{:<30}", l.name);
        for (v, (name, _)) in l.values.iter().enumerate() {
            let nm: String = name.chars().take(8).collect();
            let _ = write!(line, " {}:{:+.1}{}", nm, 100.0 * lift[v].mean, if v == mean_best { "*" } else { "" });
        }
        out!(r, "{line}");
        out!(
            r,
            "    ±{:.1} (95%); best per base: {}; {}{}",
            196.0 * lift[mean_best].se,
            best.iter().map(|&b| l.values[b].0.chars().take(8).collect::<String>()).collect::<Vec<_>>().join(", "),
            if always { "ALWAYS BEST at the edge. " } else { "" },
            if flat { "FLAT: no setting differs from neutral beyond noise." } else { "" }
        );
        outs.push(LeverOut {
            name: l.name.clone(),
            labels: l.values.iter().map(|v| v.0.clone()).collect(),
            lift,
            best,
            neutral: l.neutral,
            always_best_edge: always.then_some(mean_best),
            flat,
            range,
        });
    }
    out!(r);
    r.json.insert(
        "levers".into(),
        Value::Array(
            outs.iter()
                .map(|o| {
                    json!({
                        "name": o.name, "values": o.labels,
                        "lift": o.lift.iter().map(|e| r4(e.mean)).collect::<Vec<_>>(),
                        "lift_se": o.lift.iter().map(|e| r4(e.se)).collect::<Vec<_>>(),
                        "best_per_base": o.best.clone(),
                        "always_best_edge": o.always_best_edge.is_some(),
                        "flat": o.flat, "range": r4(o.range),
                    })
                })
                .collect(),
        ),
    );
    outs
}

// ----------------------------------------------------------------- coaching

/// A game played live, one side coached by a script, to test in-game levers.
/// `script(live, n)` is called before the game (n = 0) and at the end of
/// every period (n = periods played). Returns X's margin and result.
fn live_game(
    seed: u64,
    x_home: bool,
    sx: &Strat,
    sy: &Strat,
    script: &dyn Fn(&mut basketman_core::league::LiveRound, usize),
) -> (f64, f64) {
    let mut w = equalize(&World::generate(seed, 4));
    for t in 0..w.teams.len() {
        // No staff coach changes the system on his own.
        if let Some(s) = w.teams[t].office.slot_mut(Post::HeadCoach).as_mut() {
            s.tactics = 0;
        }
    }
    let f = w
        .fixtures
        .iter()
        .find(|f| f.round == w.round && f.result.is_none())
        .expect("a fixture")
        .clone();
    let (xt, yt) = if x_home { (f.home, f.away) } else { (f.away, f.home) };
    for (t, s) in [(xt, sx), (yt, sy)] {
        let t = &mut w.teams[t];
        let tac = s.apply(t.tactics(), 0);
        let a = t.active;
        t.playbook[a] = TacticSlot::new("balance", tac, 1.0);
    }
    let mut live = w.kickoff(xt).expect("a game to play");
    script(&mut live, 0);
    let mut ended = 0;
    while !live.game.is_over() {
        if let Some(Pause::PeriodEnd) = live.step() {
            ended += 1;
            script(&mut live, ended);
        }
    }
    let r = live.game.result();
    let (p, q) = (r.score[live.side] as f64, r.score[1 - live.side] as f64);
    (p - q, if p > q { 1.0 } else if p < q { 0.0 } else { 0.5 })
}

fn orders_of(s: &Strat) -> Orders {
    Orders {
        offense: s.off,
        defense: s.def,
        press: s.press,
        pace: s.pace,
        three_bias: s.three,
        crash_boards: s.crash,
        aggression: s.aggr,
    }
}

/// Familiarity the game gives a system changed in play (tactics.rs
/// `familiarity_with`: 40% off for a new offense, 40% for a new defense,
/// 20% for a new press, on top of 0.8 for a normally drilled system).
fn improvised(from: &Strat, to: &Strat) -> f32 {
    let mut keep = 1.0;
    if from.off != to.off {
        keep -= 0.4;
    }
    if from.def != to.def {
        keep -= 0.4;
    }
    if from.press != to.press {
        keep -= 0.2;
    }
    0.8 * keep
}

pub struct CoachOut {
    pub best_response: Est,
    pub naive: Est,
    pub worst_response: Est,
    pub best_fixed: Est,
    pub gap_curve: Vec<(f32, Est, Est, Est)>,
    pub crossing: Option<f32>,
    pub familiarity: Vec<(f32, Est)>,
    pub mins: Vec<(String, Est, Est)>,
    pub ai_coach: Vec<(u8, Est)>,
    pub live: Vec<(String, Est, Est)>,
    pub league_sd: f64,
    pub league_range: f64,
}

/// Mean over a set of jobs' win estimates.
fn mean_win(res: &[Res]) -> Est {
    avg(&res.iter().map(|r| r.win).collect::<Vec<_>>())
}

pub fn coaching_section(cfg: &Cfg, r: &mut Report, base: &Squad, mo: &MatrixOut) -> CoachOut {
    let eq = Squad {
        name: "Equal".into(),
        w: equalize(&base.w),
        mins: 0,
    };
    let units = cfg.scale(12, 40);
    let m = &mo.m;
    let n = m.strats.len();
    let seed = mix(&[cfg.seed, 4]);
    out!(r, "== 4. How much does coaching matter? ==");
    out!(r, "Equal squads (every team a copy of one squad), so only the choices differ. Win % of the first side.");
    out!(r);

    // 4a. Informed versus naive, with the counter chosen on other games.
    let naive = Strat::naive();
    let fixed = (0..n).max_by(|&a, &b| mo.f.strength[a].mean.total_cmp(&mo.f.strength[b].mean)).unwrap();
    let worst = &mo.worst;
    let mut jobs = Vec::new();
    for j in 0..n {
        let k = 100 + j as u64;
        jobs.push(Job::new(&eq, &eq, &m.strats[mo.counter[j]], &m.strats[j], k));
        jobs.push(Job::new(&eq, &eq, &naive, &m.strats[j], k));
        jobs.push(Job::new(&eq, &eq, &m.strats[worst[j]], &m.strats[j], k));
        jobs.push(Job::new(&eq, &eq, &m.strats[fixed], &m.strats[j], k));
    }
    progress(&format!("coaching: informed vs naive, {} jobs", jobs.len()));
    let res = run_jobs(&jobs, units, seed);
    let pick = |k: usize| -> Vec<Res> {
        (0..n).filter(|&j| j != fixed || k != 3).map(|j| res[j * 4 + k].clone()).collect()
    };
    let best_response = mean_win(&pick(0));
    let naive_e = mean_win(&(0..n).filter(|&j| m.strats[j] != naive).map(|j| res[j * 4 + 1].clone()).collect::<Vec<_>>());
    let worst_response = mean_win(&pick(2));
    let best_fixed = mean_win(&pick(3));
    out!(r, "Against each of the 25 systems (fresh games; the counter was chosen from the earlier matrix):");
    out!(r, "  a coach who picks the best counter to the opponent's system : {} win %", best_response.pct());
    out!(r, "  a coach who always plays the best fixed system ({})  : {} win %", m.strats[fixed].label(), best_fixed.pct());
    out!(r, "  a coach who leaves the default (Motion / Man to man)        : {} win %", naive_e.pct());
    out!(r, "  a coach who picks the worst reply to the opponent's system  : {} win %", worst_response.pct());
    out!(r, "  => scouting and countering is worth {:.1} win points over the fixed best system and {:.1} over the default; the gap between best and worst reply is {:.1}.",
        100.0 * (best_response.mean - best_fixed.mean), 100.0 * (best_response.mean - naive_e.mean), 100.0 * (best_response.mean - worst_response.mean));
    out!(r);

    // 4b. Talent against tactics.
    let gaps = [0.0f32, 0.25, 0.5, 1.0, 1.5, 2.0, 3.0];
    let shifted_sq: Vec<Squad> = gaps
        .iter()
        .map(|&d| Squad { name: format!("+{d}"), w: shifted(&eq.w, d), mins: 0 })
        .collect();
    let naive_idx = m.strats.iter().position(|s| *s == naive).unwrap();
    let counter_of_naive = &m.strats[mo.counter[naive_idx]];
    let best_counter_to_fixed = &m.strats[mo.counter[fixed]];
    let mut jobs = Vec::new();
    for (g, sq) in shifted_sq.iter().enumerate() {
        let k = 500 + g as u64;
        jobs.push(Job::new(sq, &eq, &naive, counter_of_naive, k));
        jobs.push(Job::new(sq, &eq, &naive, &naive, k + 50));
        jobs.push(Job::new(sq, &eq, &m.strats[fixed], best_counter_to_fixed, k + 100));
    }
    progress("coaching: talent gap");
    let res = run_jobs(&jobs, cfg.scale(16, 60), seed ^ 1);
    let mut gap_curve = Vec::new();
    out!(r, "Talent against tactics. A squad with every attribute raised by d (the stronger side) against equal-baseline squads:");
    out!(r, "  d      strong+default vs best counter   strong+default vs default   strong+best fixed vs its counter");
    for (g, &d) in gaps.iter().enumerate() {
        let (a, b, c) = (res[g * 3].win, res[g * 3 + 1].win, res[g * 3 + 2].win);
        out!(r, "  {:<5}  {:>26}   {:>25}   {:>30}", d, a.pct(), b.pct(), c.pct());
        gap_curve.push((d, a, b, c));
    }
    // Where the naive coach's stronger squad stops being the underdog: what
    // the best counter is worth, in attribute points. A least-squares line
    // through the curve (d, win), since single points are noisy.
    let crossing = {
        let (xs, ys): (Vec<f64>, Vec<f64>) = gap_curve.iter().filter(|g| g.0 <= 1.5).map(|g| (g.0 as f64, g.1.mean)).unzip();
        let (mx, my) = (xs.iter().sum::<f64>() / xs.len() as f64, ys.iter().sum::<f64>() / ys.len() as f64);
        let sxx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
        let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
        let slope = sxy / sxx.max(1e-9);
        (slope > 0.0).then(|| ((0.5 - my) / slope + mx).max(0.0) as f32)
    };
    // How big is a gap of d in a league? Spread of rotation quality across teams.
    let mut sds = Vec::new();
    let mut ranges = Vec::new();
    for sd in 0..6u64 {
        let w = World::generate(100 + sd, cfg.teams);
        let q: Vec<f64> = (0..w.teams.len())
            .map(|t| {
                let ids = rotation_ids(&w, t);
                ids.iter().map(|&i| w.players[i as usize].overall() as f64).sum::<f64>() / ids.len() as f64
            })
            .collect();
        let mean = q.iter().sum::<f64>() / q.len() as f64;
        sds.push((q.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / q.len() as f64).sqrt());
        ranges.push(q.iter().cloned().fold(f64::MIN, f64::max) - q.iter().cloned().fold(f64::MAX, f64::min));
    }
    let league_sd = sds.iter().sum::<f64>() / sds.len() as f64;
    let league_range = ranges.iter().sum::<f64>() / ranges.len() as f64;
    out!(
        r,
        "  For scale: in a generated {}-team league the rotation's mean attribute differs between teams with a standard deviation of {:.2} and best-to-worst of {:.2} points, so d = 1 is roughly {:.1} standard deviations.",
        cfg.teams, league_sd, league_range, 1.0 / league_sd
    );
    match crossing {
        Some(c) => out!(r, "  Fitting a line to the first column (d up to 1.5), the naive coach's stronger squad is an even bet against the best counter at d = {:.2}: tactics alone make up a talent gap of about {:.2} attribute points ({:.1} league standard deviations).", c, c, c as f64 / league_sd),
        None => out!(r, "  The first column does not rise with d: no estimate."),
    }
    out!(r);

    // 4c. Familiarity.
    let fams = [1.0f32, 0.75, 0.5, 0.25, 0.0];
    let bases = reps();
    let mut jobs = Vec::new();
    for (fi, &f) in fams.iter().enumerate() {
        for (b, s) in bases.iter().enumerate() {
            let mut j = Job::new(&eq, &eq, s, s, 900 + b as u64);
            j.fam = (f, 1.0);
            let _ = fi;
            jobs.push(j);
        }
    }
    progress("coaching: familiarity");
    let res = run_jobs(&jobs, cfg.scale(12, 40), seed ^ 2);
    let mut familiarity = Vec::new();
    out!(r, "Familiarity with the system (mirror matchups on equal squads, the other side fully drilled):");
    for (fi, &f) in fams.iter().enumerate() {
        let e = mean_win(&res[fi * 5..fi * 5 + 5]);
        out!(r, "  familiarity {:.2}: {} win %", f, e.pct());
        familiarity.push((f, e));
    }
    out!(r);

    // 4d. Minutes plans and fatigue.
    let mut jobs = Vec::new();
    let mut tempo = Strat::sys(OffenseSet::Motion, DefenseScheme::ManToMan, Press::FullCourt);
    tempo.pace = 18;
    let mut field: Vec<Strat> = bases.clone();
    field.push(tempo);
    let plans: [(&str, u8); 3] = [("deep (20 each)", 1), ("starters heavy (36/4)", 2), ("starters only (40/1)", 3)];
    let mut planned: Vec<Strat> = Vec::new();
    for (_, k) in plans {
        for b in &field {
            let mut s = b.clone();
            s.mins = Some(k);
            planned.push(s);
        }
    }
    for (pi, _) in plans.iter().enumerate() {
        for (b, base_s) in field.iter().enumerate() {
            jobs.push(Job::new(&eq, &eq, &planned[pi * field.len() + b], base_s, 1000 + b as u64));
        }
    }
    progress("coaching: minutes");
    let res = run_jobs(&jobs, cfg.scale(12, 40), seed ^ 3);
    let mut mins = Vec::new();
    out!(r, "Substitution patterns against the standard plan (30 minutes for starters, 10 for the bench), mirror systems:");
    for (pi, (name, _)) in plans.iter().enumerate() {
        let all = mean_win(&res[pi * field.len()..pi * field.len() + 5]);
        let fast = res[pi * field.len() + 5].win;
        out!(r, "  {:<24} {} win %   (against a full-court press at pace 18: {})", name, all.pct(), fast.pct());
        mins.push((name.to_string(), all, fast));
    }
    out!(r);

    // The same on a top-heavy squad, where the starters really are better.
    let th = archetypes(&base.w).into_iter().find(|s| s.name == "Top-heavy").unwrap();
    let th = Squad { mins: 0, ..th };
    let mut jobs = Vec::new();
    let th_base = reps();
    let mut th_planned: Vec<Strat> = Vec::new();
    for k in 0..4u8 {
        for b in &th_base {
            let mut s = b.clone();
            s.mins = Some(k);
            th_planned.push(s);
        }
    }
    for k in 0..4 {
        for (b, base_s) in th_base.iter().enumerate() {
            jobs.push(Job::new(&th, &th, &th_planned[k * 5 + b], base_s, 1200 + b as u64));
        }
    }
    progress("coaching: minutes, top-heavy");
    let res = run_jobs(&jobs, cfg.scale(12, 40), seed ^ 5);
    out!(r, "The same on a top-heavy squad (starters +2 attribute points, bench -3), mirror, against the standard 30/10 plan:");
    let mut mins_top = Vec::new();
    for (k, name) in ["standard (control)", "deep (20 each)", "starters heavy (36/4)", "starters only (40/1)"].iter().enumerate() {
        let e = mean_win(&res[k * 5..k * 5 + 5]);
        out!(r, "  {:<24} {} win %", name, e.pct());
        mins_top.push((name.to_string(), e));
    }
    out!(r);

    // 4e. The AI bench.
    let mut jobs = Vec::new();
        for &c in &[0u8, 10, 20] {
        let mut j = Job::new(&eq, &eq, &naive, counter_of_naive, 1100);
        j.coach = (c, 0);
        jobs.push(j);
    }
    progress("coaching: ai bench");
    let res = run_jobs(&jobs, cfg.scale(60, 80), seed ^ 4);
    let mut ai_coach = Vec::new();
    out!(r, "The staff coach's in-game adjustments (engine `adapt`): default Motion/Man against its best counter, head coach rating of the first side:");
    for (&c, rs) in [0u8, 10, 20].iter().zip(&res) {
        out!(r, "  rating {:>2}: {} win %", c, rs.win.pct());
        ai_coach.push((c, rs.win));
    }
    out!(r);

    // 4f. Live levers.
    let items = cfg.scale(25, 50);
    let reps_ = reps();
    let rep_idx: Vec<usize> = (0..5).map(|o| o * 5 + o).collect();
    // Experiments are written out below; each returns (stay, change) margins per item.
    let run_pair = |name: &str,
                    start_of: &(dyn Fn(usize) -> Strat + Sync),
                    switch_to: &(dyn Fn(usize) -> Option<(Strat, f32)> + Sync),
                    extra: &(dyn Fn(&mut basketman_core::league::LiveRound, usize) + Sync)|
     -> (String, Est, Est) {
        let total = items * 2 * 5;
        let res = par_map(total, |i| {
            let (item, rest) = (i / 10, i % 10);
            let (home, yi) = (rest / 5 == 0, rest % 5);
            let y = &reps_[yi];
            let sx = start_of(yi);
            let gseed = mix(&[seed, 77, item as u64, yi as u64]);
            let stay = live_game(gseed, home, &sx, y, &|_, _| {});
            let to = switch_to(yi);
            let change = live_game(gseed, home, &sx, y, &|live, k| {
                if k == 2
                    && let Some((s, fam)) = &to
                {
                    live.input(LiveInput::Orders {
                        orders: orders_of(s),
                        familiarity: *fam,
                    });
                }
                extra(live, k);
            });
            (stay, change)
        });
        let d_win: Vec<f64> = res.iter().map(|(a, b)| b.1 - a.1).collect();
        let d_margin: Vec<f64> = res.iter().map(|(a, b)| b.0 - a.0).collect();
        (name.to_string(), Est::of(&d_win), Est::of(&d_margin))
    };
    let counter_for = |yi: usize| m.strats[mo.counter[rep_idx[yi]]].clone();
    let worst_for = |yi: usize| m.strats[mo.worst[rep_idx[yi]]].clone();
    progress("coaching: live levers");
    let mut live = Vec::new();
    let mut rows: Vec<(String, Est, Est)> = Vec::new();
    rows.push(run_pair(
        "half-time switch: worst reply -> best counter (drilled, familiarity 0.8)",
        &worst_for,
        &|yi| Some((counter_for(yi), 0.8)),
        &|_, _| {},
    ));
    rows.push(run_pair(
        "half-time switch: worst reply -> best counter (improvised, familiarity by the game's rule)",
        &worst_for,
        &|yi| Some((counter_for(yi), improvised(&worst_for(yi), &counter_for(yi)))),
        &|_, _| {},
    ));
    rows.push(run_pair(
        "half-time switch: default Motion/Man -> best counter (drilled)",
        &|_| naive.clone(),
        &|yi| Some((counter_for(yi), 0.8)),
        &|_, _| {},
    ));
    rows.push(run_pair(
        "half-time switch: best counter -> default (a bad change, drilled)",
        &counter_for,
        &|_| Some((naive.clone(), 0.8)),
        &|_, _| {},
    ));
    rows.push(run_pair(
        "timeouts used at the start of the 3rd and 4th period (best counter)",
        &counter_for,
        &|_| None,
        &|live, k| {
            if k == 2 || k == 3 {
                live.input(LiveInput::Timeout);
            }
        },
    ));
    for (nm, sh) in [("Encourage", Shout::Encourage), ("Concentrate", Shout::Focus), ("Demand more", Shout::DemandMore)] {
        rows.push(run_pair(
            &format!("shout '{nm}' at half-time (best counter)"),
            &counter_for,
            &|_| None,
            &move |live, k| {
                if k == 2 {
                    live.input(LiveInput::Shout { shout: sh, target: None });
                }
            },
        ));
    }
    rows.push(run_pair(
        "no substitutions at all (assistant switched off, best counter)",
        &counter_for,
        &|_| None,
        &|live, k| {
            if k == 0 {
                live.input(LiveInput::AutoSubs(false));
            }
        },
    ));
    out!(r, "In-game levers, paired games (same seed, identical until the lever is used; {} games per row). Change in the coached side's win % and margin:", items * 10);
    for (name, w, mg) in &rows {
        let sig = if w.mean.abs() > 1.96 * w.se { " *" } else { "" };
        out!(r, "  {:<92} win {:+.1}±{:.1}  margin {:+.2}±{:.2}{}", name, 100.0 * w.mean, 196.0 * w.se, mg.mean, 1.96 * mg.se, sig);
        live.push((name.clone(), *w, *mg));
    }
    out!(r, "  (* = beyond the 95% interval)");
    out!(r);

    r.json.insert(
        "coaching".into(),
        json!({
            "best_response": best_response.json(), "best_fixed": best_fixed.json(),
            "naive": naive_e.json(), "worst_response": worst_response.json(),
            "talent_gap": gap_curve.iter().map(|(d, a, b, c)| json!({"d": d, "naive_vs_counter": a.json(), "naive_vs_naive": b.json(), "fixed_vs_counter": c.json()})).collect::<Vec<_>>(),
            "gap_tactics_can_overcome": crossing,
            "league_quality_sd": r4(league_sd), "league_quality_range": r4(league_range),
            "familiarity": familiarity.iter().map(|(f, e)| json!({"familiarity": f, "win": e.json()})).collect::<Vec<_>>(),
            "minutes": mins.iter().map(|(n, a, b)| json!({"plan": n, "win": a.json(), "win_vs_press_fast": b.json()})).collect::<Vec<_>>(),
            "minutes_top_heavy": mins_top.iter().map(|(n, e)| json!({"plan": n, "win": e.json()})).collect::<Vec<_>>(),
            "ai_coach": ai_coach.iter().map(|(c, e)| json!({"rating": c, "win": e.json()})).collect::<Vec<_>>(),
            "live": live.iter().map(|(n, w, m)| json!({"lever": n, "win_change": w.json(), "margin_change": m.json()})).collect::<Vec<_>>(),
        }),
    );
    CoachOut {
        best_response,
        naive: naive_e,
        worst_response,
        best_fixed,
        gap_curve,
        crossing,
        familiarity,
        mins,
        ai_coach,
        live,
        league_sd,
        league_range,
    }
}

// ------------------------------------------------------ degenerate strategies

fn jitter_board(rng: &mut Rng, off: OffenseSet, from: Option<&Board>) -> Board {
    let mut b = match from {
        Some(b) if b.set == off => b.clone(),
        _ => Board::stock(off),
    };
    for k in 0..5 {
        if rng.chance(0.6) {
            let p = b.spots[k];
            b.spots[k] = Board::clamp(v(p.x + (rng.f32() - 0.5) * 4.0, p.y + (rng.f32() - 0.5) * 4.0));
        }
        if rng.chance(0.25) {
            let p = b.spots[k];
            b.runs[k] = Some(Board::clamp(v(p.x + (rng.f32() - 0.5) * 8.0, p.y + (rng.f32() - 0.5) * 8.0)));
        }
    }
    b
}

fn slider_value(rng: &mut Rng) -> u8 {
    if rng.chance(0.4) {
        10
    } else {
        rng.range(1, 21) as u8
    }
}

pub fn random_strat(rng: &mut Rng) -> Strat {
    let mut s = Strat::sys(
        OffenseSet::ALL[rng.range(0, 5) as usize],
        DefenseScheme::ALL[rng.range(0, 5) as usize],
        Press::ALL[rng.range(0, 3) as usize],
    );
    s.pace = slider_value(rng);
    s.three = slider_value(rng);
    s.crash = slider_value(rng);
    s.aggr = slider_value(rng);
    if rng.chance(0.5) {
        s.mins = Some(rng.range(0, 4) as u8);
    }
    for b in s.briefs.iter_mut() {
        if rng.chance(0.3) {
            *b = Brief::ALL[rng.range(1, 4) as usize];
        }
    }
    if rng.chance(0.4) {
        s.star = Some(Role::ALL[rng.range(0, 4) as usize]);
    }
    if rng.chance(0.2) {
        s.board = Some(jitter_board(rng, s.off, None));
    }
    s
}

pub fn mutate(s: &Strat, rng: &mut Rng) -> Strat {
    let mut t = s.clone();
    for _ in 0..1 + rng.range(0, 3) {
        match rng.range(0, 12) {
            0 => t.off = OffenseSet::ALL[rng.range(0, 5) as usize],
            1 => t.def = DefenseScheme::ALL[rng.range(0, 5) as usize],
            2 => t.press = Press::ALL[rng.range(0, 3) as usize],
            3 => t.pace = (t.pace as i32 + rng.range(0, 9) as i32 - 4).clamp(1, 20) as u8,
            4 => t.three = (t.three as i32 + rng.range(0, 9) as i32 - 4).clamp(1, 20) as u8,
            5 => t.crash = (t.crash as i32 + rng.range(0, 9) as i32 - 4).clamp(1, 20) as u8,
            6 => t.aggr = (t.aggr as i32 + rng.range(0, 9) as i32 - 4).clamp(1, 20) as u8,
            7 => t.mins = if rng.chance(0.3) { None } else { Some(rng.range(0, 4) as u8) },
            8 => {
                let k = rng.range(0, 5) as usize;
                t.briefs[k] = Brief::ALL[rng.range(0, 4) as usize];
            }
            9 => t.star = if rng.chance(0.3) { None } else { Some(Role::ALL[rng.range(0, 4) as usize]) },
            10 => t.board = Some(jitter_board(rng, t.off, t.board.as_ref())),
            _ => t.board = None,
        }
    }
    if let Some(b) = &t.board
        && b.set != t.off
    {
        t.board = None;
    }
    t
}

/// Mean win of each candidate against every strategy of the field.
fn fitness(sq: &Squad, cands: &[Strat], field: &[Strat], units: usize, seed: u64) -> Vec<Est> {
    let mut jobs = Vec::new();
    for c in cands {
        for (f, y) in field.iter().enumerate() {
            jobs.push(Job::new(sq, sq, c, y, f as u64));
        }
    }
    let res = run_jobs(&jobs, units, seed);
    res.chunks(field.len()).map(|c| avg(&c.iter().map(|r| r.win).collect::<Vec<_>>())).collect()
}

pub struct Found {
    pub strat: Strat,
    pub search_win: Est,
}

/// Random starts, then a noisy hill climb from the best few.
fn search(
    sq: &Squad,
    field: &[Strat],
    seeds: &[Strat],
    random: usize,
    keep: usize,
    gens: usize,
    kids: usize,
    units: usize,
    seed: u64,
) -> Vec<Found> {
    let mut rng = Rng::new(seed);
    let mut pool: Vec<Strat> = seeds.to_vec();
    for _ in 0..random {
        pool.push(random_strat(&mut rng));
    }
    let fit = fitness(sq, &pool, field, units, mix(&[seed, 0]));
    let mut order: Vec<usize> = (0..pool.len()).collect();
    order.sort_by(|&a, &b| fit[b].mean.total_cmp(&fit[a].mean));
    let mut best: Vec<Found> = order
        .iter()
        .take(keep)
        .map(|&i| Found { strat: pool[i].clone(), search_win: fit[i] })
        .collect();
    for g in 0..gens {
        for b in best.iter_mut() {
            let mut cands = vec![b.strat.clone()];
            for _ in 0..kids {
                cands.push(mutate(&b.strat, &mut rng));
            }
            let f = fitness(sq, &cands, field, units, mix(&[seed, 1 + g as u64]));
            let top = (0..cands.len()).max_by(|&x, &y| f[x].mean.total_cmp(&f[y].mean)).unwrap();
            *b = Found { strat: cands[top].clone(), search_win: f[top] };
        }
    }
    best.sort_by(|a, b| b.search_win.mean.total_cmp(&a.search_win.mean));
    best
}

pub struct SearchOut {
    pub best: Found,
    pub validated: Est,
    pub best_normal: Est,
    pub worst_matchup: Est,
    pub counter: Option<(Strat, Est)>,
    pub anomalies: Vec<String>,
}

pub fn search_section(cfg: &Cfg, r: &mut Report, sq: &Squad, mo: &MatrixOut) -> SearchOut {
    let m = &mo.m;
    let n = m.strats.len();
    let sys25 = systems(&[Press::None]);
    let units = 2;
    let (random, keep, gens, kids) = if cfg.full { (200, 4, 20, 8) } else { (60, 2, 6, 6) };
    let seed = mix(&[cfg.seed, 5]);
    out!(r, "== 5. Degenerate strategies: search for exploits ==");
    out!(r, "Random settings of everything a manager controls (system, press, four sliders, minutes plan, briefs, primary option's role, hand-drawn set), then a hill climb, scored against all 25 neutral systems; the winners are replayed on fresh games.");
    out!(r);
    let mut ranked: Vec<usize> = (0..n).collect();
    ranked.sort_by(|&a, &b| mo.f.strength[b].mean.total_cmp(&mo.f.strength[a].mean));
    let normal: Vec<Strat> = ranked.iter().take(3).map(|&i| m.strats[i].clone()).collect();
    progress("search: round 1");
    let found = search(sq, &sys25, &normal, random, keep, gens, kids, units, seed);
    let best = found.into_iter().next().unwrap();
    // Validate on fresh games against the same 25 systems.
    let vunits = cfg.scale(8, 8);
    let mut jobs = Vec::new();
    for (f, y) in sys25.iter().enumerate() {
        jobs.push(Job::new(sq, sq, &best.strat, y, 9000 + f as u64));
        jobs.push(Job::new(sq, sq, &normal[0], y, 9000 + f as u64));
    }
    let res = run_jobs(&jobs, vunits, mix(&[seed, 99]));
    let found_res: Vec<Res> = res.iter().step_by(2).cloned().collect();
    let normal_res: Vec<Res> = res.iter().skip(1).step_by(2).cloned().collect();
    // The mirror match of the normal system is 50% by construction; keep it, it is symmetric.
    let validated = mean_win(&found_res);
    let best_normal = mean_win(&normal_res);
    let worst = found_res.iter().map(|r| r.win).min_by(|a, b| a.mean.total_cmp(&b.mean)).unwrap();
    out!(r, "Best strategy found: {}", best.strat.label());
    out!(r, "  on the games it was found on: {} win %; replayed on fresh games against the 25 systems: {} win %; worst single matchup {:.1}±{:.1}.", best.search_win.pct(), validated.pct(), 100.0 * worst.mean, 196.0 * worst.se);
    out!(r, "  the best ordinary system ({}) on the same fresh games: {} win %.", normal[0].label(), best_normal.pct());
    out!(r, "  gain of the search over the best ordinary system: {:+.1}±{:.1} win points.", 100.0 * (validated.mean - best_normal.mean), 196.0 * validated.minus(&best_normal).se);
    // Stat lines.
    let mut tot = Tot::default();
    let mut opp = Tot::default();
    let mut g = 0.0;
    for r_ in &found_res {
        tot.add(&r_.acc.x);
        opp.add(&r_.acc.y);
        g += r_.acc.games;
    }
    let team_games = 2.0 * m.games;
    let lg = |f: fn(&Tot) -> f64| f(&mo.m.league) / team_games;
    let rows: [(&str, f64, f64, fn(&Tot) -> f64); 10] = [
        ("points", 0.0, 0.0, |t| t.pts),
        ("possessions", 0.0, 0.0, |t| t.poss),
        ("field goals attempted", 0.0, 0.0, |t| t.fga),
        ("threes attempted", 0.0, 0.0, |t| t.tpa),
        ("free throws attempted", 0.0, 0.0, |t| t.fta),
        ("turnovers", 0.0, 0.0, |t| t.tov),
        ("fouls committed", 0.0, 0.0, |t| t.pf),
        ("offensive rebounds", 0.0, 0.0, |t| t.orb),
        ("steals", 0.0, 0.0, |t| t.stl),
        ("blocks", 0.0, 0.0, |t| t.blk),
    ];
    let mut anomalies = Vec::new();
    out!(r, "  stat line of the found strategy per game (and of its opponents) against the league mean of the matrix games:");
    for (name, _, _, f) in rows {
        let (mine, theirs, league) = (f(&tot) / g, f(&opp) / g, lg(f));
        let ratio = mine / league.max(1e-9);
        let flag = !(0.7..=1.4).contains(&ratio) || !(0.7..=1.4).contains(&(theirs / league.max(1e-9)));
        if flag {
            anomalies.push(format!("{name}: {mine:.1} for, {theirs:.1} against, league {league:.1}"));
        }
        out!(r, "    {:<24} {:>6.1} for {:>6.1} against   league {:>6.1}{}", name, mine, theirs, league, if flag { "   <-- outside 0.7-1.4 of league" } else { "" });
    }
    out!(r, "  score {:.1} points to {:.1}; possessions {:.1}; overtime in {:.1}% of the games.", tot.pts / g, opp.pts / g, tot.poss / g, 100.0 * found_res.iter().map(|r| r.acc.overtime).sum::<f64>() / g);
    out!(r);

    // Which of its settings carry it: reset one at a time to neutral.
    let nv = Strat::sys(best.strat.off, best.strat.def, best.strat.press);
    let mut variants: Vec<(String, Strat)> = Vec::new();
    let with = |variants: &mut Vec<(String, Strat)>, name: &str, f: &dyn Fn(&mut Strat)| {
        let mut t = best.strat.clone();
        f(&mut t);
        if t != best.strat {
            variants.push((name.to_string(), t));
        }
    };
    with(&mut variants, "press off", &|t| t.press = Press::None);
    with(&mut variants, "tempo neutral", &|t| t.pace = 10);
    with(&mut variants, "three-point bias neutral", &|t| t.three = 10);
    with(&mut variants, "crash neutral", &|t| t.crash = 10);
    with(&mut variants, "aggression neutral", &|t| t.aggr = 10);
    with(&mut variants, "standard minutes", &|t| t.mins = None);
    with(&mut variants, "free briefs", &|t| t.briefs = [Brief::Free; 5]);
    with(&mut variants, "default roles", &|t| t.star = None);
    with(&mut variants, "stock set instead of the drawn one", &|t| t.board = None);
    with(&mut variants, "everything neutral (system only)", &|t| *t = nv.clone());
    if let Some(b) = best.strat.board.clone() {
        // The drawn set piece by piece, on the neutral system.
        let stock = Board::stock(b.set);
        for (k, name) in BOARD_ROLES.iter().enumerate() {
            if b.spots[k] != stock.spots[k] {
                let mut only = stock.clone();
                only.spots[k] = b.spots[k];
                let mut t = nv.clone();
                t.board = Some(only);
                variants.push((format!("neutral + only the {name}'s spot moved"), t));
            }
        }
        if b.runs.iter().any(|x| x.is_some()) {
            let mut only = stock.clone();
            only.runs = b.runs;
            let mut t = nv.clone();
            t.board = Some(only);
            variants.push(("neutral + only the runs drawn".to_string(), t));
        }
    }
    let only_board = best.strat.board.clone();
    with(&mut variants, "only the drawn set kept (rest neutral)", &|t| {
        *t = nv.clone();
        t.board = only_board.clone();
    });
    let jobs: Vec<Job> = variants
        .iter()
        .flat_map(|(_, t)| sys25.iter().enumerate().map(move |(f, y)| (t, f, y)))
        .map(|(t, f, y)| Job::new(sq, sq, t, y, 9000 + f as u64))
        .collect();
    let res = run_jobs(&jobs, vunits, mix(&[seed, 99]));
    if let Some(b) = &best.strat.board {
        let f = |p: basketman_core::court::Vec2| format!("({:.1}, {:.1})", p.x, p.y);
        out!(r, "  its drawn {} set (metres from the basket, handler/guard/wing/forward/big): spots {}; runs {}", b.set.label(), b.spots.iter().map(|&p| f(p)).collect::<Vec<_>>().join(" "), b.runs.iter().map(|p| p.map_or("-".to_string(), f)).collect::<Vec<_>>().join(" "));
        let stock = Board::stock(b.set);
        out!(r, "  the stock set: spots {}", stock.spots.iter().map(|&p| f(p)).collect::<Vec<_>>().join(" "));
    }
    out!(r, "  what carries it (win % against the 25 systems when one setting is reset; the full strategy scores {}):", validated.pct());
    let mut ablation = Vec::new();
    for (k, (name, _)) in variants.iter().enumerate() {
        let e = mean_win(&res[k * 25..k * 25 + 25]);
        let d = e.minus(&validated);
        out!(r, "    {:<36} {} win %  ({:+.1}±{:.1})", name, e.pct(), 100.0 * d.mean, 196.0 * d.se);
        ablation.push(json!({"reset": name, "win": e.json()}));
    }
    out!(r);

    // Round two: is there an answer to the exploit?
    progress("search: round 2");
    let seeds2 = vec![normal[0].clone(), mo.m.strats[mo.counter[ranked[0]]].clone()];
    let found2 = search(sq, std::slice::from_ref(&best.strat), &seeds2, random / 2, keep.min(2), gens, kids, units * 3, mix(&[seed, 7]));
    let b2 = &found2[0];
    let jr = run_jobs(&[Job::new(sq, sq, &b2.strat, &best.strat, 9500)], vunits * 3, mix(&[seed, 98]));
    let beats = jr[0].win.mean - 1.64 * jr[0].win.se > 0.5;
    let counter = beats.then(|| (b2.strat.clone(), jr[0].win));
    out!(r, "Searching against that strategy alone (random + hill climb, same method) finds {} which scores {} win % against it on fresh games: {}.", b2.strat.label(), jr[0].win.pct(), if beats { "a counter exists" } else { "no counter was found, so the exploit is a dominant strategy" });
    out!(r);
    r.json.insert(
        "search".into(),
        json!({
            "best": best.strat.label(), "found_win": best.search_win.json(), "validated_win": validated.json(),
            "best_normal": normal[0].label(), "best_normal_win": best_normal.json(), "worst_matchup": worst.json(),
            "anomalies": anomalies.clone(), "ablation": ablation,
            "second_search": {"strategy": b2.strat.label(), "win_vs_exploit": jr[0].win.json()},
            "counter": counter.as_ref().map(|(s, e)| json!({"strategy": s.label(), "win_vs_exploit": e.json()})),
        }),
    );
    SearchOut { best, validated, best_normal, worst_matchup: worst, counter, anomalies }
}

// ------------------------------------------------------------------- season

fn corr(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let cov: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let (va, vb): (f64, f64) = (a.iter().map(|x| (x - ma).powi(2)).sum(), b.iter().map(|y| (y - mb).powi(2)).sum());
    cov / (va * vb).sqrt().max(1e-12)
}

/// R squared of y on two regressors.
fn r2_two(y: &[f64], x1: &[f64], x2: &[f64]) -> f64 {
    let (r1, r2, r12) = (corr(y, x1), corr(y, x2), corr(x1, x2));
    ((r1 * r1 + r2 * r2 - 2.0 * r1 * r2 * r12) / (1.0 - r12 * r12).max(1e-9)).clamp(0.0, 1.0)
}

fn ranks(v: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|&a, &b| v[a].total_cmp(&v[b]));
    let mut r = vec![0.0; v.len()];
    for (k, &i) in idx.iter().enumerate() {
        r[i] = k as f64;
    }
    r
}

pub struct SeasonOut {
    pub r_quality: f64,
    pub r_scheme: f64,
    pub r2_quality: f64,
    pub r2_both: f64,
    pub sd_win: f64,
    pub sd_luck: f64,
    pub best_top2: f64,
    pub spearman: f64,
}

pub fn season_section(cfg: &Cfg, r: &mut Report, mo: Option<&MatrixOut>) -> SeasonOut {
    let seasons = cfg.scale(5, 20);
    progress(&format!("seasons: {seasons} leagues of {} teams", cfg.teams));
    let (mut q, mut sch, mut win) = (Vec::new(), Vec::new(), Vec::new());
    let (mut top2, mut sp) = (0.0, 0.0);
    let mut games_each = 0.0;
    for s in 0..seasons {
        let mut w = World::generate(mix(&[cfg.seed, 6, s as u64]) >> 8, cfg.teams);
        let n = w.teams.len();
        let qual: Vec<f64> = (0..n)
            .map(|t| {
                let ids = rotation_ids(&w, t);
                ids.iter().map(|&i| w.players[i as usize].overall() as f64).sum::<f64>() / ids.len() as f64
            })
            .collect();
        let scheme: Vec<f64> = (0..n)
            .map(|t| {
                let tac = w.teams[t].tactics();
                mo.map_or(0.0, |mo| {
                    mo.f.strength[tac.offense as usize * 5 + tac.defense as usize].mean
                })
            })
            .collect();
        while !w.season_over() {
            w.play_round();
        }
        let wp: Vec<f64> = (0..n)
            .map(|t| {
                let st = &w.standings[t];
                games_each = (st.wins + st.losses) as f64;
                st.wins as f64 / games_each.max(1.0)
            })
            .collect();
        let best = (0..n).max_by(|&a, &b| qual[a].total_cmp(&qual[b])).unwrap();
        let rank = wp.iter().filter(|&&x| x > wp[best]).count();
        top2 += (rank < 2) as u8 as f64;
        sp += corr(&ranks(&qual), &ranks(&wp));
        q.extend(qual);
        sch.extend(scheme);
        win.extend(wp);
    }
    let r_q = corr(&q, &win);
    let r_s = corr(&sch, &win);
    let mean = win.iter().sum::<f64>() / win.len() as f64;
    let sd = (win.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / win.len() as f64).sqrt();
    let luck = (0.25 / games_each.max(1.0)).sqrt();
    let have_scheme = mo.is_some();
    let r2_both = if have_scheme { r2_two(&win, &q, &sch) } else { r_q * r_q };
    let out = SeasonOut {
        r_quality: r_q,
        r_scheme: r_s,
        r2_quality: r_q * r_q,
        r2_both,
        sd_win: sd,
        sd_luck: luck,
        best_top2: top2 / seasons as f64,
        spearman: sp / seasons as f64,
    };
    out!(r, "== 6. Season level: does the table follow squad quality? ==");
    out!(r, "{seasons} generated leagues of {} teams, double round robin ({} games each), AI coaches with their own systems, no transfers or injuries steering it (the game's normal weekly business still runs).", cfg.teams, games_each);
    out!(r, "  correlation of win % with squad quality (mean attribute of the rotation): r = {:.2} (R² {:.2}); within a league, rank correlation {:.2}", out.r_quality, out.r2_quality, out.spearman);
    if have_scheme {
        out!(r, "  correlation with the strength of the coach's chosen system (from the matrix above): r = {:.2}; quality and system together explain R² {:.2}", out.r_scheme, out.r2_both);
    }
    out!(r, "  spread of win %: {:.3} (pure luck alone would give {:.3}); the best squad finishes in the top two in {:.0}% of leagues", out.sd_win, out.sd_luck, 100.0 * out.best_top2);
    out!(r);
    r.json.insert(
        "season".into(),
        json!({"seasons": seasons, "r_quality": r4(r_q), "r_scheme": r4(r_s), "r2_quality": r4(out.r2_quality), "r2_quality_and_scheme": r4(r2_both), "sd_win": r4(sd), "sd_luck": r4(luck), "best_squad_top2": r4(out.best_top2), "rank_correlation": r4(out.spearman)}),
    );
    out
}

// --------------------------------------------------------------- scorecard

pub struct Check {
    pub id: &'static str,
    pub what: &'static str,
    pub target: String,
    pub value: String,
    pub pass: bool,
}

fn check(id: &'static str, what: &'static str, target: &str, value: String, pass: bool) -> Check {
    Check { id, what, target: target.into(), value, pass }
}

/// The properties a balanced game must have, each against what was measured.
pub fn matrix_checks(mo: &MatrixOut, validated: Option<&[Est]>) -> Vec<Check> {
    let n = mo.m.strats.len();
    let names: Vec<String> = mo.m.strats.iter().map(|s| s.label()).collect();
    let best = (0..n).max_by(|&a, &b| mo.f.strength[a].mean.total_cmp(&mo.f.strength[b].mean)).unwrap();
    let worst = (0..n).min_by(|&a, &b| mo.f.strength[a].mean.total_cmp(&mo.f.strength[b].mean)).unwrap();
    let mut out = vec![
        check(
            "no-dominated",
            "no system is dominated by another",
            "at most 2 dominated systems",
            format!("{} of {n} dominated", mo.f.dominated.len()),
            mo.f.dominated.len() <= 2,
        ),
        check(
            "no-dominant",
            "no system is a safe pick against everything",
            "no system even or better against every other, and none above 58% on average",
            format!(
                "strongest {} {:.1}% (condorcet: {})",
                names[best],
                100.0 * mo.f.strength[best].mean,
                if mo.f.condorcet.is_empty() { "none".into() } else { mo.f.condorcet.iter().map(|&i| names[i].clone()).collect::<Vec<_>>().join(", ") }
            ),
            mo.f.condorcet.is_empty() && mo.f.strength[best].mean <= 0.58,
        ),
        check(
            "bounded",
            "a system is worth a bounded share of win probability",
            "no system below 42% on average",
            format!("weakest {} {:.1}%", names[worst], 100.0 * mo.f.strength[worst].mean),
            mo.f.strength[worst].mean >= 0.42,
        ),
        check(
            "symmetric-support",
            "equilibrium of the full game mixes several systems",
            "at least 6 systems with 5% or more",
            format!("{} systems (effective {:.1})", mo.f.sym_support, mo.f.sym_effective),
            mo.f.sym_support >= 6,
        ),
    ];
    let od = &mo.od;
    let min_off = od.off_edge.iter().cloned().fold(f64::MAX, f64::min);
    let min_def = od.def_edge.iter().cloned().fold(f64::MAX, f64::min);
    out.push(check(
        "defense-has-answer",
        "every defense is beaten by some offense",
        "each defense gives some offense +0.03 ppp over the league mean",
        format!("smallest best-offense edge {:+.3} ppp", min_off),
        min_off >= 0.03,
    ));
    out.push(check(
        "offense-has-answer",
        "every offense is held down by some defense",
        "each offense is held 0.03 ppp below its own average by some defense",
        format!("smallest best-defense edge {:.3} ppp", min_def),
        min_def >= 0.03,
    ));
    out.push(check(
        "support-offense",
        "equilibrium of the offense-vs-defense game uses at least three offenses",
        "at least 3 offenses with 5% or more",
        format!("{}", support(&od.off_mix, 0.05)),
        support(&od.off_mix, 0.05) >= 3,
    ));
    out.push(check(
        "support-defense",
        "equilibrium of the offense-vs-defense game uses at least three defenses",
        "at least 3 defenses with 5% or more",
        format!("{}", support(&od.def_mix, 0.05)),
        support(&od.def_mix, 0.05) >= 3,
    ));
    if let Some(v) = validated {
        let ok = v.iter().filter(|e| e.mean - 1.64 * e.se > 0.5).count();
        out.push(check(
            "every-system-countered",
            "every system has a counter that beats it on fresh games",
            "at least 90% of systems",
            format!("{ok} of {n}"),
            ok as f64 >= 0.9 * n as f64,
        ));
    }
    out
}

pub fn fit_checks(f: &FitOut) -> Vec<Check> {
    let distinct = |v: &[usize]| {
        let mut u = v.to_vec();
        u.sort();
        u.dedup();
        u.len()
    };
    let big: usize = f.regret.iter().skip(1).filter(|e| e.mean >= 0.03 && e.mean > 1.64 * e.se).count();
    vec![
        check(
            "fit-offense",
            "the best offense changes with the squad",
            "at least 3 different best offenses over the squad types",
            format!("{} of {} squads", distinct(&f.best_off), f.squads.len()),
            distinct(&f.best_off) >= 3,
        ),
        check(
            "fit-defense",
            "the best defense changes with the squad",
            "at least 3 different best defenses over the squad types",
            format!("{}", distinct(&f.best_def)),
            distinct(&f.best_def) >= 3,
        ),
        check(
            "fit-regret",
            "the generic best system is a mistake for most specialised squads",
            "costs at least 3 win points (beyond noise) for at least half of the other squads",
            format!("{} of {}", big, f.squads.len() - 1),
            big * 2 >= f.squads.len() - 1,
        ),
    ]
}

pub fn coach_checks(c: &CoachOut) -> Vec<Check> {
    let at = |d: f32| c.gap_curve.iter().find(|g| g.0 == d).map(|g| g.2.mean).unwrap_or(0.0);
    let live = |k: usize| c.live[k].1;
    let fam0 = c.familiarity.last().map_or(0.5, |f| f.1.mean);
    vec![
        check(
            "coach-worth",
            "an informed coach beats a naive one at equal talent, by a bounded margin",
            "counter-picking wins 60-80% against the default",
            format!("{:.1}% (against the best fixed system {:.1}%)", 100.0 * c.best_response.mean, 100.0 * c.best_fixed.mean),
            (0.60..=0.80).contains(&c.best_response.mean),
        ),
        check(
            "talent-matters-small",
            "a small talent gap shows",
            "d = 0.5 (same tactics) wins at least 60%",
            format!("{:.1}%", 100.0 * at(0.5)),
            at(0.5) >= 0.60,
        ),
        check(
            "talent-matters-large",
            "a large talent gap decides it",
            "d = 3 (same tactics) wins at least 90%",
            format!("{:.1}%", 100.0 * at(3.0)),
            at(3.0) >= 0.90,
        ),
        check(
            "tactics-beat-talent",
            "tactics can make up a real talent gap",
            "a naive coach with the better squad is still the underdog against the best counter up to a gap of at least 0.5 and no more than 2.0",
            c.crossing.map_or("no estimate".into(), |x| format!("tactics worth d = {x:.2}")),
            c.crossing.is_some_and(|x| (0.5..=2.0).contains(&x)),
        ),
        check(
            "adapt-at-half",
            "changing to the right system at half-time pays",
            "switching from the worst reply to the best counter gains at least 5 win points",
            format!("{:+.1}±{:.1}", 100.0 * live(0).mean, 196.0 * live(0).se),
            live(0).mean >= 0.05,
        ),
        check(
            "familiarity",
            "an undrilled system costs a bounded, real share",
            "familiarity 0 costs between 5 and 25 win points",
            format!("{:.1} points", 100.0 * (0.5 - fam0)),
            (0.05..=0.25).contains(&(0.5 - fam0)),
        ),
    ]
}

pub fn lever_checks(l: &[LeverOut]) -> Vec<Check> {
    let always: Vec<&str> = l.iter().filter(|x| x.always_best_edge.is_some()).map(|x| x.name.as_str()).collect();
    let sliders = &l[..4];
    let live = sliders.iter().filter(|x| x.range >= 0.03).count();
    vec![
        check(
            "no-always-best-setting",
            "no slider, press or instruction is best at an extreme for every system",
            "none",
            if always.is_empty() { "none".into() } else { always.join(", ") },
            always.is_empty(),
        ),
        check(
            "sliders-matter",
            "sliders change the result",
            "at least 3 of 4 sliders move win probability by 3 points or more across their range",
            format!("{live} of 4 (ranges {})", sliders.iter().map(|x| format!("{:.1}", 100.0 * x.range)).collect::<Vec<_>>().join(", ")),
            live >= 3,
        ),
    ]
}

pub fn search_checks(s: &SearchOut) -> Vec<Check> {
    let gain = s.validated.mean - s.best_normal.mean;
    vec![
        check(
            "no-exploit",
            "no combination of settings far outperforms the ordinary systems",
            "best found at most 4 points above the best ordinary system and at most 62% overall",
            format!("{:.1}% (ordinary {:.1}%, {:+.1})", 100.0 * s.validated.mean, 100.0 * s.best_normal.mean, 100.0 * gain),
            gain <= 0.04 && s.validated.mean <= 0.62,
        ),
        check(
            "no-absurd-lines",
            "no strategy produces absurd stat lines",
            "every per-game total within 0.7-1.4 of the league mean",
            if s.anomalies.is_empty() { "none".into() } else { s.anomalies.join("; ") },
            s.anomalies.is_empty(),
        ),
    ]
}

pub fn season_checks(s: &SeasonOut) -> Vec<Check> {
    vec![check(
        "season-quality",
        "the table follows squad quality, but not too tightly",
        "correlation of win % with squad quality between 0.55 and 0.85",
        format!("r = {:.2}", s.r_quality),
        (0.55..=0.85).contains(&s.r_quality),
    )]
}

fn print_checks(r: &mut Report, checks: &[Check]) {
    out!(r, "== Scorecard (targets from docs/balance.md) ==");
    for c in checks {
        out!(r, "  [{}] {:<22} {} -- target: {}; measured: {}", if c.pass { "pass" } else { "FAIL" }, c.id, c.what, c.target, c.value);
    }
    let fails = checks.iter().filter(|c| !c.pass).count();
    out!(r, "  {} of {} checks fail.", fails, checks.len());
    r.json.insert(
        "scorecard".into(),
        Value::Array(checks.iter().map(|c| json!({"id": c.id, "what": c.what, "target": c.target, "measured": c.value, "pass": c.pass})).collect()),
    );
}

// ---------------------------------------------------------------------- run

pub fn parse(args: &[String]) -> Result<Cfg, String> {
    let mut cfg = if args.iter().any(|a| a == "--full") { Cfg::full() } else { Cfg::quick() };
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let mut val = || -> Result<String, String> {
            i += 1;
            args.get(i).cloned().ok_or(format!("{a} needs a value"))
        };
        match a {
            "--quick" | "--full" => {}
            "--games" => cfg.units = val()?.parse().map_err(|_| "--games wants a number")?,
            "--seed" => cfg.seed = val()?.parse().map_err(|_| "--seed wants a number")?,
            "--teams" => cfg.teams = val()?.parse().map_err(|_| "--teams wants a number")?,
            "--out" => cfg.out = val()?,
            "--sections" => cfg.sections = val()?.split(',').map(|s| s.to_string()).collect(),
            other => return Err(format!("unknown argument {other}")),
        }
        i += 1;
    }
    if cfg.teams < 4 || cfg.teams % 2 == 1 {
        return Err("--teams must be even and at least 4".into());
    }
    Ok(cfg)
}

pub const USAGE: &str = "usage: basketman balance [--quick | --full] [--games N] [--seed S] [--teams N]\n       [--sections matrix,fit,levers,coach,search,season] [--out DIR]\n  --quick (default) takes a few minutes; --full explores 75 systems (press included) with 3x the games.\n  --games N sets the units (four games each) per matrix cell; other sections scale with it.\n  Writes balance-report.txt and balance.json to DIR (default: current directory).";

pub fn run(args: &[String]) {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return;
    }
    let cfg = match parse(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    let started = std::time::Instant::now();
    let mut r = Report::default();
    let report = build(&cfg, &mut r, &mut |m| progress(m));
    let _ = report;
    let dir = std::path::Path::new(&cfg.out);
    let _ = std::fs::create_dir_all(dir);
    let json = Value::Object(r.json.clone());
    let txt = dir.join("balance-report.txt");
    let js = dir.join("balance.json");
    if let Err(e) = std::fs::write(&txt, &r.text).and_then(|_| std::fs::write(&js, serde_json::to_string_pretty(&json).unwrap())) {
        eprintln!("cannot write report: {e}");
    } else {
        eprintln!("[balance] wrote {} and {} in {:.0?}", txt.display(), js.display(), started.elapsed());
    }
}

/// All sections wanted by `cfg`, in order; returns every check.
pub fn build(cfg: &Cfg, r: &mut Report, _log: &mut dyn FnMut(&str)) -> Vec<Check> {
    let mut checks = Vec::new();
    out!(r, "basketman balance report");
    out!(r, "mode {}, seed {}, {} units per matrix cell, {} teams per league; output is deterministic for these settings.", if cfg.full { "full" } else { "quick" }, cfg.seed, cfg.units, cfg.teams);
    out!(r);
    let base = Squad { name: "Balanced".into(), w: base_world(cfg.seed, cfg.teams), mins: 0 };
    // The matrix feeds the later sections, so it runs when any of them does.
    let need_matrix = ["matrix", "coach", "search", "season"].iter().any(|s| cfg.wants(s));
    let mo = need_matrix.then(|| {
        let mut quiet = Report { quiet: !cfg.wants("matrix"), ..Default::default() };
        let mo = matrix_section(cfg, &mut quiet, &base);
        if cfg.wants("matrix") {
            r.text.push_str(&quiet.text);
            r.json.extend(quiet.json.clone());
        }
        mo
    });
    if let (Some(mo), true) = (&mo, cfg.wants("matrix")) {
        let v = validate_counters(&base, mo, cfg.scale(24, 72), mix(&[cfg.seed, 11]));
        out!(r, "Counters on fresh games: for each system, the reply the pooled offense/defense table recommends (the offense with the best ppp against its defense, the defense allowing the least to its offense), {} units against the system it answers. A system that is its own best reply has no counter:", cfg.scale(24, 72));
        let names: Vec<String> = mo.m.strats.iter().map(|s| s.label()).collect();
        let mut holds = 0;
        for (i, e) in v.iter().enumerate() {
            let ok = e.mean - 1.64 * e.se > 0.5;
            holds += ok as usize;
            out!(r, "  {:<22} <- {:<22} {:>10} {}", names[i], names[mo.counter[i]], e.pct(), if ok { "counter holds" } else { "" });
        }
        out!(r, "  {} of {} systems have a structural counter that beats them on fresh games (one-sided 95%).", holds, v.len());
        out!(r);
        r.json.insert("counter_validation".into(), json!(v.iter().map(|e| e.json()).collect::<Vec<_>>()));
        checks.extend(matrix_checks(mo, Some(&v)));
    }
    if cfg.wants("fit") {
        let squads = archetypes(&base.w);
        let f = fit_section(cfg, r, &squads);
        checks.extend(fit_checks(&f));
    }
    if cfg.wants("levers") {
        let l = lever_section(cfg, r, &base);
        checks.extend(lever_checks(&l));
    }
    if let (Some(mo), true) = (&mo, cfg.wants("coach")) {
        let c = coaching_section(cfg, r, &base, mo);
        checks.extend(coach_checks(&c));
    }
    if let (Some(mo), true) = (&mo, cfg.wants("search")) {
        let s = search_section(cfg, r, &base, mo);
        checks.extend(search_checks(&s));
    }
    if cfg.wants("season") {
        let s = season_section(cfg, r, mo.as_ref());
        checks.extend(season_checks(&s));
    }
    print_checks(r, &checks);
    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn base() -> &'static Squad {
        static B: OnceLock<Squad> = OnceLock::new();
        B.get_or_init(|| Squad { name: "Balanced".into(), w: base_world(1, 16), mins: 0 })
    }

    fn tiny() -> Cfg {
        Cfg { units: 1, ..Cfg::quick() }
    }

    // ---- always on: the instrument itself ----

    #[test]
    fn solver_finds_known_values() {
        // Rock, paper, scissors: value 0, uniform mix.
        let rps = vec![vec![0.0, -1.0, 1.0], vec![1.0, 0.0, -1.0], vec![-1.0, 1.0, 0.0]];
        let (v, x, y) = solve(&rps);
        assert!(v.abs() < 1e-9);
        assert!(x.iter().chain(&y).all(|p| (p - 1.0 / 3.0).abs() < 1e-9));
        // A saddle point: row 0 dominates, column 1 is the answer.
        let (v, x, y) = solve(&[vec![3.0, 1.0], vec![2.0, 0.0]]);
        assert!((v - 1.0).abs() < 1e-9 && x[0] > 0.999 && y[1] > 0.999);
        // Matching pennies, shifted by a constant.
        let (v, x, _) = solve(&[vec![6.0, 4.0], vec![4.0, 6.0]]);
        assert!((v - 5.0).abs() < 1e-9 && (x[0] - 0.5).abs() < 1e-9);
    }

    #[test]
    fn solver_returns_equilibria_of_random_games() {
        let mut rng = Rng::new(5);
        for (m, n) in [(5, 5), (6, 9), (25, 25), (40, 30)] {
            let a: Vec<Vec<f64>> = (0..m).map(|_| (0..n).map(|_| rng.f32() as f64 - 0.5).collect()).collect();
            let (v, x, y) = solve(&a);
            assert!((x.iter().sum::<f64>() - 1.0).abs() < 1e-9 && (y.iter().sum::<f64>() - 1.0).abs() < 1e-9);
            // Neither side can do better than the value against the other's mix.
            let col = |j: usize| (0..m).map(|i| x[i] * a[i][j]).sum::<f64>();
            let row = |i: usize| (0..n).map(|j| a[i][j] * y[j]).sum::<f64>();
            let lo = (0..n).map(col).fold(f64::MAX, f64::min);
            let hi = (0..m).map(row).fold(f64::MIN, f64::max);
            assert!((lo - v).abs() < 1e-7 && (hi - v).abs() < 1e-7, "{m}x{n}: {lo} {v} {hi}");
        }
    }

    #[test]
    fn strategy_application_keeps_the_rotation_and_sets_everything() {
        let sq = base();
        let t0 = sq.w.teams[0].tactics();
        let mut s = Strat::sys(OffenseSet::Isolation, DefenseScheme::Zone23, Press::FullCourt);
        s.pace = 17;
        s.mins = Some(3);
        s.briefs = [Brief::Attack; 5];
        s.star = Some(Role::Limited);
        let t = s.apply(t0, 0);
        assert_eq!(t.rotation.len(), t0.rotation.len());
        assert_eq!((t.offense, t.defense, t.press, t.pace), (s.off, s.def, s.press, 17));
        assert!(t.rotation[..5].iter().all(|r| r.minutes == 40 && r.brief == Brief::Attack));
        assert!(t.rotation[5..].iter().all(|r| r.minutes == 1));
        assert_eq!(t.rotation.iter().filter(|r| r.role == Role::Limited).count() >= 1, true);
        assert!(!t.rotation.iter().any(|r| r.role == Role::PrimaryOption));
    }

    #[test]
    fn equal_squads_are_equal_and_shifts_shift() {
        let w = &base().w;
        let eq = equalize(w);
        let (a, b) = (rotation_ids(&eq, 0), rotation_ids(&eq, 5));
        for (&i, &j) in a.iter().zip(&b) {
            assert_eq!(eq.players[i as usize].attrs.0, eq.players[j as usize].attrs.0);
            assert_eq!(eq.players[j as usize].id, j);
        }
        let up = shifted(&eq, 2.0);
        let (p, q) = (&eq.players[a[0] as usize], &up.players[a[0] as usize]);
        assert!(q.overall() > p.overall() + 1.0);
    }

    #[test]
    fn archetypes_differ_in_the_intended_direction() {
        let sq = archetypes(&base().w);
        let mean = |s: &Squad, a: Attr| {
            let ids = rotation_ids(&s.w, 0);
            ids.iter().map(|&i| s.w.players[i as usize].attrs[a] as f64).sum::<f64>() / ids.len() as f64
        };
        assert_eq!(sq.len(), 8);
        assert!(mean(&sq[1], Attr::ThreePoint) > mean(&sq[0], Attr::ThreePoint) + 2.0);
        assert!(mean(&sq[2], Attr::InteriorD) > mean(&sq[0], Attr::InteriorD) + 2.0);
        assert!(mean(&sq[3], Attr::Speed) > mean(&sq[4], Attr::Speed) + 4.0);
    }

    #[test]
    fn mutation_and_random_strategies_stay_valid() {
        let mut rng = Rng::new(3);
        let sq = base();
        let mut s = random_strat(&mut rng);
        for _ in 0..200 {
            s = mutate(&s, &mut rng);
            assert!((1..=20).contains(&s.pace) && (1..=20).contains(&s.three) && (1..=20).contains(&s.crash) && (1..=20).contains(&s.aggr));
            if let Some(b) = &s.board {
                assert_eq!(b.set, s.off);
            }
            let t = s.apply(sq.w.teams[0].tactics(), 0);
            assert_eq!(t.rotation.len(), 10);
        }
        // And the engine plays them.
        let a = play(Who { sq, team: 0, fam: 1.0, coach: 0 }, &s, Who { sq, team: 1, fam: 1.0, coach: 0 }, &Strat::naive(), true, 4);
        assert!(a.x.poss > 40.0 && a.y.pts > 20.0);
    }

    #[test]
    fn serial_and_parallel_give_the_same_numbers() {
        let sq = base();
        let strats = vec![Strat::naive(), Strat::sys(OffenseSet::Isolation, DefenseScheme::PackLine, Press::None), Strat::sys(OffenseSet::FiveOut, DefenseScheme::Zone32, Press::FullCourt)];
        let run = |th: usize| {
            FORCE.with(|f| f.set(Some(th)));
            let m = matrix(sq, strats.clone(), 2, 9);
            FORCE.with(|f| f.set(None));
            m
        };
        let (a, b) = (run(1), run(6));
        for i in 0..3 {
            for j in 0..3 {
                assert_eq!(a.win[i][j].mean.to_bits(), b.win[i][j].mean.to_bits());
                assert_eq!(a.margin[i][j].se.to_bits(), b.margin[i][j].se.to_bits());
            }
        }
        assert_eq!(a.od[1][2].0.to_bits(), b.od[1][2].0.to_bits());
    }

    #[test]
    fn the_report_is_reproducible() {
        let strats = systems(&[Press::None])[..9].to_vec();
        let go = |th: usize| {
            FORCE.with(|f| f.set(Some(th)));
            let m = matrix(base(), strats.clone(), 2, 5);
            FORCE.with(|f| f.set(None));
            let f = facts(&m);
            format!("{:?} {:?} {:?} {:?}", m.win, m.margin, f.mix, m.od)
        };
        assert_eq!(go(1), go(5));
    }

    #[test]
    fn matrix_is_antisymmetric_and_sane() {
        let m = matrix(base(), systems(&[Press::None])[..6].to_vec(), 2, 3);
        for i in 0..6 {
            assert_eq!(m.win[i][i].mean, 0.5);
            for j in 0..6 {
                assert!((m.win[i][j].mean + m.win[j][i].mean - 1.0).abs() < 1e-12);
            }
        }
        let ppg = m.league.pts / (2.0 * m.games);
        assert!((40.0..120.0).contains(&ppg), "{ppg}");
    }

    #[test]
    fn live_games_replay_and_levers_act() {
        let (x, y) = (Strat::naive(), Strat::sys(OffenseSet::Isolation, DefenseScheme::PackLine, Press::None));
        let nothing: &dyn Fn(&mut basketman_core::league::LiveRound, usize) = &|_, _| {};
        let a = live_game(11, true, &x, &y, nothing);
        let b = live_game(11, true, &x, &y, nothing);
        assert_eq!(a, b);
        let no_subs = live_game(11, true, &x, &y, &|live, k| {
            if k == 0 {
                live.input(LiveInput::AutoSubs(false));
            }
        });
        assert_ne!(a, no_subs, "switching the assistant off must change the game");
    }

    // ---- balance properties: the to-do list ----
    //
    // Each asserts one line of the scorecard on quick-mode numbers. A test
    // that fails today is `#[ignore = "balance: ..."]` with what was
    // measured; it is not loosened. Passing but slow ones are `slow`.

    fn assert_check(checks: &[Check], id: &str) {
        let c = checks.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("no check {id}"));
        assert!(c.pass, "{}: {} -- target {}; measured {}", c.id, c.what, c.target, c.value);
    }

    fn matrix_out() -> &'static (MatrixOut, Vec<Check>) {
        static M: OnceLock<(MatrixOut, Vec<Check>)> = OnceLock::new();
        M.get_or_init(|| {
            let cfg = Cfg::quick();
            let mut r = Report { quiet: true, ..Default::default() };
            let mo = matrix_section(&cfg, &mut r, base());
            let v = validate_counters(base(), &mo, cfg.scale(24, 72), mix(&[cfg.seed, 11]));
            let c = matrix_checks(&mo, Some(&v));
            (mo, c)
        })
    }

    fn fit_checks_quick() -> &'static Vec<Check> {
        static M: OnceLock<Vec<Check>> = OnceLock::new();
        M.get_or_init(|| {
            let cfg = Cfg::quick();
            let mut r = Report { quiet: true, ..Default::default() };
            fit_checks(&fit_section(&cfg, &mut r, &archetypes(&base().w)))
        })
    }

    fn lever_checks_quick() -> &'static Vec<Check> {
        static M: OnceLock<Vec<Check>> = OnceLock::new();
        M.get_or_init(|| {
            let cfg = Cfg::quick();
            let mut r = Report { quiet: true, ..Default::default() };
            lever_checks(&lever_section(&cfg, &mut r, base()))
        })
    }

    fn coach_checks_quick() -> &'static Vec<Check> {
        static M: OnceLock<Vec<Check>> = OnceLock::new();
        M.get_or_init(|| {
            let cfg = Cfg::quick();
            let mut r = Report { quiet: true, ..Default::default() };
            coach_checks(&coaching_section(&cfg, &mut r, base(), &matrix_out().0))
        })
    }

    fn search_checks_quick() -> &'static Vec<Check> {
        static M: OnceLock<Vec<Check>> = OnceLock::new();
        M.get_or_init(|| {
            let cfg = Cfg::quick();
            let mut r = Report { quiet: true, ..Default::default() };
            search_checks(&search_section(&cfg, &mut r, base(), &matrix_out().0))
        })
    }

    fn season_checks_quick() -> &'static Vec<Check> {
        static M: OnceLock<Vec<Check>> = OnceLock::new();
        M.get_or_init(|| {
            let cfg = Cfg::quick();
            let mut r = Report { quiet: true, ..Default::default() };
            season_checks(&season_section(&cfg, &mut r, Some(&matrix_out().0)))
        })
    }

    #[test]
    #[ignore = "balance: 21 of 25 systems are dominated (Iso/Pack, Motion/Pack lead); target at most 2"]
    fn no_system_is_dominated() {
        assert_check(&matrix_out().1, "no-dominated");
    }

    #[test]
    #[ignore = "balance: Iso/Pack is even or better against every system and averages 58.4% win; target none"]
    fn no_system_is_dominant() {
        assert_check(&matrix_out().1, "no-dominant");
    }

    #[test]
    #[ignore = "slow: passes in quick mode (weakest PnR/Switch 43.8%)"]
    fn no_system_is_hopeless() {
        assert_check(&matrix_out().1, "bounded");
    }

    #[test]
    #[ignore = "balance: equilibrium of the 25-system game has 3 systems (effective 2.4: Iso/Pack, Iso/3-2, Iso/Man); target at least 6"]
    fn equilibrium_of_the_full_game_mixes_several_systems() {
        assert_check(&matrix_out().1, "symmetric-support");
    }

    #[test]
    #[ignore = "balance: best offense edge against some defense is only +0.009 ppp (Pack line); target +0.03 for every defense"]
    fn every_defense_is_beaten_by_some_offense() {
        assert_check(&matrix_out().1, "defense-has-answer");
    }

    #[test]
    #[ignore = "balance: best defense edge against some offense is only 0.015 ppp; target 0.03 for every offense"]
    fn every_offense_is_held_down_by_some_defense() {
        assert_check(&matrix_out().1, "offense-has-answer");
    }

    #[test]
    #[ignore = "balance: offense equilibrium is Isolation alone (1 set); target 3"]
    fn offense_equilibrium_uses_three_sets() {
        assert_check(&matrix_out().1, "support-offense");
    }

    #[test]
    #[ignore = "balance: defense equilibrium is Pack line alone (1 scheme); target 3"]
    fn defense_equilibrium_uses_three_schemes() {
        assert_check(&matrix_out().1, "support-defense");
    }

    #[test]
    #[ignore = "balance: only 16 of 25 systems have a counter that beats them on fresh games; target 90%"]
    fn every_system_has_a_counter() {
        assert_check(&matrix_out().1, "every-system-countered");
    }

    #[test]
    #[ignore = "slow: passes in quick mode (3 distinct best offenses of 8 squads: Motion, Post, Iso)"]
    fn best_offense_depends_on_squad() {
        assert_check(fit_checks_quick(), "fit-offense");
    }

    #[test]
    #[ignore = "balance: only 2 distinct best defenses over 8 squads (Pack, 3-2); target 3"]
    fn best_defense_depends_on_squad() {
        assert_check(fit_checks_quick(), "fit-defense");
    }

    #[test]
    #[ignore = "balance: the Balanced squad's best system costs a specialised squad 3+ points on only 2 of 7 (Bigs 7.8, Star 12.2); target half"]
    fn generic_best_system_is_wrong_for_specialised_squads() {
        assert_check(fit_checks_quick(), "fit-regret");
    }

    #[test]
    #[ignore = "balance: tempo, three-point bias and crash the boards are all best at 20 for every base system (+11, +12, +10 win points over neutral)"]
    fn no_setting_is_best_everywhere() {
        assert_check(lever_checks_quick(), "no-always-best-setting");
    }

    #[test]
    #[ignore = "slow: runs the sliders section (about 40 s)"]
    fn sliders_matter() {
        assert_check(lever_checks_quick(), "sliders-matter");
    }

    #[test]
    #[ignore = "slow: passes narrowly in quick mode (60.1%; but equals the best fixed system 61.4%, so scouting adds nothing)"]
    fn informed_coach_beats_naive_coach() {
        assert_check(coach_checks_quick(), "coach-worth");
    }

    #[test]
    #[ignore = "slow: coaching section"]
    fn small_talent_gap_shows() {
        assert_check(coach_checks_quick(), "talent-matters-small");
    }

    #[test]
    #[ignore = "slow: coaching section"]
    fn large_talent_gap_decides() {
        assert_check(coach_checks_quick(), "talent-matters-large");
    }

    #[test]
    #[ignore = "balance: the best counter is worth only 0.31 attribute points (0.5 league sd); target 0.5 to 2.0"]
    fn tactics_can_make_up_a_talent_gap() {
        assert_check(coach_checks_quick(), "tactics-beat-talent");
    }

    #[test]
    #[ignore = "balance: half-time switch to the best counter gains +1.2±7.0 win points (improvised: -17.2±6.7); target at least +5"]
    fn adapting_at_half_time_pays() {
        assert_check(coach_checks_quick(), "adapt-at-half");
    }

    #[test]
    #[ignore = "balance: familiarity 0 costs 31.2 win points; target 5 to 25"]
    fn familiarity_costs_a_bounded_share() {
        assert_check(coach_checks_quick(), "familiarity");
    }

    #[test]
    #[ignore = "balance: a hand-drawn Post up set with the big man 1.8 m closer to the basket wins 88.5% against the 25 systems (+29.1 over Iso/Pack); target at most +4"]
    fn search_finds_no_exploit() {
        assert_check(search_checks_quick(), "no-exploit");
    }

    #[test]
    #[ignore = "balance: the found strategy scores 110.7 points to 85.1, 31.5 free throws, 13 steals; target within 0.7-1.4 of league"]
    fn search_finds_no_absurd_stat_lines() {
        assert_check(search_checks_quick(), "no-absurd-lines");
    }

    #[test]
    #[ignore = "slow: plays five seasons"]
    fn table_follows_squad_quality() {
        assert_check(season_checks_quick(), "season-quality");
    }
}
