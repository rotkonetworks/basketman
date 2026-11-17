//! `basketman realism [--games N] [--seasons N] [--out DIR] [--external FILE]`
//!
//! Lays the simulated league beside real basketball. The targets are in
//! `docs/realism-targets.json` (embedded here) with their sources in
//! `docs/realism.md`. Two kinds of runs feed the metrics:
//!
//! * league games, recorded, with each club's own tactics and coach on the
//!   bench (the way `lab league` plays them): shooting, shot zones, play
//!   sources, turnovers, fouls, timeouts, substitutions, game flow;
//! * full seasons of the three-division career world: tables, competitive
//!   balance, minutes, scoring leaders, the shape of the squads.
//!
//! Every metric is a ratio of sums over clusters (a game, or one division in
//! one season), so its confidence interval comes from the spread of the
//! clusters (delta method; Student t for few clusters). A difference from a
//! target that lies inside the interval is not a finding.
//!
//! Games are simulated on worker threads (`RAYON_NUM_THREADS` sets how many),
//! each from its own seed, and gathered in game order, so the report does not
//! depend on the number of threads.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use basketman_core::command::Command;
use basketman_core::court::{HOOP, KEY_DEPTH, KEY_HALF_WIDTH, ShotKind, v};
use basketman_core::engine::{BoxLine, GameResult, simulate_recorded_crewed};
use basketman_core::inbox::Stop;
use basketman_core::league::{Fixture, World, lineup};
use basketman_core::player::Position;
use basketman_core::officials::Ruling;
use basketman_core::replay::{EventKind, FoulKind, Recording, Source, TurnoverKind};
use basketman_core::rng::Rng;
use basketman_core::shotchart::{Shot, Zone, shots};
use basketman_core::tactics::{DefenseScheme, Press};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// The targets file and the registry of what the code can compute

pub const TARGETS_JSON: &str = include_str!("../../../docs/realism-targets.json");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub group: String,
    pub description: String,
    pub unit: String,
    /// None where no real figure was found and no design value is set.
    pub target: Option<f64>,
    pub low: Option<f64>,
    pub high: Option<f64>,
    /// "hard" or "soft".
    pub severity: String,
    /// "sourced", "derived" (own calculation or scaling) or "judgement".
    pub basis: String,
    pub source: String,
}

pub fn parse_targets(text: &str) -> Result<Vec<Target>, String> {
    #[derive(Deserialize)]
    struct File {
        metrics: Vec<Target>,
    }
    serde_json::from_str::<File>(text)
        .map(|f| f.metrics)
        .map_err(|e| e.to_string())
}

/// Every metric the instrument computes. A target's id must be in here or in
/// `UNMEASURABLE`.
pub const KNOWN: &[&str] = &[
    // pace and efficiency
    "pace", "ppg", "ortg", "fga_pg", "tpa_pg", "fta_pg", "ast_pg", "tov_pg", "stl_pg", "blk_pg",
    // shooting
    "fg_pct", "two_pct", "three_pct", "ft_pct", "three_rate", "ft_rate", "efg", "ts",
    // rebounds, assists, turnovers, steals, blocks
    "orb_pct", "drb_pct", "ast_per_fgm", "tov_per100", "tov_pct", "stl_per100", "blk_per100",
    // fouls
    "pf_pg", "foulout_pg", "andone_rate", "foul_shooting_share", "foul_offensive_share",
    "pf_home_minus_away", "fta_home_minus_away",
    // turnovers by kind
    "tov_stolen_share", "tov_badpass_share", "tov_offfoul_share", "tov_shotclock_share",
    // zones
    "z_ra_share", "z_paint_share", "z_mid_share", "z_c3_share", "z_atb3_share", "z_c3_of_3pa",
    "dist_0_3_share", "dist_3_10_share", "dist_10_16_share", "dist_16_3p_share", "dist_0_3_fg",
    "dist_3_10_fg", "dist_10_16_fg", "dist_16_3p_fg", "z_ra_fg", "z_paint_fg", "z_mid_fg", "z_c3_fg", "z_atb3_fg", "ast_share_two", "ast_share_three",
    // how shots come about
    "src_transition_share", "src_drive_share", "src_pullup_share", "src_catchshoot_share",
    "src_cut_share", "src_roll_share", "src_post_share", "src_putback_share", "src_forced_share",
    "src_transition_efg", "src_drive_efg", "src_pullup_efg", "src_catchshoot_efg", "src_cut_efg",
    "src_roll_efg", "src_post_efg", "src_putback_efg", "src_forced_efg",
    // possessions by kind
    "ptype_transition_share", "ptype_transition_ppp", "ptype_pnr_share", "ptype_pnr_ppp",
    "ptype_post_share", "ptype_post_ppp", "ptype_spotup_share", "ptype_spotup_ppp",
    "ptype_iso_share", "ptype_iso_ppp", "ptype_cut_share", "ptype_cut_ppp",
    "ptype_halfcourt_ppp", "putback_per_poss", "second_chance_pts_pg", "pts_off_tov_pg",
    "fastbreak_pts_pg", "zone_poss_share", "press_poss_share",
    // game level
    "home_win_pct", "margin_mean_abs", "margin_median_abs", "close5_share", "blowout20_share",
    "ot_rate", "halftime_leader_win_pct", "comeback10_rate", "comeback15_rate",
    // coaching
    "timeouts_pg", "subs_pg", "adjust_pg",
    // tables and balance (division-seasons)
    "winpct_sd", "winpct_talent_sd", "noll_scully", "ortg_sd", "drtg_sd", "strength_win_r", "payroll_win_r",
    "prev_win_r",
    // players
    "rotation_size", "starters_mpg", "bench_mpg", "top_mpg", "top_scorer_ppg", "top10_ppg_mean",
    "top_scorer_pts_share", "top_fga_share", "three_pct_p10", "three_pct_p90", "ft_pct_p10",
    "ft_pct_p90", "reb_leader_rpg", "ast_leader_apg", "height_guard_cm", "height_forward_cm",
    "height_center_cm", "weight_guard_kg", "weight_forward_kg", "weight_center_kg", "age_mean",
    "peak_age",
    // diagnostics without a real counterpart
    "diag_engine_poss", "diag_segment_poss", "diag_segment_points_ok", "diag_poss_seconds",
];

/// What cannot be computed today, and what the engine or recording would need.
pub const UNMEASURABLE: &[(&str, &str)] = &[
    (
        "challenge_use_pg",
        "No coach's challenge exists in the engine; a challenge command and a referee review of a call would be needed.",
    ),
    (
        "challenge_success",
        "As above; also needs a record of which calls were wrong (the referees work in progress).",
    ),
    (
        "crew_foul_sd",
        "There is no referee in the recording: a crew id per game and per foul (whistle) is needed to see the spread of fouls between crews.",
    ),
    (
        "call_accuracy",
        "The engine has no notion of a wrong call; the referee work must log each whistle (or non-whistle) with the contact it judged and a correct/incorrect flag, as the NBA Last Two Minute reports do.",
    ),
    (
        "ptype_handoff_share",
        "Hand-offs are not an action in the event log; an Event for a hand-off (giver, receiver) is needed.",
    ),
    (
        "ptype_offscreen_share",
        "Screens are logged for the ball handler only (Screen events); a screen away from the ball and the shooter who uses it is not recorded.",
    ),
    (
        "ptype_pnr_split",
        "Pick-and-roll possessions are found by their Screen event; the ball handler's own shot and the roll man's cannot be told apart except for the Roll source, so Synergy's ball-handler / roll-man split is not available.",
    ),
    (
        "shot_open_share",
        "Contest is a number 0..1 per shot, not the tracked closest-defender distance used for 'open' and 'wide open' shares; a defender distance at release is needed in the Shot event.",
    ),
    (
        "career_length",
        "Needs a world run for 15 or more seasons to see whole careers (retirement exists in retire.rs); not part of the default run.",
    ),
];

// ---------------------------------------------------------------------------
// Statistics: a metric is a ratio of sums over clusters

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Ratio,
    Median,
}

/// Metrics that are fractions in the code and percentages in the report and
/// in the targets file.
fn is_percent(id: &str) -> bool {
    id.contains("_pct")
        && !id.starts_with("winpct")
        || id.ends_with("_share")
        || id.ends_with("_rate")
        || id.ends_with("_efg")
        || id.ends_with("_fg")
        || id.starts_with("ast_share_")
        || id == "z_c3_of_3pa"
        || id == "efg"
        || id == "ts"
}

fn kind_of(id: &str) -> Kind {
    if id == "margin_median_abs" { Kind::Median } else { Kind::Ratio }
}

/// Two-sided 95% Student t quantile for `df` degrees of freedom.
fn t95(df: usize) -> f64 {
    const T: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179,
        2.160, 2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064,
        2.060, 2.056, 2.052, 2.048, 2.045, 2.042,
    ];
    match df {
        0 => f64::INFINITY,
        1..=30 => T[df - 1],
        31..=60 => 2.0,
        _ => 1.96,
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Estimate {
    pub value: f64,
    pub lo: f64,
    pub hi: f64,
    /// Clusters.
    pub n: usize,
}

fn estimate(kind: Kind, rows: &[(f64, f64)]) -> Option<Estimate> {
    let n = rows.len();
    if n == 0 {
        return None;
    }
    match kind {
        Kind::Median => {
            let mut x: Vec<f64> = rows.iter().map(|r| r.0).collect();
            x.sort_by(|a, b| a.total_cmp(b));
            let m = if n % 2 == 1 { x[n / 2] } else { 0.5 * (x[n / 2 - 1] + x[n / 2]) };
            // Order-statistic interval for the median.
            let d = 0.98 * (n as f64).sqrt();
            let lo = ((n as f64) / 2.0 - d).floor().max(0.0) as usize;
            let hi = (((n as f64) / 2.0 + d).ceil() as usize).min(n - 1);
            Some(Estimate { value: m, lo: x[lo], hi: x[hi], n })
        }
        Kind::Ratio => {
            let sn: f64 = rows.iter().map(|r| r.0).sum();
            let sd: f64 = rows.iter().map(|r| r.1).sum();
            if sd == 0.0 {
                return None;
            }
            let r = sn / sd;
            if n < 2 {
                return Some(Estimate { value: r, lo: f64::NEG_INFINITY, hi: f64::INFINITY, n });
            }
            let dbar = sd / n as f64;
            let ss: f64 = rows.iter().map(|x| (x.0 - r * x.1).powi(2)).sum();
            let var = ss / (n as f64 - 1.0) / n as f64 / (dbar * dbar);
            let h = t95(n - 1) * var.sqrt();
            Some(Estimate { value: r, lo: r - h, hi: r + h, n })
        }
    }
}

/// Rows contributed by one cluster: (metric id, numerator, denominator).
type Rows = Vec<(&'static str, f64, f64)>;

#[derive(Default)]
struct Pool {
    rows: BTreeMap<&'static str, Vec<(f64, f64)>>,
}

impl Pool {
    fn add(&mut self, rows: Rows) {
        for (id, n, d) in rows {
            self.rows.entry(id).or_default().push((n, d));
        }
    }

    fn estimates(&self) -> BTreeMap<&'static str, Estimate> {
        self.rows
            .iter()
            .filter_map(|(id, r)| {
                estimate(kind_of(id), r).map(|mut e| {
                    if is_percent(id) {
                        e.value *= 100.0;
                        e.lo *= 100.0;
                        e.hi *= 100.0;
                    }
                    (*id, e)
                })
            })
            .collect()
    }
}

fn sd(x: &[f64]) -> f64 {
    let n = x.len() as f64;
    if x.len() < 2 {
        return 0.0;
    }
    let m = x.iter().sum::<f64>() / n;
    (x.iter().map(|a| (a - m).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
}

pub(crate) fn pearson(a: &[f64], b: &[f64]) -> Option<f64> {
    let n = a.len().min(b.len());
    if n < 3 {
        return None;
    }
    let ma = a[..n].iter().sum::<f64>() / n as f64;
    let mb = b[..n].iter().sum::<f64>() / n as f64;
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for i in 0..n {
        sab += (a[i] - ma) * (b[i] - mb);
        saa += (a[i] - ma).powi(2);
        sbb += (b[i] - mb).powi(2);
    }
    (saa > 0.0 && sbb > 0.0).then(|| sab / (saa * sbb).sqrt())
}

fn percentile(x: &mut [f64], q: f64) -> Option<f64> {
    if x.is_empty() {
        return None;
    }
    x.sort_by(|a, b| a.total_cmp(b));
    let pos = q * (x.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    Some(x[lo] + (x[hi] - x[lo]) * (pos - lo as f64))
}

// ---------------------------------------------------------------------------
// Box-score metrics: from a `GameResult` alone (league games, seasons, files
// of externally played games)

fn totals(r: &GameResult, side: usize) -> BoxLine {
    let mut t = BoxLine::default();
    for (_, l) in &r.boxes[side] {
        t.add(l);
    }
    t
}

fn poss_est(t: &BoxLine) -> f64 {
    t.fga as f64 + 0.44 * t.fta as f64 - t.orb as f64 + t.tov as f64
}

/// Foul limit of the rules the games were played under (FIBA: five).
const FOUL_LIMIT: u16 = 5;

fn box_rows(r: &GameResult) -> Rows {
    let (h, a) = (totals(r, 0), totals(r, 1));
    let both = |f: &dyn Fn(&BoxLine) -> f64| f(&h) + f(&a);
    let mut out: Rows = Vec::new();
    let ph = poss_est(&h);
    let pa = poss_est(&a);
    let p = ph + pa;
    let f = |x: u16| x as f64;
    out.push(("pace", p, 2.0));
    out.push(("ppg", both(&|t| f(t.pts)), 2.0));
    out.push(("ortg", 100.0 * both(&|t| f(t.pts)), p));
    out.push(("fga_pg", both(&|t| f(t.fga)), 2.0));
    out.push(("tpa_pg", both(&|t| f(t.tpa)), 2.0));
    out.push(("fta_pg", both(&|t| f(t.fta)), 2.0));
    out.push(("ast_pg", both(&|t| f(t.ast)), 2.0));
    out.push(("tov_pg", both(&|t| f(t.tov)), 2.0));
    out.push(("stl_pg", both(&|t| f(t.stl)), 2.0));
    out.push(("blk_pg", both(&|t| f(t.blk)), 2.0));
    out.push(("fg_pct", both(&|t| f(t.fgm)), both(&|t| f(t.fga))));
    out.push((
        "two_pct",
        both(&|t| f(t.fgm) - f(t.tpm)),
        both(&|t| f(t.fga) - f(t.tpa)),
    ));
    out.push(("three_pct", both(&|t| f(t.tpm)), both(&|t| f(t.tpa))));
    out.push(("ft_pct", both(&|t| f(t.ftm)), both(&|t| f(t.fta))));
    out.push(("three_rate", both(&|t| f(t.tpa)), both(&|t| f(t.fga))));
    out.push(("ft_rate", both(&|t| f(t.fta)), both(&|t| f(t.fga))));
    out.push((
        "efg",
        both(&|t| f(t.fgm) + 0.5 * f(t.tpm)),
        both(&|t| f(t.fga)),
    ));
    out.push((
        "ts",
        both(&|t| f(t.pts)),
        both(&|t| 2.0 * (f(t.fga) + 0.44 * f(t.fta))),
    ));
    let orb = both(&|t| f(t.orb));
    let drb = both(&|t| f(t.drb));
    out.push(("orb_pct", orb, orb + drb));
    out.push(("drb_pct", drb, orb + drb));
    out.push(("ast_per_fgm", both(&|t| f(t.ast)), both(&|t| f(t.fgm))));
    out.push(("tov_per100", 100.0 * both(&|t| f(t.tov)), p));
    out.push((
        "tov_pct",
        both(&|t| f(t.tov)),
        both(&|t| f(t.fga) + 0.44 * f(t.fta) + f(t.tov)),
    ));
    out.push(("stl_per100", 100.0 * both(&|t| f(t.stl)), p));
    out.push(("blk_per100", 100.0 * both(&|t| f(t.blk)), p));
    out.push(("pf_pg", both(&|t| f(t.pf)), 2.0));
    let fouled_out: usize = r
        .boxes
        .iter()
        .flatten()
        .filter(|(_, l)| l.pf >= FOUL_LIMIT)
        .count();
    out.push(("foulout_pg", fouled_out as f64, 2.0));
    out.push(("pf_home_minus_away", f(h.pf) - f(a.pf), 1.0));
    out.push(("fta_home_minus_away", f(h.fta) - f(a.fta), 1.0));
    // game level
    let margin = r.score[0] as f64 - r.score[1] as f64;
    out.push(("home_win_pct", (margin > 0.0) as u8 as f64, 1.0));
    out.push(("margin_mean_abs", margin.abs(), 1.0));
    out.push(("margin_median_abs", margin.abs(), 1.0));
    out.push(("close5_share", (margin.abs() <= 5.0) as u8 as f64, 1.0));
    out.push(("blowout20_share", (margin.abs() >= 20.0) as u8 as f64, 1.0));
    out.push(("ot_rate", (r.periods.len() > 4) as u8 as f64, 1.0));
    if r.periods.len() >= 2 {
        let half = r.periods[..2].iter().fold(0i32, |s, p| s + p[0] as i32 - p[1] as i32);
        if half != 0 && margin != 0.0 {
            out.push((
                "halftime_leader_win_pct",
                ((half > 0) == (margin > 0.0)) as u8 as f64,
                1.0,
            ));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Event metrics: from a recording

#[derive(Default)]
struct Seg {
    side: u8,
    pts: u32,
    second: u32,
    orb: bool,
    orbs: u32,
    fb: bool,
    screen: bool,
    post: bool,
    tov: bool,
    first: Option<Source>,
    zone: bool,
    press: bool,
}

fn zone5(s: &Shot) -> usize {
    match s.zone {
        Zone::CornerThree => 3,
        Zone::AboveBreakThree => 4,
        _ => {
            let at = v(s.x, s.y);
            if at.dist(HOOP) < 1.25 {
                0
            } else if s.x.abs() < KEY_HALF_WIDTH && s.y < KEY_DEPTH {
                1
            } else {
                2
            }
        }
    }
}

fn source_id(s: Source) -> (&'static str, &'static str) {
    match s {
        Source::Transition => ("src_transition_share", "src_transition_efg"),
        Source::Drive => ("src_drive_share", "src_drive_efg"),
        Source::PullUp => ("src_pullup_share", "src_pullup_efg"),
        Source::CatchAndShoot => ("src_catchshoot_share", "src_catchshoot_efg"),
        Source::Cut => ("src_cut_share", "src_cut_efg"),
        Source::Roll => ("src_roll_share", "src_roll_efg"),
        Source::Post => ("src_post_share", "src_post_efg"),
        Source::Putback => ("src_putback_share", "src_putback_efg"),
        Source::Forced => ("src_forced_share", "src_forced_efg"),
    }
}

/// The initial defensive scheme and press of both sides, from the lineups.
type Setup = [(DefenseScheme, Press); 2];

fn event_rows(rec: &Recording, r: &GameResult, setup: Setup) -> Rows {
    let mut out: Rows = Vec::new();
    let ids = [0, 1].map(|s| r.boxes[s].iter().map(|b| b.0).collect::<Vec<_>>());
    let list = shots(rec, &ids);
    let n = list.len() as f64;
    // zones
    let mut att = [0f64; 5];
    let mut made = [0f64; 5];
    for s in &list {
        let z = zone5(s);
        att[z] += 1.0;
        made[z] += s.made as u8 as f64;
    }
    let names = ["ra", "paint", "mid", "c3", "atb3"];
    let share = ["z_ra_share", "z_paint_share", "z_mid_share", "z_c3_share", "z_atb3_share"];
    let fg = ["z_ra_fg", "z_paint_fg", "z_mid_fg", "z_c3_fg", "z_atb3_fg"];
    for i in 0..5 {
        let _ = names[i];
        out.push((share[i], att[i], n));
        out.push((fg[i], made[i], att[i]));
    }
    out.push(("z_c3_of_3pa", att[3], att[3] + att[4]));
    // Distance bins as Basketball-Reference publishes them (feet from the basket).
    let mut da = [0f64; 4];
    let mut dm = [0f64; 4];
    for s in &list {
        let ft = s.distance() as f64 / 0.3048;
        let b = if s.kind == ShotKind::Three {
            continue;
        } else if ft < 3.0 {
            0
        } else if ft < 10.0 {
            1
        } else if ft < 16.0 {
            2
        } else {
            3
        };
        da[b] += 1.0;
        dm[b] += s.made as u8 as f64;
    }
    let dshare = ["dist_0_3_share", "dist_3_10_share", "dist_10_16_share", "dist_16_3p_share"];
    let dfg = ["dist_0_3_fg", "dist_3_10_fg", "dist_10_16_fg", "dist_16_3p_fg"];
    for b in 0..4 {
        out.push((dshare[b], da[b], n));
        out.push((dfg[b], dm[b], da[b]));
    }
    let (mut a2, mut m2, mut a3, mut m3) = (0.0, 0.0, 0.0, 0.0);
    for s in &list {
        if s.made {
            if s.kind == ShotKind::Three {
                m3 += 1.0;
                a3 += s.assisted as u8 as f64;
            } else {
                m2 += 1.0;
                a2 += s.assisted as u8 as f64;
            }
        }
    }
    out.push(("ast_share_two", a2, m2));
    out.push(("ast_share_three", a3, m3));
    // sources
    let mut per_src: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
    for s in &list {
        let k = Source::ALL.iter().position(|&x| x == s.source).unwrap_or(0);
        let e = per_src.entry(k).or_default();
        e.0 += 1.0;
        e.1 += if s.made { s.kind.points() as f64 / 2.0 } else { 0.0 };
    }
    for (k, src) in Source::ALL.iter().enumerate() {
        let (sh, ef) = source_id(*src);
        let (c, pts) = per_src.get(&k).copied().unwrap_or((0.0, 0.0));
        out.push((sh, c, n));
        out.push((ef, pts, c));
    }
    // events: turnovers, fouls, and-ones, timeouts, subs, segments
    let (mut tov, mut stolen, mut badpass, mut offfoul, mut clock) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut fouls, mut f_shoot, mut f_off) = (0.0, 0.0, 0.0);
    let (mut and1, mut fgm) = (0.0, 0.0);
    let (mut timeouts, mut subs, mut adjusts) = (0.0, 0.0, 0.0);
    let mut segs: Vec<Seg> = Vec::new();
    let mut setup = setup;
    let mut margin_min = [0i32; 2];
    for e in &rec.events {
        // trailing, as seen by each side
        let d = e.score[0] as i32 - e.score[1] as i32;
        margin_min[0] = margin_min[0].min(d);
        margin_min[1] = margin_min[1].min(-d);
        match &e.kind {
            EventKind::Timeout { .. } => timeouts += 1.0,
            EventKind::Substitution { .. } => subs += 1.0,
            EventKind::Adjust { side, defense, press, .. } => {
                adjusts += 1.0;
                setup[*side as usize & 1] = (*defense, *press);
            }
            EventKind::Foul { kind, .. } => {
                fouls += 1.0;
                f_shoot += (*kind == FoulKind::Shooting) as u8 as f64;
                f_off += kind.is_offensive() as u8 as f64;
            }
            EventKind::Shot { made, fouled, .. } => {
                if *made {
                    fgm += 1.0;
                    and1 += *fouled as u8 as f64;
                }
            }
            EventKind::Turnover { kind, stolen_by, .. } => {
                tov += 1.0;
                stolen += stolen_by.is_some() as u8 as f64;
                badpass += (*kind == TurnoverKind::BadPass) as u8 as f64;
                offfoul += (*kind == TurnoverKind::OffensiveFoul) as u8 as f64;
                clock += (*kind == TurnoverKind::ShotClock) as u8 as f64;
            }
            _ => {}
        }
        let relevant = matches!(
            e.kind,
            EventKind::Shot { .. }
                | EventKind::Rebound { .. }
                | EventKind::Turnover { .. }
                | EventKind::FreeThrows { .. }
                | EventKind::FastBreak { .. }
                | EventKind::Screen { .. }
                | EventKind::PostUp { .. }
        );
        if !relevant {
            continue;
        }
        if segs.last().is_none_or(|s| s.side != e.offense) {
            let d = setup[1 - e.offense as usize & 1];
            segs.push(Seg {
                side: e.offense,
                zone: matches!(d.0, DefenseScheme::Zone23 | DefenseScheme::Zone32),
                press: d.1 != Press::None,
                ..Default::default()
            });
        }
        let s = segs.last_mut().unwrap();
        let add = |s: &mut Seg, p: u32| {
            s.pts += p;
            if s.orb {
                s.second += p;
            }
        };
        match &e.kind {
            EventKind::Shot { made, kind, source, .. } => {
                if s.first.is_none() {
                    s.first = Some(*source);
                }
                if *made {
                    add(s, kind.points() as u32);
                }
            }
            EventKind::FreeThrows { made, .. } => add(s, *made as u32),
            EventKind::Rebound { offensive: true, .. } => {
                s.orb = true;
                s.orbs += 1;
            }
            EventKind::Turnover { .. } => s.tov = true,
            EventKind::FastBreak { .. } => s.fb = true,
            EventKind::Screen { .. } => s.screen = true,
            EventKind::PostUp { .. } => s.post = true,
            _ => {}
        }
    }
    out.push(("tov_stolen_share", stolen, tov));
    out.push(("tov_badpass_share", badpass, tov));
    out.push(("tov_offfoul_share", offfoul, tov));
    out.push(("tov_shotclock_share", clock, tov));
    out.push(("foul_shooting_share", f_shoot, fouls));
    out.push(("foul_offensive_share", f_off, fouls));
    out.push(("andone_rate", and1, fgm));
    out.push(("timeouts_pg", timeouts, 2.0));
    out.push(("subs_pg", subs, 2.0));
    out.push(("adjust_pg", adjusts, 2.0));
    // comebacks: the winner trailed by at least this much at some time
    let w = if r.score[0] > r.score[1] { Some(0) } else if r.score[1] > r.score[0] { Some(1) } else { None };
    if let Some(w) = w {
        let deficit = -margin_min[w];
        out.push(("comeback10_rate", (deficit >= 10) as u8 as f64, 1.0));
        out.push(("comeback15_rate", (deficit >= 15) as u8 as f64, 1.0));
    }
    // possessions by kind
    let np = segs.len() as f64;
    let pts_all: f64 = segs.iter().map(|s| s.pts as f64).sum();
    // A crew may count a shot on the line wrongly: the event says what the
    // shot was, the score what the crew called.
    let miscount: f64 = rec
        .calls
        .iter()
        .filter_map(|c| match (c.truth, c.called) {
            // A count a review turned over stands as the engine saw it.
            _ if c.review.is_some_and(|v| v.overturned) => None,
            (Ruling::Value(t), Ruling::Value(k)) => Some(k as f64 - t as f64),
            _ => None,
        })
        .sum();
    out.push(("diag_engine_poss", (r.possessions[0] + r.possessions[1]) as f64, 1.0));
    out.push(("diag_segment_poss", np, 1.0));
    out.push((
        "diag_segment_points_ok",
        (pts_all + miscount == (r.score[0] + r.score[1]) as f64) as u8 as f64,
        1.0,
    ));
    out.push(("diag_poss_seconds", 2.0 * 2400.0, np.max(1.0)));
    let class = |s: &Seg| -> u8 {
        if s.fb || s.first == Some(Source::Transition) {
            0
        } else if s.post || s.first == Some(Source::Post) {
            1
        } else if s.screen || s.first == Some(Source::Roll) {
            2
        } else {
            match s.first {
                Some(Source::CatchAndShoot) => 3,
                Some(Source::Drive) | Some(Source::PullUp) => 4,
                Some(Source::Cut) => 5,
                _ => 6,
            }
        }
    };
    let mut cnt = [0f64; 7];
    let mut pts = [0f64; 7];
    let (mut second, mut off_tov, mut fb_pts, mut zone, mut press, mut orbs) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for (i, s) in segs.iter().enumerate() {
        let c = class(s) as usize;
        cnt[c] += 1.0;
        pts[c] += s.pts as f64;
        second += s.second as f64;
        orbs += s.orbs as f64;
        zone += s.zone as u8 as f64;
        press += s.press as u8 as f64;
        if s.fb || s.first == Some(Source::Transition) {
            fb_pts += s.pts as f64;
        }
        if i > 0 && segs[i - 1].tov && segs[i - 1].side != s.side {
            off_tov += s.pts as f64;
        }
    }
    let names = [
        ("ptype_transition_share", "ptype_transition_ppp"),
        ("ptype_post_share", "ptype_post_ppp"),
        ("ptype_pnr_share", "ptype_pnr_ppp"),
        ("ptype_spotup_share", "ptype_spotup_ppp"),
        ("ptype_iso_share", "ptype_iso_ppp"),
        ("ptype_cut_share", "ptype_cut_ppp"),
    ];
    for (c, (sh, pp)) in names.iter().enumerate() {
        out.push((sh, cnt[c], np));
        out.push((pp, pts[c], cnt[c]));
    }
    let half: f64 = cnt[1..].iter().sum();
    let hpts: f64 = pts[1..].iter().sum();
    out.push(("ptype_halfcourt_ppp", hpts, half));
    out.push(("putback_per_poss", orbs, np));
    out.push(("second_chance_pts_pg", second, 2.0));
    out.push(("pts_off_tov_pg", off_tov, 2.0));
    out.push(("fastbreak_pts_pg", fb_pts, 2.0));
    out.push(("zone_poss_share", zone, np));
    out.push(("press_poss_share", press, np));
    out
}

// ---------------------------------------------------------------------------
// Tables and players: from the finished games of a division in a season

/// A finished game with who played it.
pub struct Played<'a> {
    pub division: usize,
    pub home: usize,
    pub away: usize,
    pub result: &'a GameResult,
}

#[derive(Default)]
struct Line {
    team: usize,
    games: u32,
    b: BoxLine,
}

/// What the world knows about a team or player that boxes do not.
#[derive(Default)]
pub struct Extras {
    /// Strength of each team at the start of the season (minutes-weighted
    /// overall of its rotation), and its payroll.
    pub strength: BTreeMap<usize, f64>,
    pub payroll: BTreeMap<usize, f64>,
    /// Last season's win percentage of each team and the division it was in.
    pub prev_win: BTreeMap<usize, (usize, f64)>,
    /// Player id -> (position, height cm, weight kg, age).
    pub bodies: BTreeMap<u32, (Position, f64, f64, f64)>,
}

/// Season-level and player-level rows for one division: one cluster.
fn table_rows(games: &[Played], division: usize, x: &Extras) -> (Rows, BTreeMap<usize, f64>) {
    let mut out: Rows = Vec::new();
    let mut win: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
    let mut team: BTreeMap<usize, (f64, f64, f64)> = BTreeMap::new(); // pts for, pts against, poss
    let mut players: BTreeMap<u32, Line> = BTreeMap::new();
    let mut team_games: BTreeMap<usize, u32> = BTreeMap::new();
    for g in games.iter().filter(|g| g.division == division) {
        for (side, t) in [(0usize, g.home), (1usize, g.away)] {
            let mine = totals(g.result, side);
            let theirs = totals(g.result, 1 - side);
            let e = win.entry(t).or_default();
            e.0 += (g.result.score[side] > g.result.score[1 - side]) as u8 as f64;
            e.1 += 1.0;
            let tt = team.entry(t).or_default();
            tt.0 += mine.pts as f64;
            tt.1 += theirs.pts as f64;
            tt.2 += 0.5 * (poss_est(&mine) + poss_est(&theirs));
            *team_games.entry(t).or_default() += 1;
            for (pid, l) in &g.result.boxes[side] {
                if l.secs == 0 {
                    continue;
                }
                let e = players.entry(*pid).or_default();
                e.team = t;
                e.games += 1;
                e.b.add(l);
            }
        }
    }
    if win.len() < 4 {
        return (out, BTreeMap::new());
    }
    let ids: Vec<usize> = win.keys().copied().collect();
    let wp: Vec<f64> = ids.iter().map(|t| win[t].0 / win[t].1).collect();
    let gpt = win.values().map(|w| w.1).sum::<f64>() / win.len() as f64;
    let s = sd(&wp);
    out.push(("winpct_sd", s, 1.0));
    let luck = 0.25 / gpt;
    out.push(("noll_scully", s / luck.sqrt(), 1.0));
    out.push(("winpct_talent_sd", (s * s - luck).max(0.0).sqrt(), 1.0));
    let ortg: Vec<f64> = ids.iter().map(|t| 100.0 * team[t].0 / team[t].2).collect();
    let drtg: Vec<f64> = ids.iter().map(|t| 100.0 * team[t].1 / team[t].2).collect();
    out.push(("ortg_sd", sd(&ortg), 1.0));
    out.push(("drtg_sd", sd(&drtg), 1.0));
    let pick = |m: &BTreeMap<usize, f64>| -> Option<Vec<f64>> {
        ids.iter().map(|t| m.get(t).copied()).collect()
    };
    if let Some(r) = pick(&x.strength).and_then(|a| pearson(&a, &wp)) {
        out.push(("strength_win_r", r, 1.0));
    }
    if let Some(r) = pick(&x.payroll).and_then(|a| pearson(&a, &wp)) {
        out.push(("payroll_win_r", r, 1.0));
    }
    let (mut pw, mut cw) = (Vec::new(), Vec::new());
    for (i, t) in ids.iter().enumerate() {
        if let Some((d, w)) = x.prev_win.get(t)
            && *d == division
        {
            pw.push(*w);
            cw.push(wp[i]);
        }
    }
    if let Some(r) = pearson(&pw, &cw) {
        out.push(("prev_win_r", r, 1.0));
    }
    // players: minutes
    let mut by_team: BTreeMap<usize, Vec<(u32, f64, f64, f64, f64)>> = BTreeMap::new();
    for (pid, l) in &players {
        let g = l.games as f64;
        by_team.entry(l.team).or_default().push((
            *pid,
            l.b.secs as f64 / 60.0 / g,
            l.b.pts as f64 / g,
            l.b.fga as f64,
            g,
        ));
    }
    // rotation size: players of ten minutes a game or more, in half the games or more
    let mut rot = 0.0;
    for (t, v) in &by_team {
        let tg = team_games[t] as f64;
        rot += v.iter().filter(|p| p.1 >= 10.0 && p.4 >= 0.5 * tg).count() as f64;
    }
    if !by_team.is_empty() {
        out.push(("rotation_size", rot / by_team.len() as f64, 1.0));
    }
    let (mut st, mut bn, mut tp, mut ts, mut tf) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let mut nt = 0.0;
    for v in by_team.values_mut() {
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        if v.len() < 10 {
            continue;
        }
        nt += 1.0;
        st += v[..5].iter().map(|p| p.1).sum::<f64>() / 5.0;
        bn += v[5..10].iter().map(|p| p.1).sum::<f64>() / 5.0;
        tp += v[0].1;
        let pts: f64 = v.iter().map(|p| p.2 * p.4).sum();
        let top = v.iter().map(|p| p.2 * p.4).fold(0.0, f64::max);
        ts += top / pts.max(1.0);
        let fga: f64 = v.iter().map(|p| p.3).sum();
        tf += v.iter().map(|p| p.3).fold(0.0, f64::max) / fga.max(1.0);
    }
    if nt > 0.0 {
        out.push(("starters_mpg", st / nt, 1.0));
        out.push(("bench_mpg", bn / nt, 1.0));
        out.push(("top_mpg", tp / nt, 1.0));
        out.push(("top_scorer_pts_share", ts / nt, 1.0));
        out.push(("top_fga_share", tf / nt, 1.0));
    }
    // qualifiers: at least 70% of the team's games
    let qual: Vec<(&u32, &Line)> = players
        .iter()
        .filter(|(_, l)| l.games as f64 >= 0.7 * team_games[&l.team] as f64)
        .collect();
    let per = |l: &Line, x: u16| x as f64 / l.games as f64;
    let mut ppg: Vec<f64> = qual.iter().map(|(_, l)| per(l, l.b.pts)).collect();
    ppg.sort_by(|a, b| b.total_cmp(a));
    if let Some(top) = ppg.first() {
        out.push(("top_scorer_ppg", *top, 1.0));
        let k = ppg.len().min(10);
        out.push(("top10_ppg_mean", ppg[..k].iter().sum::<f64>() / k as f64, 1.0));
    }
    let max_of = |f: &dyn Fn(&Line) -> f64| qual.iter().map(|(_, l)| f(l)).fold(f64::NAN, f64::max);
    let rl = max_of(&|l| per(l, l.b.orb + l.b.drb));
    if rl.is_finite() {
        out.push(("reb_leader_rpg", rl, 1.0));
    }
    let al = max_of(&|l| per(l, l.b.ast));
    if al.is_finite() {
        out.push(("ast_leader_apg", al, 1.0));
    }
    let mut t3: Vec<f64> = qual
        .iter()
        .filter(|(_, l)| per(l, l.b.tpa) >= 1.5)
        .map(|(_, l)| l.b.tpm as f64 / l.b.tpa as f64)
        .collect();
    let mut f3: Vec<f64> = qual
        .iter()
        .filter(|(_, l)| per(l, l.b.fta) >= 1.5)
        .map(|(_, l)| l.b.ftm as f64 / l.b.fta as f64)
        .collect();
    if let (Some(a), Some(b)) = (percentile(&mut t3, 0.1), percentile(&mut t3, 0.9)) {
        out.push(("three_pct_p10", a, 1.0));
        out.push(("three_pct_p90", b, 1.0));
    }
    if let (Some(a), Some(b)) = (percentile(&mut f3, 0.1), percentile(&mut f3, 0.9)) {
        out.push(("ft_pct_p10", a, 1.0));
        out.push(("ft_pct_p90", b, 1.0));
    }
    // bodies and age of the men who play (ten minutes a game or more)
    let mut grp: [(f64, f64, f64); 3] = [(0.0, 0.0, 0.0); 3];
    let (mut age, mut na) = (0.0, 0.0);
    for (pid, l) in &players {
        if l.b.secs as f64 / 60.0 / (l.games as f64) < 10.0 {
            continue;
        }
        if let Some((pos, h, w, a)) = x.bodies.get(pid) {
            let k = match pos {
                Position::PG | Position::SG => 0,
                Position::SF | Position::PF => 1,
                Position::C => 2,
            };
            grp[k].0 += h;
            grp[k].1 += w;
            grp[k].2 += 1.0;
            age += a;
            na += 1.0;
        }
    }
    let hid = ["height_guard_cm", "height_forward_cm", "height_center_cm"];
    let wid = ["weight_guard_kg", "weight_forward_kg", "weight_center_kg"];
    for k in 0..3 {
        if grp[k].2 > 0.0 {
            out.push((hid[k], grp[k].0 / grp[k].2, 1.0));
            out.push((wid[k], grp[k].1 / grp[k].2, 1.0));
        }
    }
    if na > 0.0 {
        out.push(("age_mean", age / na, 1.0));
    }
    let wmap = ids.iter().copied().zip(wp.iter().copied()).collect();
    (out, wmap)
}

// ---------------------------------------------------------------------------
// Running the simulations

pub(crate) fn threads() -> usize {
    std::env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
}

/// `f(0..n)` on worker threads, results in index order.
pub(crate) fn par_map<R: Send>(workers: usize, n: usize, f: impl Fn(usize) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..n).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..workers.max(1).min(n.max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let r = f(i);
                    out.lock().unwrap()[i] = Some(r);
                }
            });
        }
    });
    out.into_inner().unwrap().into_iter().map(|r| r.unwrap()).collect()
}

/// League games of one division: every club's own tactics and coach, each
/// game recorded. Returns the pooled metric rows, one cluster per game.
fn league_games(games: usize, division: usize, seed_base: u64, workers: usize) -> Vec<Rows> {
    if games == 0 {
        return Vec::new();
    }
    let per_world = 132;
    let worlds = games.div_ceil(per_world);
    let ws: Vec<World> = (0..worlds)
        .map(|k| World::generate_tiers(seed_base + k as u64, 12, 3))
        .collect();
    par_map(workers, games, |i| {
        let w = &ws[i / per_world];
        let teams = w.division_teams(division);
        let n = teams.len();
        // ordered pairs, a different one each game; the pair order is
        // rotated per world so that no pair is always first
        let j = (i % per_world + (i / per_world) * 7) % (n * (n - 1));
        let (a, mut b) = (j / (n - 1), j % (n - 1));
        if b >= a {
            b += 1;
        }
        let (h, aw) = (teams[a], teams[b]);
        let home = lineup(&w.teams[h], &w.players);
        let away = lineup(&w.teams[aw], &w.players);
        let setup = [
            (home.tactics.defense, home.tactics.press),
            (away.tactics.defense, away.tactics.press),
        ];
        let mut rng = Rng::new(seed_base.wrapping_mul(1_000_003).wrapping_add(i as u64 * 7919 + 17));
        // A real game has a crew and a house; so does this one.
        let crew = w.bare_crew(h, seed_base.wrapping_add(i as u64));
        let (r, rec) = simulate_recorded_crewed(&home, &away, &w.rules, &mut rng, crew.as_ref());
        let mut rows = box_rows(&r);
        rows.extend(event_rows(&rec, &r, setup));
        rows
    })
}

/// One season of a career world, played to the draft.
pub(crate) struct SeasonData {
    pub division_games: Vec<Fixture>,
    pub extras: Extras,
    pub teams_division: Vec<usize>,
}

pub(crate) fn strength_of(w: &World, t: usize) -> f64 {
    let l = lineup(&w.teams[t], &w.players);
    let (mut s, mut m) = (0.0, 0.0);
    for (slot, p) in l.tactics.rotation.iter().zip(&l.players) {
        s += slot.minutes as f64 * p.overall() as f64;
        m += slot.minutes as f64;
    }
    s / m.max(1.0)
}

fn bodies_of(w: &World) -> BTreeMap<u32, (Position, f64, f64, f64)> {
    w.players
        .iter()
        .map(|p| {
            (
                p.id as u32,
                (p.position, p.body.height_cm as f64, p.body.weight_kg as f64, p.age as f64),
            )
        })
        .collect()
}

/// Play `seasons` consecutive seasons of a three-division career world and
/// hand each, when its regular season is over, to `f`.
pub(crate) fn play_world(seed: u64, seasons: usize, mut f: impl FnMut(SeasonData, &World)) {
    let mut w = World::new_career_tiers(seed, 12, 3);
    let mut done = 0usize;
    let mut start: Option<(usize, Extras)> = None;
    let mut snapped: Option<u16> = None;
    let mut guard = 0usize;
    while done < seasons {
        guard += 1;
        assert!(guard < 20_000, "the season never ended");
        let stop = w.apply(Command::Advance).expect("advance").stop.expect("a stop");
        match stop {
            Stop::MatchDay => {
                if start.as_ref().is_none_or(|s| s.0 != w.season as usize) {
                    let mut x = Extras::default();
                    for t in 0..w.teams.len() {
                        x.strength.insert(t, strength_of(&w, t));
                        x.payroll.insert(t, w.payroll(t) as f64);
                    }
                    start = Some((w.season as usize, x));
                }
                w.apply(Command::PlayRound).expect("play round");
                // The regular season is over when every table game is played;
                // the playoffs that follow are not part of it.
                let over = w.fixtures.iter().filter(|g| !g.playoff).all(|g| g.result.is_some());
                if over && snapped != Some(w.season) {
                    snapped = Some(w.season);
                    let (_, mut x) = start.take().unwrap_or_default();
                    x.bodies = bodies_of(&w);
                    let games: Vec<Fixture> = w
                        .fixtures
                        .iter()
                        .filter(|g| !g.playoff && g.result.is_some())
                        .cloned()
                        .collect();
                    let teams_division = w.teams.iter().map(|t| t.division).collect();
                    f(SeasonData { division_games: games, extras: x, teams_division }, &w);
                    done += 1;
                }
            }
            Stop::Mail => {}
            Stop::Draft => {
                let _ = w.apply(Command::DraftAuto { all: true });
            }
        }
    }
}

/// Peak age from the league's players: the age with the best mean overall
/// among ages with enough players, after three-year smoothing.
fn peak_age(w: &World) -> Option<f64> {
    let mut by_age: BTreeMap<u8, (f64, f64)> = BTreeMap::new();
    for p in &w.players {
        if w.team_of(p.id).is_none() {
            continue;
        }
        let e = by_age.entry(p.age).or_default();
        e.0 += p.overall() as f64;
        e.1 += 1.0;
    }
    let mut best: Option<(f64, f64)> = None;
    for (&a, _) in &by_age {
        let (mut s, mut n) = (0.0, 0.0);
        for k in a.saturating_sub(1)..=a + 1 {
            if let Some(e) = by_age.get(&k) {
                s += e.0;
                n += e.1;
            }
        }
        if n >= 40.0 && best.is_none_or(|b| s / n > b.0) {
            best = Some((s / n, a as f64));
        }
    }
    best.map(|b| b.1)
}

fn seasons_run(seasons: usize, seed_base: u64) -> (Vec<Rows>, Vec<Rows>) {
    const PER_WORLD: usize = 3;
    let worlds = seasons.div_ceil(PER_WORLD);
    let (mut d1, mut other) = (Vec::new(), Vec::new());
    for k in 0..worlds {
        let n = PER_WORLD.min(seasons - k * PER_WORLD);
        let mut prev: BTreeMap<usize, (usize, f64)> = BTreeMap::new();
        play_world(seed_base + k as u64, n, |mut s, w| {
            s.extras.prev_win = prev.clone();
            let played: Vec<Played> = s
                .division_games
                .iter()
                .map(|g| Played {
                    division: s.teams_division[g.home],
                    home: g.home,
                    away: g.away,
                    result: g.result.as_ref().unwrap(),
                })
                .collect();
            for d in 0..3 {
                let (mut rows, wmap) = table_rows(&played, d, &s.extras);
                if d == 0 {
                    if let Some(a) = peak_age(w) {
                        rows.push(("peak_age", a, 1.0));
                    }
                    d1.push(rows);
                } else {
                    other.push(rows);
                }
                for (t, wp) in wmap {
                    prev.insert(t, (d, wp));
                }
            }
        });
    }
    (d1, other)
}

// ---------------------------------------------------------------------------
// Externally played games

/// Games played elsewhere (by human or AI managers through the agents
/// interface). The file is a JSON array whose items are either a
/// `Fixture` as the world saves it (`{"home":..,"away":..,"division":..,
/// "result":{..}}`) or a bare `GameResult` (`{"score":..,"boxes":..}`), or an
/// object `{"label": "...", "games": [...]}` holding such an array.
pub fn load_external(text: &str) -> Result<(String, Vec<(usize, usize, usize, GameResult)>), String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let (label, items) = match value {
        serde_json::Value::Array(a) => ("external".to_string(), a),
        serde_json::Value::Object(mut o) if o.contains_key("fixtures") => {
            // a saved World (`basketman play` save_game)
            let label = o.get("label").and_then(|l| l.as_str()).unwrap_or("saved world").to_string();
            match o.remove("fixtures") {
                Some(serde_json::Value::Array(a)) => (label, a),
                _ => return Err("\"fixtures\" must be an array".into()),
            }
        }
        serde_json::Value::Object(mut o) => {
            let label = o.get("label").and_then(|l| l.as_str()).unwrap_or("external").to_string();
            match o.remove("games") {
                Some(serde_json::Value::Array(a)) => (label, a),
                _ => return Err("an object needs a \"games\" array".into()),
            }
        }
        _ => return Err("expected an array of games".into()),
    };
    let mut out = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        if item.get("result").is_some() {
            let f: Fixture = serde_json::from_value(item).map_err(|e| format!("game {i}: {e}"))?;
            if let Some(r) = f.result {
                if !f.playoff {
                    out.push((f.division, f.home, f.away, r));
                }
            }
        } else {
            let r: GameResult = serde_json::from_value(item).map_err(|e| format!("game {i}: {e}"))?;
            out.push((0, 0, 1, r));
        }
    }
    Ok((label, out))
}

fn external_rows(games: &[(usize, usize, usize, GameResult)]) -> Vec<Rows> {
    let mut rows: Vec<Rows> = games.iter().map(|g| box_rows(&g.3)).collect();
    let played: Vec<Played> = games
        .iter()
        .map(|g| Played { division: g.0, home: g.1, away: g.2, result: &g.3 })
        .collect();
    let divs: std::collections::BTreeSet<usize> = games.iter().map(|g| g.0).collect();
    for d in divs {
        let (mut r, _) = table_rows(&played, d, &Extras::default());
        r.retain(|x| !x.0.starts_with("height") && !x.0.starts_with("weight"));
        rows.push(r);
    }
    rows
}

// ---------------------------------------------------------------------------
// The report

#[derive(Clone, Debug, Serialize)]
pub struct Row {
    pub id: String,
    pub group: String,
    pub description: String,
    pub unit: String,
    pub value: Option<f64>,
    pub lo: Option<f64>,
    pub hi: Option<f64>,
    pub n: usize,
    pub target: Option<f64>,
    pub low: Option<f64>,
    pub high: Option<f64>,
    pub severity: String,
    pub basis: String,
    /// "inside", "outside", "noise" (outside but the interval reaches the
    /// band), "no target" or "not computed".
    pub verdict: String,
    /// Distance beyond the band in units of the band's half width, signed
    /// (negative: below).
    pub miss: f64,
    pub external: Option<f64>,
    pub external_n: usize,
}

fn judge(t: &Target, e: Option<&Estimate>) -> (String, f64) {
    let (Some(e), Some(lo), Some(hi)) = (e, t.low, t.high) else {
        return (if e.is_some() { "no target" } else { "not computed" }.into(), 0.0);
    };
    let half = ((hi - lo) / 2.0).max(1e-9);
    let miss = if e.value < lo {
        (e.value - lo) / half
    } else if e.value > hi {
        (e.value - hi) / half
    } else {
        0.0
    };
    if miss == 0.0 {
        return ("inside".into(), 0.0);
    }
    // outside: is it inside the noise?
    let reaches = e.hi >= lo && e.lo <= hi;
    (if reaches { "noise" } else { "outside" }.into(), miss)
}

pub fn build_report(
    targets: &[Target],
    est: &BTreeMap<&'static str, Estimate>,
    ext: Option<&BTreeMap<&'static str, Estimate>>,
) -> Vec<Row> {
    let mut rows: Vec<Row> = targets
        .iter()
        .map(|t| {
            let e = est.get(t.id.as_str());
            let (verdict, miss) = judge(t, e);
            let x = ext.and_then(|m| m.get(t.id.as_str()));
            Row {
                id: t.id.clone(),
                group: t.group.clone(),
                description: t.description.clone(),
                unit: t.unit.clone(),
                value: e.map(|e| e.value),
                lo: e.map(|e| e.lo).filter(|v| v.is_finite()),
                hi: e.map(|e| e.hi).filter(|v| v.is_finite()),
                n: e.map_or(0, |e| e.n),
                target: t.target,
                low: t.low,
                high: t.high,
                severity: t.severity.clone(),
                basis: t.basis.clone(),
                verdict,
                miss,
                external: x.map(|e| e.value),
                external_n: x.map_or(0, |e| e.n),
            }
        })
        .collect();
    // worst first: outside (beyond noise) before noise, by size; hard first at equal size
    rows.sort_by(|a, b| {
        let key = |r: &Row| {
            let rank = match r.verdict.as_str() {
                "outside" => 0,
                "noise" => 1,
                "inside" => 2,
                _ => 3,
            };
            (rank, -r.miss.abs(), r.severity != "hard", r.id.clone())
        };
        let (ka, kb) = (key(a), key(b));
        ka.0.cmp(&kb.0)
            .then(ka.1.total_cmp(&kb.1))
            .then(ka.2.cmp(&kb.2))
            .then(ka.3.cmp(&kb.3))
    });
    rows
}

fn fmt(x: Option<f64>) -> String {
    match x {
        Some(v) if v.abs() >= 100.0 => format!("{v:.1}"),
        Some(v) if v.abs() >= 1.0 => format!("{v:.2}"),
        Some(v) => format!("{v:.3}"),
        None => "-".into(),
    }
}

fn print_report(rows: &[Row], has_ext: bool) {
    println!(
        "{:<26} {:>9} {:>19} {:>8} {:>17}  {:<5} {}{}",
        "metric", "value", "95% interval", "target", "band", "kind", "verdict", if has_ext { "   external" } else { "" }
    );
    for r in rows {
        let ci = match (r.lo, r.hi) {
            (Some(a), Some(b)) => format!("{}..{}", fmt(Some(a)), fmt(Some(b))),
            _ => "-".into(),
        };
        let band = match (r.low, r.high) {
            (Some(a), Some(b)) => format!("{}..{}", fmt(Some(a)), fmt(Some(b))),
            _ => "-".into(),
        };
        let verdict = match r.verdict.as_str() {
            "outside" => format!("OUTSIDE by {:.1} half-bands {}", r.miss.abs(), if r.miss < 0.0 { "(low)" } else { "(high)" }),
            "noise" => format!("outside by {:.1} half-bands, within noise", r.miss.abs()),
            v => v.to_string(),
        };
        println!(
            "{:<26} {:>9} {:>19} {:>8} {:>17}  {:<5} {}{}",
            r.id,
            fmt(r.value),
            ci,
            fmt(r.target),
            band,
            if r.severity == "hard" { "HARD" } else { "soft" },
            verdict,
            if has_ext { format!("   {}", fmt(r.external)) } else { String::new() }
        );
    }
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// The whole run as data, so a test can run it small and twice.
pub struct Config {
    pub games: usize,
    /// Worker threads; 0 takes `RAYON_NUM_THREADS`.
    pub threads: usize,
    pub seasons: usize,
}

pub fn measure(c: &Config) -> BTreeMap<&'static str, Estimate> {
    let mut pool = Pool::default();
    for rows in league_games(c.games, 0, 7_000, if c.threads > 0 { c.threads } else { threads() }) {
        pool.add(rows);
    }
    let (d1, _) = seasons_run(c.seasons, 9_000);
    for rows in d1 {
        pool.add(rows);
    }
    pool.estimates()
}

pub fn run(args: &[String]) {
    let games: usize = arg_value(args, "--games").and_then(|s| s.parse().ok()).unwrap_or(3000);
    let seasons: usize = arg_value(args, "--seasons").and_then(|s| s.parse().ok()).unwrap_or(12);
    let out = arg_value(args, "--out");
    let targets_text = match arg_value(args, "--targets") {
        Some(p) => std::fs::read_to_string(&p).unwrap_or_else(|e| {
            eprintln!("cannot read {p}: {e}");
            std::process::exit(2)
        }),
        None => TARGETS_JSON.to_string(),
    };
    let targets = parse_targets(&targets_text).unwrap_or_else(|e| {
        eprintln!("targets: {e}");
        std::process::exit(2)
    });
    let t0 = std::time::Instant::now();
    eprintln!("simulating {games} top-division league games on {} threads ...", threads());
    let mut pool = Pool::default();
    for rows in league_games(games, 0, 7_000, threads()) {
        pool.add(rows);
    }
    eprintln!("  done in {:.0?}", t0.elapsed());
    // lower divisions: a quarter of the games each, for the by-division table
    let lower = games / 4;
    let mut lower_pools = [Pool::default(), Pool::default()];
    for (i, d) in [1usize, 2].into_iter().enumerate() {
        for rows in league_games(lower, d, 7_500 + 100 * d as u64, threads()) {
            lower_pools[i].add(rows);
        }
    }
    eprintln!("  lower divisions done at {:.0?}", t0.elapsed());
    let (mut d1, mut other) = (Vec::new(), Vec::new());
    if seasons > 0 {
        eprintln!("playing {seasons} seasons of the three-division world ...");
        (d1, other) = seasons_run(seasons, 9_000);
        eprintln!("  done at {:.0?}", t0.elapsed());
    }
    for rows in d1 {
        pool.add(rows);
    }
    let est = pool.estimates();
    let ext = arg_value(args, "--external").map(|p| {
        let text = std::fs::read_to_string(&p).unwrap_or_else(|e| {
            eprintln!("cannot read {p}: {e}");
            std::process::exit(2)
        });
        let (label, games) = load_external(&text).unwrap_or_else(|e| {
            eprintln!("{p}: {e}");
            std::process::exit(2)
        });
        eprintln!("external: {label}, {} games", games.len());
        let mut pe = Pool::default();
        for r in external_rows(&games) {
            pe.add(r);
        }
        (label, pe.estimates())
    });
    let report = build_report(&targets, &est, ext.as_ref().map(|e| &e.1));
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    println!("basketman realism: commit {commit}, {games} league games (+{lower} per lower division), {seasons} seasons\n");
    print_report(&report, ext.is_some());
    let mut diag: Vec<(&&str, &Estimate)> = est.iter().filter(|(k, _)| !targets.iter().any(|t| t.id == **k)).collect();
    diag.sort_by_key(|(k, _)| **k);
    println!("\nComputed, no target:");
    for (k, e) in diag {
        println!("  {:<26} {:>9}  ({}..{})", k, fmt(Some(e.value)), fmt(Some(e.lo)), fmt(Some(e.hi)));
    }
    println!("\nBy division (league games): value in divisions 1 / 2 / 3");
    let by_div = [
        pool.estimates(),
        lower_pools[0].estimates(),
        lower_pools[1].estimates(),
    ];
    for id in ["pace", "ortg", "fg_pct", "three_pct", "ft_pct", "tov_per100", "orb_pct", "pf_pg"] {
        let g = |i: usize| by_div[i].get(id).map(|e| e.value);
        println!("  {:<12} {:>8} {:>8} {:>8}", id, fmt(g(0)), fmt(g(1)), fmt(g(2)));
    }
    if !other.is_empty() {
        let mut p = Pool::default();
        for r in other {
            p.add(r);
        }
        let e = p.estimates();
        println!("\nLower divisions in the seasons (divisions 2 and 3 pooled): win% sd {}, top scorer ppg {}",
            fmt(e.get("winpct_sd").map(|x| x.value)), fmt(e.get("top_scorer_ppg").map(|x| x.value)));
    }
    println!("\nCannot be computed yet:");
    for (id, why) in UNMEASURABLE {
        println!("  {id}: {why}");
    }
    if let Some(dir) = out {
        let _ = std::fs::create_dir_all(&dir);
        let json = serde_json::json!({
            "commit": commit,
            "games": games,
            "seasons": seasons,
            "metrics": report,
            "unmeasurable": UNMEASURABLE.iter().map(|(a, b)| serde_json::json!({"id": a, "needs": b})).collect::<Vec<_>>(),
            "no_target": est.iter().filter(|(k, _)| !targets.iter().any(|t| t.id == **k)).map(|(k, e)| serde_json::json!({"id": k, "value": e.value, "lo": e.lo, "hi": e.hi})).collect::<Vec<_>>(),
        });
        let path = format!("{dir}/realism.json");
        std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
        eprintln!("wrote {path}");
    }
    eprintln!("total {:.0?}", t0.elapsed());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_targets_file_parses_and_every_id_is_known() {
        let targets = parse_targets(TARGETS_JSON).expect("docs/realism-targets.json parses");
        assert!(targets.len() > 40, "{} targets", targets.len());
        let mut seen = std::collections::BTreeSet::new();
        for t in &targets {
            assert!(seen.insert(t.id.clone()), "{} is listed twice", t.id);
            assert!(
                KNOWN.contains(&t.id.as_str()) || UNMEASURABLE.iter().any(|u| u.0 == t.id),
                "target {} is not a metric the code knows",
                t.id
            );
            assert!(t.severity == "hard" || t.severity == "soft", "{}: severity", t.id);
            assert!(["sourced", "derived", "judgement", "none"].contains(&t.basis.as_str()), "{}: basis", t.id);
            if let (Some(lo), Some(hi), Some(x)) = (t.low, t.high, t.target) {
                assert!(lo <= x && x <= hi, "{}: target {x} outside its band {lo}..{hi}", t.id);
            }
            if t.target.is_some() {
                assert!(t.low.is_some() && t.high.is_some(), "{}: a target needs a band", t.id);
                assert!(!t.source.is_empty(), "{}: a target needs a source", t.id);
            }
        }
        // the code's own lists have no duplicates
        let mut k = std::collections::BTreeSet::new();
        for id in KNOWN {
            assert!(k.insert(*id), "{id} is listed twice in KNOWN");
        }
    }

    #[test]
    fn every_metric_the_code_computes_is_registered() {
        let c = Config { games: 4, threads: 0, seasons: 0 };
        let est = measure(&c);
        for id in est.keys() {
            assert!(KNOWN.contains(id), "{id} is computed but not in KNOWN");
        }
        assert!(est.contains_key("pace") && est.contains_key("z_c3_fg"));
        // percent metrics are percentages, as the targets file states them
        let targets = parse_targets(TARGETS_JSON).unwrap();
        for t in targets.iter().filter(|t| t.unit == "%") {
            if let Some(e) = est.get(t.id.as_str()) {
                assert!(e.value >= 0.0 && e.value <= 100.0, "{} = {} is not a percentage", t.id, e.value);
                assert!(is_percent(&t.id), "{} has unit % but is not scaled", t.id);
            }
        }
    }

    #[test]
    fn the_report_is_deterministic_whatever_the_threads() {
        let targets = parse_targets(TARGETS_JSON).unwrap();
        let one = Config { games: 8, threads: 1, seasons: 0 };
        let three = Config { games: 8, threads: 3, seasons: 0 };
        let a = build_report(&targets, &measure(&one), None);
        let b = build_report(&targets, &measure(&three), None);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn possessions_found_in_the_events_match_the_engine_and_the_score() {
        let est = measure(&Config { games: 6, threads: 0, seasons: 0 });
        let eng = est["diag_engine_poss"].value;
        let seg = est["diag_segment_poss"].value;
        assert!((eng - seg).abs() / eng < 0.03, "engine {eng} vs events {seg}");
        assert_eq!(est["diag_segment_points_ok"].value, 1.0, "points in the possessions add up to the score");
    }

    #[test]
    fn external_games_parse_in_both_shapes() {
        let w = World::generate_tiers(3, 12, 3);
        let (h, a) = (lineup(&w.teams[0], &w.players), lineup(&w.teams[1], &w.players));
        let mut rng = Rng::new(1);
        let (r, _) = basketman_core::engine::simulate_recorded(&h, &a, &w.rules, &mut rng);
        let bare = serde_json::to_string(&vec![&r]).unwrap();
        let (_, g) = load_external(&bare).unwrap();
        assert_eq!(g.len(), 1);
        let fixture = Fixture {
            round: 0,
            home: 0,
            away: 1,
            result: Some(r.clone()),
            playoff: false,
            day: 0,
            division: 0,
        };
        let wrapped = serde_json::json!({"label": "mine", "games": [fixture]}).to_string();
        let (label, g) = load_external(&wrapped).unwrap();
        assert_eq!(label, "mine");
        assert_eq!(g[0].3.score, r.score);
        assert!(!box_rows(&g[0].3).is_empty());
    }
}
