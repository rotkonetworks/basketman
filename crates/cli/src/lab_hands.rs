//! `basketman lab hands [games]`: what the hands do. Passes by kind, what
//! became of them, steals and turnovers, and who scores. Same pairings as
//! `lab stats`, so the numbers can be set beside it.

use basketman_core::engine::{BoxLine, Lineup, simulate_recorded};
use basketman_core::league::World;
use basketman_core::player::Player;
use basketman_core::replay::{EventKind, HandKind, PassKind, TurnoverKind};
use basketman_core::rng::Rng;
use basketman_core::tactics::{DefenseScheme, OffenseSet, Tactics};

fn line<'w>(w: &'w World, t: &Tactics) -> Vec<&'w Player> {
    t.rotation.iter().map(|s| &w.players[s.player as usize]).collect()
}

fn mk<'a>(w: &'a World, t: &'a Tactics) -> Lineup<'a> {
    Lineup { tactics: t, players: line(w, t), familiarity: 1.0, belief: 0.55, coach: 0 }
}

pub fn run(games: usize) {
    let w = World::generate(7, 12);
    let n = w.teams.len();
    let mut tot = BoxLine::default();
    let mut poss = 0u32;
    let mut kinds = [0u32; 7];
    let mut passes = 0u32;
    let mut outcomes = [0u32; 6];
    let mut by_kind = [[0u32; 6]; 7];
    let mut tov = [0u32; 6];
    let mut stolen = 0u32;
    let mut hands = [0u32; 10];
    let mut frames = 0u64;
    let mut shares: Vec<f32> = Vec::new();
    let mut ast_made = (0u32, 0u32);
    let mut secs = 0.0f64;
    let mut by_set = [(0u32, 0u32, 0u32, 0u32); 5];
    for g in 0..games {
        let (a, b) = (g % n, (g * 5 + 1 + g / n) % n);
        if a == b {
            continue;
        }
        let (mut ta, mut tb) = (w.teams[a].tactics().clone(), w.teams[b].tactics().clone());
        let set = (g / 3) % OffenseSet::ALL.len();
        if g % 3 != 0 {
            ta.offense = OffenseSet::ALL[set];
            tb.offense = OffenseSet::ALL[(g / 3 + 2) % OffenseSet::ALL.len()];
            ta.defense = DefenseScheme::ALL[(g / 2) % DefenseScheme::ALL.len()];
            tb.defense = DefenseScheme::ALL[(g / 2 + 1) % DefenseScheme::ALL.len()];
        }
        let (la, lb) = (mk(&w, &ta), mk(&w, &tb));
        let mut rng = Rng::new(900 + g as u64 * 7919);
        let t0 = std::time::Instant::now();
        let (r, rec) = simulate_recorded(&la, &lb, &w.rules, &mut rng);
        secs += t0.elapsed().as_secs_f64();
        poss += (r.possessions[0] + r.possessions[1]) as u32;
        for side in 0..2 {
            let team_pts: u32 = r.boxes[side].iter().map(|(_, l)| l.pts as u32).sum();
            if let Some(m) = r.boxes[side].iter().map(|(_, l)| l.pts).max() {
                shares.push(m as f32 / team_pts.max(1) as f32);
            }
            for (_, l) in &r.boxes[side] {
                tot.add(l);
            }
        }
        for p in &rec.passes {
            passes += 1;
            kinds[p.kind as usize] += 1;
            outcomes[p.outcome as usize] += 1;
            by_kind[p.kind as usize][p.outcome as usize] += 1;
        }
        for e in &rec.events {
            match &e.kind {
                EventKind::Turnover { kind, stolen_by, .. } => {
                    let k = match kind {
                        TurnoverKind::BadPass => 0,
                        TurnoverKind::Stripped => 1,
                        TurnoverKind::OffensiveFoul => 2,
                        TurnoverKind::ShotClock => 3,
                        TurnoverKind::Pressed => 4,
                        TurnoverKind::ThreeSeconds => 5,
                    };
                    tov[k] += 1;
                    stolen += stolen_by.is_some() as u32;
                }
                EventKind::Shot { made, assist, .. } if *made => {
                    ast_made.0 += 1;
                    ast_made.1 += assist.is_some() as u32;
                }
                _ => {}
            }
        }
        let off = set;
        let s = &mut by_set[off];
        s.0 += r.score[0] as u32 + r.score[1] as u32;
        s.1 += (r.possessions[0] + r.possessions[1]) as u32;
        for f in &rec.frames {
            frames += 1;
            for t in 0..2 {
                for k in 0..5 {
                    for h in f.pose[t][k].hands {
                        let i = match h.kind {
                            HandKind::Rest => 0,
                            HandKind::Guard => 1,
                            HandKind::Dribble => 2,
                            HandKind::Hold => 3,
                            HandKind::Reach => 4,
                            HandKind::Up => 5,
                            HandKind::Lane => 6,
                            HandKind::Target => 7,
                            HandKind::Catch => 8,
                            HandKind::Throw => 9,
                        };
                        hands[i] += 1;
                    }
                }
            }
        }
    }
    let p = poss.max(1) as f32;
    println!("games {games}  possessions {poss}  {:.2} s/game", secs / games.max(1) as f64);
    println!(
        "pts/100 {:.1}  FG {:.1}%  3P {:.1}%  ast/fgm {:.3}  assisted share of made {:.1}%",
        tot.pts as f32 / p * 100.0,
        tot.fgm as f32 / tot.fga.max(1) as f32 * 100.0,
        tot.tpm as f32 / tot.tpa.max(1) as f32 * 100.0,
        tot.ast as f32 / tot.fgm.max(1) as f32,
        ast_made.1 as f32 / ast_made.0.max(1) as f32 * 100.0,
    );
    println!(
        "pass outcomes: caught {:.1}% bobbled {:.1}% tipped {:.1}% stolen {:.1}% out {:.1}% missed {:.1}%",
        outcomes[0] as f32 * 100.0 / passes.max(1) as f32, outcomes[1] as f32 * 100.0 / passes.max(1) as f32, outcomes[2] as f32 * 100.0 / passes.max(1) as f32, outcomes[3] as f32 * 100.0 / passes.max(1) as f32, outcomes[4] as f32 * 100.0 / passes.max(1) as f32, outcomes[5] as f32 * 100.0 / passes.max(1) as f32
    );
    println!("passes per 100 possessions {:.1}", passes as f32 / p * 100.0);
    let names = ["chest", "bounce", "overhead", "lob", "push", "handoff", "outlet"];
    let _ = PassKind::Chest;
    print!("pass kinds:");
    for (i, n) in names.iter().enumerate() {
        print!(" {n} {:.1}%", kinds[i] as f32 / passes.max(1) as f32 * 100.0);
    }
    println!();
    for (i, n) in names.iter().enumerate() {
        let t: u32 = by_kind[i].iter().sum();
        if t > 0 {
            println!("  {n:<9} n {t:>6}: caught {:.1} bobbled {:.1} tipped {:.1} stolen {:.1} out {:.1} missed {:.1}", by_kind[i][0] as f32 * 100.0 / t as f32, by_kind[i][1] as f32 * 100.0 / t as f32, by_kind[i][2] as f32 * 100.0 / t as f32, by_kind[i][3] as f32 * 100.0 / t as f32, by_kind[i][4] as f32 * 100.0 / t as f32, by_kind[i][5] as f32 * 100.0 / t as f32);
        }
    }
    let tt: u32 = tov.iter().sum();
    println!(
        "turnovers/100 {:.1} (bad pass {:.1}, stripped {:.1}, off foul {:.1}, clock {:.1}, pressed {:.1}, 3s {:.1}); steals/100 {:.1}",
        tt as f32 / p * 100.0, tov[0] as f32 / p * 100.0, tov[1] as f32 / p * 100.0, tov[2] as f32 / p * 100.0,
        tov[3] as f32 / p * 100.0, tov[4] as f32 / p * 100.0, tov[5] as f32 / p * 100.0, stolen as f32 / p * 100.0
    );
    println!(
        "bad passes per 100 passes {:.2}",
        tov[0] as f32 / passes.max(1) as f32 * 100.0
    );
    shares.sort_by(|a, b| a.total_cmp(b));
    if !shares.is_empty() {
        println!(
            "top scorer's share of his team's points: median {:.1}% (p10 {:.1}, p90 {:.1})",
            shares[shares.len() / 2] * 100.0,
            shares[shares.len() / 10] * 100.0,
            shares[shares.len() * 9 / 10] * 100.0
        );
    }
    let hn = ["rest", "guard", "dribble", "hold", "reach", "up", "lane", "target", "catch", "throw"];
    print!("hand states:");
    for (i, n) in hn.iter().enumerate() {
        print!(" {n} {:.1}%", hands[i] as f64 / (frames as f64 * 20.0) * 100.0);
    }
    println!();
    let _ = by_set;
}
