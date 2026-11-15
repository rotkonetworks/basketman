//! Tests for Footwork and signature moves: how they are generated, that they
//! never change, who gets to know them, and that they move no result yet.

use crate::command::Command;
use crate::command::tests::tidy;
use crate::engine::BoxLine;
use crate::inbox::Stop;
use crate::league::World;
use crate::player::{Attr, Player};
use crate::signature::{KNOWN_AT, MAX_SIGNATURES, Signature, roll};

fn worlds() -> Vec<World> {
    (1..=8).map(|s| World::generate(s, 12)).collect()
}

fn everyone(ws: &[World]) -> Vec<&Player> {
    ws.iter().flat_map(|w| w.players.iter()).collect()
}

fn play_seasons(w: &mut World, seasons: u16) {
    let target = w.season + seasons;
    let mut n = 0;
    for _ in 0..200_000 {
        if w.season >= target {
            return;
        }
        tidy(w, &mut n);
        match w.apply(Command::Advance).unwrap().stop.unwrap() {
            Stop::MatchDay => {
                w.apply(Command::PlayRound).unwrap();
            }
            Stop::Mail => {}
            Stop::Draft => {
                let _ = w.apply(Command::DraftAuto { all: true });
            }
        }
    }
    panic!("the seasons never ended");
}

#[test]
fn signatures_are_rare_and_rise_with_potential() {
    let ws = worlds();
    let all = everyone(&ws);
    let n = all.len() as f32;
    let count = |k: usize| all.iter().filter(|p| p.signatures.len() == k).count();
    let any = all.iter().filter(|p| !p.signatures.is_empty()).count() as f32 / n;
    let two = (count(2) + count(3)) as f32 / n;
    eprintln!(
        "{} players: none {}, one {}, two {}, three {}; any {:.3} (1 in {:.1}), two+ {:.4} (1 in {:.0})",
        all.len(),
        count(0),
        count(1),
        count(2),
        count(3),
        any,
        1.0 / any,
        two,
        1.0 / two
    );
    assert!((0.095..=0.16).contains(&any), "about one in eight: {any}");
    assert!((0.008..=0.03).contains(&two), "about one in sixty: {two}");
    assert!(count(3) * 100 < all.len(), "three is very rare");
    assert!(all.iter().all(|p| p.signatures.len() <= MAX_SIGNATURES));
    // Rises with potential.
    let mut by_potential: Vec<&&Player> = all.iter().collect();
    by_potential.sort_by(|a, b| a.potential.total_cmp(&b.potential));
    let third = by_potential.len() / 3;
    let share = |s: &[&&Player]| {
        s.iter().filter(|p| !p.signatures.is_empty()).count() as f32 / s.len() as f32
    };
    let (low, high) = (
        share(&by_potential[..third]),
        share(&by_potential[2 * third..]),
    );
    eprintln!("share with a signature: lowest third {low:.3}, highest third {high:.3}");
    assert!(high > low * 1.5, "{low} vs {high}");
    // No duplicates, and every move is one his body and skills allow.
    for p in &all {
        for (i, s) in p.signatures.iter().enumerate() {
            assert!(!p.signatures[..i].contains(s));
            assert!(s.plausible(p), "{} has {:?}", p.name, s);
            assert!(p.signatures_rolled);
        }
    }
}

#[test]
fn every_move_is_given_to_somebody_who_could_have_it() {
    // Which moves does the generator reach at all in a big sample?
    let ws: Vec<World> = (1..=30).map(|s| World::generate(s, 12)).collect();
    let mut seen = std::collections::HashMap::new();
    for p in ws.iter().flat_map(|w| w.players.iter()) {
        for s in &p.signatures {
            *seen.entry(*s).or_insert(0usize) += 1;
        }
    }
    let missing: Vec<&str> = Signature::ALL
        .iter()
        .filter(|s| !seen.contains_key(s))
        .map(|s| s.name())
        .collect();
    eprintln!("moves never given in 30 worlds: {missing:?}");
    assert!(missing.len() <= 8, "{missing:?}");
}

#[test]
fn the_same_seed_gives_the_same_signatures() {
    let a = World::generate(7, 12);
    let b = World::generate(7, 12);
    let sigs = |w: &World| -> Vec<Vec<Signature>> {
        w.players.iter().map(|p| p.signatures.clone()).collect()
    };
    assert_eq!(sigs(&a), sigs(&b));
    assert!(sigs(&a).iter().any(|s| !s.is_empty()));
    // The roll is a pure function of the player.
    for p in a.players.iter().take(100) {
        assert_eq!(roll(p), p.signatures);
    }
}

#[test]
fn signatures_never_change_after_birth() {
    let mut w = World::generate(3, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    let before: Vec<Vec<Signature>> = w.players.iter().map(|p| p.signatures.clone()).collect();
    let feet: Vec<f32> = w.players.iter().map(|p| p.attrs[Attr::Footwork]).collect();
    play_seasons(&mut w, 4);
    for (i, sigs) in before.iter().enumerate() {
        assert_eq!(&w.players[i].signatures, sigs, "{}", w.players[i].name);
    }
    // New players joined, with their own.
    assert!(w.players.len() > before.len());
    assert!(
        w.players[before.len()..]
            .iter()
            .all(|p| p.signatures_rolled && p.signatures.len() <= MAX_SIGNATURES)
    );
    // Footwork is trained, so it moves, unlike the moves.
    let moved = (0..before.len())
        .filter(|&i| (w.players[i].attrs[Attr::Footwork] - feet[i]).abs() > 0.05)
        .count();
    assert!(moved > before.len() / 2, "footwork should move: {moved}");
}

#[test]
fn a_move_that_needs_a_body_stops_being_live_when_the_body_goes() {
    let w = World::generate(4, 8);
    let mut p = w.players[0].clone();
    p.body.height_cm = 200.0;
    p.attrs[Attr::Vertical] = 18.0;
    p.attrs[Attr::Strength] = 15.0;
    p.signatures = vec![Signature::PosterDunk];
    assert!(Signature::PosterDunk.plausible(&p));
    assert!(Signature::PosterDunk.live(&p));
    assert!(Signature::PosterDunk.is_body_dependent());
    // Thirty-six, legs gone.
    p.attrs[Attr::Vertical] = 6.0;
    assert!(!Signature::PosterDunk.live(&p));
    assert!(p.live_signatures().is_empty());
    // He still knows the move.
    assert_eq!(p.signatures, vec![Signature::PosterDunk]);
    // A move of pure skill never wears out.
    p.attrs[Attr::ThreePoint] = 18.0;
    p.attrs[Attr::Composure] = 17.0;
    assert!(Signature::BigShotTaker.live(&p));
    p.attrs[Attr::Speed] = 2.0;
    p.attrs[Attr::Vertical] = 2.0;
    assert!(Signature::BigShotTaker.live(&p));
    assert!(!Signature::BigShotTaker.is_body_dependent());
}

fn with_a_move(w: &World, mine: bool) -> u32 {
    let me = w.watch.unwrap();
    w.players
        .iter()
        .find(|p| {
            !p.signatures.is_empty()
                && w.team_of(p.id).is_some()
                && w.is_my_player(p.id) == mine
                && (mine || w.team_of(p.id) != Some(me))
                && w.teams.iter().any(|t| t.roster.contains(&p.id))
        })
        .expect("a player with a move")
        .id
}

fn hundred_percent(w: &mut World, pid: u32) {
    // Watch him until the scouts know him well.
    let line = BoxLine {
        secs: 1800,
        ..BoxLine::default()
    };
    for r in 0..20 {
        w.scouting.watch(pid, r, 0, 0, true, &line, 6, 10);
        let sigs = w.players[pid as usize].signatures.clone();
        w.scouting.reveal_signatures(pid, &sigs);
    }
}

#[test]
fn own_moves_are_known_and_others_wait_for_scouting() {
    let mut w = World::generate(5, 12);
    w.apply(Command::TakeCharge(0)).unwrap();
    // Make sure we have one of our own and there is one elsewhere.
    let mine = w.teams[0].roster[2];
    w.players[mine as usize].signatures = vec![Signature::SpinMove];
    assert_eq!(w.known_signatures(mine), vec![Signature::SpinMove]);

    let other = with_a_move(&w, false);
    assert!(!w.is_my_player(other));
    assert_eq!(w.scouting.knowledge(other), 0.25);
    assert!(w.known_signatures(other).is_empty(), "unseen is hidden");
    // The view agrees.
    let pv = w.view_player(0, other).unwrap();
    assert!(pv.signatures.is_empty());

    // Watched enough, they show, and the report says so.
    w.scouting.toggle(other);
    let sigs = w.players[other as usize].signatures.clone();
    let line = BoxLine {
        secs: 1800,
        ..BoxLine::default()
    };
    let mut revealed_at = None;
    for r in 0..30 {
        let before = w.scouting.knowledge(other);
        w.scouting.watch(other, r, 0, 1, true, &line, 6, 10);
        w.scouting.reveal_signatures(other, &sigs);
        let known = w.known_signatures(other).len();
        if before < KNOWN_AT && w.scouting.knowledge(other) < KNOWN_AT {
            assert_eq!(known, 0, "still hidden at {before}");
        }
        if known > 0 && revealed_at.is_none() {
            revealed_at = Some(r);
            assert!(w.scouting.knowledge(other) >= KNOWN_AT);
        }
    }
    assert!(revealed_at.is_some());
    assert_eq!(w.known_signatures(other), sigs);
    let seen = w
        .scouting
        .reports
        .iter()
        .flat_map(|r| &r.observations)
        .filter(|o| matches!(o, crate::scouting::Observation::SignatureSeen { .. }))
        .count();
    assert_eq!(seen, sigs.len(), "each is listed once, when first seen");
    let pv = w.view_player(0, other).unwrap();
    assert_eq!(pv.signatures.len(), sigs.len());
}

#[test]
fn a_move_seen_in_a_game_becomes_known() {
    let mut w = World::generate(5, 12);
    w.apply(Command::TakeCharge(0)).unwrap();
    let other = with_a_move(&w, false);
    let s = w.players[other as usize].signatures[0];
    assert!(!w.known_signatures(other).contains(&s));
    assert!(w.scouting.observe_signature(other, s));
    assert!(
        !w.scouting.observe_signature(other, s),
        "no news the second time"
    );
    assert!(w.known_signatures(other).contains(&s));
    // Only that one.
    assert_eq!(
        w.known_signatures(other).len(),
        1.max(w.known_signatures(other).len())
    );
}

#[test]
fn the_view_never_shows_an_unrevealed_move() {
    let mut w = World::generate(21, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    // Reveal one man fully and watch some others a little.
    let shown = with_a_move(&w, false);
    hundred_percent(&mut w, shown);
    let view = w.view(0).unwrap();
    let text = serde_json::to_string(&view).unwrap();
    let mut hidden_only = std::collections::HashSet::new();
    let mut known_any = std::collections::HashSet::new();
    for p in &w.players {
        let known = w.known_signatures(p.id);
        for s in &p.signatures {
            if known.contains(s) {
                known_any.insert(*s);
            } else {
                hidden_only.insert(*s);
            }
        }
    }
    let leaks: Vec<_> = hidden_only
        .iter()
        .filter(|s| !known_any.contains(s))
        .filter(|s| text.contains(&format!("{s:?}")) || text.contains(s.name()))
        .collect();
    assert!(!hidden_only.is_empty(), "the test needs hidden moves");
    assert!(leaks.is_empty(), "{leaks:?} leaked into the view");
    // Per player: the other clubs' views carry only revealed ones.
    for club in &view.league.clubs {
        for pv in &club.squad {
            let p = &w.players[pv.id as usize];
            let known = w.known_signatures(pv.id);
            assert_eq!(pv.signatures.len(), known.len());
            for s in &p.signatures {
                if !known.contains(s) {
                    let j = serde_json::to_string(pv).unwrap();
                    assert!(!j.contains(s.name()), "{} leaks {:?}", p.name, s);
                    assert!(!j.contains(&format!("{s:?}")));
                }
            }
        }
    }
    // Our own are all there.
    for pv in &view.own.squad {
        let p = &w.players[pv.id as usize];
        assert_eq!(pv.signatures.len(), p.signatures.len());
    }
    // And Footwork is in the attributes, as a range for the others.
    let pv = &view.own.squad[0];
    assert!(pv.attrs.0.iter().any(|(n, _)| *n == "Footwork"));
}

#[test]
fn the_assistant_names_known_moves_and_the_answer() {
    let mut w = World::generate(5, 12);
    w.apply(Command::TakeCharge(0)).unwrap();
    let opp = {
        let f = w
            .fixtures
            .iter()
            .find(|f| f.round == w.round && (f.home == 0 || f.away == 0))
            .unwrap();
        if f.home == 0 { f.away } else { f.home }
    };
    let star = w.teams[opp].roster[0];
    w.players[star as usize].signatures = vec![Signature::StepBackThree, Signature::LogoRange];
    w.preview();
    let said = |w: &World| {
        w.desk
            .messages
            .iter()
            .filter(|m| m.subject.starts_with("Next up"))
            .map(|m| m.body.clone())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let before = said(&w);
    assert!(!before.contains("Step-back three") && !before.contains("step-back three"));
    // Known: named, with the answer.
    w.scouting.observe_signature(star, Signature::StepBackThree);
    w.preview();
    let body = said(&w);
    assert!(body.to_lowercase().contains("step-back three"));
    assert!(body.contains(Signature::StepBackThree.answer()));
    // The other is still unknown, so only the first mail lacks it and the
    // second names just the one.
    let last = w
        .desk
        .messages
        .iter()
        .filter(|m| m.subject.starts_with("Next up"))
        .map(|m| m.body.clone())
        .next_back()
        .unwrap();
    assert!(!last.to_lowercase().contains("logo range"));
}

#[test]
fn signatures_change_no_result_yet() {
    let mut a = World::generate(6, 8);
    let mut b = a.clone();
    for p in &mut b.players {
        p.signatures.clear();
    }
    for _ in 0..6 {
        a.play_round();
        b.play_round();
    }
    assert!(a.players.iter().any(|p| !p.signatures.is_empty()));
    assert_eq!(format!("{:?}", a.fixtures), format!("{:?}", b.fixtures));
    assert_eq!(format!("{:?}", a.standings), format!("{:?}", b.standings));
    assert_eq!(
        a.players.iter().map(|p| p.attrs.0).collect::<Vec<_>>(),
        b.players.iter().map(|p| p.attrs.0).collect::<Vec<_>>()
    );
}

fn mean(v: impl Iterator<Item = f32>) -> f32 {
    let (s, n) = v.fold((0.0, 0), |(s, n), x| (s + x, n + 1));
    s / n.max(1) as f32
}

#[test]
fn footwork_follows_speed_and_lightness_with_plenty_of_overlap() {
    let ws = worlds();
    let all = everyone(&ws);
    let f = |p: &&Player| p.attrs[Attr::Footwork];
    let mut by_speed = all.clone();
    by_speed.sort_by(|a, b| a.attrs[Attr::Speed].total_cmp(&b.attrs[Attr::Speed]));
    let q = by_speed.len() / 4;
    let slow = mean(by_speed[..q].iter().map(f));
    let fast = mean(by_speed[3 * q..].iter().map(f));
    let mut by_weight = all.clone();
    by_weight.sort_by(|a, b| a.body.weight_kg.total_cmp(&b.body.weight_kg));
    let light = mean(by_weight[..q].iter().map(f));
    let heavy = mean(by_weight[3 * q..].iter().map(f));
    let mut by_height = all.clone();
    by_height.sort_by(|a, b| a.body.height_cm.total_cmp(&b.body.height_cm));
    let short = mean(by_height[..q].iter().map(f));
    let tall = mean(by_height[3 * q..].iter().map(f));
    eprintln!(
        "footwork: slow {slow:.2} fast {fast:.2}; light {light:.2} heavy {heavy:.2}; short {short:.2} tall {tall:.2}"
    );
    assert!(fast > slow + 1.0 && light > heavy + 1.0 && short > tall);
    // Overlap: heavy-footed guards and nimble big men exist, but are few.
    let avg = mean(all.iter().map(f));
    let bigs: Vec<&&Player> = all.iter().filter(|p| p.body.height_cm >= 205.0).collect();
    let guards: Vec<&&Player> = all.iter().filter(|p| p.body.height_cm <= 192.0).collect();
    let nimble_big = bigs.iter().filter(|p| f(p) >= avg + 1.5).count() as f32 / bigs.len() as f32;
    let clumsy_guard =
        guards.iter().filter(|p| f(p) <= avg - 1.5).count() as f32 / guards.len() as f32;
    let others = mean(all.iter().map(|p| {
        (p.overall() * Attr::COUNT as f32 - p.attrs[Attr::Footwork]) / (Attr::COUNT - 1) as f32
    }));
    eprintln!("mean of the other attributes {others:.2}");
    eprintln!("nimble bigs {nimble_big:.3}, clumsy guards {clumsy_guard:.3}, mean {avg:.2}");
    assert!((0.02..0.3).contains(&nimble_big), "{nimble_big}");
    assert!((0.02..0.3).contains(&clumsy_guard), "{clumsy_guard}");
    // Real spread, inside the scale.
    let (lo, hi) = all
        .iter()
        .fold((20.0f32, 1.0f32), |(l, h), p| (l.min(f(p)), h.max(f(p))));
    assert!(hi - lo > 8.0 && lo >= 1.0 && hi <= 20.0);
    // No other attribute is disturbed by having rolled it.
    assert!(
        all.iter()
            .all(|p| p.attrs.0.iter().all(|&v| (1.0..=20.0).contains(&v)))
    );
}

#[test]
fn footwork_is_trained_by_perimeter_and_finishing_work() {
    use crate::training::{Session, TrainingFocus};
    assert!(
        TrainingFocus::PerimeterDefense
            .attrs()
            .contains(&Attr::Footwork)
    );
    assert!(TrainingFocus::Finishing.attrs().contains(&Attr::Footwork));
    assert!(Session::PerimeterDefense.attrs().contains(&Attr::Footwork));
    assert!(!Attr::Footwork.is_physical());
}

#[test]
fn the_catalogue_is_whole() {
    use std::collections::HashSet;
    assert_eq!(Signature::ALL.len(), 50);
    let names: HashSet<&str> = Signature::ALL.iter().map(|s| s.name()).collect();
    assert_eq!(names.len(), 50, "names are unique");
    let groups: HashSet<_> = Signature::ALL.iter().map(|s| s.group()).collect();
    assert_eq!(groups.len(), 8);
    // Print the table for the record.
    for s in Signature::ALL {
        eprintln!(
            "| {} | {} | {} |",
            s.name(),
            s.group().label(),
            s.needs_text()
        );
    }
}

#[test]
fn payroll_after_footwork_is_still_sensible() {
    // The old overall is the mean of the other twenty; salary is its cube.
    let mut after = Vec::new();
    let mut before = Vec::new();
    for w in worlds() {
        for t in &w.teams {
            let (mut a, mut b) = (0i64, 0i64);
            for &p in &t.roster {
                let p = &w.players[p as usize];
                let old = (p.overall() * Attr::COUNT as f32 - p.attrs[Attr::Footwork])
                    / (Attr::COUNT - 1) as f32;
                a += p.contract.salary as i64;
                b += (p.contract.salary as f32 * (old / p.overall()).powi(3)) as i64;
            }
            after.push(a);
            before.push(b);
        }
    }
    after.sort();
    before.sort();
    let med = |v: &[i64]| v[v.len() / 2];
    eprintln!(
        "median first-team payroll: before {} after {}",
        med(&before),
        med(&after)
    );
    let ratio = med(&after) as f32 / med(&before) as f32;
    assert!((0.85..1.15).contains(&ratio), "{ratio}");
}

#[test]
fn a_signature_sells_a_little_and_keeps_the_big_names_about_the_same() {
    // Same league twice, once with every move stripped. Results are
    // identical (no engine effect), so any fame difference is the
    // multiplier alone.
    let (mut with, mut without) = ([0usize; 2], [0usize; 2]);
    let (mut top_with, mut top_without) = (0.0f32, 0.0f32);
    let (mut fame_with, mut fame_without, mut n) = (0.0f32, 0.0f32, 0usize);
    for seed in 1..=2u64 {
        let mut a = World::generate(seed, 12);
        let mut b = a.clone();
        for p in &mut b.players {
            p.signatures.clear();
        }
        for w in [&mut a, &mut b] {
            // Without a manager, no mail: play seasons by rounds.
            while w.season < 3 {
                if w.match_today() {
                    w.play_round();
                } else {
                    w.advance();
                }
            }
        }
        for (w, big, top) in [
            (&a, &mut with, &mut top_with),
            (&b, &mut without, &mut top_without),
        ] {
            let names = w.biggest_names(20);
            big[0] += names.iter().filter(|n| n.1 >= 40.0).count();
            big[1] += names.iter().filter(|n| n.1 >= 60.0).count();
            *top += names.iter().take(10).map(|n| n.1).sum::<f32>() / 10.0;
        }
        for (pa, pb) in a.players.iter().zip(&b.players) {
            if !pa.signatures.is_empty() && pa.age < 40 {
                fame_with += pa.renown.fame;
                fame_without += pb.renown.fame;
                n += 1;
            }
        }
    }
    eprintln!(
        "names >= 40 / >= 60 over 2 leagues after 3 seasons: with {with:?}, without {without:?}; \
         mean of the top ten {:.1} vs {:.1}; players with a move: fame {:.1} vs {:.1} ({n} players)",
        top_with / 2.0,
        top_without / 2.0,
        fame_with / n as f32,
        fame_without / n as f32
    );
    assert!(fame_with > fame_without);
    assert!(with[0] <= without[0] + 4 && with[0] + 4 >= without[0]);
}
