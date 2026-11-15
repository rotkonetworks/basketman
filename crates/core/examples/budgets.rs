//! Measures club money over three seasons and several seeds: budgets after
//! each season, income and spending by category for a top, a middle and a
//! bottom club, overdrafts, luxury tax, ticket prices and fill, facilities.
//! `budgets d` also prints every club of seed 1.
//!
//! It also measures fame, fans, media and retirement over the same runs: the
//! spread of fame across the league at the start and after 3 and 6 seasons,
//! what a top name adds at the gate and in the shop at home and away on
//! otherwise identical clubs, fan base trajectories for the clubs that win,
//! finish mid-table and lose, gaffes and good stories a season, retirements
//! and the player count a season, and the clubs that went out and bought a
//! name. No club is managed by a person here, so no media work is assigned:
//! the gaffe and story counts are for players doing none.
use basketman_core::finance::Cat;
use basketman_core::league::World;

const SEEDS: [u64; 6] = [1, 2, 3, 4, 5, 6];

/// (season, fame and age of every active player) at the start and after
/// seasons 3 and 6.
type Snap = (u16, Vec<(f32, u8, u8)>);

fn quant(v: &mut [f32], p: usize) -> f32 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[(v.len() - 1) * p / 100]
}

fn pct(v: &mut [i64], p: usize) -> i64 {
    v.sort();
    v[(v.len() - 1) * p / 100]
}

fn main() {
    let seasons: u16 = std::env::var("SEASONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let detail = std::env::args().nth(1).is_some();
    let mut all: Vec<Vec<i64>> = vec![Vec::new(); seasons as usize];
    // (income by cat, count) for top, mid, bottom, per season index.
    let mut by_rank: Vec<[[i64; 20]; 3]> = vec![[[0; 20]; 3]; seasons as usize];
    let mut rank_n = vec![[0i64; 3]; seasons as usize];
    let mut nets_mid: Vec<i64> = Vec::new();
    let mut nets_top: Vec<i64> = Vec::new();
    let mut nets_bot: Vec<i64> = Vec::new();
    let mut overdraft_rounds = vec![0i64; seasons as usize];
    let mut frozen_rounds = vec![0i64; seasons as usize];
    let mut clubs_overdrawn = vec![0i64; seasons as usize];
    let mut clubs_frozen_ever = vec![0i64; seasons as usize];
    let mut longest_frozen = 0i64;
    let mut tax_paid = vec![0i64; seasons as usize];
    let mut tax_clubs = vec![0i64; seasons as usize];
    let mut tax_share = vec![0i64; seasons as usize];
    let mut payrolls: Vec<i64> = Vec::new();
    let mut price_ratio: Vec<f32> = Vec::new();
    let mut fills: Vec<f32> = Vec::new();
    let mut levels = [[0i64; 6]; 3];
    let mut total_clubs = 0i64;
    let mut snaps: Vec<Snap> = Vec::new();
    // Per season index: gaffes, caught, stories, retired, farewells, hunts.
    let mut events = vec![[0u64; 6]; seasons as usize];
    // Per season index (0 = start): active, all, free agents, on clubs.
    let mut pop = vec![[0u64; 4]; seasons as usize + 1];
    // Fan base of the clubs that finished top two, middle and bottom two,
    // as a multiple of the start, per season index (0 = start).
    let mut traj = vec![[0f64; 3]; seasons as usize + 1];
    let mut traj_n = vec![[0f64; 3]; seasons as usize + 1];
    let mut hunts: Vec<(u16, i64, i64, usize)> = Vec::new();
    let mut hall: Vec<(usize, usize, String)> = Vec::new();
    // (home attendance, shop per round, away attendance, star pull) without
    // and with a top name.
    let mut star_ab = [[0f64; 4]; 2];
    let mut star_n = 0f64;
    for seed in SEEDS {
        let mut w = World::generate(seed, 12);
        let n = w.teams.len();
        let fame_snap = |w: &World, y: u16| -> Snap {
            (
                y,
                w.players
                    .iter()
                    .filter(|p| !p.is_retired())
                    .map(|p| (p.renown.fame, p.age, p.renown.farewell as u8))
                    .collect(),
            )
        };
        let base_fans: Vec<f64> = (0..n).map(|t| w.fan_base(t) as f64).collect();
        let census = |w: &World| {
            let (on, free, _, all) = w.count_by_status();
            [
                w.active_players() as u64,
                all as u64,
                free as u64,
                on as u64,
            ]
        };
        for (k, v) in census(&w).iter().enumerate() {
            pop[0][k] += v;
        }
        traj[0] = [traj[0][0] + 1.0, traj[0][1] + 1.0, traj[0][2] + 1.0];
        traj_n[0] = [traj_n[0][0] + 1.0, traj_n[0][1] + 1.0, traj_n[0][2] + 1.0];
        snaps.push(fame_snap(&w, 0));
        let mut seen = w.fame.counts;
        for year in 1..=seasons {
            let y = year as usize - 1;
            let mut run = vec![0i64; n];
            let mut ever_od = vec![false; n];
            let mut ever_fz = vec![false; n];
            while w.season < year {
                if w.match_today() {
                    w.play_round();
                    for t in 0..n {
                        if w.teams[t].budget < 0 {
                            overdraft_rounds[y] += 1;
                            ever_od[t] = true;
                        }
                        if w.spending_frozen(t) {
                            frozen_rounds[y] += 1;
                            ever_fz[t] = true;
                            run[t] += 1;
                            longest_frozen = longest_frozen.max(run[t]);
                        } else {
                            run[t] = 0;
                        }
                    }
                    if w.round == 12 {
                        for t in 0..n {
                            payrolls.push(w.payroll(t) / 1000);
                            price_ratio.push(w.ticket_price(t) as f32 / w.fair_price(t) as f32);
                        }
                    }
                } else {
                    w.advance();
                }
            }
            clubs_overdrawn[y] += ever_od.iter().filter(|&&b| b).count() as i64;
            clubs_frozen_ever[y] += ever_fz.iter().filter(|&&b| b).count() as i64;
            let mut b: Vec<i64> = w.teams.iter().map(|t| t.budget / 1000).collect();
            all[y].extend(b.iter().copied());
            b.sort();
            println!(
                "seed {seed} after {year}: min {} med {} max {}",
                b[0],
                b[b.len() / 2],
                b[b.len() - 1]
            );
            let now = w.fame.counts;
            events[y][0] = events[y][0].wrapping_add((now.gaffes - seen.gaffes) as u64);
            events[y][1] += (now.caught - seen.caught) as u64;
            events[y][2] += (now.stories - seen.stories) as u64;
            events[y][3] += (now.retired - seen.retired) as u64;
            events[y][4] += (now.farewells - seen.farewells) as u64;
            seen = now;
            for (k, v) in census(&w).iter().enumerate() {
                pop[y + 1][k] += v;
            }
            if year == 3 || year == seasons {
                snaps.push(fame_snap(&w, year));
            }
            if year == seasons {
                let names: Vec<String> = w
                    .hall_of_fame()
                    .iter()
                    .take(3)
                    .map(|h| {
                        let p = &w.players[h.player as usize];
                        format!(
                            "{} ({:.0}: {} MVP, {} titles, peak fame {:.0})",
                            p.name, h.score, p.renown.mvps, p.renown.titles, p.renown.peak
                        )
                    })
                    .collect();
                hall.push((
                    w.fame.hall.len(),
                    w.fame.counts.farewells as usize,
                    names.join("; "),
                ));
            }
            {
                let table = &w.history.last().unwrap().table;
                for (place, &(t, _, _)) in table.iter().enumerate() {
                    let r = if place < 2 {
                        0
                    } else if place >= n - 2 {
                        2
                    } else if place >= n / 2 - 1 && place <= n / 2 {
                        1
                    } else {
                        continue;
                    };
                    // The base before the season's close moved it is not kept;
                    // the base after the close is what the year earned.
                    traj[y + 1][r] += w.fan_base(t) as f64 / base_fans[t];
                    traj_n[y + 1][r] += 1.0;
                }
            }
            if year == 1 {
                // A top name or none, on otherwise identical clubs.
                for c in 0..n {
                    let mut arms = [w.clone(), w.clone()];
                    for (k, a) in arms.iter_mut().enumerate() {
                        for t in 0..n {
                            for &p in &a.teams[t].roster.clone() {
                                a.players[p as usize].renown.fame = 0.0;
                            }
                        }
                        if k == 1
                            && let Some(&best) = a.teams[c].roster.iter().max_by(|&&x, &&y| {
                                a.players[x as usize]
                                    .overall()
                                    .total_cmp(&a.players[y as usize].overall())
                            })
                        {
                            a.players[best as usize].renown.fame = 75.0;
                            a.players[best as usize].renown.tenure_club = Some(c);
                            a.players[best as usize].renown.tenure = 5;
                        }
                    }
                    for (k, a) in arms.iter().enumerate() {
                        let price = a.ticket_price(c);
                        let home = a.attendance_priced(&a.typical_draw(c), c, price);
                        let away_fx = a
                            .fixtures
                            .iter()
                            .position(|f| f.away == c)
                            .expect("a visit");
                        let host = a.fixtures[away_fx].home;
                        let away =
                            a.attendance_priced(&a.draw_for(away_fx), host, a.ticket_price(host));
                        star_ab[k][0] += home as f64;
                        star_ab[k][1] += a.merch_week(c) as f64;
                        star_ab[k][2] += away as f64;
                        star_ab[k][3] += a.star_pull(c, c) as f64;
                    }
                    star_n += 1.0;
                }
            }
            for hnt in w.fame.hunts.iter().filter(|h| h.season + 1 == year) {
                events[y][5] += 1;
                let closing = w.econ.history[y][hnt.club].closing / 1000;
                hunts.push((
                    hnt.season,
                    hnt.fee / 1000,
                    closing,
                    hnt.from.is_some() as usize,
                ));
            }
            let h = w.econ.history.last().unwrap();
            let table = &w.history.last().unwrap().table;
            for (place, &(t, _, _)) in table.iter().enumerate() {
                let r = if place == 0 {
                    0
                } else if place == n - 1 {
                    2
                } else if place == n / 2 {
                    1
                } else {
                    continue;
                };
                for (i, c) in Cat::ALL.iter().enumerate() {
                    by_rank[y][r][i] += h[t].total(*c);
                }
                rank_n[y][r] += 1;
                match r {
                    0 => nets_top.push(h[t].net() / 1000),
                    1 => nets_mid.push(h[t].net() / 1000),
                    _ => nets_bot.push(h[t].net() / 1000),
                }
            }
            for (t, l) in h.iter().enumerate() {
                let paid = -l.total(Cat::LuxuryTax);
                tax_paid[y] += paid;
                tax_share[y] += l.total(Cat::TaxShare);
                tax_clubs[y] += (paid > 0) as i64;
                fills.push(l.average_attendance() as f32 / w.arena(t).capacity.max(1) as f32);
            }
            for t in 0..n {
                let f = w.facilities(t);
                levels[0][f.training as usize] += 1;
                levels[1][f.medical as usize] += 1;
                levels[2][f.youth as usize] += 1;
            }
            total_clubs += n as i64;
            if detail && seed == 1 {
                for (t, l) in h.iter().enumerate() {
                    println!(
                        "  {:<20} cap {:>5} att {:>5} gate {:>4} spons {:>4} bcast {:>3} shop {:>3} prize {:>3} wages {:>4} staff {:>4} fac {:>3} tax {:>4} net {:>5} close {:>5} pay {:>4} price {}",
                        w.teams[t].name,
                        w.arena(t).capacity,
                        l.average_attendance(),
                        l.total(Cat::Gate) / 1000,
                        l.total(Cat::Sponsorship) / 1000,
                        l.total(Cat::Broadcast) / 1000,
                        l.total(Cat::Merchandise) / 1000,
                        l.total(Cat::Prize) / 1000,
                        -l.total(Cat::PlayerWages) / 1000,
                        -l.total(Cat::StaffWages) / 1000,
                        -l.total(Cat::FacilityRunning) / 1000,
                        (l.total(Cat::LuxuryTax) + l.total(Cat::TaxShare)) / 1000,
                        l.net() / 1000,
                        l.closing / 1000,
                        w.payroll(t) / 1000,
                        w.ticket_price(t),
                    );
                }
            }
        }
    }
    for (y, v) in all.iter_mut().enumerate() {
        v.sort();
        let n = v.len();
        println!(
            "ALL after {}: min {} p10 {} p25 {} med {} p75 {} p90 {} max {}",
            y + 1,
            v[0],
            v[n / 10],
            v[n / 4],
            v[n / 2],
            v[3 * n / 4],
            v[9 * n / 10],
            v[n - 1]
        );
    }
    println!("\nIncome and spending by category, thousands a season, mean per club");
    let names = ["top", "middle", "bottom"];
    for y in 0..seasons as usize {
        println!("season {}", y + 1);
        print!("  {:<24}", "");
        for nm in names {
            print!("{nm:>9}");
        }
        println!();
        for (i, c) in Cat::ALL.iter().enumerate() {
            if (0..3).all(|r| by_rank[y][r][i] == 0) {
                continue;
            }
            print!("  {:<24}", c.label());
            for r in 0..3 {
                print!("{:>9}", by_rank[y][r][i] / rank_n[y][r].max(1) / 1000);
            }
            println!();
        }
        print!("  {:<24}", "NET");
        for r in 0..3 {
            let net: i64 = by_rank[y][r].iter().sum();
            print!("{:>9}", net / rank_n[y][r].max(1) / 1000);
        }
        println!();
    }
    println!(
        "\nnet a season, thousands: top med {} (min {} max {}), middle med {} (min {} max {}), bottom med {} (min {} max {})",
        pct(&mut nets_top.clone(), 50),
        pct(&mut nets_top.clone(), 0),
        pct(&mut nets_top.clone(), 100),
        pct(&mut nets_mid.clone(), 50),
        pct(&mut nets_mid.clone(), 0),
        pct(&mut nets_mid.clone(), 100),
        pct(&mut nets_bot.clone(), 50),
        pct(&mut nets_bot.clone(), 0),
        pct(&mut nets_bot.clone(), 100)
    );
    println!(
        "\nOverdraft (club-rounds below zero / frozen, clubs ever below zero / frozen, of 72 a season)"
    );
    for y in 0..seasons as usize {
        println!(
            "  season {}: {} / {} club-rounds, {} / {} clubs",
            y + 1,
            overdraft_rounds[y],
            frozen_rounds[y],
            clubs_overdrawn[y],
            clubs_frozen_ever[y]
        );
    }
    println!("  longest run of frozen rounds: {longest_frozen}");
    println!("\nLuxury tax (thousands): paid / shared / clubs paying, per season");
    for y in 0..seasons as usize {
        println!(
            "  season {}: {} / {} / {} clubs of 72",
            y + 1,
            tax_paid[y] / 1000,
            tax_share[y] / 1000,
            tax_clubs[y]
        );
    }
    println!(
        "  payroll at mid-season, thousands: min {} p25 {} med {} p75 {} max {} (cap 950)",
        pct(&mut payrolls.clone(), 0),
        pct(&mut payrolls.clone(), 25),
        pct(&mut payrolls.clone(), 50),
        pct(&mut payrolls.clone(), 75),
        pct(&mut payrolls.clone(), 100)
    );
    price_ratio.sort_by(|a, b| a.total_cmp(b));
    fills.sort_by(|a, b| a.total_cmp(b));
    println!(
        "\nAI ticket price over the reference: min {:.2} med {:.2} max {:.2}",
        price_ratio[0],
        price_ratio[price_ratio.len() / 2],
        price_ratio[price_ratio.len() - 1]
    );
    println!(
        "average fill: min {:.2} med {:.2} max {:.2}",
        fills[0],
        fills[fills.len() / 2],
        fills[fills.len() - 1]
    );
    println!("\nFacility levels at the end of each season (clubs at level 1..5, of {total_clubs})");
    for (i, nm) in ["training", "medical", "youth"].iter().enumerate() {
        println!("  {nm}: {:?}", &levels[i][1..]);
    }

    println!(
        "\nFame across the league (active players, {} seeds)",
        SEEDS.len()
    );
    let mut pooled: std::collections::BTreeMap<u16, Vec<(f32, u8, u8)>> = Default::default();
    for (season, rows) in snaps {
        pooled.entry(season).or_default().extend(rows);
    }
    for (season, rows) in &pooled {
        let mut f: Vec<f32> = rows.iter().map(|r| r.0).collect();
        let n = f.len();
        let ge = |t: f32| rows.iter().filter(|r| r.0 >= t).count() as f32 / SEEDS.len() as f32;
        let old =
            rows.iter().filter(|r| r.1 >= 35 && r.0 >= 40.0).count() as f32 / SEEDS.len() as f32;
        println!(
            "  after {season}: players {} median {:.1} p75 {:.1} p90 {:.1} p99 {:.1} max {:.1} | a league holds >=60: {:.1}, >=40: {:.1}, >=20: {:.1}, >=8: {:.1} | aged 35+ and >=40: {:.1}",
            n / SEEDS.len(),
            quant(&mut f, 50),
            quant(&mut f, 75),
            quant(&mut f, 90),
            quant(&mut f, 99),
            quant(&mut f, 100),
            ge(60.0),
            ge(40.0),
            ge(20.0),
            ge(8.0),
            old
        );
    }
    println!(
        "\nA top name against none, on otherwise identical clubs (after season 1, mean of {} clubs)",
        star_n
    );
    println!(
        "  {:<10}{:>12}{:>14}{:>12}{:>10}",
        "", "home crowd", "shop a round", "away crowd", "pull"
    );
    for (k, label) in ["no name", "fame 75"].iter().enumerate() {
        println!(
            "  {:<10}{:>12.0}{:>14.0}{:>12.0}{:>10.2}",
            label,
            star_ab[k][0] / star_n,
            star_ab[k][1] / star_n,
            star_ab[k][2] / star_n,
            star_ab[k][3] / star_n
        );
    }
    println!(
        "  the name adds {:+.0} people at home, {:+.0} in the shop a round, and {:+.0} people to the crowd at the arenas he visits",
        (star_ab[1][0] - star_ab[0][0]) / star_n,
        (star_ab[1][1] - star_ab[0][1]) / star_n,
        (star_ab[1][2] - star_ab[0][2]) / star_n
    );
    println!(
        "\nFan base as a multiple of the start: clubs finishing top two / middle / bottom two"
    );
    for y in 0..=seasons as usize {
        let f = |r: usize| traj[y][r] / traj_n[y][r].max(1.0);
        println!("  after {y}: {:.2} / {:.2} / {:.2}", f(0), f(1), f(2));
    }
    println!(
        "\nPer season, all {} leagues: gaffes, stopped by the press officer, good stories, retirements, farewells announced, names bought",
        SEEDS.len()
    );
    let per = SEEDS.len() as f64;
    for (y, e) in events.iter().enumerate() {
        println!(
            "  season {}: gaffes {:.1}, caught {:.1}, stories {:.1}, retired {:.1}, farewells {:.1}, hunts {:.1} (per league)",
            y + 1,
            e[0] as f64 / per,
            e[1] as f64 / per,
            e[2] as f64 / per,
            e[3] as f64 / per,
            e[4] as f64 / per,
            e[5] as f64 / per
        );
    }
    println!("\nPlayers per league: active / ever created / free agents / on club books");
    for (y, p) in pop.iter().enumerate() {
        println!(
            "  after {y}: {:.0} / {:.0} / {:.0} / {:.0}",
            p[0] as f64 / per,
            p[1] as f64 / per,
            p[2] as f64 / per,
            p[3] as f64 / per
        );
    }
    println!(
        "\nHall of fame after {seasons} seasons, per league: members, farewell seasons announced, the top three"
    );
    for (i, (n, f, names)) in hall.iter().enumerate() {
        println!("  seed {}: {n} members, {f} farewells; {names}", SEEDS[i]);
    }
    println!(
        "\nNames bought by clubs with empty halls and money: {}",
        hunts.len()
    );
    for (season, fee, closing, from) in hunts.iter().take(40) {
        println!(
            "  season {}: fee {}k ({}), the club's budget at the season's end {}k",
            season + 1,
            fee,
            if *from == 1 {
                "from a club"
            } else {
                "free agent"
            },
            closing
        );
    }
    if !hunts.is_empty() {
        println!(
            "  lowest closing budget among them: {}k",
            hunts.iter().map(|h| h.2).min().unwrap_or(0)
        );
    }
}
