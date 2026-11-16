//! Compares a bare league (old calendar) with a career that starts in the
//! preseason: injuries, money and development over one season.
//! `cargo run --release -p basketman-core --example calendar_measure`

use basketman_core::league::World;

fn run(mut w: World) -> (f64, f64, f64, f64) {
    let start: Vec<f32> = w.players.iter().map(|p| p.overall()).collect();
    let budget0: i64 = w.teams.iter().map(|t| t.budget).sum();
    while !w.season_over() {
        w.play_round();
    }
    let clubs = w.teams.len() as f64;
    let hurt: usize = w.players.iter().map(|p| p.injuries.len()).sum();
    let budget: i64 = w.teams.iter().map(|t| t.budget).sum();
    let mut grown = 0.0;
    let mut n = 0.0;
    for t in &w.teams {
        for &p in &t.roster {
            grown += (w.players[p as usize].overall() - start[p as usize]) as f64;
            n += 1.0;
        }
    }
    let sharp: f64 = w.teams.iter().flat_map(|t| &t.roster).map(|&p| w.players[p as usize].sharp as f64).sum::<f64>() / n;
    (hurt as f64 / clubs, (budget - budget0) as f64 / clubs, grown / n, sharp)
}

fn main() {
    let seeds = 1..=6u64;
    let mut a = (0.0, 0.0, 0.0, 0.0);
    let mut b = (0.0, 0.0, 0.0, 0.0);
    let k = seeds.clone().count() as f64;
    for s in seeds {
        let x = run(World::generate(s, 12));
        let y = run(World::new_career(s, 12));
        a = (a.0 + x.0 / k, a.1 + x.1 / k, a.2 + x.2 / k, a.3 + x.3 / k);
        b = (b.0 + y.0 / k, b.1 + y.1 / k, b.2 + y.2 / k, b.3 + y.3 / k);
    }
    println!("                     injuries/club  budget change/club  overall change  sharpness");
    println!("bare league          {:>13.2}  {:>18.0}  {:>14.3}  {:>9.3}", a.0, a.1, a.2, a.3);
    println!("career (preseason)   {:>13.2}  {:>18.0}  {:>14.3}  {:>9.3}", b.0, b.1, b.2, b.3);
}
