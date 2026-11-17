//! Where a defensive scheme wants its five players, given the floor.
//!
//! These are the coaching rules ("ball-you-man", zone slides, drop coverage)
//! written as functions of the ball and the attackers. Players then *try* to
//! get there: how late they are is decided in `floor.rs` by their speed,
//! reactions and how well they know the system.

use crate::court::{HOOP, LENGTH, Vec2, v};
use crate::tactics::DefenseScheme;

/// A man this close to the basket, with the ball away from the paint, is
/// fronted, a body's width up the line to the ball.
const FRONT_RANGE: f32 = 2.4;
const FRONT_GAP: f32 = 0.7;

pub(super) struct View {
    pub scheme: DefenseScheme,
    /// Attackers and defenders, current positions.
    pub o: [Vec2; 5],
    pub d: [Vec2; 5],
    /// Where the ball is, or where a pass in the air will arrive.
    pub ball: Vec2,
    /// Attacker with the ball, or about to catch it.
    pub owner: Option<usize>,
    /// `guard[dk]` is the attacker defender `dk` is matched with.
    pub guard: [usize; 5],
    /// Extra room each defender gives on the ball because he is slower.
    pub cushion: [f32; 5],
    /// 0 = no outside shot, 1 = must be guarded at the line.
    pub threat: [f32; 5],
    /// -1 = passive, 1 = gambling.
    pub aggression: f32,
    /// The ball handler has got past his defender.
    pub beaten: bool,
    /// Defender playing drop coverage behind a ball screen.
    pub drop: Option<usize>,
    /// The screener's man is stepping up to show himself to the ball handler.
    pub hedge: bool,
    /// Nobody defends further up the floor than this.
    pub pickup: f32,
    /// A shot is in the air.
    pub boxout: bool,
    /// The ball handler is attacking off the dribble.
    pub driving: bool,
    /// The ball is held with a back to the basket.
    pub posted: bool,
}

#[derive(Copy, Clone)]
pub(super) struct Aims {
    pub at: [Vec2; 5],
    pub on_ball: Option<usize>,
    pub helper: Option<usize>,
}

pub(super) fn aims(view: &View) -> Aims {
    let mut a = match view.scheme {
        DefenseScheme::ManToMan | DefenseScheme::SwitchEverything | DefenseScheme::PackLine => {
            man(view)
        }
        DefenseScheme::Zone23 => zone23(view),
        DefenseScheme::Zone32 => zone32(view),
    };
    if view.boxout {
        boxout(view, &mut a);
    } else if view.beaten {
        help(view, &mut a);
    }
    for p in &mut a.at {
        p.y = p.y.min(view.pickup).clamp(0.3, LENGTH - 0.3);
        p.x = p.x.clamp(-7.3, 7.3);
    }
    a
}

pub(super) fn is_zone(scheme: DefenseScheme) -> bool {
    matches!(scheme, DefenseScheme::Zone23 | DefenseScheme::Zone32)
}

fn man(view: &View) -> Aims {
    let pack = view.scheme == DefenseScheme::PackLine;
    let on_ball = view
        .owner
        .and_then(|i| view.guard.iter().position(|&g| g == i));
    let mut at = [Vec2::ZERO; 5];
    for (dk, aim) in at.iter_mut().enumerate() {
        let i = view.guard[dk];
        let m = view.o[i];
        let dh = m.dist(HOOP);
        *aim = if Some(dk) == on_ball {
            // Between the ball and the basket. Sag off a non-shooter.
            let mut gap = 1.05 - 0.25 * view.aggression + view.cushion[dk];
            if dh > 5.5 {
                gap += (1.0 - view.threat[i]).max(0.0) * 1.3;
            }
            if dh < 3.0 {
                gap = 0.55;
            }
            m.toward(HOOP, gap.min(dh * 0.8))
        } else if dh < FRONT_RANGE && view.ball.dist(HOOP) > 3.0 && view.ball.dist(m) > 2.0 {
            // A man planted at the rim with the ball out on the floor is
            // fronted: the defender stands on the ball side, between the
            // passer and him, so the entry has to go over a body and a hand
            // and the man cannot simply be fed under the basket.
            m.toward(view.ball, FRONT_GAP)
        } else if dh < 4.0 {
            // Post player or cutter: stay attached. On the ball side that
            // means three-quarter fronting from the high side, so the entry
            // pass has to go past a hand (WABC L3 p. 82).
            if m.dist(view.ball) < 7.0 && view.ball.dist(HOOP) > 4.5 {
                m.toward(HOOP, 0.45f32.min(dh * 0.5)).lerp(view.ball, 0.11)
            } else {
                m.toward(HOOP, 0.6f32.min(dh * 0.6)).lerp(view.ball, 0.08)
            }
        } else {
            // Ball-you-man. One pass away a pressure defense sits in the
            // passing lane; the pack line sits in the driving gap instead.
            // Two passes away everybody is in help.
            let near = m.dist(view.ball) < 6.8;
            let (mut up, to_ball) = match (pack, near) {
                (false, true) => (0.17 - 0.06 * view.aggression, 0.10),
                (false, false) => (0.33, 0.25),
                (true, true) => (0.36, 0.18),
                (true, false) => (0.50, 0.25),
            };
            // Sag off non-shooters, stay attached to the dangerous ones.
            up += (1.0 - view.threat[i]).max(0.0) * 0.16;
            let hot = ((view.threat[i] - 0.75) * 4.0).clamp(0.0, 1.0);
            let keyed = (view.threat[i] - 1.0).clamp(0.0, 0.6);
            up *= 1.0 - 0.4 * hot - 0.6 * keyed;
            m.lerp(HOOP, up).lerp(view.ball, to_ball)
        };
    }
    // Stunt at a dribbler, dig at a post: the nearest perimeter defender
    // below the ball steps at it and recovers. Never off a dangerous
    // shooter (WABC L1 Team 1.1.8, L2 pp. 228-229).
    if (view.driving || view.posted)
        && let Some(ob) = on_ball
    {
        let reach = view.ball.dist(HOOP) + 1.0;
        let mut best: Option<(f32, usize)> = None;
        for dk in 0..5 {
            let i = view.guard[dk];
            if dk == ob
                || view.drop == Some(dk)
                || view.threat[i] > if pack { 1.15 } else { 0.9 }
                || view.o[i].dist(HOOP) < 4.0
                || view.d[dk].dist(HOOP) > reach
            {
                continue;
            }
            let dist = view.d[dk].dist(view.ball);
            if dist < 5.5 && best.is_none_or(|b| dist < b.0) {
                best = Some((dist, dk));
            }
        }
        if let Some((_, dk)) = best {
            let spot = view.ball.toward(HOOP, 1.2);
            at[dk] = at[dk].lerp(spot, if view.posted { 0.55 } else { 0.4 });
        }
        // The low man loads to the lane as the drive starts rather than
        // after his team-mate is beaten.
        if view.driving {
            let mut low: Option<(f32, usize)> = None;
            for dk in 0..5 {
                let d = at[dk].dist(HOOP);
                if dk != ob && view.drop != Some(dk) && low.is_none_or(|l| d < l.0) {
                    low = Some((d, dk));
                }
            }
            if let Some((d, dk)) = low
                && d < 4.5
            {
                let spot = HOOP.toward(view.ball, 1.5);
                at[dk] = at[dk].lerp(spot, if pack { 0.7 } else { 0.5 });
            }
        }
    }
    // Drop coverage: the screener's defender backs off to contain the ball.
    if let Some(dk) = view.drop
        && Some(dk) != on_ball
    {
        // Stay between the ball and the basket, but never let the roller
        // get behind.
        let roller = view.o[view.guard[dk]];
        let depth =
            (view.ball.dist(HOOP) * if pack { 0.4 } else { 0.5 }).min(roller.dist(HOOP) + 1.0);
        at[dk] = HOOP.toward(view.ball, depth).lerp(roller, 0.15);
        // Showing: out in front of the handler for a moment, the roller left.
        if view.hedge {
            at[dk] = view.ball.toward(HOOP, 1.5);
        }
    }
    Aims {
        at,
        on_ball,
        helper: None,
    }
}

/// Step into the path of a beaten defender's man, and cover for whoever did.
fn help(view: &View, a: &mut Aims) {
    // The two defenders best placed to stop the ball leave their men for the
    // path to the basket: the nearest first, and a second from the other
    // side of the floor, a step behind him. Whoever they leave is open.
    let mut cands: Vec<(f32, usize, Vec2)> = Vec::new();
    for dk in 0..5 {
        if Some(dk) == a.on_ball {
            continue;
        }
        let q = view.d[dk];
        let on_path = q.project(view.ball, HOOP);
        if on_path.dist(view.ball) < 0.8 {
            continue;
        }
        let mut cost = q.dist(on_path);
        if view.drop == Some(dk) {
            cost -= 1.5;
        }
        if cost < 3.8 {
            cands.push((cost, dk, on_path));
        }
    }
    cands.sort_by(|x, y| x.0.total_cmp(&y.0));
    let Some(&(_, helper, on_path)) = cands.first() else {
        return;
    };
    a.at[helper] = if on_path.dist(HOOP) < 2.2 {
        HOOP.toward(view.ball, 1.1)
    } else {
        on_path
    };
    a.helper = Some(helper);
    if is_zone(view.scheme) {
        return;
    }
    // The second: a man whose own attacker is not near the ball, one step
    // behind the first, so the pass out has to beat him too.
    let second = cands.iter().skip(1).find(|c| view.o[view.guard[c.1]].dist(view.ball) > 3.5);
    if let Some(&(_, dk, p)) = second {
        let base = if p.dist(HOOP) < 2.2 { HOOP.toward(view.ball, 1.1) } else { p };
        a.at[dk] = base.toward(HOOP, 0.9_f32.min(base.dist(HOOP)));
    }
    // Help the helper: the nearest free defender splits his own man and the
    // one just left alone.
    let left = view.o[view.guard[helper]];
    let mut cover: Option<(f32, usize)> = None;
    for dk in 0..5 {
        if Some(dk) == a.on_ball || dk == helper || second.is_some_and(|s| s.1 == dk) {
            continue;
        }
        let dist = view.d[dk].dist(left);
        if dist < 6.5 && cover.is_none_or(|c| dist < c.0) {
            cover = Some((dist, dk));
        }
    }
    if let Some((_, dk)) = cover {
        a.at[dk] = view.o[view.guard[dk]].lerp(left, 0.5).lerp(HOOP, 0.12);
    }
}

fn boxout(view: &View, a: &mut Aims) {
    for dk in 0..5 {
        a.at[dk] = if is_zone(view.scheme) {
            // No assigned man to find: drop to the nearest piece of paint.
            let q = view.d[dk];
            HOOP.toward(q, q.dist(HOOP).min(2.6))
        } else {
            let m = view.o[view.guard[dk]];
            if m.dist(HOOP) > 4.6 {
                HOOP.toward(m, 3.0)
            } else {
                m.toward(HOOP, 0.8)
            }
        };
    }
    a.on_ball = None;
}

/// Ball side: -1, 0 (middle of the floor) or 1.
fn side(ball: Vec2, middle: f32) -> f32 {
    if ball.x > middle {
        1.0
    } else if ball.x < -middle {
        -1.0
    } else {
        0.0
    }
}

/// Zone defenders shade toward an attacker standing in their area.
fn match_up(view: &View, a: &mut Aims) {
    for dk in 0..5 {
        if Some(dk) == a.on_ball {
            continue;
        }
        let spot = a.at[dk];
        let mut near: Option<(f32, Vec2)> = None;
        for (i, &m) in view.o.iter().enumerate() {
            if Some(i) == view.owner {
                continue;
            }
            let dist = m.dist(spot);
            if dist < 2.8 && near.is_none_or(|n| dist < n.0) {
                near = Some((dist, m));
            }
        }
        if let Some((_, m)) = near {
            a.at[dk] = spot.lerp(m.toward(HOOP, 0.6), 0.4);
        }
    }
}

fn pressure(ball: Vec2) -> Vec2 {
    let dh = ball.dist(HOOP);
    let gap: f32 = if dh < 3.6 { 0.6 } else { 1.1 };
    ball.toward(HOOP, gap.min(dh * 0.8))
}

/// 2-3 zone. Defenders 0 and 1 are the guards up top (left, right), 2 and 3
/// the forwards on the baseline (left, right), 4 the centre.
fn zone23(view: &View) -> Aims {
    let b = view.ball;
    let dh = b.dist(HOOP);
    let s = side(b, 1.0);
    let g = |s: f32| if s > 0.0 { 1 } else { 0 };
    let f = |s: f32| if s > 0.0 { 3 } else { 2 };
    let mut at = [
        v(-2.3, 6.9),
        v(2.3, 6.9),
        v(-3.7, 2.8),
        v(3.7, 2.8),
        v(0.0, 3.2),
    ];
    let on_ball;
    if dh < 3.6 {
        // Ball in the paint: collapse on it.
        let bs = if s == 0.0 { 1.0 } else { s };
        if b.x.abs() < 1.5 {
            on_ball = 4;
        } else {
            on_ball = f(bs);
            at[4] = v(bs * 0.6, 2.4);
        }
        at[0] = v(-1.8, 5.2);
        at[1] = v(1.8, 5.2);
        if on_ball != 4 {
            at[f(-bs)] = v(-bs * 1.0, 2.4);
        }
    } else if dh < 5.0 && b.y > 3.6 {
        // High post, the soft spot: the centre has to come up.
        on_ball = 4;
        at[0] = v(-1.5, 5.9);
        at[1] = v(1.5, 5.9);
        at[2] = v(-3.0, 2.6);
        at[3] = v(3.0, 2.6);
    } else if b.y < 3.6 && b.x.abs() > 4.0 {
        // Corner: the forward goes out, the centre fronts the block.
        on_ball = f(s);
        at[g(s)] = v(s * 4.4, 5.2);
        at[g(-s)] = v(s * 0.4, 5.2);
        at[4] = v(s * 1.9, 2.3);
        at[f(-s)] = v(-s * 0.9, 2.6);
    } else if s == 0.0 {
        on_ball = g(if b.x >= 0.0 { 1.0 } else { -1.0 });
        let other = 1 - on_ball;
        at[other] = v(if other == 1 { 1.6 } else { -1.6 }, 6.2);
        at[4] = v(0.0, 3.6);
    } else {
        // Wing. The guard owns it, but the forward bumps out until the
        // guard arrives, which is what leaves the corner open on a quick
        // reversal.
        let guard = g(s);
        let bump = b.y < 6.2 && view.d[guard].dist(b) > 2.5;
        on_ball = if bump { f(s) } else { guard };
        at[guard] = pressure(b);
        at[g(-s)] = v(s * 0.5, 5.6);
        at[f(s)] = if b.y < 6.5 {
            v(s * 4.9, 3.3)
        } else {
            v(s * 4.3, 3.0)
        };
        at[4] = v(s * 1.3, 3.0);
        at[f(-s)] = v(-s * 1.2, 2.7);
    }
    at[on_ball] = pressure(b);
    let mut a = Aims {
        at,
        on_ball: Some(on_ball),
        helper: None,
    };
    match_up(view, &mut a);
    a
}

/// 3-2 zone. Defender 0 is the point, 1 and 2 the wings (right, left),
/// 3 and 4 the two bigs underneath (left, right).
fn zone32(view: &View) -> Aims {
    let b = view.ball;
    let dh = b.dist(HOOP);
    let s = side(b, 2.2);
    let wing = |s: f32| if s > 0.0 { 1 } else { 2 };
    let low = |s: f32| if s > 0.0 { 4 } else { 3 };
    let mut at = [
        v(0.0, 7.6),
        v(4.4, 5.9),
        v(-4.4, 5.9),
        v(-2.7, 2.6),
        v(2.7, 2.6),
    ];
    let on_ball;
    if dh < 3.6 {
        let bs = if b.x >= 0.0 { 1.0 } else { -1.0 };
        on_ball = low(bs);
        at[low(-bs)] = v(-bs * 0.4, 2.3);
        at[wing(-bs)] = v(-bs * 1.6, 2.8);
        at[wing(bs)] = v(bs * 3.4, 4.2);
        at[0] = v(0.0, 4.8);
    } else if dh < 5.0 && b.y > 3.6 {
        // High post: nobody lives there. The point has to drop onto the
        // ball from behind.
        on_ball = 0;
        at[1] = v(3.2, 4.8);
        at[2] = v(-3.2, 4.8);
        at[3] = v(-1.2, 2.6);
        at[4] = v(1.2, 2.6);
    } else if b.y < 3.6 && b.x.abs() > 4.0 {
        // Corner: a big has to run all the way out from the block.
        let bs = if b.x >= 0.0 { 1.0 } else { -1.0 };
        on_ball = low(bs);
        at[low(-bs)] = v(bs * 1.2, 2.3);
        at[wing(bs)] = v(bs * 4.6, 4.2);
        at[0] = v(bs * 0.6, 5.2);
        at[wing(-bs)] = v(-bs * 1.4, 2.9);
    } else if s == 0.0 {
        on_ball = 0;
        if view.driving {
            // Penetration down the middle: the two bigs close the door.
            at[3] = v(-1.1, 3.1);
            at[4] = v(1.1, 3.1);
        }
    } else {
        on_ball = wing(s);
        at[0] = v(s * 1.0, 5.6);
        at[wing(-s)] = v(-s * 1.6, 4.3);
        // One big cheats toward the corner, which leaves the other alone
        // at the rim.
        at[low(s)] = v(s * 3.7, 2.4);
        at[low(-s)] = v(s * 0.3, 2.6);
    }
    at[on_ball] = pressure(b);
    let mut a = Aims {
        at,
        on_ball: Some(on_ball),
        helper: None,
    };
    match_up(view, &mut a);
    a
}

/// Where five defenders run when the other team has the ball and is already
/// coming: the one who can get in front of the ball first goes to stop it,
/// the others find the attackers nearest the basket and get goal side of them.
/// Nobody waits to be matched up, and nobody is held at a pickup line.
pub(super) struct Rush {
    /// Attackers and defenders, current positions.
    pub o: [Vec2; 5],
    pub d: [Vec2; 5],
    pub ball: Vec2,
    /// The attacker with the ball (or about to catch it).
    pub handler: usize,
}

pub(super) struct Back {
    pub at: [Vec2; 5],
    /// The attacker each defender has picked up.
    pub man: [usize; 5],
    /// The defender who stops the ball.
    pub stopper: usize,
}

pub(super) fn get_back(r: &Rush) -> Back {
    let to_ball = r.ball.dist(HOOP);
    // Where the ball will have to go through, a few metres up the line.
    let gate = HOOP.toward(r.ball, (to_ball * 0.55).clamp(0.6, 6.0));
    let mut stopper = 0;
    let mut best = f32::MAX;
    for dk in 0..5 {
        let behind = (r.d[dk].y - r.ball.y).max(0.0);
        let cost = r.d[dk].dist(gate) + behind * 2.0;
        if cost < best {
            best = cost;
            stopper = dk;
        }
    }
    let mut at = [Vec2::ZERO; 5];
    let mut man = [0usize; 5];
    // They cannot out-run a man to where he is going, so they run to the
    // rim and get there first: a point on the line from the basket to the
    // man, halfway out, where they can be goal side when he arrives.
    let gap = (to_ball * 0.55).clamp(0.6, 6.0);
    at[stopper] = HOOP.toward(r.ball, gap);
    man[stopper] = r.handler;
    // The rest: closest to the basket first, each takes the most dangerous
    // man that is left.
    let mut atk: Vec<usize> = (0..5).filter(|&k| k != r.handler).collect();
    atk.sort_by(|&a, &b| r.o[a].dist(HOOP).total_cmp(&r.o[b].dist(HOOP)));
    let mut def: Vec<usize> = (0..5).filter(|&k| k != stopper).collect();
    def.sort_by(|&a, &b| r.d[a].dist(HOOP).total_cmp(&r.d[b].dist(HOOP)));
    for (&dk, &k) in def.iter().zip(atk.iter()) {
        man[dk] = k;
        let a = r.o[k];
        at[dk] = HOOP.toward(a, (a.dist(HOOP) * 0.5).clamp(1.0, 5.0));
    }
    for p in &mut at {
        p.y = p.y.clamp(0.3, LENGTH - 0.3);
        p.x = p.x.clamp(-7.3, 7.3);
    }
    Back { at, man, stopper }
}
