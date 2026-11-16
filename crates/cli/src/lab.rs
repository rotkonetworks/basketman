//! `basketman lab [games]`: what beats what.
//!
//! Plays every offensive set against every defensive scheme with the same
//! squads and prints points per possession, so the engine's tactical balance
//! can be checked against how the real game works.

use basketman_core::court::ShotKind;
use basketman_core::engine::{BoxLine, Lineup, simulate_recorded};
use basketman_core::league::World;
use basketman_core::player::{Attr, Player, Position};
use basketman_core::replay::{EventKind, ShotHow, Source};
use basketman_core::rng::Rng;
use basketman_core::tactics::{DefenseScheme, OffenseSet, Press, Role, Tactics};

#[derive(Default, Clone, Copy)]
pub struct Cell {
    /// Turnovers by kind: bad pass, stripped, offensive foul, shot clock, pressed.
    pub tov_kinds: [u32; 6],
    pub possessions: u32,
    pub secs: f32,
    pub line: BoxLine,
    pub opp: BoxLine,
    /// Attempts and summed contest by shot kind: rim, mid, three.
    pub shots: [(u32, u32, f32); 3],
    pub assisted: u32,
    pub mistakes: u32,
    /// Attempts, makes and points by how the shot came about.
    pub sources: [(u32, u32, u32); 9],
    /// Shots by position of the shooter (PG..C) and source.
    pub by_position: [[u32; 9]; 5],
    /// Screens, and the shot the handler took within five seconds of each:
    /// count, summed contest, points.
    pub screens: u32,
    pub screen_shots: (u32, f32, u32),
    pub screen_kinds: [u32; 3],
    /// Screens after which the handler got a step on his man.
    pub screen_beaten: u32,
    /// Distance from the handler to his nearest defender 0.8 s after a screen: sum, count.
    pub screen_gap: (f32, u32),
    pub help_stops: u32,
    pub late_help: u32,
    /// Contact fatigue and on-court ticks of the attacking side's guards.
    pub guards_leaned: (f64, u64),
    pub drive: [[f32; 4]; 3],
    /// After a ball screen, by how the handler's man got past (over, under):
    /// screens, shots, contest, points, beaten, rim, mid, three, pull-ups, drives.
    pub by_cover: [[f32; 10]; 2],
    /// Post moves, ground gained, moves won; shots from the post: contest, rim.
    pub post: [f32; 3],
    /// Shots from the post: summed contest and shots at the rim.
    pub post_shots: [f32; 2],
}

impl Cell {
    pub fn ppp(&self) -> f32 {
        self.line.pts as f32 / self.possessions.max(1) as f32
    }
}

/// `games` games of `attack` (using `offense`) against `defend` (using
/// `defense`), counting only the possessions of the attacking team.
pub fn cell(w: &World, tune: &dyn Fn(&mut Tactics, &mut Tactics), games: usize, seed: u64) -> Cell {
    duel(w, w, tune, games, seed, 1.0)
}

/// Like `cell`, with the attacking squads taken from `wa` and the defending
/// ones from `w`, so one side's players can be altered. `familiarity` is how
/// well the attackers know their tactic.
pub fn duel(
    wa: &World,
    w: &World,
    tune: &dyn Fn(&mut Tactics, &mut Tactics),
    games: usize,
    seed: u64,
    familiarity: f32,
) -> Cell {
    let mut out = Cell::default();
    let n = w.teams.len();
    for g in 0..games {
        let (a, b) = (g % n, (g * 5 + 1 + g / n) % n);
        if a == b {
            continue;
        }
        let (mut ta, mut tb) = (w.teams[a].tactics().clone(), w.teams[b].tactics().clone());
        tune(&mut ta, &mut tb);
        let la = Lineup {
            tactics: &ta,
            players: line(wa, &ta),
            familiarity,
            belief: 0.55,
            coach: 0,
        };
        let lb = Lineup {
            tactics: &tb,
            players: line(w, &tb),
            familiarity: 1.0,
            belief: 0.55,
            coach: 0,
        };
        // Alternate home court so it cancels out.
        let side = g % 2;
        let mut rng = Rng::new(seed.wrapping_add(g as u64 * 7919));
        let (r, rec) = if side == 0 {
            simulate_recorded(&la, &lb, &w.rules, &mut rng)
        } else {
            simulate_recorded(&lb, &la, &w.rules, &mut rng)
        };
        out.possessions += r.possessions[side] as u32;
        out.secs += r.periods.len() as f32 * 600.0 / 2.0;
        for (_, l) in &r.boxes[side] {
            out.line.add(l);
        }
        for (_, l) in &r.boxes[1 - side] {
            out.opp.add(l);
        }
        for (i, (id, _)) in r.boxes[side].iter().enumerate() {
            if is_guard(&wa.players[*id as usize])
                && let (Some(c), Some(n)) = (rec.leaned[side].get(i), rec.leaned_ticks[side].get(i))
            {
                out.guards_leaned.0 += *c as f64;
                out.guards_leaned.1 += *n as u64;
            }
        }
        for (a, b) in out.drive.iter_mut().zip(rec.drive) {
            for (x, y) in a.iter_mut().zip(b) {
                *x += y;
            }
        }
        for (a, b) in out.post.iter_mut().zip(rec.post) {
            *a += b;
        }
        out.help_stops += rec.help_stops;
        out.late_help += rec.late_help;
        let mut screened: Option<(u32, f32)> = None;
        let mut cover: Option<usize> = None;
        for e in &rec.events {
            if e.offense as usize != side {
                continue;
            }
            match &e.kind {
                EventKind::Screen { handler, .. } => {
                    cover = rec
                        .screens
                        .iter()
                        .find(|x| x.0 == e.time)
                        .map(|x| x.1 as usize);
                    if let Some(c) = cover {
                        out.by_cover[c][0] += 1.0;
                    }
                    out.screens += 1;
                    screened = Some((*handler, e.time));
                    let f = &rec.frames[rec.frame_at(e.time + 0.8)];
                    if let Some((s, k)) = f.holder
                        && s as usize == side
                        && r.boxes[side][f.who[side][k as usize] as usize].0 == *handler
                    {
                        let p = f.pos[side][k as usize];
                        // The man who was on him when the screen came.
                        let f0 = &rec.frames[rec.frame_at(e.time)];
                        let h0 = f0.pos[side]
                            .iter()
                            .zip(f0.who[side])
                            .find(|(_, w)| *w == f.who[side][k as usize])
                            .map_or(p, |(q, _)| *q);
                        let on = (0..5)
                            .min_by(|&a, &b| {
                                f0.pos[1 - side][a]
                                    .dist(h0)
                                    .total_cmp(&f0.pos[1 - side][b].dist(h0))
                            })
                            .unwrap_or(0);
                        let near = f.pos[1 - side][on].dist(p);
                        out.screen_gap.0 += near;
                        out.screen_gap.1 += 1;
                    }
                }
                EventKind::Shot {
                    who,
                    kind,
                    source,
                    made,
                    contest,
                    assist,
                    fouled,
                    ..
                } => {
                    if let Some((h, at)) = screened.take()
                        && h == *who
                        && e.time - at < 5.0
                    {
                        out.screen_kinds[match kind {
                            ShotKind::Rim => 0,
                            ShotKind::Mid => 1,
                            ShotKind::Three => 2,
                        }] += 1;
                        out.screen_shots.0 += 1;
                        out.screen_shots.1 += *contest;
                        out.screen_shots.2 += if *made { kind.points() as u32 } else { 0 };
                        if let Some(c) = cover {
                            let b = &mut out.by_cover[c];
                            b[1] += 1.0;
                            b[2] += *contest;
                            b[3] += if *made { kind.points() as f32 } else { 0.0 };
                            b[5 + match kind {
                                ShotKind::Rim => 0,
                                ShotKind::Mid => 1,
                                ShotKind::Three => 2,
                            }] += 1.0;
                            if *source == Source::PullUp {
                                b[8] += 1.0;
                            }
                            if *source == Source::Drive {
                                b[9] += 1.0;
                            }
                        }
                    }
                    if *fouled && !*made {
                        continue;
                    }
                    let pos = wa.players[*who as usize].position as usize;
                    out.by_position[pos][*source as usize] += 1;
                    let by = &mut out.sources[*source as usize];
                    by.0 += 1;
                    by.1 += *made as u32;
                    by.2 += if *made { kind.points() as u32 } else { 0 };
                    let s = &mut out.shots[match kind {
                        ShotKind::Rim => 0,
                        ShotKind::Mid => 1,
                        ShotKind::Three => 2,
                    }];
                    s.0 += 1;
                    s.1 += *made as u32;
                    s.2 += contest;
                    out.assisted += assist.is_some() as u32;
                    if *source == Source::Post {
                        out.post_shots[0] += *contest;
                        out.post_shots[1] += (*kind == ShotKind::Rim) as u8 as f32;
                    }
                }
                EventKind::Turnover { kind, .. } => {
                    out.tov_kinds[*kind as usize] += 1;
                }
                EventKind::Mistake { .. } => out.mistakes += 1,
                EventKind::Beaten { who, .. } => {
                    if let Some((h, at)) = screened
                        && h == *who
                        && e.time - at < 1.0
                    {
                        out.screen_beaten += 1;
                        if let Some(c) = cover {
                            out.by_cover[c][4] += 1.0;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn detail(label: &str, c: &Cell) {
    let p = c.possessions.max(1) as f32;
    let fga = c.line.fga.max(1) as f32;
    let share = |i: usize| 100.0 * c.shots[i].0 as f32 / fga;
    let pct = |i: usize| 100.0 * c.shots[i].1 as f32 / c.shots[i].0.max(1) as f32;
    let open = |i: usize| c.shots[i].2 / c.shots[i].0.max(1) as f32;
    let misses = (c.line.fga - c.line.fgm) as f32;
    println!(
        "  {label:<18} {:.3} ppp {:>4.1}s | rim {:>2.0}% {:>2.0}% c{:.2} | mid {:>2.0}% {:>2.0}% c{:.2} | three {:>2.0}% {:>2.0}% c{:.2} | tov {:>4.1}% orb {:>2.0}% fta/fga {:.2} ast {:>2.0}% blk {:.1}% pf {:.1}%",
        c.ppp(),
        c.secs / p,
        share(0),
        pct(0),
        open(0),
        share(1),
        pct(1),
        open(1),
        share(2),
        pct(2),
        open(2),
        100.0 * c.line.tov as f32 / p,
        100.0 * c.line.orb as f32 / (c.line.orb as f32 + c.opp.drb as f32).max(1.0),
        c.line.fta as f32 / fga,
        100.0 * c.assisted as f32 / c.line.fgm.max(1) as f32,
        100.0 * c.opp.blk as f32 / misses.max(1.0),
        100.0 * c.opp.pf as f32 / p,
    );
}

pub fn run(games: usize) {
    let w = World::generate(7, 12);
    println!("Points per possession, {games} games per cell (rows attack, columns defend)");
    println!("{}\n", noise(games));
    print!("{:<16}", "");
    for d in DefenseScheme::ALL {
        print!("{:>18}", d.label());
    }
    println!("{:>10}", "mean");
    let mut cells = Vec::new();
    for o in OffenseSet::ALL {
        print!("{:<16}", o.label());
        let mut sum = 0.0;
        for d in DefenseScheme::ALL {
            let c = cell(
                &w,
                &|a, b| {
                    a.offense = o;
                    b.defense = d;
                },
                games,
                11,
            );
            print!("{:>18.3}", c.ppp());
            sum += c.ppp();
            cells.push((o, d, c));
        }
        println!("{:>10.3}", sum / DefenseScheme::ALL.len() as f32);
    }
    print!("{:<16}", "mean");
    for d in DefenseScheme::ALL {
        let col: Vec<f32> = cells
            .iter()
            .filter(|c| c.1 == d)
            .map(|c| c.2.ppp())
            .collect();
        print!("{:>18.3}", col.iter().sum::<f32>() / col.len() as f32);
    }
    println!("\n\nHow each matchup plays (share of shots, make %, c = average contest)\n");
    for (o, d, c) in &cells {
        detail(&format!("{:.8} v {:.8}", o.label(), d.label()), c);
    }
}

fn line<'w>(w: &'w World, t: &Tactics) -> Vec<&'w Player> {
    t.rotation
        .iter()
        .map(|s| &w.players[s.player as usize])
        .collect()
}

/// A copy of the league with some players changed.
fn altered(w: &World, change: impl Fn(&mut Player)) -> World {
    let mut out = w.clone();
    for p in &mut out.players {
        change(p);
        for a in Attr::ALL {
            p.attrs[a] = p.attrs[a].clamp(1.0, 20.0);
        }
    }
    out
}

fn is_big(p: &Player) -> bool {
    matches!(p.position, Position::PF | Position::C)
}

fn is_guard(p: &Player) -> bool {
    matches!(p.position, Position::PG | Position::SG)
}

/// One row of an experiment: the same matchup with two kinds of squad.
/// How big a gap between two cells has to be before it means anything.
/// A possession is worth about 1.1 points give or take 1.1, and a game has
/// roughly 74 of them for the side being measured.
fn noise(games: usize) -> String {
    let se = 1.1 / (games as f32 * 74.0).sqrt();
    format!(
        "Differences between two cells smaller than about {:.3} are noise.",
        2.0 * se * std::f32::consts::SQRT_2
    )
}

fn row(label: &str, a: f32, b: f32) {
    println!("    {label:<34} {a:>7.3} {b:>7.3}   {:+.3}", b - a);
}

/// `basketman lab roles`: the same tactics with different players, to check
/// that a scheme only works when the squad suits it.
pub fn roles(games: usize) {
    println!("{}", noise(games));
    let w = World::generate(7, 12);
    let ppp = |wa: &World, wd: &World, o: OffenseSet, d: DefenseScheme, press: Press| {
        duel(
            wa,
            wd,
            &|a, b| {
                a.offense = o;
                b.defense = d;
                b.press = press;
            },
            games,
            23,
            1.0,
        )
    };
    use DefenseScheme::*;
    use OffenseSet::*;
    let head = |title: &str, a: &str, b: &str| {
        println!("\n{title}");
        println!("    {:<34} {a:>7} {b:>7}   change", "points per possession");
    };

    // Shooting decides whether packing the paint pays.
    let bricks = altered(&w, |p| p.attrs[Attr::ThreePoint] -= 4.0);
    let snipers = altered(&w, |p| p.attrs[Attr::ThreePoint] += 4.0);
    head(
        "Outside shooting against defenses that protect the paint",
        "poor",
        "good",
    );
    for d in [ManToMan, Zone23, Zone32, PackLine] {
        row(
            &format!("Motion v {}", d.label()),
            ppp(&bricks, &w, Motion, d, Press::None).ppp(),
            ppp(&snipers, &w, Motion, d, Press::None).ppp(),
        );
    }

    // Switching needs big men who can stay with guards.
    // Switching needs five defenders who can guard anyone: big men who can
    // move their feet and guards who can hold their ground inside.
    let versatile = altered(&w, |p| {
        if is_big(p) {
            p.attrs[Attr::Speed] += 5.0;
            p.attrs[Attr::PerimeterD] += 5.0;
        } else {
            p.attrs[Attr::InteriorD] += 5.0;
            p.attrs[Attr::Strength] += 4.0;
            p.attrs[Attr::Block] += 3.0;
        }
    });
    head(
        "Screens against specialists and against five versatile defenders",
        "special",
        "versat.",
    );
    for o in [PickAndRoll, Motion] {
        for d in [ManToMan, SwitchEverything] {
            row(
                &format!("{} v {}", o.label(), d.label()),
                ppp(&w, &w, o, d, Press::None).ppp(),
                ppp(&w, &versatile, o, d, Press::None).ppp(),
            );
        }
    }

    // A post offense is only as good as the post player.
    let towers = altered(&w, |p| {
        if is_big(p) {
            p.attrs[Attr::PostMoves] += 5.0;
            p.attrs[Attr::Strength] += 3.0;
            p.attrs[Attr::CloseShot] += 3.0;
        }
    });
    head(
        "Post play with ordinary and dominant big men",
        "normal",
        "dominant",
    );
    for d in [ManToMan, SwitchEverything, Zone23, PackLine] {
        row(
            &format!("Post up v {}", d.label()),
            ppp(&w, &w, PostUp, d, Press::None).ppp(),
            ppp(&towers, &w, PostUp, d, Press::None).ppp(),
        );
    }
    row(
        "Motion v Man to man (same squads)",
        ppp(&w, &w, Motion, ManToMan, Press::None).ppp(),
        ppp(&towers, &w, Motion, ManToMan, Press::None).ppp(),
    );

    // Five out with big men who cannot shoot is four out and a spectator.
    let stretch = altered(&w, |p| {
        if is_big(p) {
            p.attrs[Attr::ThreePoint] += 6.0;
        }
    });
    let planted = altered(&w, |p| {
        if is_big(p) {
            p.attrs[Attr::ThreePoint] -= 6.0;
        }
    });
    head(
        "Spacing with big men who can or cannot shoot",
        "cannot",
        "can",
    );
    for o in [FiveOut, Motion, PostUp] {
        row(
            &format!("{} v Man to man", o.label()),
            ppp(&planted, &w, o, ManToMan, Press::None).ppp(),
            ppp(&stretch, &w, o, ManToMan, Press::None).ppp(),
        );
    }

    // Isolation needs someone worth isolating.
    let best: Vec<u32> = w
        .teams
        .iter()
        .filter_map(|t| {
            t.tactics()
                .rotation
                .iter()
                .find(|s| s.role == Role::PrimaryOption)
                .map(|s| s.player)
        })
        .collect();
    let stars = altered(&w, |p| {
        if best.contains(&p.id) {
            p.attrs[Attr::Speed] += 4.0;
            p.attrs[Attr::Handling] += 4.0;
            p.attrs[Attr::CloseShot] += 4.0;
            p.attrs[Attr::MidRange] += 4.0;
        }
    });
    head(
        "Isolation when the first option is ordinary or a star",
        "ordinary",
        "star",
    );
    for d in [ManToMan, PackLine, Zone23] {
        row(
            &format!("Isolation v {}", d.label()),
            ppp(&w, &w, Isolation, d, Press::None).ppp(),
            ppp(&stars, &w, Isolation, d, Press::None).ppp(),
        );
    }
    row(
        "Motion v Man to man (same squads)",
        ppp(&w, &w, Motion, ManToMan, Press::None).ppp(),
        ppp(&stars, &w, Motion, ManToMan, Press::None).ppp(),
    );

    // A press is a bet against the other team's guards.
    let shaky = altered(&w, |p| {
        if is_guard(p) {
            p.attrs[Attr::Handling] -= 5.0;
            p.attrs[Attr::Passing] -= 5.0;
        }
    });
    let steady = altered(&w, |p| {
        if is_guard(p) {
            p.attrs[Attr::Handling] += 5.0;
            p.attrs[Attr::Passing] += 5.0;
        }
    });
    head(
        "Full-court press against shaky and steady ball handlers",
        "no press",
        "press",
    );
    for (label, wa) in [("Shaky guards", &shaky), ("Steady guards", &steady)] {
        let (off, on) = (
            ppp(wa, &w, Motion, ManToMan, Press::None),
            ppp(wa, &w, Motion, ManToMan, Press::FullCourt),
        );
        row(label, off.ppp(), on.ppp());
        println!(
            "    {:<34} {:>6.1}% {:>6.1}%",
            "  turnovers per possession",
            100.0 * off.line.tov as f32 / off.possessions as f32,
            100.0 * on.line.tov as f32 / on.possessions as f32
        );
    }

    // Knowing the system.
    head(
        "The same offense, half learned and fully drilled",
        "40%",
        "100%",
    );
    for o in [Motion, PickAndRoll] {
        let run =
            |familiarity: f32| duel(&w, &w, &|a, _| a.offense = o, games, 23, familiarity).ppp();
        row(&format!("{} v Man to man", o.label()), run(0.4), run(1.0));
    }
}

/// `basketman lab shots`: where each offense gets its shots from.
pub fn shots(games: usize) {
    let w = World::generate(7, 12);
    for d in [DefenseScheme::ManToMan, DefenseScheme::Zone23] {
        println!(
            "\nShare of shots and points per shot by source, against {}\n",
            d.label()
        );
        print!("{:<16}", "");
        for s in Source::ALL {
            print!("{:>16}", s.label());
        }
        println!();
        for o in OffenseSet::ALL {
            let c = cell(
                &w,
                &|a, b| {
                    a.offense = o;
                    b.defense = d;
                },
                games,
                11,
            );
            let total: u32 = c.sources.iter().map(|s| s.0).sum();
            print!("{:<16}", o.label());
            for s in c.sources {
                print!(
                    "{:>9.0}% {:>5.2}",
                    100.0 * s.0 as f32 / total.max(1) as f32,
                    s.2 as f32 / s.0.max(1) as f32
                );
            }
            println!();
            for (pos, row) in c.by_position.iter().enumerate() {
                print!("  {:<14}", Position::ALL[pos].label());
                for n in row {
                    print!("{:>9.1}%      ", 100.0 * *n as f32 / total.max(1) as f32);
                }
                println!(
                    "{:>6.1}%",
                    100.0 * row.iter().sum::<u32>() as f32 / total.max(1) as f32
                );
            }
        }
    }
}

/// Box-score totals that do not wrap over a thousand games.
#[derive(Default)]
struct Wide {
    pts: u64,
    fgm: u64,
    fga: u64,
    tpa: u64,
    ftm: u64,
    fta: u64,
    orb: u64,
    drb: u64,
    ast: u64,
    stl: u64,
    blk: u64,
    tov: u64,
    pf: u64,
}

impl Wide {
    fn add(&mut self, l: &BoxLine) {
        self.pts += l.pts as u64;
        self.fgm += l.fgm as u64;
        self.fga += l.fga as u64;
        self.tpa += l.tpa as u64;
        self.ftm += l.ftm as u64;
        self.fta += l.fta as u64;
        self.orb += l.orb as u64;
        self.drb += l.drb as u64;
        self.ast += l.ast as u64;
        self.stl += l.stl as u64;
        self.blk += l.blk as u64;
        self.tov += l.tov as u64;
        self.pf += l.pf as u64;
    }
}

/// Everything `basketman lab stats` counts over a batch of games.
#[derive(Default)]
struct Totals {
    games: u32,
    possessions: u32,
    line: Wide,
    shots: [(u32, u32); 3],
    tov_kinds: [u32; 6],
    sources: [(u32, u32, u32); 9],
    by_kind: [[u32; 3]; 9],
    margins: Vec<u32>,
    pair_frames: u64,
    overlaps: u64,
    deep: u64,
    worst: f32,
    stuck: u32,
    /// Contact fatigue and on-court ticks by weight class: light, middle, heavy.
    leaned: [(f64, u64); 3],
    fouls: [u32; 8],
    charge_src: [u32; 5],
    /// Free throws and seconds by quartile of how well a player draws fouls.
    drawn: [(u64, u64); 4],
    pf_by_quartile: [(u64, u64); 4],
    charges: Vec<(u32, u32)>,
    frames: u64,
    secs: f64,
    /// Shots on which the shooter was fouled, and how many of those went in
    /// (and-ones), by kind; rebounds taken, offensive ones, and the putbacks
    /// and tip-ins that followed.
    fouled: [(u32, u32); 3],
    rebounds: (u32, u32),
    tipped: (u32, u32),
    /// Seconds and counts: shot to rebound after a miss (defensive, offensive), shot to the
    /// next shot, turnover or foul after a make, rebound to the next such event.
    gaps: [(f64, u32); 5],
    /// All shots by source, and the ones that drew a shooting foul.
    foul_src: [(u32, u32); 9],
    foul_shots: [u32; 8],
    /// Tip-ins made and missed attempts counted by their own board.
    tip_ins: (u32, u32),
    /// Attempts and makes by how the shooter's body was, per kind of shot.
    hows: [[(u32, u32); 6]; 3],
}

/// `basketman lab stats [games]`: league-wide numbers over many games of
/// mixed clubs and tactics, with body overlap measured from the recording.
#[allow(clippy::needless_range_loop)]
pub fn stats(games: usize) {
    use basketman_core::engine::contact::radius;
    use basketman_core::replay::TurnoverKind;
    let w = World::generate(7, 12);
    let n = w.teams.len();
    let mut t = Totals::default();
    // Quartiles of how well a player gets himself fouled.
    let score = |p: &Player| {
        0.25 * p.attrs[Attr::Handling]
            + 0.25 * p.attrs[Attr::Strength]
            + 0.2 * p.attrs[Attr::CloseShot]
            + 0.15 * p.attrs[Attr::Composure]
            + 0.15 * p.attrs[Attr::Iq]
    };
    let mut sorted: Vec<f32> = w.players.iter().map(&score).collect();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let cuts = [
        sorted[sorted.len() / 4],
        sorted[sorted.len() / 2],
        sorted[sorted.len() * 3 / 4],
    ];
    let quartile = |p: &Player| cuts.iter().filter(|c| score(p) >= **c).count();
    for g in 0..games {
        let (a, b) = (g % n, (g * 5 + 1 + g / n) % n);
        if a == b {
            continue;
        }
        let (mut ta, mut tb) = (w.teams[a].tactics().clone(), w.teams[b].tactics().clone());
        if g % 3 != 0 {
            ta.offense = OffenseSet::ALL[(g / 3) % OffenseSet::ALL.len()];
            tb.offense = OffenseSet::ALL[(g / 3 + 2) % OffenseSet::ALL.len()];
            ta.defense = DefenseScheme::ALL[(g / 2) % DefenseScheme::ALL.len()];
            tb.defense = DefenseScheme::ALL[(g / 2 + 1) % DefenseScheme::ALL.len()];
        }
        if g % 7 == 0 {
            tb.press = Press::FullCourt;
        }
        let la = Lineup {
            tactics: &ta,
            players: line(&w, &ta),
            familiarity: 1.0,
            belief: 0.55,
            coach: 0,
        };
        let lb = Lineup {
            tactics: &tb,
            players: line(&w, &tb),
            familiarity: 1.0,
            belief: 0.55,
            coach: 0,
        };
        let mut rng = Rng::new(900 + g as u64 * 7919);
        let t0 = std::time::Instant::now();
        let (r, rec) = simulate_recorded(&la, &lb, &w.rules, &mut rng);
        t.secs += t0.elapsed().as_secs_f64();
        t.games += 1;
        t.possessions += (r.possessions[0] + r.possessions[1]) as u32;
        for side in 0..2 {
            for (_, l) in &r.boxes[side] {
                t.line.add(l);
            }
        }
        t.margins.push(r.score[0].abs_diff(r.score[1]) as u32);
        t.stuck += rec.stuck;
        for side in 0..2 {
            for (id, l) in &r.boxes[side] {
                let q = quartile(&w.players[*id as usize]);
                t.drawn[q].0 += l.fta as u64;
                t.drawn[q].1 += l.secs as u64;
                t.pf_by_quartile[q].0 += l.pf as u64;
                t.pf_by_quartile[q].1 += l.secs as u64;
            }
        }
        for (a, b) in t.charge_src.iter_mut().zip(rec.charges) {
            *a += b;
        }
        for side in 0..2 {
            for (i, (id, _)) in r.boxes[side].iter().enumerate() {
                let kg = w.players[*id as usize].body.weight_kg;
                let class = if kg < 85.0 {
                    0
                } else if kg < 100.0 {
                    1
                } else {
                    2
                };
                if let (Some(c), Some(n)) = (rec.leaned[side].get(i), rec.leaned_ticks[side].get(i))
                {
                    t.leaned[class].0 += *c as f64;
                    t.leaned[class].1 += *n as u64;
                }
            }
        }
        for e in &rec.events {
            match &e.kind {
                EventKind::Shot {
                    kind,
                    source,
                    made,
                    fouled,
                    how,
                    ..
                } => {
                    let ki = match kind {
                        ShotKind::Rim => 0,
                        ShotKind::Mid => 1,
                        ShotKind::Three => 2,
                    };
                    if !(*fouled && !*made) {
                        let h = &mut t.hows[ki][*how as usize];
                        h.0 += 1;
                        h.1 += *made as u32;
                    }
                    t.foul_src[*source as usize].0 += 1;
                    t.foul_src[*source as usize].1 += *fouled as u32;
                    if *fouled {
                        t.fouled[ki].0 += 1;
                        t.fouled[ki].1 += *made as u32;
                    }
                    if *fouled && !*made {
                        continue;
                    }
                    let s = &mut t.shots[match kind {
                        ShotKind::Rim => 0,
                        ShotKind::Mid => 1,
                        ShotKind::Three => 2,
                    }];
                    s.0 += 1;
                    s.1 += *made as u32;
                    t.by_kind[*source as usize][match kind {
                        ShotKind::Rim => 0,
                        ShotKind::Mid => 1,
                        ShotKind::Three => 2,
                    }] += 1;
                    let by = &mut t.sources[*source as usize];
                    by.0 += 1;
                    by.1 += *made as u32;
                    by.2 += if *made { kind.points() as u32 } else { 0 };
                }
                EventKind::Rebound { offensive, .. } => {
                    t.rebounds.0 += 1;
                    t.rebounds.1 += *offensive as u32;
                }
                EventKind::Foul { kind, on, shots, .. } => {
                    t.fouls[*kind as usize] += 1;
                    t.foul_shots[*kind as usize] += *shots as u32;
                    if *kind == basketman_core::replay::FoulKind::Charging {
                        t.charges.push((*on, g as u32));
                    }
                }
                EventKind::Turnover { kind, .. } => {
                    t.tov_kinds[match kind {
                        TurnoverKind::BadPass => 0,
                        TurnoverKind::Stripped => 1,
                        TurnoverKind::OffensiveFoul => 2,
                        TurnoverKind::ShotClock => 3,
                        TurnoverKind::Pressed => 4,
                        TurnoverKind::ThreeSeconds => 5,
                    }] += 1;
                }
                _ => {}
            }
        }
        for (i, e) in rec.events.iter().enumerate() {
            if let EventKind::Shot { who, made, source, fouled, .. } = &e.kind
                && *source == Source::Putback
                && *made
                && i > 0
                && matches!(&rec.events[i - 1].kind, EventKind::Rebound { who: w, offensive: true } if w == who)
                && (rec.events[i - 1].time - e.time).abs() < 0.05
            {
                t.tip_ins.0 += 1;
                t.tip_ins.1 += *fouled as u32;
            }
        }
        {
            let ev = &rec.events;
            let live = |k: &EventKind| matches!(k, EventKind::Shot { .. } | EventKind::Turnover { .. } | EventKind::Foul { .. } | EventKind::PeriodEnd);
            for (i, e) in ev.iter().enumerate() {
                if let EventKind::Shot { made, fouled, .. } = &e.kind {
                    if *fouled {
                        continue;
                    }
                    let next = ev[i + 1..].iter().find(|n| live(&n.kind) || matches!(n.kind, EventKind::Rebound { .. }));
                    let Some(n) = next else { continue };
                    if n.period != e.period {
                        continue;
                    }
                    if *made {
                        if live(&n.kind) {
                            t.gaps[2].0 += (n.time - e.time) as f64;
                            t.gaps[2].1 += 1;
                        }
                    } else if let EventKind::Rebound { offensive, .. } = &n.kind {
                        let g = &mut t.gaps[*offensive as usize];
                        g.0 += (n.time - e.time) as f64;
                        g.1 += 1;
                        // And on to what happens next.
                        if let Some(m) = ev[i + 1..].iter().skip(1).find(|m| live(&m.kind)) && m.period == e.period {
                            let g = &mut t.gaps[3 + *offensive as usize];
                            g.0 += (m.time - n.time) as f64;
                            g.1 += 1;
                        }
                    }
                }
            }
        }
        // Body overlap: every pair of the ten, every frame.
        for f in &rec.frames {
            t.frames += 1;
            let mut all = [(basketman_core::court::Vec2::ZERO, 0.0f32); 10];
            for s in 0..2 {
                for k in 0..5 {
                    let id = r.boxes[s][f.who[s][k] as usize].0 as usize;
                    all[s * 5 + k] = (f.pos[s][k], radius(&w.players[id].body));
                }
            }
            for i in 0..10 {
                for j in (i + 1)..10 {
                    t.pair_frames += 1;
                    let pen = all[i].1 + all[j].1 - all[i].0.dist(all[j].0);
                    if pen > 0.0 {
                        t.overlaps += 1;
                    }
                    if pen > 0.02 {
                        t.deep += 1;
                    }
                    t.worst = t.worst.max(pen);
                }
            }
        }
    }
    let g = t.games.max(1) as f32;
    let p = t.possessions.max(1) as f32;
    let l = &t.line;
    let fga = l.fga.max(1) as f32;
    let per100 = |x: f32| 100.0 * x / p;
    println!("games {}  possessions/game {:.1}  FGA/game {:.1}  fouled/FGA {:.2}%  FTA/game {:.1}", t.games, p / g, l.fga as f32 / g, 100.0 * t.fouled.iter().map(|f| f.0).sum::<u32>() as f32 / fga, l.fta as f32 / g);
    println!(
        "points/100 poss {:.2}  FG% {:.2}  3PA rate {:.2}%  FTA/100 {:.2}  FT% {:.2}",
        per100(l.pts as f32),
        100.0 * l.fgm as f32 / fga,
        100.0 * l.tpa as f32 / fga,
        per100(l.fta as f32),
        100.0 * l.ftm as f32 / l.fta.max(1) as f32
    );
    let total: u32 = t.shots.iter().map(|s| s.0).sum();
    for (name, s) in ["rim", "mid", "three"].iter().zip(t.shots) {
        println!(
            "  {name:<6} share {:.2}%  make {:.2}%",
            100.0 * s.0 as f32 / total.max(1) as f32,
            100.0 * s.1 as f32 / s.0.max(1) as f32
        );
    }
    println!(
        "PF/100 {:.2}  TOV/100 {:.2}  STL/100 {:.2}  BLK/100 {:.2}  AST/FGM {:.3}  ORB% {:.2}",
        per100(l.pf as f32),
        per100(l.tov as f32),
        per100(l.stl as f32),
        per100(l.blk as f32),
        l.ast as f32 / l.fgm.max(1) as f32,
        100.0 * l.orb as f32 / (l.orb + l.drb).max(1) as f32
    );
    println!(
        "turnovers/game: badpass {:.2} stripped {:.2} offfoul {:.2} shotclock {:.3} pressed {:.2}",
        t.tov_kinds[0] as f32 / g,
        t.tov_kinds[1] as f32 / g,
        t.tov_kinds[2] as f32 / g,
        t.tov_kinds[3] as f32 / g,
        t.tov_kinds[4] as f32 / g
    );
    println!("make % by how the body was (attempts, rim / mid / three / all):");
    for (i, how) in ShotHow::ALL.iter().enumerate() {
        let (mut a, mut m) = (0, 0);
        let mut line = String::new();
        for ki in 0..3 {
            let (n, k) = t.hows[ki][i];
            a += n;
            m += k;
            line += &format!(" {:>5.1}% of {:>6} |", 100.0 * k as f32 / n.max(1) as f32, n);
        }
        println!(
            "  {:<16}{}  all {:.1}% of {}",
            how.label(),
            line,
            100.0 * m as f32 / a.max(1) as f32,
            a
        );
    }
    let stotal: u32 = t.sources.iter().map(|s| s.0).sum();
    print!("sources (share, pts/shot):");
    for (s, c) in Source::ALL.iter().zip(t.sources) {
        print!(
            " {} {:.1}% {:.2};",
            s.label(),
            100.0 * c.0 as f32 / stotal.max(1) as f32,
            c.2 as f32 / c.0.max(1) as f32
        );
    }
    println!();
    print!("rim/mid/three % by source:");
    for (s, k) in Source::ALL.iter().zip(t.by_kind) {
        let n = k.iter().sum::<u32>().max(1) as f32;
        print!(
            " {} {:.0}/{:.0}/{:.0};",
            s.label(),
            100.0 * k[0] as f32 / n,
            100.0 * k[1] as f32 / n,
            100.0 * k[2] as f32 / n
        );
    }
    println!();
    t.margins.sort();
    let at = |q: f32| t.margins[((t.margins.len() - 1) as f32 * q) as usize];
    println!("winning margin median {} p90 {}", at(0.5), at(0.9));
    println!(
        "overlap: {} of {} pair-frames ({:.4}%), deeper than 2 cm {}, worst {:.3} m, in {} frames",
        t.overlaps,
        t.pair_frames,
        100.0 * t.overlaps as f64 / t.pair_frames.max(1) as f64,
        t.deep,
        t.worst,
        t.frames
    );
    {
        let (n, m): (u32, u32) = t.fouled.iter().fold((0, 0), |a, f| (a.0 + f.0, a.1 + f.1));
        println!(
            "fouled shots {:.2}/game, and-ones {:.2}/game ({:.1}% of them go in; {:.1}% of makes); rim {}/{} mid {}/{} three {}/{}",
            n as f32 / g,
            m as f32 / g,
            100.0 * m as f32 / n.max(1) as f32,
            100.0 * m as f32 / l.fgm.max(1) as f32,
            t.fouled[0].1, t.fouled[0].0, t.fouled[1].1, t.fouled[1].0, t.fouled[2].1, t.fouled[2].0
        );
        let put = t.sources[Source::Putback as usize];
        println!(
            "rebounds {:.1}/game, offensive {:.1}; putback shots {:.2}/game = {:.0}% of offensive boards, {:.2}% of shots",
            t.rebounds.0 as f32 / g,
            t.rebounds.1 as f32 / g,
            put.0 as f32 / g,
            100.0 * put.0 as f32 / t.rebounds.1.max(1) as f32,
            100.0 * put.0 as f32 / stotal.max(1) as f32
        );
    }
    {
        let m = |i: usize| t.gaps[i].0 / t.gaps[i].1.max(1) as f64;
        println!(
            "seconds: miss to defensive board {:.2} ({}), to offensive board {:.2} ({}); make to next event {:.2}; defensive board to next event {:.2}, offensive board to next {:.2}",
            m(0), t.gaps[0].1, m(1), t.gaps[1].1, m(2), m(3), m(4)
        );
    }
    println!(
        "tip-ins made {:.2}/game ({:.2}/game of them fouled)",
        t.tip_ins.0 as f32 / g,
        t.tip_ins.1 as f32 / g
    );
    print!("shooting fouls per shot by source:");
    for (s, c) in Source::ALL.iter().zip(t.foul_src) {
        print!(" {} {:.1}% of {:.1}/game;", s.label(), 100.0 * c.1 as f32 / c.0.max(1) as f32, c.0 as f32 / g);
    }
    println!();
    print!("free throws awarded per game by foul kind:");
    for (k, n) in basketman_core::replay::FoulKind::ALL.iter().zip(t.foul_shots) {
        print!(" {} {:.2};", k.label(), n as f32 / g);
    }
    println!();
    print!("fouls per game by kind:");
    for (k, n) in basketman_core::replay::FoulKind::ALL.iter().zip(t.fouls) {
        print!(" {} {:.2};", k.label(), n as f32 / g);
    }
    println!();
    // Charges drawn are meant to be rare and concentrated in a few men.
    {
        let mut by: std::collections::BTreeMap<u32, u32> = Default::default();
        for (p, _) in &t.charges {
            *by.entry(*p).or_default() += 1;
        }
        let mut v: Vec<u32> = by.values().copied().collect();
        v.sort_by(|a, b| b.cmp(a));
        let total: u32 = v.iter().sum();
        let top: u32 = v.iter().take(5).sum();
        println!(
            "charges drawn {:.3} per team game; {} players drew any; the top five drew {:.0}% of them",
            total as f32 / (2.0 * g),
            v.len(),
            100.0 * top as f32 / total.max(1) as f32
        );
    }
    println!(
        "charges called per game: stood in the way on purpose {:.2} ({:.2} attempts), happened to be set {:.2}, sold and bought {:.2} (sold and not bought {:.2})",
        t.charge_src[0] as f32 / g,
        t.charge_src[3] as f32 / g,
        t.charge_src[1] as f32 / g,
        t.charge_src[2] as f32 / g,
        t.charge_src[4] as f32 / g
    );
    print!(
        "free throws and fouls per 36 minutes by how well the player draws fouls (lowest to highest quartile):"
    );
    for (d, f) in t.drawn.iter().zip(t.pf_by_quartile) {
        print!(
            " {:.2} FTA / {:.2} PF;",
            d.0 as f32 * 2160.0 / d.1.max(1) as f32,
            f.0 as f32 * 2160.0 / f.1.max(1) as f32
        );
    }
    println!();
    print!("contact fatigue per on-court minute (natural about 0.06):");
    for (name, (c, n)) in ["light", "middle", "heavy"].iter().zip(t.leaned) {
        print!(" {name} {:.4}", c / (n.max(1) as f64 / 600.0));
    }
    println!();
    println!(
        "stuck episodes {} ({:.3}/game)   shot-clock violations/game {:.3}",
        t.stuck,
        t.stuck as f32 / g,
        t.tov_kinds[3] as f32 / g
    );
    println!(
        "speed {:.1} games/s ({} games in {:.1}s of simulation)",
        t.games as f64 / t.secs,
        t.games,
        t.secs
    );
}

/// Mean contest over all shots of a cell.
fn mean_contest(c: &Cell) -> f32 {
    let n: u32 = c.shots.iter().map(|s| s.0).sum();
    c.shots.iter().map(|s| s.2).sum::<f32>() / n.max(1) as f32
}

/// `basketman lab physics [games]`: the same squads with the bodies of one
/// club's players changed, to show that size and strength matter.
pub fn physics(games: usize) {
    let w = World::generate(7, 12);
    println!("{}", noise(games));
    println!("\nBall screens: bigs wider and heavier (+4 cm shoulders, +20 kg)");
    let wide = altered(&w, |p| {
        if is_big(p) {
            p.body.shoulder_width_cm += 4.0;
            p.body.weight_kg += 20.0;
        }
    });
    let slight = altered(&w, |p| {
        if is_big(p) {
            p.body.shoulder_width_cm -= 3.0;
            p.body.weight_kg -= 12.0;
        }
    });
    for d in [DefenseScheme::ManToMan, DefenseScheme::PackLine] {
        let run = |wa: &World| {
            duel(
                wa,
                &w,
                &|a, b| {
                    a.offense = OffenseSet::PickAndRoll;
                    b.defense = d;
                },
                games,
                31,
                1.0,
            )
        };
        let (a, b) = (run(&slight), run(&wide));
        let look = |c: &Cell| {
            (
                c.screen_shots.1 / c.screen_shots.0.max(1) as f32,
                c.screen_shots.2 as f32 / c.screen_shots.0.max(1) as f32,
            )
        };
        println!(
            "  Pick and roll v {:<12} slight {:.3} ppp contest {:.3} | wide {:.3} ppp contest {:.3}",
            d.label(),
            a.ppp(),
            mean_contest(&a),
            b.ppp(),
            mean_contest(&b)
        );
        let kinds = |c: &Cell| {
            let n = c.screen_kinds.iter().sum::<u32>().max(1) as f32;
            format!(
                "rim {:.0}% mid {:.0}% three {:.0}%",
                100.0 * c.screen_kinds[0] as f32 / n,
                100.0 * c.screen_kinds[1] as f32 / n,
                100.0 * c.screen_kinds[2] as f32 / n
            )
        };
        println!(
            "    handler's shots after the screen: {} -> {}",
            kinds(&a),
            kinds(&b)
        );
        println!(
            "    the man who was on the handler, 0.8 s after the screen: {:.2} m -> {:.2} m",
            a.screen_gap.0 / a.screen_gap.1.max(1) as f32,
            b.screen_gap.0 / b.screen_gap.1.max(1) as f32
        );
        println!(
            "    handler beats his man off the screen {:.1}% -> {:.1}%; his shot after it: contest {:.3} -> {:.3}, points per shot {:.3} -> {:.3} ({} and {} screens)",
            100.0 * a.screen_beaten as f32 / a.screens.max(1) as f32,
            100.0 * b.screen_beaten as f32 / b.screens.max(1) as f32,
            look(&a).0,
            look(&b).0,
            look(&a).1,
            look(&b).1,
            a.screens,
            b.screens
        );
    }
}

/// Post play and rebounding with big men who are heavier and stronger.
pub fn physics_post(games: usize) {
    let w = World::generate(7, 12);
    println!("\nPost play and rebounding: bigs +20 kg and +4 Strength v the same bigs as they are");
    let heavy = altered(&w, |p| {
        if is_big(p) {
            p.body.weight_kg += 20.0;
            p.attrs[Attr::Strength] += 4.0;
        }
    });
    let run = |wa: &World| {
        duel(
            wa,
            &w,
            &|a, b| {
                a.offense = OffenseSet::PostUp;
                b.defense = DefenseScheme::ManToMan;
            },
            games,
            41,
            1.0,
        )
    };
    let heavy_only = altered(&w, |p| {
        if is_big(p) {
            p.body.weight_kg += 20.0;
        }
    });
    let (a, b, c2) = (run(&w), run(&heavy), run(&heavy_only));
    let post = |c: &Cell| {
        let s = c.sources[Source::Post as usize];
        (
            s.0 as f32 / c.possessions.max(1) as f32 * 100.0,
            s.2 as f32 / s.0.max(1) as f32,
            100.0 * s.1 as f32 / s.0.max(1) as f32,
        )
    };
    let orb =
        |c: &Cell| 100.0 * c.line.orb as f32 / (c.line.orb as f32 + c.opp.drb as f32).max(1.0);
    println!(
        "  post shots per 100 possessions {:.1} -> {:.1}; points per post shot {:.3} -> {:.3}; make {:.1}% -> {:.1}%",
        post(&a).0,
        post(&b).0,
        post(&a).1,
        post(&b).1,
        post(&a).2,
        post(&b).2
    );
    println!(
        "  points per possession {:.3} -> {:.3}; offensive rebound rate {:.1}% -> {:.1}%",
        a.ppp(),
        b.ppp(),
        orb(&a),
        orb(&b)
    );
    println!(
        "  weight alone (+20 kg, Strength unchanged): post shots {:.1} -> {:.1}; points per post shot {:.3} -> {:.3}; points per possession {:.3} -> {:.3}; offensive rebound rate {:.1}% -> {:.1}%",
        post(&a).0,
        post(&c2).0,
        post(&a).1,
        post(&c2).1,
        a.ppp(),
        c2.ppp(),
        orb(&a),
        orb(&c2)
    );
}

/// What a body in the lane does to a drive: shots off drives split by how
/// the drive met the defense.
pub fn physics_lane(games: usize) {
    let w = World::generate(7, 12);
    println!(
        "\nShots off drives by how the drive met the defense (Isolation and Five out, man to man and pack line)"
    );
    let mut sum = [[0.0f32; 4]; 3];
    for set in [OffenseSet::Isolation, OffenseSet::FiveOut] {
        for d in [DefenseScheme::ManToMan, DefenseScheme::PackLine] {
            let c = cell(
                &w,
                &|a, b| {
                    a.offense = set;
                    b.defense = d;
                },
                games,
                53,
            );
            for (a, b) in sum.iter_mut().zip(c.drive) {
                for (x, y) in a.iter_mut().zip(b) {
                    *x += y;
                }
            }
        }
    }
    let total: f32 = sum.iter().map(|d| d[0]).sum();
    for (name, d) in ["clean drive", "met a late helper", "stopped by a set body"]
        .iter()
        .zip(sum)
    {
        println!(
            "  {name:<22} {:>5.1}% of drive shots, {:>4.0}% at the rim, contest {:.2}, {:.3} points each",
            100.0 * d[0] / total.max(1.0),
            100.0 * d[1] / d[0].max(1.0),
            d[2] / d[0].max(1.0),
            d[3] / d[0].max(1.0)
        );
    }
}

/// Leaning on a man all night: the same guards against ordinary and against
/// heavy, strong defenders.
pub fn physics_tiring(games: usize) {
    let w = World::generate(7, 12);
    println!("\nGuards guarded by ordinary defenders and by defenders +15 kg and +3 Strength");
    let heavy = altered(&w, |p| {
        p.body.weight_kg += 15.0;
        p.attrs[Attr::Strength] += 3.0;
    });
    let run = |wd: &World| {
        duel(
            &w,
            wd,
            &|a, b| {
                a.offense = OffenseSet::Isolation;
                b.defense = DefenseScheme::ManToMan;
            },
            games,
            61,
            1.0,
        )
    };
    let (a, b) = (run(&w), run(&heavy));
    let per = |c: &Cell| c.guards_leaned.0 / (c.guards_leaned.1.max(1) as f64 / 600.0);
    println!(
        "  contact fatigue per on-court minute of the guards {:.4} -> {:.4} (natural about 0.06)",
        per(&a),
        per(&b)
    );
}

/// What a batch of games with one side defending and the other attacking
/// shows about the whistle.
#[derive(Default)]
struct Calls {
    games: u32,
    poss: u32,
    /// Fouls committed by the defending side, by kind.
    by_kind: [u32; 8],
    /// Charges drawn by the defenders, blocking fouls they committed.
    charges: u32,
    blocking: u32,
    /// Free throws the attackers took.
    fta: u32,
    /// Sold and bought, sold and not bought.
    flops: (u32, u32),
    /// Per game: (whistle, defensive fouls, free throws).
    per_game: Vec<(f32, u32, u32)>,
}

impl Calls {
    fn fouls(&self) -> u32 {
        self.by_kind
            .iter()
            .enumerate()
            .filter(|(i, _)| !basketman_core::replay::FoulKind::ALL[*i].is_offensive())
            .map(|(_, n)| *n)
            .sum()
    }
}

/// Play `games` games with squads `wa` attacking and `wd` defending,
/// counting what the defending side did with its hands and body.
fn calls(
    wa: &World,
    wd: &World,
    tune: &dyn Fn(&mut Tactics, &mut Tactics),
    games: usize,
    seed: u64,
) -> Calls {
    use basketman_core::replay::FoulKind;
    let mut out = Calls::default();
    let n = wd.teams.len();
    for g in 0..games {
        let (a, b) = (g % n, (g * 5 + 1 + g / n) % n);
        if a == b {
            continue;
        }
        let (mut ta, mut tb) = (wa.teams[a].tactics().clone(), wd.teams[b].tactics().clone());
        tune(&mut ta, &mut tb);
        let la = Lineup {
            tactics: &ta,
            players: line(wa, &ta),
            familiarity: 1.0,
            belief: 0.55,
            coach: 0,
        };
        let lb = Lineup {
            tactics: &tb,
            players: line(wd, &tb),
            familiarity: 1.0,
            belief: 0.55,
            coach: 0,
        };
        let side = g % 2;
        let mut rng = Rng::new(seed.wrapping_add(g as u64 * 7919));
        let (r, rec) = if side == 0 {
            simulate_recorded(&la, &lb, &wd.rules, &mut rng)
        } else {
            simulate_recorded(&lb, &la, &wd.rules, &mut rng)
        };
        out.games += 1;
        out.poss += r.possessions[side] as u32;
        out.flops.0 += rec.charges[2];
        out.flops.1 += rec.charges[4];
        let (mut pf, mut fta) = (0, 0);
        for e in &rec.events {
            if e.offense as usize != side {
                continue;
            }
            if let EventKind::Foul { kind, .. } = &e.kind {
                out.by_kind[*kind as usize] += 1;
                if *kind == FoulKind::Charging {
                    out.charges += 1;
                }
                if *kind == FoulKind::Blocking {
                    out.blocking += 1;
                }
                if !kind.is_offensive() {
                    pf += 1;
                }
            }
        }
        for (_, l) in &r.boxes[side] {
            out.fta += l.fta as u32;
            fta += l.fta as u32;
        }
        out.per_game.push((rec.whistle, pf, fta));
    }
    out
}

/// `basketman lab fouls [games]`: fouls and contact within the rules.
pub fn fouls(games: usize) {
    let w = World::generate(7, 12);
    println!("{}", noise(games));
    let per = |n: u32, c: &Calls| n as f32 / c.games.max(1) as f32;
    let clamp = |p: &mut Player| {
        for a in Attr::ALL {
            p.attrs[a] = p.attrs[a].clamp(1.0, 20.0);
        }
    };
    let bump = |x: u8, d: i32| (x as i32 + d).clamp(1, 20) as u8;

    println!(
        "\nDefenders with more head: IQ +5, Composure +5, Interior D +4, Perimeter D +3, Work rate +3, Determination +5"
    );
    let sharp = altered(&w, |p| {
        p.attrs[Attr::Iq] += 5.0;
        p.attrs[Attr::Composure] += 5.0;
        p.attrs[Attr::InteriorD] += 4.0;
        p.attrs[Attr::PerimeterD] += 3.0;
        p.attrs[Attr::WorkRate] += 3.0;
        p.personality.determination = bump(p.personality.determination, 5);
    });
    let tune = |_: &mut Tactics, _: &mut Tactics| {};
    let (a, b) = (
        calls(&w, &w, &tune, games, 71),
        calls(&w, &sharp, &tune, games, 71),
    );
    println!(
        "  charges drawn per game {:.2} -> {:.2}; blocking fouls committed {:.2} -> {:.2}; defensive fouls per game {:.1} -> {:.1}",
        per(a.charges, &a),
        per(b.charges, &b),
        per(a.blocking, &a),
        per(b.blocking, &b),
        per(a.fouls(), &a),
        per(b.fouls(), &b)
    );

    println!(
        "\nStrong defenders (+4 Strength): disciplined (IQ +3, Composure +4, Determination +4, Temperament +4) or hot-tempered (IQ -3, Composure -6, Determination -6, Temperament -8)"
    );
    let make = |iq: f32, comp: f32, det: i32, temp: i32| {
        altered(&w, |p| {
            p.attrs[Attr::Strength] += 4.0;
            p.attrs[Attr::Iq] += iq;
            p.attrs[Attr::Composure] += comp;
            p.personality.determination = bump(p.personality.determination, det);
            p.personality.temperament = bump(p.personality.temperament, temp);
            clamp(p);
        })
    };
    let (calm, hot) = (make(3.0, 4.0, 4, 4), make(-3.0, -6.0, -6, -8));
    let (a, b) = (
        calls(&w, &calm, &tune, games, 73),
        calls(&w, &hot, &tune, games, 73),
    );
    println!(
        "  defensive fouls per game {:.1} -> {:.1} (fouls per 100 possessions {:.1} -> {:.1}); free throws given up {:.1} -> {:.1}",
        per(a.fouls(), &a),
        per(b.fouls(), &b),
        100.0 * a.fouls() as f32 / a.poss.max(1) as f32,
        100.0 * b.fouls() as f32 / b.poss.max(1) as f32,
        per(a.fta, &a),
        per(b.fta, &b)
    );

    println!("\nThe defense's aggression slider (4, 10, 16)");
    for agg in [4u8, 10, 16] {
        let c = calls(&w, &w, &|_, b| b.aggression = agg, games, 75);
        println!(
            "  aggression {agg:>2}: defensive fouls per game {:.1}, free throws {:.1}",
            per(c.fouls(), &c),
            per(c.fta, &c)
        );
    }

    println!("\nThe crew's whistle (drawn per game, 0.85 loose to 1.15 tight), games in thirds");
    let c = calls(&w, &w, &tune, games * 3, 77);
    let mut pg = c.per_game.clone();
    pg.sort_by(|x, y| x.0.total_cmp(&y.0));
    for (name, part) in ["loose", "middle", "tight"]
        .iter()
        .zip(pg.chunks(pg.len().div_ceil(3)))
    {
        let k = part.len().max(1) as f32;
        println!(
            "  {name:<7} whistle {:.2}: defensive fouls per game {:.1}, free throws {:.1}",
            part.iter().map(|x| x.0).sum::<f32>() / k,
            part.iter().map(|x| x.1 as f32).sum::<f32>() / k,
            part.iter().map(|x| x.2 as f32).sum::<f32>() / k
        );
    }

    println!(
        "\nSelling contact: players who sell (Professionalism -8, Temperament -8) and players who do not (+6, +6)"
    );
    let sell = altered(&w, |p| {
        p.personality.professionalism = bump(p.personality.professionalism, -8);
        p.personality.temperament = bump(p.personality.temperament, -8);
    });
    let honest = altered(&w, |p| {
        p.personality.professionalism = bump(p.personality.professionalism, 6);
        p.personality.temperament = bump(p.personality.temperament, 6);
    });
    let (a, b) = (
        calls(&w, &honest, &tune, games, 79),
        calls(&w, &sell, &tune, games, 79),
    );
    println!(
        "  contact sold and bought per game {:.2} -> {:.2}; sold and not bought (defender on the floor, play goes on) {:.2} -> {:.2}",
        per(a.flops.0, &a),
        per(b.flops.0, &b),
        per(a.flops.1, &a),
        per(b.flops.1, &b)
    );
}

/// `basketman lab mix [games]`: shot and source mix against each defense.
pub fn mix(games: usize) {
    let w = World::generate(7, 12);
    for d in DefenseScheme::ALL {
        let mut all = [[0u32; 3]; 2];
        let mut src = [[(0u32, 0u32); 9]; 2];
        let mut pts = [(0u32, 0u32); 2];
        let mut fouls = [(0u32, 0u32); 2];
        for o in OffenseSet::ALL {
            let c = cell(
                &w,
                &|a, b| {
                    a.offense = o;
                    b.defense = d;
                },
                games,
                11,
            );
            for (i, on) in [true, o == OffenseSet::Motion].into_iter().enumerate() {
                if !on {
                    continue;
                }
                for k in 0..3 {
                    all[i][k] += c.shots[k].0;
                }
                for k in 0..9 {
                    src[i][k].0 += c.sources[k].0;
                    src[i][k].1 += c.sources[k].2;
                }
                pts[i].0 += c.line.pts as u32;
                pts[i].1 += c.possessions;
                fouls[i].0 += c.opp.pf as u32;
                fouls[i].1 += c.possessions;
            }
        }
        for (i, name) in ["all   ", "motion"].iter().enumerate() {
            let n = all[i].iter().sum::<u32>().max(1) as f32;
            let sn = src[i].iter().map(|s| s.0).sum::<u32>().max(1) as f32;
            print!(
                "{:<14} {name} rim {:>4.1} mid {:>4.1} three {:>4.1} | ppp {:.3} pf/100 {:>4.1} |",
                d.label(),
                100.0 * all[i][0] as f32 / n,
                100.0 * all[i][1] as f32 / n,
                100.0 * all[i][2] as f32 / n,
                pts[i].0 as f32 / pts[i].1.max(1) as f32,
                100.0 * fouls[i].0 as f32 / fouls[i].1.max(1) as f32,
            );
            for (s, k) in Source::ALL.iter().zip(src[i]) {
                print!(" {:.8} {:.1}", s.label(), 100.0 * k.0 as f32 / sn);
            }
            println!();
        }
    }
}

/// `basketman lab zone [seeds]`: the zone test's setup, with the shot mix by
/// source against man to man and the 2-3 zone.
pub fn zone(seeds: usize) {
    use basketman_core::league::lineup;
    let w = World::generate(9, 2);
    println!("offense of team 0: {:?}", w.teams[0].tactics().offense);
    for d in [DefenseScheme::ManToMan, DefenseScheme::Zone23] {
        let home = lineup(&w.teams[0], &w.players);
        let mut tactics = w.teams[1].tactics().clone();
        tactics.defense = d;
        let mut away = lineup(&w.teams[1], &w.players);
        away.tactics = &tactics;
        let mut n = [0u32; 3];
        let mut src = [0u32; 9];
        let mut pts = 0u32;
        let mut poss = 0u32;
        for seed in 0..seeds as u64 {
            let (r, rec) = simulate_recorded(&home, &away, &w.rules, &mut Rng::new(seed));
            pts += r.score[0] as u32;
            poss += r.possessions[0] as u32;
            for e in rec.events.iter().filter(|e| e.offense == 0) {
                if let EventKind::Shot { kind, source, .. } = &e.kind {
                    n[match kind {
                        ShotKind::Rim => 0,
                        ShotKind::Mid => 1,
                        ShotKind::Three => 2,
                    }] += 1;
                    src[*source as usize] += 1;
                }
            }
        }
        let t = n.iter().sum::<u32>() as f32;
        print!(
            "{:<14} rim {:.1} mid {:.1} three {:.1} ppp {:.3} |",
            d.label(),
            100.0 * n[0] as f32 / t,
            100.0 * n[1] as f32 / t,
            100.0 * n[2] as f32 / t,
            pts as f32 / poss as f32
        );
        for (s, c) in Source::ALL.iter().zip(src) {
            print!(" {:.6} {:.1}", s.label(), 100.0 * c as f32 / t);
        }
        println!();
    }
}

/// What a pick and roll gave the ball handler and the roll man.
fn screen_line(label: &str, c: &Cell) {
    let shots = c.screen_shots.0.max(1) as f32;
    let n = c.screen_kinds.iter().sum::<u32>().max(1) as f32;
    let poss = c.possessions.max(1) as f32;
    let roll = c.sources[Source::Roll as usize];
    let pull = c.sources[Source::PullUp as usize];
    let drive = c.sources[Source::Drive as usize];
    println!(
        "  {label:<26} ppp {:.3} | after the screen: {:.1} shots per 100 screens, contest {:.3}, {:.3} pts each, rim/mid/three {:.0}/{:.0}/{:.0}% | beaten {:.1}% gap {:.2} m | roll {:.1} per 100 poss {:.2} pts each | pull-up {:.1} {:.2} | drive {:.1} {:.2} | pf {:.1} | screens {:.0} per 100 poss, off fouls {:.1}",
        c.ppp(),
        100.0 * c.screen_shots.0 as f32 / c.screens.max(1) as f32,
        c.screen_shots.1 / shots,
        c.screen_shots.2 as f32 / shots,
        100.0 * c.screen_kinds[0] as f32 / n,
        100.0 * c.screen_kinds[1] as f32 / n,
        100.0 * c.screen_kinds[2] as f32 / n,
        100.0 * c.screen_beaten as f32 / c.screens.max(1) as f32,
        c.screen_gap.0 / c.screen_gap.1.max(1) as f32,
        100.0 * roll.0 as f32 / poss,
        roll.2 as f32 / roll.0.max(1) as f32,
        100.0 * pull.0 as f32 / poss,
        pull.2 as f32 / pull.0.max(1) as f32,
        100.0 * drive.0 as f32 / poss,
        drive.2 as f32 / drive.0.max(1) as f32,
        100.0 * c.opp.pf as f32 / poss,
        100.0 * c.screens as f32 / poss,
        100.0 * c.tov_kinds[2] as f32 / poss,
    );
}

/// `basketman lab screens [games]`: what the screener's body and the
/// defender's choice of how to get past it do for the ball handler.
pub fn screens(games: usize) {
    let w = World::generate(7, 12);
    println!("{}", noise(games));
    let tuned = |f: &dyn Fn(&mut Player)| altered(&w, |p| f(p));
    let slight = tuned(&|p| {
        if is_big(p) {
            p.body.shoulder_width_cm -= 3.0;
            p.body.weight_kg -= 12.0;
        }
    });
    let wide = tuned(&|p| {
        if is_big(p) {
            p.body.shoulder_width_cm += 4.0;
            p.body.weight_kg += 20.0;
        }
    });
    let good = tuned(&|p| {
        if is_big(p) {
            p.attrs[Attr::Strength] += 4.0;
            p.attrs[Attr::Iq] += 3.0;
        }
    });
    let run = |wa: &World, d: DefenseScheme, aggression: Option<u8>| {
        duel(
            wa,
            &w,
            &|a, b| {
                a.offense = OffenseSet::PickAndRoll;
                b.defense = d;
                if let Some(x) = aggression {
                    b.aggression = x;
                }
            },
            games,
            31,
            1.0,
        )
    };
    for d in [DefenseScheme::ManToMan, DefenseScheme::PackLine] {
        println!(
            "\nPick and roll v {}: the screener's body and skill (same squads otherwise)",
            d.label()
        );
        screen_line("slight big (-3 cm, -12 kg)", &run(&slight, d, None));
        screen_line("as they are", &run(&w, d, None));
        screen_line("wide, heavy (+4 cm, +20 kg)", &run(&wide, d, None));
        screen_line("good (+4 Str, +3 IQ)", &run(&good, d, None));
    }
    println!("\nOver and under in the same games (pick and roll, same squads)");
    for d in [DefenseScheme::ManToMan, DefenseScheme::PackLine] {
        cover_lines(d.label(), &run(&w, d, None));
    }
    println!(
        "\nPick and roll v man to man: how hard the defense fights over screens (aggression slider)"
    );
    for x in [4u8, 10, 16] {
        screen_line(
            &format!("aggression {x}"),
            &run(&w, DefenseScheme::ManToMan, Some(x)),
        );
    }
    println!("\nPick and roll v pack line: the same");
    for x in [4u8, 10, 16] {
        screen_line(
            &format!("aggression {x}"),
            &run(&w, DefenseScheme::PackLine, Some(x)),
        );
    }
}

/// `basketman lab pnr [games]`: where pick and roll possessions end.
pub fn pnr(games: usize) {
    let w = World::generate(7, 12);
    for d in DefenseScheme::ALL {
        let c = cell(
            &w,
            &|a, b| {
                a.offense = OffenseSet::PickAndRoll;
                b.defense = d;
            },
            games,
            31,
        );
        let p = c.possessions.max(1) as f32 / 100.0;
        println!(
            "PnR v {:<16} ppp {:.3} | per 100 poss: fga {:.1} fta {:.1} tov {:.1} (bad pass {:.1} stripped {:.1} off foul {:.1} clock {:.1} pressed {:.1}) pf {:.1} orb {:.1}",
            d.label(),
            c.ppp(),
            c.line.fga as f32 / p,
            c.line.fta as f32 / p,
            c.line.tov as f32 / p,
            c.tov_kinds[0] as f32 / p,
            c.tov_kinds[1] as f32 / p,
            c.tov_kinds[2] as f32 / p,
            c.tov_kinds[3] as f32 / p,
            c.tov_kinds[4] as f32 / p,
            c.opp.pf as f32 / p,
            c.line.orb as f32 / p,
        );
    }
}

/// How the ball handler fares after a screen when his man goes over it and
/// when he goes under, in the same games.
fn cover_lines(label: &str, c: &Cell) {
    for (i, name) in ["over", "under"].iter().enumerate() {
        let b = &c.by_cover[i];
        let shots = b[1].max(1.0);
        println!(
            "  {label:<22} {name:<5} {:>6.0} screens | beaten {:>4.1}% | shot {:>4.1}% of them: contest {:.3}, {:.3} pts each, rim/mid/three {:.0}/{:.0}/{:.0}%, pull-up {:.0}% drive {:.0}% of the shots",
            b[0],
            100.0 * b[4] / b[0].max(1.0),
            100.0 * b[1] / b[0].max(1.0),
            b[2] / shots,
            b[3] / shots,
            100.0 * b[5] / shots,
            100.0 * b[6] / shots,
            100.0 * b[7] / shots,
            100.0 * b[8] / shots,
            100.0 * b[9] / shots,
        );
    }
}

/// `basketman lab charges [seasons]`: who draws the charges over full
/// seasons of a twelve-club league (every club at home to every other).
pub fn charges(seasons: usize) {
    use basketman_core::league::lineup;
    use basketman_core::replay::FoulKind;
    let (mut share5, mut share10, mut lead_rate, mut mean_rate, mut per_team_game) =
        (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut low = 0.0f64;
    let mut lead_ratio = 0.0f64;
    let mut src = [0u64; 5];
    let mut all_games = 0u64;
    for s in 0..seasons {
        let w = World::generate(100 + s as u64, 12);
        let n = w.teams.len();
        let mut drawn = vec![0u32; w.players.len()];
        let mut games = vec![0u32; w.players.len()];
        let mut played = 0u32;
        for h in 0..n {
            for a in 0..n {
                if h == a {
                    continue;
                }
                played += 1;
                let (home, away) = (
                    lineup(&w.teams[h], &w.players),
                    lineup(&w.teams[a], &w.players),
                );
                let mut rng = Rng::new(1000 * s as u64 + played as u64);
                let (r, rec) = simulate_recorded(&home, &away, &w.rules, &mut rng);
                for boxes in &r.boxes {
                    for (id, l) in boxes {
                        if l.secs > 0 {
                            games[*id as usize] += 1;
                        }
                    }
                }
                all_games += 1;
                for (a, b) in src.iter_mut().zip(rec.charges) {
                    *a += b as u64;
                }
                for e in &rec.events {
                    if let EventKind::Foul { kind, on, .. } = &e.kind
                        && *kind == FoulKind::Charging
                    {
                        drawn[*on as usize] += 1;
                    }
                }
            }
        }
        let total: u32 = drawn.iter().sum();
        let mut order: Vec<usize> = (0..drawn.len()).filter(|&i| games[i] >= 20).collect();
        order.sort_by(|&a, &b| drawn[b].cmp(&drawn[a]));
        let top = |k: usize| order.iter().take(k).map(|&i| drawn[i]).sum::<u32>() as f64;
        let rate = |i: usize| drawn[i] as f64 / games[i].max(1) as f64;
        let mean = total as f64 / order.iter().map(|&i| games[i] as f64).sum::<f64>();
        let lead = rate(order[0]);
        let few =
            order.iter().filter(|&&i| rate(i) < 0.25 * mean).count() as f64 / order.len() as f64;
        println!(
            "season {s}: {:.3} charges per team game, top 5 draw {:.0}%, top 10 {:.0}%, leader {} ({:.3} a game, {:.1}x the average of {:.3}); {:.0}% of players draw under a quarter of the average",
            total as f64 / (2.0 * played as f64),
            100.0 * top(5) / total as f64,
            100.0 * top(10) / total as f64,
            drawn[order[0]],
            lead,
            lead / mean,
            mean,
            100.0 * few,
        );
        share5 += top(5) / total as f64;
        share10 += top(10) / total as f64;
        lead_rate += lead;
        mean_rate += mean;
        lead_ratio += lead / mean;
        low += few;
        per_team_game += total as f64 / (2.0 * played as f64);
    }
    let g = all_games as f64 * 2.0;
    println!(
        "per team game: planted {:.3} (attempts {:.2}), set by chance {:.3}, sold and bought {:.3}, sold and not bought {:.3}",
        src[0] as f64 / g,
        src[3] as f64 / g,
        src[1] as f64 / g,
        src[2] as f64 / g,
        src[4] as f64 / g
    );
    let k = seasons as f64;
    println!(
        "average of {seasons} seasons: {:.3} charges per team game; top 5 draw {:.0}%, top 10 {:.0}%; leaders {:.3} a game ({:.1}x the average of {:.3}); {:.0}% of players draw under a quarter of the average",
        per_team_game / k,
        100.0 * share5 / k,
        100.0 * share10 / k,
        lead_rate / k,
        lead_ratio / k,
        mean_rate / k,
        100.0 * low / k
    );
}

/// `basketman lab hunters`: how the charge urge is spread over the league.
pub fn hunters() {
    use basketman_core::engine::contact::{charge_urge, charge_will};
    let mut skill = Vec::new();
    let mut will = Vec::new();
    let mut urge = Vec::new();
    for seed in 100..106u64 {
        let w = World::generate(seed, 12);
        for t in &w.teams {
            for s in t.tactics().rotation.iter().take(10) {
                let p = &w.players[s.player as usize];
                let a = |x: Attr| p.attrs[x];
                let sk = (0.3 * a(Attr::Iq)
                    + 0.2 * a(Attr::Composure)
                    + 0.2 * p.personality.determination as f32
                    + 0.15 * a(Attr::WorkRate)
                    + 0.15 * a(Attr::PerimeterD))
                    / 20.0;
                skill.push(sk);
                will.push(charge_will(&p.personality));
                urge.push(charge_urge(sk, &p.personality));
            }
        }
    }
    let q = |v: &mut Vec<f32>| {
        v.sort_by(|a, b| a.total_cmp(b));
        [0.05, 0.25, 0.5, 0.75, 0.9, 0.95, 0.99]
            .iter()
            .map(|&f| format!("{:.2}", v[((v.len() - 1) as f32 * f) as usize]))
            .collect::<Vec<_>>()
            .join(" ")
    };
    println!("players {}", skill.len());
    println!("skill  p5 p25 p50 p75 p90 p95 p99: {}", q(&mut skill));
    println!("will   p5 p25 p50 p75 p90 p95 p99: {}", q(&mut will));
    println!("urge   p5 p25 p50 p75 p90 p95 p99: {}", q(&mut urge));
}

/// `basketman lab post [games]`: what mass alone does in the post, for the
/// man with the ball and for the man guarding him.
pub fn post(games: usize) {
    let w = World::generate(7, 12);
    println!("{}", noise(games));
    let with = |kg: f32, strength: f32| {
        altered(&w, |p| {
            if is_big(p) {
                p.body.weight_kg += kg;
                p.attrs[Attr::Strength] += strength;
            }
        })
    };
    let run = |wa: &World, wd: &World, d: DefenseScheme| {
        duel(
            wa,
            wd,
            &|a, b| {
                a.offense = OffenseSet::PostUp;
                b.defense = d;
            },
            games,
            41,
            1.0,
        )
    };
    let line = |label: &str, c: &Cell| {
        let s = c.sources[Source::Post as usize];
        let p = c.possessions.max(1) as f32;
        println!(
            "  {label:<34} ground {:+.3} m a move, won {:.1}% | post shots {:.1}/100 poss, {:.3} pts each, make {:.1}%, contest {:.3}, at the rim {:.0}% | ppp {:.3}",
            c.post[1] / c.post[0].max(1.0),
            100.0 * c.post[2] / c.post[0].max(1.0),
            100.0 * s.0 as f32 / p,
            s.2 as f32 / s.0.max(1) as f32,
            100.0 * s.1 as f32 / s.0.max(1) as f32,
            c.post_shots[0] / s.0.max(1) as f32,
            100.0 * c.post_shots[1] / s.0.max(1) as f32,
            c.ppp(),
        );
    };
    let schemes = [DefenseScheme::ManToMan, DefenseScheme::PackLine];
    for d in schemes {
        println!("\nPost up v {}: the big man with the ball", d.label());
        line("as they are", &run(&w, &w, d));
        line("+20 kg", &run(&with(20.0, 0.0), &w, d));
        line("+20 kg, +4 Strength", &run(&with(20.0, 4.0), &w, d));
        line("-15 kg", &run(&with(-15.0, 0.0), &w, d));
        println!("Post up v {}: the big man guarding him", d.label());
        line("as they are", &run(&w, &w, d));
        line("+20 kg", &run(&w, &with(20.0, 0.0), d));
        line("+20 kg, +4 Strength", &run(&w, &with(20.0, 4.0), d));
        line("-15 kg", &run(&w, &with(-15.0, 0.0), d));
    }
}

/// `basketman lab press [games]`: what a full-court press does to the team
/// pressing and to the team pressed.
pub fn press(games: usize) {
    let w = World::generate(7, 12);
    let n = w.teams.len();
    for (label, press) in [
        ("no press", Press::None),
        ("three-quarter", Press::ThreeQuarter),
        ("full court", Press::FullCourt),
    ] {
        let (mut poss, mut tov, mut pts, mut pressing_tired, mut pressed_tired, mut ticks) =
            (0u32, 0u32, 0u32, 0.0f64, 0.0f64, 0u64);
        for g in 0..games {
            let (a, b) = (g % n, (g * 5 + 1 + g / n) % n);
            if a == b {
                continue;
            }
            let (mut ta, mut tb) = (w.teams[a].tactics().clone(), w.teams[b].tactics().clone());
            tb.press = press;
            ta.press = Press::None;
            let la = Lineup {
                tactics: &ta,
                players: line(&w, &ta),
                familiarity: 1.0,
                belief: 0.55,
                coach: 0,
            };
            let lb = Lineup {
                tactics: &tb,
                players: line(&w, &tb),
                familiarity: 1.0,
                belief: 0.55,
                coach: 0,
            };
            let mut rng = Rng::new(700 + g as u64 * 7919);
            let (r, rec) = simulate_recorded(&la, &lb, &w.rules, &mut rng);
            poss += r.possessions[0] as u32;
            for (_, l) in &r.boxes[0] {
                tov += l.tov as u32;
                pts += l.pts as u32;
            }
            pressed_tired += rec.fatigue[0] as f64;
            pressing_tired += rec.fatigue[1] as f64;
            ticks += rec.fatigue_ticks as u64;
        }
        println!(
            "{label:<14} the pressed team: {:.1} turnovers and {:.1} points per 100 possessions; fatigue of the five on the floor, averaged over the game: pressing {:.3}, pressed {:.3}",
            100.0 * tov as f32 / poss as f32,
            100.0 * pts as f32 / poss as f32,
            pressing_tired / ticks as f64,
            pressed_tired / ticks as f64,
        );
    }
}

/// `basketman lab handling [games]`: what a team of poor ball handlers and
/// passers does to its turnovers.
pub fn handling(games: usize) {
    use basketman_core::league::lineup;
    let w = World::generate(2, 8);
    let mut bad = w.clone();
    for &p in &w.teams[0].roster {
        bad.players[p as usize].attrs[Attr::Handling] = 2.0;
        bad.players[p as usize].attrs[Attr::Passing] = 2.0;
    }
    let run = |world: &World| {
        let (mut tov, mut games_played) = (0u32, 0u32);
        for g in 0..games {
            let other = 1 + g % 7;
            let (home, away) = (
                lineup(&world.teams[0], &world.players),
                lineup(&world.teams[other], &world.players),
            );
            let mut rng = Rng::new(5000 + g as u64);
            let (r, _) = if g % 2 == 0 {
                simulate_recorded(&home, &away, &world.rules, &mut rng)
            } else {
                simulate_recorded(&away, &home, &world.rules, &mut rng)
            };
            let side = g % 2;
            tov += r.boxes[side].iter().map(|(_, l)| l.tov as u32).sum::<u32>();
            games_played += 1;
        }
        tov as f32 / games_played as f32
    };
    let (a, b) = (run(&w), run(&bad));
    println!(
        "turnovers per game: as they are {a:.2}, handling and passing 2: {b:.2} (+{:.2})",
        b - a
    );
    // The same through the league's own rounds.
    let mut season = bad.clone();
    for _ in 0..6 {
        season.play_round();
    }
    let m = &season.team_stats[0];
    println!(
        "six league rounds: {} turnovers in {} games",
        m.own.tov, m.games
    );
}

/// `basketman lab handling_rounds [worlds]`: the assistant's test, over many worlds.
pub fn handling_rounds(worlds: usize) {
    let (mut gap, mut seen) = (0.0f32, 0u32);
    let mut low = 0u32;
    for seed in 0..worlds as u64 {
        let w = World::generate(seed, 8);
        let mut bad = w.clone();
        for &p in &w.teams[0].roster {
            bad.players[p as usize].attrs[Attr::Handling] = 2.0;
            bad.players[p as usize].attrs[Attr::Passing] = 2.0;
        }
        let mut good = w.clone();
        for _ in 0..6 {
            bad.play_round();
            good.play_round();
        }
        let tov = |x: &World| x.team_stats[0].own.tov as f32 / x.team_stats[0].games.max(1) as f32;
        let league = {
            let l = bad.league_totals();
            l.own.tov as f32 / l.games.max(1) as f32
        };
        gap += tov(&bad) - tov(&good);
        seen += 1;
        low += (tov(&bad) - league < 1.0) as u32;
    }
    println!(
        "{worlds} worlds: bad handlers turn it over {:.2} more a game over six rounds; {} of {} would not be told to work on it",
        gap / seen as f32,
        low,
        seen
    );
}

/// `lab frames <game> <from> <to>`: the frames of one recorded game between
/// two replay times, with the events, for looking at what the engine did.
/// Game `g` is the same one the engine tests record.
pub fn frames(g: usize, from: f32, to: f32) {
    use basketman_core::court::HOOP;
    use basketman_core::replay::to_court;
    let w = World::generate(7, 12);
    let a = g % 12;
    let b = (g * 5 + 1 + g / 12) % 12;
    let b = if a == b { (b + 1) % 12 } else { b };
    let hl = basketman_core::league::lineup(&w.teams[a], &w.players);
    let al = basketman_core::league::lineup(&w.teams[b], &w.players);
    let (res, rec) = simulate_recorded(&hl, &al, &w.rules, &mut Rng::new(300 + g as u64));
    let name = |id: u32| {
        w.players
            .get(id as usize)
            .map_or("?", |p| p.name.as_str())
            .to_string()
    };
    let _ = res;
    for e in rec.events.iter().filter(|e| e.time >= from && e.time <= to) {
        println!(
            "EVENT {:.2} {}",
            e.time,
            e.text(|p| Box::leak(name(p).into_boxed_str()))
        );
    }
    for f in rec.frames.iter().filter(|f| f.time >= from && f.time <= to) {
        let hoop = to_court(HOOP, f.offense as usize);
        let mut who = String::new();
        for (s, side) in f.pose.iter().enumerate() {
            for (k, p) in side.iter().enumerate() {
                if p.action != basketman_core::replay::Action::Standing
                    && p.action != basketman_core::replay::Action::Moving
                {
                    who.push_str(&format!(
                        " {}{}:{:?}/{:?}/{} ",
                        s, k, p.action, p.stage, p.lift_cm
                    ));
                }
            }
        }
        println!(
            "t {:.1} {}{}{} off {} ball ({:.2},{:.2}) h {:.2} d {:.2} holder {:?} at {} |{}",
            f.time,
            if f.cut { "CUT " } else { "" },
            if f.swish { "SWISH " } else { "" },
            if f.whistle { "WHISTLE " } else { "" },
            f.offense,
            f.ball.x,
            f.ball.y,
            f.ball_height,
            f.ball.dist(hoop),
            f.holder,
            f.holder.map_or(String::new(), |(s, k)| format!(
                "({:.2},{:.2})",
                f.pos[s as usize][k as usize].x, f.pos[s as usize][k as usize].y
            )),
            who
        );
    }
}

/// `lab clockviolations <games>`: where shot clock violations come from.
pub fn clock_violations(games: usize) {
    let w = World::generate(7, 12);
    let mut found = 0;
    for g in 0..games {
        let a = g % 12;
        let b = (g * 5 + 1 + g / 12) % 12;
        let b = if a == b { (b + 1) % 12 } else { b };
        let hl = basketman_core::league::lineup(&w.teams[a], &w.players);
        let al = basketman_core::league::lineup(&w.teams[b], &w.players);
        let (_, rec) = simulate_recorded(&hl, &al, &w.rules, &mut Rng::new(300 + g as u64));
        for (i, e) in rec.events.iter().enumerate() {
            if let EventKind::Turnover {
                kind: basketman_core::replay::TurnoverKind::ShotClock,
                ..
            } = &e.kind
            {
                found += 1;
                println!(
                    "game {g} t {:.1} clock {:.1}; previous events:",
                    e.time, e.clock
                );
                for p in rec.events[i.saturating_sub(4)..i].iter() {
                    println!("   {:.1} {:?}", p.time, std::mem::discriminant(&p.kind));
                    println!("      {}", p.text(|_| "x"));
                }
            }
        }
    }
    println!("{found} violations in {games} games");
}

/// `lab diag [games]`: what the recording shows around shots.
pub fn diag(games: usize) {
    use basketman_core::replay::{Action, ShotStage, Style, to_court};
    let w = World::generate(7, 12);
    let mut shots = 0;
    let mut speed_at_release = Vec::new();
    let mut away = [0u32; 3];
    let mut total_mates = 0;
    let mut frames_total = 0;
    let mut styles = [0u32; 5];
    let mut layup_dist = Vec::new();
    let mut hang = Vec::new();
    for g in 0..games {
        let a = g % 12;
        let b = (g * 5 + 1 + g / 12) % 12;
        let b = if a == b { (b + 1) % 12 } else { b };
        let hl = basketman_core::league::lineup(&w.teams[a], &w.players);
        let al = basketman_core::league::lineup(&w.teams[b], &w.players);
        let (_, rec) = simulate_recorded(&hl, &al, &w.rules, &mut Rng::new(300 + g as u64));
        frames_total += rec.frames.len();
        for e in rec.events.iter() {
            let EventKind::Shot { .. } = &e.kind else {
                continue;
            };
            let i = rec.frame_at(e.time);
            let f = &rec.frames[i];
            let o = f.offense as usize;
            let Some(k) = (0..5).find(|&k| {
                f.pose[o][k].action == Action::Shooting
                    && matches!(f.pose[o][k].stage, ShotStage::Release)
            }) else {
                continue;
            };
            shots += 1;
            styles[match f.pose[o][k].style {
                Style::Jumper => 0,
                Style::Layup => 1,
                Style::Dunk => 2,
                Style::Hook => 3,
                Style::Fadeaway => 4,
            }] += 1;
            let prev = &rec.frames[i.saturating_sub(1)];
            if f.pose[o][k].style == Style::Jumper || f.pose[o][k].style == Style::Hook {
                speed_at_release.push(f.pos[o][k].dist(prev.pos[o][k]) / 0.1);
            }
            let hoop = to_court(basketman_core::court::HOOP, o);
            if f.pose[o][k].style == Style::Layup {
                layup_dist.push(f.pos[o][k].dist(hoop));
            }
            let later = &rec.frames[rec.frame_at(e.time + 1.0)];
            for j in 0..5 {
                if j == k {
                    continue;
                }
                total_mates += 1;
                let d = later.pos[o][j].dist(hoop) - f.pos[o][j].dist(hoop);
                if d > 1.0 {
                    away[0] += 1;
                }
                if d > 2.0 {
                    away[1] += 1;
                }
                if d > 3.0 {
                    away[2] += 1;
                }
            }
            // How long he stays still after landing.
            let mut n = 0;
            for fr in &rec.frames[i..(i + 30).min(rec.frames.len())] {
                if fr.pose[o][k].action == Action::Shooting {
                    n += 1;
                }
            }
            hang.push(n as f32 / 10.0);
        }
    }
    let avg = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
    println!(
        "games {games}: {frames_total} frames, shots seen {shots}; jumper/layup/dunk/hook/fade {styles:?}"
    );
    println!(
        "jump shooter speed at release: mean {:.2} m/s, above 2 m/s: {:.0}%",
        avg(&speed_at_release),
        100.0 * speed_at_release.iter().filter(|s| **s > 2.0).count() as f32
            / speed_at_release.len().max(1) as f32
    );
    println!(
        "teammates more than 1/2/3 m further from the hoop a second after release: {:.0}% {:.0}% {:.0}%",
        100.0 * away[0] as f32 / total_mates.max(1) as f32,
        100.0 * away[1] as f32 / total_mates.max(1) as f32,
        100.0 * away[2] as f32 / total_mates.max(1) as f32
    );
    println!(
        "layup release distance from rim: mean {:.2} m",
        avg(&layup_dist)
    );
    println!(
        "seconds of Shooting pose from release on (follow-through): mean {:.2}",
        avg(&hang)
    );
}

/// `basketman lab league [games]`: league games as they are played, with
/// every club's real coach on the bench (the other lab runs use coach 0):
/// pace, scoring, and how many timeouts and changes of system the benches
/// make.
pub fn league(games: usize) {
    let w = World::generate(7, 12);
    let n = w.teams.len();
    let (mut poss, mut pts, mut played, mut timeouts, mut adjusts) = (0u32, 0u32, 0u32, 0u32, 0u32);
    let mut by_timeouts = [0u32; 8];
    let mut calm = 0u32;
    for g in 0..games {
        let (a, b) = (g % n, (g * 5 + 1 + g / n) % n);
        if a == b {
            continue;
        }
        let home = basketman_core::league::lineup(&w.teams[a], &w.players);
        let away = basketman_core::league::lineup(&w.teams[b], &w.players);
        let mut rng = Rng::new(1700 + g as u64 * 7919);
        let (r, rec) = simulate_recorded(&home, &away, &w.rules, &mut rng);
        played += 1;
        poss += (r.possessions[0] + r.possessions[1]) as u32;
        pts += (r.score[0] + r.score[1]) as u32;
        let (mut t, mut adj) = (0u32, 0u32);
        for e in &rec.events {
            match e.kind {
                EventKind::Timeout { .. } => t += 1,
                EventKind::Adjust { .. } => adj += 1,
                _ => {}
            }
        }
        timeouts += t;
        adjusts += adj;
        by_timeouts[(t as usize).min(7)] += 1;
        if adj == 0 {
            calm += 1;
        }
    }
    let g = played.max(1) as f32;
    println!(
        "games {played}  possessions/game {:.1}  points/100 poss {:.2}",
        poss as f32 / g,
        pts as f32 / poss.max(1) as f32 * 100.0
    );
    println!(
        "timeouts/game {:.2} (both benches)  changes of system/game {:.2}  games with no change {:.0}%",
        timeouts as f32 / g,
        adjusts as f32 / g,
        calm as f32 / g * 100.0
    );
    println!("games by number of timeouts (0..7+): {by_timeouts:?}");
}

/// Make rates of the launch model at fixed spots, by size of error at the
/// ring: the table the shooting errors are fitted from. `LAUNCH="lat,depth"`
/// sets the unit error in metres: lat0,lat per m,depth0,depth per m.
pub fn launch() {
    use basketman_core::court::HOOP;
    use basketman_core::engine::make_rate;
    use basketman_core::replay::Style;
    let spots: [(&str, f32, f32, f32, Style); 10] = [
        ("free throw 4.2", 0.0, HOOP.y + 4.2, 2.6, Style::Jumper),
        ("mid 3.0 straight", 0.0, HOOP.y + 3.0, 2.9, Style::Jumper),
        ("mid 5.0 straight", 0.0, HOOP.y + 5.0, 2.9, Style::Jumper),
        ("mid 5.0 wing", 3.5, HOOP.y + 3.5, 2.9, Style::Jumper),
        ("three 7.0 top", 0.0, HOOP.y + 7.0, 3.0, Style::Jumper),
        ("three 7.0 wing", 5.0, HOOP.y + 5.0, 3.0, Style::Jumper),
        ("corner three 6.6", 6.5, HOOP.y + 0.8, 3.0, Style::Jumper),
        ("three 9.0", 0.0, HOOP.y + 9.0, 3.0, Style::Jumper),
        ("layup front", 0.0, HOOP.y + 1.0, 3.2, Style::Layup),
        ("layup side", 1.4, HOOP.y + 0.4, 3.2, Style::Layup),
    ];
    let unit: Vec<f32> = std::env::var("LAUNCH")
        .ok()
        .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_else(|| vec![0.04, 0.006, 0.085, 0.012]);
    let scales = [0.25f32, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];
    print!("{:<20}", "scale of error");
    for k in scales {
        print!("{k:>7.2}");
    }
    println!();
    for (name, x, y, z, st) in spots {
        print!("{name:<20}");
        for k in scales {
            let d = ((x - HOOP.x).powi(2) + (y - HOOP.y).powi(2)).sqrt();
            let lat = unit[0] + unit[1] * d;
            let depth = unit[2] + unit[3] * d;
            let r = make_rate((x, y, z), st, (lat * k, depth * k), 3000, 1);
            print!("{:>6.1}%", r * 100.0);
        }
        println!();
    }
}

/// `basketman lab exploit [games]`: Post up with the big man drawn close to
/// the basket, against the stock set, so the way the points come can be read.
pub fn exploit(games: usize) {
    use basketman_core::tactics::Board;
    let w = World::generate(7, 12);
    for (name, spot) in [
        ("stock", None),
        ("big man at (1.1, 1.7)", Some(basketman_core::court::v(1.1, 1.7))),
        ("big man at (0.0, 1.2)", Some(basketman_core::court::v(0.0, 1.2))),
    ] {
        let c = cell(
            &w,
            &|a, b| {
                a.offense = OffenseSet::PostUp;
                a.defense = DefenseScheme::ManToMan;
                b.offense = OffenseSet::Motion;
                b.defense = DefenseScheme::ManToMan;
                a.board = spot.map(|p| {
                    let mut bd = Board::stock(OffenseSet::PostUp);
                    bd.spots[4] = p;
                    bd
                });
            },
            games,
            5,
        );
        let l = &c.line;
        let shots: u32 = c.shots.iter().map(|s| s.0).sum();
        println!(
            "{name}: {:.3} pts/poss, {:.2} poss/game-side, FG {:.1}% FTA/poss {:.3} TOV/poss {:.3}",
            c.ppp(),
            c.possessions as f32 / games as f32,
            100.0 * l.fgm as f32 / l.fga.max(1) as f32,
            l.fta as f32 / c.possessions.max(1) as f32,
            l.tov as f32 / c.possessions.max(1) as f32
        );
        print!("   by kind (share, make): ");
        for (n, s) in ["rim", "mid", "three"].iter().zip(c.shots) {
            print!("{n} {:.0}% {:.0}% (contest {:.2}); ", 100.0 * s.0 as f32 / shots.max(1) as f32, 100.0 * s.1 as f32 / s.0.max(1) as f32, s.2 / s.0.max(1) as f32);
        }
        println!();
        print!("   by source (share, pts/shot): ");
        let st: u32 = c.sources.iter().map(|s| s.0).sum();
        for (src, s) in Source::ALL.iter().zip(c.sources) {
            if s.0 * 50 >= st {
                print!("{} {:.0}% {:.2}; ", src.label(), 100.0 * s.0 as f32 / st.max(1) as f32, s.2 as f32 / s.0.max(1) as f32);
            }
        }
        println!();
        println!("   turnovers by kind (bad pass, stripped, off foul, clock, press, 3s): {:?}", c.tov_kinds);
    }
}

/// `basketman lab stance [games]`: what the stance model costs and buys, on
/// otherwise identical squads over several worlds. Defenders with quick
/// feet against slow ones, and drivers with quick feet against slow ones;
/// the make rate by how the body was comes from `lab stats`.
pub fn stance(games: usize) {
    use basketman_core::league::lineup;
    let worlds = [61u64, 62, 63, 64];
    // (points per 100 possessions conceded by the altered side, attempts at the rim by the other,
    // rim make, blocks) over all worlds.
    let run = |change: &dyn Fn(&mut Player)| {
        let (mut pts, mut poss, mut fouls, mut rim, mut rim_made) = (0.0f32, 0.0f32, 0.0f32, 0u32, 0u32);
        for &seed in &worlds {
            let w = World::generate(seed, 12);
            for g in 0..games {
                let a = g % 12;
                let b = (g * 5 + 1 + g / 12) % 12;
                if a == b {
                    continue;
                }
                let mut players = w.players.clone();
                for &p in &w.teams[a].roster {
                    change(&mut players[p as usize]);
                    for at in Attr::ALL {
                        players[p as usize].attrs[at] = players[p as usize].attrs[at].clamp(1.0, 20.0);
                    }
                }
                let hl = lineup(&w.teams[a], &players);
                let al = lineup(&w.teams[b], &w.players);
                let (r, rec) = simulate_recorded(&hl, &al, &w.rules, &mut Rng::new(seed * 1000 + g as u64));
                pts += r.score[1] as f32;
                poss += r.possessions[1] as f32;
                for e in &rec.events {
                    if let EventKind::Shot { kind, made, fouled, who, .. } = &e.kind {
                        let theirs = r.boxes[1].iter().any(|(id, _)| id == who);
                        if theirs && *kind == ShotKind::Rim && !(*fouled && !*made) {
                            rim += 1;
                            rim_made += *made as u32;
                        }
                    }
                    if let EventKind::Foul { .. } = &e.kind {
                        fouls += 1.0;
                    }
                }
            }
        }
        (100.0 * pts / poss, 100.0 * rim_made as f32 / rim.max(1) as f32, fouls)
    };
    println!("defenders' Footwork and Speed, the other side's points per 100 possessions / rim make % / fouls");
    for (label, delta) in [("slow feet (-5)", -5.0f32), ("as they are", 0.0), ("quick feet (+5)", 5.0)] {
        let (ppp, rim, fouls) = run(&|p: &mut Player| {
            p.attrs[Attr::Footwork] += delta;
            p.attrs[Attr::Speed] += delta;
        });
        println!("  {label:<18} {ppp:>7.2}  rim {rim:>5.1}%  fouls {fouls:>6.0}");
    }
    println!("Footwork only:");
    for (label, delta) in [("slow feet (-5)", -5.0f32), ("quick feet (+5)", 5.0)] {
        let (ppp, rim, fouls) = run(&|p: &mut Player| p.attrs[Attr::Footwork] += delta);
        println!("  {label:<18} {ppp:>7.2}  rim {rim:>5.1}%  fouls {fouls:>6.0}");
    }
}
