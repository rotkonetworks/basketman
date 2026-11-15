//! Three divisions: how far apart they are, what a season costs and brings
//! in at each level, and whether a promoted Third Division side can stay up.
//! `cargo run --release -p basketman-core --example tiers_measure`

use basketman_core::engine::simulate_game;
use basketman_core::league::{World, lineup};
use basketman_core::rng::Rng;

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

fn main() {
    let (mut top_vs_bottom, mut top_vs_mid, mut top_vs_all) = (vec![], vec![], vec![]);
    let mut rows = Vec::new();
    for seed in 1..=3u64 {
        let mut w = World::new_career_tiers(seed, 12, 3);
        while !w.season_over() {
            w.play_round();
        }
        let tables = w.division_tables();
        for d in 0..3 {
            let ids = &tables[d];
            let gates: Vec<f64> = ids.iter().map(|&t| w.econ.ledgers[t].total(basketman_core::finance::Cat::Gate) as f64).collect();
            let wages: Vec<f64> = ids.iter().map(|&t| w.payroll(t) as f64).collect();
            let cap: Vec<f64> = ids.iter().map(|&t| w.econ.arenas[t].capacity as f64).collect();
            let fans: Vec<f64> = ids.iter().map(|&t| w.fan_base(t) as f64).collect();
            let bud: Vec<f64> = ids.iter().map(|&t| w.teams[t].budget as f64).collect();
            let ov: Vec<f64> = ids.iter().map(|&t| {
                let mut o: Vec<f32> = w.teams[t].roster.iter().map(|&p| w.players[p as usize].overall()).collect();
                o.sort_by(|a, b| b.total_cmp(a));
                o.iter().take(8).sum::<f32>() as f64 / 8.0
            }).collect();
            let income: Vec<f64> = ids.iter().map(|&t| {
                use basketman_core::finance::Cat;
                let l = &w.econ.ledgers[t];
                (l.total(Cat::Gate) + l.total(Cat::Sponsorship) + l.total(Cat::Broadcast) + l.total(Cat::Merchandise) + l.total(Cat::Prize)) as f64
            }).collect();
            rows.push((d, mean(&ov), mean(&cap), mean(&fans), mean(&wages), mean(&gates), mean(&income), mean(&bud)));
        }
        // The two best of the Third play the First's clubs.
        let promoted: Vec<usize> = tables[2].iter().take(2).copied().collect();
        let first = &tables[0];
        for &p in &promoted {
            for (rank, &f) in first.iter().enumerate() {
                let mut wins = 0.0;
                let n = 20;
                for g in 0..n {
                    let (h, a) = if g % 2 == 0 { (p, f) } else { (f, p) };
                    let r = simulate_game(&lineup(&w.teams[h], &w.players), &lineup(&w.teams[a], &w.players), &w.rules, &mut Rng::new(seed * 1000 + g));
                    let side = if h == p { 0 } else { 1 };
                    if r.score[side] > r.score[1 - side] { wins += 1.0; }
                }
                let rate = wins / n as f64;
                top_vs_all.push(rate);
                if rank >= 10 { top_vs_bottom.push(rate); }
                if (4..10).contains(&rank) { top_vs_mid.push(rate); }
            }
        }
    }
    println!("div  top8 ovr  capacity   fans   payroll    gate   income   budget after a season");
    for d in 0..3 {
        let r: Vec<_> = rows.iter().filter(|r| r.0 == d).collect();
        let m = |f: fn(&(usize, f64, f64, f64, f64, f64, f64, f64)) -> f64| r.iter().map(|x| f(x)).sum::<f64>() / r.len() as f64;
        println!("{d}   {:>8.2} {:>9.0} {:>7.0} {:>9.0} {:>8.0} {:>8.0} {:>10.0}", m(|x| x.1), m(|x| x.2), m(|x| x.3), m(|x| x.4), m(|x| x.5), m(|x| x.6), m(|x| x.7));
    }
    println!("Third Division's two best against the First Division, win rate per game:");
    println!("  all clubs {:.2}, the top four {:.2}, mid-table {:.2}, bottom two {:.2}",
        mean(&top_vs_all),
        0.0, mean(&top_vs_mid), mean(&top_vs_bottom));
}
