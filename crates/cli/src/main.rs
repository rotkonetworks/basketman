//! Headless season runner: `basketman [seed] [clubs per division] [divisions]`, or
//! `basketman lab [games]` for the tactics matrix.
//! Used to eyeball stat distributions while tuning the engine.
//! `basketman balance` measures whether tactics are balanced (docs/balance.md).
//! `basketman play` is the line-delimited JSON protocol for agents and
//! `basketman mcp` the same as an MCP server; see docs/agents.md.

mod agent;
mod balance;
mod export;
mod feeds;
mod lab;
mod lab_officials;
mod mcp;
mod play;
mod realism;

use basketman_core::engine::{BoxLine, GameResult};
use basketman_core::league::World;

fn pct(made: u32, att: u32) -> f32 {
    if att == 0 {
        0.0
    } else {
        100.0 * made as f32 / att as f32
    }
}

fn print_box(w: &World, home: usize, away: usize, r: &GameResult) {
    println!(
        "{} {} - {} {}",
        w.teams[home].name, r.score[0], r.score[1], w.teams[away].name
    );
    let quarters: Vec<String> = r
        .periods
        .iter()
        .map(|p| format!("{}-{}", p[0], p[1]))
        .collect();
    println!("  by period: {}", quarters.join(", "));
    for (side, team) in [home, away].into_iter().enumerate() {
        println!(
            "\n  {:<22} MIN  PTS    FG   3PT    FT  REB AST STL BLK  TO  PF",
            w.teams[team].name
        );
        for (pid, l) in &r.boxes[side] {
            println!(
                "  {:<22} {:>3}  {:>3} {:>2}/{:<2} {:>2}/{:<2} {:>2}/{:<2}  {:>3} {:>3} {:>3} {:>3} {:>3} {:>3}",
                w.players[*pid as usize].name,
                l.secs / 60,
                l.pts,
                l.fgm,
                l.fga,
                l.tpm,
                l.tpa,
                l.ftm,
                l.fta,
                l.reb(),
                l.ast,
                l.stl,
                l.blk,
                l.tov,
                l.pf
            );
        }
    }
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--export-game") {
        let seed: u64 = std::env::args()
            .nth(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let Some(path) = std::env::args().nth(3) else {
            eprintln!("usage: basketman --export-game <seed> <out-file>");
            std::process::exit(2);
        };
        if let Err(e) = export::export_game(seed, &path) {
            eprintln!("cannot write {path}: {e}");
            std::process::exit(1);
        }
        return;
    }
    match std::env::args().nth(1).as_deref() {
        Some("play") => return play::run(),
        Some("mcp") => return mcp::run(),
        Some("realism") => {
            let rest: Vec<String> = std::env::args().skip(2).collect();
            return realism::run(&rest);
        }
        Some("balance") => {
            let rest: Vec<String> = std::env::args().skip(2).collect();
            return balance::run(&rest);
        }
        _ => {}
    }
    let mut args = std::env::args().skip(1);
    if std::env::args().nth(1).as_deref() == Some("lab") {
        let what = std::env::args().nth(2);
        let games = std::env::args().nth(3).and_then(|s| s.parse().ok());
        match what.as_deref() {
            Some("mix") => lab::mix(games.unwrap_or(100)),
            Some("zone") => lab::zone(games.unwrap_or(200)),
            Some("screens") => lab::screens(games.unwrap_or(150)),
            Some("pnr") => lab::pnr(games.unwrap_or(150)),
            Some("charges") => lab::charges(games.unwrap_or(4)),
            Some("hunters") => lab::hunters(),
            Some("post") => lab::post(games.unwrap_or(150)),
            Some("press") => lab::press(games.unwrap_or(400)),
            Some("handling") => lab::handling(games.unwrap_or(300)),
            Some("handling_rounds") => lab::handling_rounds(games.unwrap_or(30)),
            Some("roles") => lab::roles(games.unwrap_or(60)),
            Some("shots") => lab::shots(games.unwrap_or(60)),
            Some("stats") => lab::stats(games.unwrap_or(400)),
            Some("finish") => lab::finish(games.unwrap_or(300)),
            Some("stance") => lab::stance(games.unwrap_or(120)),
            Some("transition") if std::env::args().nth(3).as_deref() == Some("watch") => {
                let arg = |i: usize| std::env::args().nth(i).and_then(|s| s.parse::<usize>().ok());
                lab::transition_watch(arg(4).unwrap_or(0), arg(5).unwrap_or(0), 8.0)
            }
            Some("transition") => lab::transition(games.unwrap_or(300)),
            Some("league") => lab::league(games.unwrap_or(300)),
            Some("shape") => lab::shape(games.unwrap_or(40)),
            Some("foulkinds") => lab::foulkinds(games.unwrap_or(10)),
            Some("payroll") => lab::payroll(games.unwrap_or(12)),
            Some("minutes") => lab::minutes(games.unwrap_or(20)),
            Some("launch") => lab::launch(),
            Some("exploit") => lab::exploit(games.unwrap_or(200)),
            Some("diag") => lab::diag(games.unwrap_or(12)),
            Some("clockviolations") => lab::clock_violations(games.unwrap_or(20)),
            Some("frames") => {
                let a = std::env::args()
                    .nth(4)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.0);
                let b = std::env::args()
                    .nth(5)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10.0);
                lab::frames(games.unwrap_or(0), a, b)
            }
            Some("fouls") => lab::fouls(games.unwrap_or(200)),
            Some("officials") => lab_officials::run(games.unwrap_or(600)),
            Some("survival") => {
                let first = std::env::args().nth(4).and_then(|s| s.parse().ok()).unwrap_or(100);
                let with = std::env::args().nth(5).as_deref() != Some("without");
                lab_officials::survival(games.unwrap_or(96), first, with)
            }
            Some("physics") => {
                lab::physics(games.unwrap_or(300));
                lab::physics_post(games.unwrap_or(300));
                lab::physics_lane(games.unwrap_or(300));
                lab::physics_tiring(games.unwrap_or(300));
            }
            Some(n) => lab::run(n.parse().unwrap_or(60)),
            None => lab::run(60),
        }
        return;
    }
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let n_teams: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(12);
    let divisions: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);

    let mut w = World::new_career_tiers(seed, n_teams, divisions);
    let first = w.play_round()[0];
    let f = &w.fixtures[first];
    print_box(&w, f.home, f.away, f.result.as_ref().unwrap());
    while !w.season_over() {
        w.play_round();
    }

    for d in 0..w.division_count() {
        println!("\nStandings: {}", w.division_name(d));
        for (rank, t) in w.table_of(d).into_iter().enumerate() {
            let s = &w.standings[t];
            let games = (s.wins + s.losses) as f32;
            println!(
                "  {:>2}. {:<22} {:>2}-{:<2}  {:>5.1} - {:<5.1}",
                rank + 1,
                w.teams[t].name,
                s.wins,
                s.losses,
                s.points_for as f32 / games,
                s.points_against as f32 / games
            );
        }
    }

    let mut total = BoxLine::default();
    for l in &w.season_stats {
        total.add(l);
    }
    let team_games = (w.fixtures.len() * 2) as f32;
    let possessions: u32 = w
        .fixtures
        .iter()
        .filter_map(|f| f.result.as_ref())
        .map(|r| (r.possessions[0] + r.possessions[1]) as u32)
        .sum();
    println!("\nLeague averages per team game");
    println!(
        "  {:.1} pts, {:.1} possessions, FG {:.1}%, 3P {:.1}% on {:.1} att, FT {:.1}% on {:.1} att",
        total.pts as f32 / team_games,
        possessions as f32 / team_games,
        pct(total.fgm as u32, total.fga as u32),
        pct(total.tpm as u32, total.tpa as u32),
        total.tpa as f32 / team_games,
        pct(total.ftm as u32, total.fta as u32),
        total.fta as f32 / team_games,
    );
    println!(
        "  {:.1} reb ({:.1} off), {:.1} ast, {:.1} stl, {:.1} blk, {:.1} tov, {:.1} fouls",
        total.reb() as f32 / team_games,
        total.orb as f32 / team_games,
        total.ast as f32 / team_games,
        total.stl as f32 / team_games,
        total.blk as f32 / team_games,
        total.tov as f32 / team_games,
        total.pf as f32 / team_games,
    );

    println!("\nTop scorers");
    let mut scorers: Vec<usize> = (0..w.players.len())
        .filter(|&p| w.games_played[p] > 0)
        .collect();
    scorers.sort_by(|&a, &b| {
        let ppg = |p: usize| w.season_stats[p].pts as f32 / w.games_played[p] as f32;
        ppg(b).total_cmp(&ppg(a))
    });
    for &p in scorers.iter().take(8) {
        let (l, g) = (&w.season_stats[p], w.games_played[p] as f32);
        println!(
            "  {:<22} {:<2} {:>5.1} ppg {:>4.1} rpg {:>4.1} apg {:>4.1} mpg  {:>4.1} fga {:>4.1} 3pa {:>4.1} fta  ovr {:.1}",
            w.players[p].name,
            w.players[p].position.label(),
            l.pts as f32 / g,
            l.reb() as f32 / g,
            l.ast as f32 / g,
            l.secs as f32 / 60.0 / g,
            l.fga as f32 / g,
            l.tpa as f32 / g,
            l.fta as f32 / g,
            w.players[p].overall(),
        );
    }
}
