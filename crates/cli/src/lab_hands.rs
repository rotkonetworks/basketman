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
        let (r, rec) = if let Ok(spec) = std::env::var("HANDS_TRACE")
            && spec.split(':').next().and_then(|s| s.parse::<usize>().ok()) == Some(g)
        {
            // The configuration of the engine's own tests.
            let b2 = {
                let b = (g * 5 + 1 + g / 12) % 12;
                if g % 12 == b { (b + 1) % 12 } else { b }
            };
            let hl = basketman_core::league::lineup(&w.teams[g % 12], &w.players);
            let al = basketman_core::league::lineup(&w.teams[b2], &w.players);
            simulate_recorded(&hl, &al, &w.rules, &mut Rng::new(std::env::var("HANDS_SEED").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(300) + g as u64))
        } else {
            simulate_recorded(&la, &lb, &w.rules, &mut rng)
        };
        secs += t0.elapsed().as_secs_f64();
        if let Ok(spec) = std::env::var("HANDS_TRACE") {
            let mut it = spec.split(':');
            let (tg, tt): (usize, f32) = (it.next().and_then(|s| s.parse().ok()).unwrap_or(0), it.next().and_then(|s| s.parse().ok()).unwrap_or(0.0));
            if tg == g {
                for f in &rec.frames {
                    if f.time > tt - 1.5 && f.time < tt + 0.5 {
                        println!("t {:.2} cut {} holder {:?} ball {:.1},{:.1} z {:.2} offense {}", f.time, f.cut, f.holder, f.ball.x, f.ball.y, f.ball_height, f.offense);
                    }
                }
                for k in &rec.pokes {
                    if k.time > tt - 3.0 && k.time < tt + 0.5 {
                        println!("poke {:.2} {:?} gap {} cm", k.time, k.result, k.gap_cm);
                    }
                }
                for p in &rec.passes {
                    if p.time > tt - 3.0 && p.time < tt + 0.5 {
                        println!("pass {:.2} {:?} {:?}", p.time, p.kind, p.outcome);
                    }
                }
                for e in &rec.events {
                    if e.time > tt - 3.0 && e.time < tt + 0.5 {
                        println!("event {:.2} {:?}", e.time, e.kind);
                    }
                }
            }
        }
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

/// Does each rating keep its meaning? Team A of each game has one attribute
/// moved for every man, against the same opponent and seeds; the table shows
/// what changed for team A.
pub fn relations(games: usize) {
    use basketman_core::player::Attr;
    let base = World::generate(41, 12);
    let n = base.teams.len();
    let rows: [(&str, Attr); 3] = [
        ("passing", Attr::Passing),
        ("handling", Attr::Handling),
        ("ballcare", Attr::Handling),
    ];
    println!("{:<12} {:>6} | {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}", "attribute", "delta", "tov/100", "badpass", "strips", "ast/fgm", "stl/100", "pts/100", "pass/100");
    for (name, attr) in rows {
        for delta in [-8.0f32, 0.0, 6.0] {
            let mut w = base.clone();
            let (mut tov, mut bad, mut strip, mut poss) = (0u32, 0u32, 0u32, 0u32);
            let mut outc = [0u32; 6];
            let (mut ast, mut fgm, mut stl, mut dposs, mut pts, mut passes) = (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
            for g in 0..games {
                let a = g % n;
                let b = (g * 5 + 1 + g / n) % n;
                if a == b {
                    continue;
                }
                // Team a's players altered for this game.
                let ids: Vec<u32> = w.teams[a].roster.iter().map(|&p| p as u32).collect();
                for &p in &ids {
                    let pl = &mut w.players[p as usize];
                    pl.attrs[attr] = (base.players[p as usize].attrs[attr] + delta).clamp(1.0, 20.0);
                    if name == "ballcare" {
                        pl.attrs[Attr::Passing] = if delta < 0.0 { 2.0 } else { base.players[p as usize].attrs[Attr::Passing] };
                    }
                }
                let (ta, tb) = (w.teams[a].tactics().clone(), w.teams[b].tactics().clone());
                let (la, lb) = (mk(&w, &ta), mk(&w, &tb));
                let mut rng = Rng::new(7000 + g as u64 * 31);
                let (r, rec) = simulate_recorded(&la, &lb, &w.rules, &mut rng);
                for &p in &ids {
                    w.players[p as usize].attrs[attr] = base.players[p as usize].attrs[attr];
                    w.players[p as usize].attrs[Attr::Passing] = base.players[p as usize].attrs[Attr::Passing];
                }
                poss += r.possessions[0] as u32;
                dposs += r.possessions[1] as u32;
                for (id, l) in &r.boxes[0] {
                    let _ = id;
                    tov += l.tov as u32;
                    ast += l.ast as u32;
                    fgm += l.fgm as u32;
                    pts += l.pts as u32;
                }
                for (_, l) in &r.boxes[1] {
                    let _ = l;
                }
                for (_, l) in &r.boxes[0] {
                    stl += l.stl as u32;
                }
                passes += rec.passes.iter().filter(|p| p.side == 0).count() as u32;
                for p in rec.passes.iter().filter(|p| p.side == 0) {
                    outc[p.outcome as usize] += 1;
                }
                let mine: std::collections::HashSet<u32> = r.boxes[0].iter().map(|(id, _)| *id as u32).collect();
                for e in &rec.events {
                    if let EventKind::Turnover { who, kind, .. } = &e.kind
                        && mine.contains(&(*who as u32))
                    {
                        match kind {
                            TurnoverKind::BadPass => bad += 1,
                            TurnoverKind::Stripped => strip += 1,
                            _ => {}
                        }
                    }
                }
            }
            let p = poss.max(1) as f32;
            println!(
                "{:<12} {:>+6.0} | {:>8.2} {:>8.2} {:>8.2} {:>8.3} {:>8.2} {:>8.1} {:>8.1}",
                name,
                delta,
                tov as f32 / p * 100.0,
                bad as f32 / p * 100.0,
                strip as f32 / p * 100.0,
                ast as f32 / fgm.max(1) as f32,
                stl as f32 / dposs.max(1) as f32 * 100.0,
                pts as f32 / p * 100.0,
                passes as f32 / p * 100.0
            );
            let pn = passes.max(1) as f32;
            println!("{:>20} caught {:.1} bobbled {:.1} tipped {:.1} stolen {:.1} out {:.1} missed {:.1}", "", outc[0] as f32 * 100.0 / pn, outc[1] as f32 * 100.0 / pn, outc[2] as f32 * 100.0 / pn, outc[3] as f32 * 100.0 / pn, outc[4] as f32 * 100.0 / pn, outc[5] as f32 * 100.0 / pn);
        }
    }
}

/// One half-court possession written out from the recording: who has the
/// ball, each pass with its kind and what became of it, who cut where, and
/// how it ended. `set` is the offense (0 Motion ... by `OffenseSet::ALL`).
pub fn possession(set: usize, nth: usize) {
    use basketman_core::court::{HOOP, Vec2};
    let w = World::generate(7, 12);
    let n = w.teams.len();
    let mut shown = 0;
    for g in 0..60usize {
        let (a, b) = (g % n, (g * 5 + 1 + g / n) % n);
        if a == b {
            continue;
        }
        let (mut ta, mut tb) = (w.teams[a].tactics().clone(), w.teams[b].tactics().clone());
        ta.offense = OffenseSet::ALL[set % OffenseSet::ALL.len()];
        ta.defense = DefenseScheme::ManToMan;
        tb.defense = DefenseScheme::ManToMan;
        tb.offense = OffenseSet::Motion;
        let (la, lb) = (mk(&w, &ta), mk(&w, &tb));
        let (r, rec) = simulate_recorded(&la, &lb, &w.rules, &mut Rng::new(4000 + g as u64));
        // The names, by player id.
        let name = |id: u32| -> String { { let n = &w.players[id as usize].name; let mut it = n.split(' '); let first = it.next().unwrap_or("?"); format!("{}.{}#{}", &first[..1], it.next_back().unwrap_or("?"), id) } };
        let slot_name = |side: usize, slot: u8, f: &basketman_core::replay::Frame| -> String {
            let who = f.who[side][slot as usize] as usize;
            name(r.boxes[side][who].0 as u32)
        };
        // Possessions of side 0 in the half court, found by frames where
        // side 0 has the offense without a cut for at least eight seconds.
        let mut i = 0;
        while i < rec.frames.len() {
            if rec.frames[i].offense != 0 {
                i += 1;
                continue;
            }
            let start = i;
            while i < rec.frames.len() && rec.frames[i].offense == 0 && !rec.frames[i].cut {
                i += 1;
            }
            let (t0, t1) = (rec.frames[start].time, rec.frames[i - 1].time);
            let ps: Vec<_> = rec.passes.iter().filter(|p| p.side == 0 && p.time >= t0 && p.time <= t1 && !p.inbound).collect();
            if t1 - t0 < 9.0 || ps.len() < 3 {
                continue;
            }
            shown += 1;
            if shown <= nth {
                continue;
            }
            println!("{:?} offense, possession from {:.1} s to {:.1} s of the recording ({:.1} s)", ta.offense, t0, t1, t1 - t0);
            let mut last_cut = [-9.0f32; 5];
            for f in &rec.frames[start..i] {
                // Who has it.
                if let Some((s, k)) = f.holder
                    && s == 0
                    && (f.time * 10.0).round() as i64 % 20 == 0
                {
                    println!("  {:6.1}  ball with {}", f.time, slot_name(0, k, f));
                }
            }
            for p in &ps {
                let f = &rec.frames[rec.frame_at(p.time)];
                let to_name = slot_name(0, p.to, f);
                println!(
                    "  {:6.1}  {} passes to {} ({:?}, {:?})",
                    p.time, slot_name(0, p.from, f), to_name, p.kind, p.outcome
                );
            }
            // Cuts: a man without the ball who goes at least three metres
            // toward the basket in a second and a half at a run.
            for j in start + 15..i {
                let (f0, f1) = (&rec.frames[j - 15], &rec.frames[j]);
                let hoop = Vec2 { x: HOOP.x, y: HOOP.y };
                let _ = hoop;
                for k in 0..5 {
                    if f1.holder == Some((0, k as u8)) || f0.holder == Some((0, k as u8)) {
                        continue;
                    }
                    let rim = if f1.offense == 0 { Vec2 { x: 26.425, y: 7.5 } } else { Vec2 { x: 1.575, y: 7.5 } };
                    let (d0, d1) = (f0.pos[0][k].dist(rim), f1.pos[0][k].dist(rim));
                    if d0 - d1 > 3.0 && d1 < 5.5 && f1.time - last_cut[k] > 3.0 {
                        last_cut[k] = f1.time;
                        println!("  {:6.1}  {} cuts to the basket ({:.1} m out to {:.1} m)", f0.time, slot_name(0, k as u8, f1), d0, d1);
                    }
                }
            }
            for e in rec.events.iter().filter(|e| e.time >= t0 && e.time <= t1 + 1.0 && e.offense == 0) {
                match &e.kind {
                    EventKind::Shot { who, kind, made, source, assist, .. } => println!(
                        "  {:6.1}  {} shoots a {:?} ({:?}) and {}{}",
                        e.time, name(*who as u32), kind, source, if *made { "makes it" } else { "misses" },
                        assist.map_or(String::new(), |a| format!(", assisted by {}", name(a as u32)))
                    ),
                    EventKind::Turnover { who, kind, .. } => println!("  {:6.1}  turnover by {} ({:?})", e.time, name(*who as u32), kind),
                    _ => {}
                }
            }
            return;
        }
    }
    println!("no such possession found");
}
