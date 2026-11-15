//! Bodies on the floor: how big each player is, how well he holds his
//! ground, and the pass that keeps two of them from standing in one place.
//!
//! Only plain arithmetic and `sqrt` are used so positions stay identical on
//! every target. Everything runs in a fixed order (index `i` before `j`,
//! `i < j`) with a fixed number of passes, so the result depends on the input
//! alone.

use crate::court::{HALF_WIDTH, LENGTH, Vec2, v};
use crate::player::Body;

/// Relaxation passes per tick. Enough for a pile of ten at the rim.
const PASSES: usize = 12;
/// Pairs closer than contact distance plus this count as touching.
const TOUCH: f32 = 0.02;
/// Speed (m/s) at and above which a player gets no credit for being set.
const SET_SPEED: f32 = 1.2;
/// Extra anchor a motionless player has over one at speed.
const SET_BONUS: f32 = 1.0;
/// Pairs closer than contact distance plus this are worth a look this tick.
const CANDIDATE: f32 = 0.6;
/// Overlap left on purpose so float noise never reads as penetration.
const SLACK: f32 = 0.002;

/// One player's body as the floor sees it.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Bulk {
    /// Radius of the body in metres.
    pub r: f32,
    /// How much ground he holds: weight scaled by strength.
    pub anchor: f32,
}

/// What one body felt this tick.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Touch {
    /// Index of the body he is in contact with (the nearest), or `NONE`.
    pub with: u8,
    /// Closing speed along the line between them before it was removed.
    pub closing: f32,
    /// Metres the resolver moved him this tick.
    pub pushed: f32,
    /// He was nearly still when it happened.
    pub set: bool,
}

impl Touch {
    pub const NONE: u8 = 255;
    pub const CLEAR: Touch = Touch {
        with: Touch::NONE,
        closing: 0.0,
        pushed: 0.0,
        set: false,
    };

    pub fn is_some(&self) -> bool {
        self.with != Touch::NONE
    }
}

/// Skill below which nobody goes hunting for a charge, and how fast the
/// urge grows with skill above it (full at `FLOOR + 1 / RATE`).
const HUNT_FLOOR: f32 = 0.38;
const HUNT_RATE: f32 = 4.0;
/// Skill and will multiplied can fall short of this and the man still stays
/// at home; above it the urge climbs fast, so the specialists are few.
const HUNT_BAR: f32 = 0.18;
const HUNT_STEEP: f32 = 3.0;
/// Will: the character score below which a man will not put his body in the
/// way, and how fast the will grows above it.
const WILL_FLOOR: f32 = 0.30;
const WILL_RATE: f32 = 3.0;

/// How willing a man is to put his body in a driver's way and take the
/// hit, 0..=1, read off his character: loyal team men with little ambition
/// of their own and some authority on the floor do it; the rest would
/// rather not. Steep, so that most are plainly one or the other.
pub fn charge_will(p: &crate::player::Personality) -> f32 {
    let x = (0.4 * p.loyalty as f32
        + 0.3 * (20 - p.ambition.min(20)) as f32
        + 0.3 * p.leadership as f32)
        / 20.0;
    ((x - WILL_FLOOR) * WILL_RATE).clamp(0.0, 1.0)
}

/// How much a man is inclined to step in and take a charge, 0..=1: the
/// head for it, the nerve and the work rate and defensive skill to be there
/// in time (`skill`, 0..=1, a weighted share of those), times the will.
/// Both have to be high: the specialists are few.
pub fn charge_urge(skill: f32, p: &crate::player::Personality) -> f32 {
    let skill = ((skill - HUNT_FLOOR) * HUNT_RATE).clamp(0.0, 1.0);
    ((skill * charge_will(p) - HUNT_BAR) * HUNT_STEEP).clamp(0.0, 1.0)
}

/// Body radius: half the shoulder width plus a small margin for arms.
pub fn radius(body: &Body) -> f32 {
    (body.shoulder_width_cm / 200.0 + 0.03).clamp(0.22, 0.32)
}

/// Weight scaled by strength as he plays right now (`strength` is the
/// fatigue-scaled attribute, 1..=20): 90 kg at 10 Strength is 90.
pub fn anchor(weight_kg: f32, strength: f32) -> f32 {
    weight_kg * (0.6 + 0.04 * strength.clamp(1.0, 20.0))
}

/// `x.clamp(lo, hi)` for `lo <= hi`, without the range check or the NaN
/// handling of `min` and `max`.
#[inline(always)]
pub fn bound(x: f32, lo: f32, hi: f32) -> f32 {
    let x = if x < lo { lo } else { x };
    if x > hi { hi } else { x }
}

/// Keep a body of radius `r` inside the court.
pub fn keep_in(p: Vec2, r: f32) -> Vec2 {
    // `max` then `min`: the same as `clamp` here, without its range check.
    v(
        bound(p.x, -HALF_WIDTH + r, HALF_WIDTH - r),
        bound(p.y, r, LENGTH - r),
    )
}

fn pinned(p: Vec2, r: f32) -> bool {
    // On or against a line of the court.
    let e = 1e-4;
    p.x >= HALF_WIDTH - r - e || p.x <= -HALF_WIDTH + r + e || p.y <= r + e || p.y >= LENGTH - r - e
}

/// Direction used when two bodies sit on exactly the same point: fixed by
/// their indices so the outcome never depends on anything else.
fn fallback(i: usize, j: usize) -> Vec2 {
    match (i + j) % 4 {
        0 => v(1.0, 0.0),
        1 => v(0.0, 1.0),
        2 => v(-1.0, 0.0),
        _ => v(0.0, -1.0),
    }
}

/// How firmly body `i` holds, 1 at speed and up to 2 when standing.
fn hold(vel: Vec2) -> f32 {
    1.0 + SET_BONUS * (1.0 - vel.len() / SET_SPEED).clamp(0.0, 1.0)
}

/// Resolve overlaps among bodies. The lighter and weaker one gives more
/// ground, a player who is set gives less than one on the move, and the
/// closing part of the relative velocity is taken out so running into a body
/// slows you. Positions end inside the court.
pub fn resolve(pos: &mut [Vec2], vel: &mut [Vec2], bulk: &[Bulk], touch: &mut [Touch]) {
    resolve_n(pos.len(), pos, vel, bulk, touch);
}

/// `resolve` for the ten bodies of a game: the count is known, so the pair
/// sweep unrolls.
pub fn resolve_ten(
    pos: &mut [Vec2; 10],
    vel: &mut [Vec2; 10],
    bulk: &[Bulk; 10],
    touch: &mut [Touch; 10],
) {
    resolve_n(10, pos, vel, bulk, touch);
}

#[inline(always)]
fn resolve_n(n: usize, pos: &mut [Vec2], vel: &mut [Vec2], bulk: &[Bulk], touch: &mut [Touch]) {
    debug_assert!(touch.len() == n && vel.len() == n && bulk.len() == n);
    assert!(n <= 16, "at most sixteen bodies");
    // Only pairs near enough to meet this tick are looked at, in the same
    // order as a full sweep (i before j).
    let mut pairs = [(0u8, 0u8); 120];
    let mut count = 0;
    let xs: [f32; 16] = std::array::from_fn(|i| if i < n { pos[i].x } else { 0.0 });
    let ys: [f32; 16] = std::array::from_fn(|i| if i < n { pos[i].y } else { 0.0 });
    let rs: [f32; 16] = std::array::from_fn(|i| if i < n { bulk[i].r } else { 0.0 });
    for i in 0..n {
        // One bit per partner, so no branch is taken on a guess.
        let mut near = 0u16;
        for j in (i + 1)..n {
            let reach = rs[i] + rs[j] + CANDIDATE;
            let (dx, dy) = (xs[j] - xs[i], ys[j] - ys[i]);
            near |= ((dx * dx + dy * dy < reach * reach) as u16) << j;
        }
        while near != 0 {
            pairs[count] = (i as u8, near.trailing_zeros() as u8);
            count += 1;
            near &= near - 1;
        }
    }
    // Bodies are kept inside the court by whoever moves them; a pile of
    // bodies is kept inside by the passes below.
    if count == 0 {
        for i in 0..n {
            touch[i] = Touch::CLEAR;
            pos[i] = keep_in(pos[i], bulk[i].r);
        }
        return;
    }
    // Nobody within touching distance: nothing to push and nothing felt.
    let close = pairs[..count].iter().any(|&(i, j)| {
        let (i, j) = (i as usize, j as usize);
        let reach = rs[i] + rs[j] + TOUCH;
        let (dx, dy) = (xs[j] - xs[i], ys[j] - ys[i]);
        dx * dx + dy * dy <= reach * reach
    });
    if !close {
        for i in 0..n {
            let p = keep_in(pos[i], bulk[i].r);
            touch[i] = Touch {
                set: vel[i].dot(vel[i]) < SET_SPEED * SET_SPEED * 0.25,
                pushed: if p == pos[i] { 0.0 } else { p.dist(pos[i]) },
                ..Touch::CLEAR
            };
            pos[i] = p;
        }
        return;
    }
    for t in touch.iter_mut() {
        *t = Touch::CLEAR;
    }
    let start: [Vec2; 16] = std::array::from_fn(|i| if i < n { pos[i] } else { Vec2::ZERO });
    let mut near = [f32::MAX; 16];
    let mut weight = [0.0f32; 16];
    let mut weighed = false;
    for i in 0..n {
        touch[i].set = vel[i].dot(vel[i]) < SET_SPEED * SET_SPEED * 0.25;
        pos[i] = keep_in(pos[i], bulk[i].r);
    }
    let pairs = &pairs[..count];
    for _ in 0..PASSES {
        let mut any = false;
        for &(i, j) in pairs {
            let (i, j) = (i as usize, j as usize);
            let reach = bulk[i].r + bulk[j].r;
            let d = pos[j] - pos[i];
            let l2 = d.dot(d);
            if l2 >= reach * reach {
                continue;
            }
            if !weighed {
                weigh(&mut weight, vel, bulk);
                weighed = true;
            }
            any = true;
            let l = l2.sqrt();
            let dir = if l > 1e-4 {
                d * (1.0 / l)
            } else {
                fallback(i, j)
            };
            let gap = reach + SLACK - if l > 1e-4 { l } else { 0.0 };
            // A body against the wall cannot give ground.
            let wi = weight[i]
                * if pinned(pos[i], bulk[i].r) {
                    1000.0
                } else {
                    1.0
                };
            let wj = weight[j]
                * if pinned(pos[j], bulk[j].r) {
                    1000.0
                } else {
                    1.0
                };
            let si = wj / (wi + wj);
            pos[i] = keep_in(pos[i] - dir * (gap * si), bulk[i].r);
            pos[j] = keep_in(pos[j] + dir * (gap * (1.0 - si)), bulk[j].r);
        }
        if !any {
            break;
        }
    }
    // Running into a body: take out the closing speed, the lighter one
    // losing more of his.
    for &(i, j) in pairs {
        let (i, j) = (i as usize, j as usize);
        let reach = bulk[i].r + bulk[j].r + TOUCH;
        let d = pos[j] - pos[i];
        let l2 = d.dot(d);
        if l2 > reach * reach || l2 < 1e-8 {
            continue;
        }
        // Speeds as they were before anybody was slowed.
        if !weighed {
            weigh(&mut weight, vel, bulk);
            weighed = true;
        }
        let l = l2.sqrt();
        let dir = d * (1.0 / l);
        let closing = (vel[i] - vel[j]).dot(dir);
        for (a, b) in [(i, j), (j, i)] {
            if l < near[a] {
                near[a] = l;
                touch[a].with = b as u8;
                touch[a].closing = closing.max(0.0);
            }
        }
        if closing > 0.0 {
            let si = weight[j] / (weight[i] + weight[j]);
            vel[i] = vel[i] - dir * (closing * si);
            vel[j] = vel[j] + dir * (closing * (1.0 - si));
        }
    }
    for i in 0..n {
        // Untouched bodies did not move at all.
        touch[i].pushed = if pos[i] == start[i] {
            0.0
        } else {
            pos[i].dist(start[i])
        };
    }
}

/// Every body's holding weight from the speeds as they are now.
fn weigh(weight: &mut [f32; 16], vel: &[Vec2], bulk: &[Bulk]) {
    for i in 0..vel.len() {
        weight[i] = bulk[i].anchor.max(1.0) * hold(vel[i]);
    }
}

/// A ball screen as the bodies see it.
pub struct Screen {
    /// Ball handler, screener and the defender who has to get past.
    pub handler: Vec2,
    pub screener: Vec2,
    pub defender: Vec2,
    /// Where the ball handler is going.
    pub toward: Vec2,
    pub r_handler: f32,
    pub r_screener: f32,
    pub r_defender: f32,
    pub anchor_screener: f32,
    /// The defender's strength, quickness and alertness mixed, 1..=20.
    pub fight: f32,
}

/// Seconds a defender is held up by a screen. Zero when his path to the
/// ball handler's route is clear of the screener's body; otherwise longer
/// the wider and heavier the screener and the weaker, slower or less alert
/// the defender. Going over the screen (through the gap between handler and
/// screener) is quicker than going under, round the back of the screener.
pub fn screen_delay(s: &Screen) -> f32 {
    /// Seconds for an average defender held by an average screener.
    const BASE: f32 = 0.42;
    let path_end = s.toward;
    let nearest = s.screener.project(s.defender, path_end);
    let reach = s.r_screener + s.r_defender;
    if nearest.dist(s.screener) >= reach + 0.15 {
        return 0.0;
    }
    let bulk = (s.anchor_screener / 90.0) * (s.r_screener / 0.27);
    // Room to squeeze between handler and screener, or go under.
    let gap = s.handler.dist(s.screener) - s.r_handler - s.r_screener;
    let route = if gap >= 2.0 * s.r_defender { 0.7 } else { 1.35 };
    (BASE * bulk * (10.0 / s.fight.max(1.0)).clamp(0.5, 2.0) * route).clamp(0.08, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bulk(r: f32, anchor: f32) -> Bulk {
        Bulk { r, anchor }
    }

    fn resolve(pos: &mut [Vec2], vel: &mut [Vec2], b: &[Bulk]) {
        let mut t = vec![Touch::CLEAR; pos.len()];
        super::resolve(pos, vel, b, &mut t);
    }

    fn screen(screener: Vec2, anchor: f32, r: f32, fight: f32) -> Screen {
        Screen {
            handler: v(0.0, 8.0),
            screener,
            defender: v(0.0, 7.0),
            toward: v(0.0, 4.0),
            r_handler: 0.27,
            r_screener: r,
            r_defender: 0.27,
            anchor_screener: anchor,
            fight,
        }
    }

    #[test]
    fn a_clear_path_is_not_delayed_and_a_body_in_it_is() {
        let clear = screen_delay(&screen(v(3.0, 7.0), 90.0, 0.27, 10.0));
        let hit = screen_delay(&screen(v(0.3, 6.5), 90.0, 0.27, 10.0));
        assert_eq!(clear, 0.0);
        assert!(hit > 0.15, "{hit}");
    }

    #[test]
    fn wider_heavier_screeners_and_weaker_defenders_cost_more_time() {
        let at = v(0.3, 6.5);
        let base = screen_delay(&screen(at, 90.0, 0.27, 10.0));
        assert!(screen_delay(&screen(at, 120.0, 0.27, 10.0)) > base);
        assert!(screen_delay(&screen(at, 90.0, 0.31, 10.0)) > base);
        assert!(screen_delay(&screen(at, 90.0, 0.27, 6.0)) > base);
        assert!(screen_delay(&screen(at, 90.0, 0.27, 16.0)) < base);
    }

    #[test]
    fn contact_is_reported_with_the_partner() {
        let mut pos = [v(0.0, 5.0), v(0.4, 5.0), v(5.0, 5.0)];
        let mut vel = [v(3.0, 0.0), Vec2::ZERO, Vec2::ZERO];
        let mut t = [Touch::CLEAR; 3];
        super::resolve(&mut pos, &mut vel, &[bulk(0.27, 90.0); 3], &mut t);
        assert_eq!(t[0].with, 1);
        assert_eq!(t[1].with, 0);
        assert_eq!(t[2].with, Touch::NONE);
        assert!(t[0].closing > 2.9 && t[1].set && !t[0].set);
    }

    #[test]
    fn overlapping_bodies_end_at_contact_distance() {
        let mut pos = [v(0.0, 5.0), v(0.3, 5.0)];
        let mut vel = [Vec2::ZERO; 2];
        resolve(&mut pos, &mut vel, &[bulk(0.27, 90.0), bulk(0.27, 90.0)]);
        let d = pos[0].dist(pos[1]);
        assert!((0.54 - 1e-4..0.56).contains(&d), "{d}");
    }

    #[test]
    fn the_heavier_stronger_body_moves_less() {
        let mut pos = [v(-0.2, 5.0), v(0.2, 5.0)];
        let (a, b) = (pos[0], pos[1]);
        let mut vel = [Vec2::ZERO; 2];
        resolve(&mut pos, &mut vel, &[bulk(0.27, 130.0), bulk(0.27, 70.0)]);
        assert!(pos[0].dist(a) < pos[1].dist(b) * 0.7);
    }

    #[test]
    fn a_player_who_is_set_holds_better_than_one_moving() {
        let mut pos = [v(-0.2, 5.0), v(0.2, 5.0)];
        let (a, b) = (pos[0], pos[1]);
        let mut vel = [Vec2::ZERO, v(-3.0, 0.0)];
        resolve(&mut pos, &mut vel, &[bulk(0.27, 90.0), bulk(0.27, 90.0)]);
        assert!(pos[0].dist(a) < pos[1].dist(b));
    }

    #[test]
    fn running_into_a_body_takes_out_the_closing_speed() {
        let mut pos = [v(0.0, 5.0), v(0.5, 5.0)];
        let mut vel = [v(4.0, 1.0), Vec2::ZERO];
        resolve(&mut pos, &mut vel, &[bulk(0.27, 90.0), bulk(0.27, 90.0)]);
        let closing = (vel[0] - vel[1]).dot((pos[1] - pos[0]).norm());
        assert!(closing.abs() < 1e-3, "{closing}");
        // The sideways part is not touched.
        assert!((vel[0].y - 1.0).abs() < 1e-4);
    }

    #[test]
    fn three_in_a_row_settle() {
        let mut pos = [v(0.0, 5.0), v(0.1, 5.0), v(0.2, 5.0)];
        let mut vel = [Vec2::ZERO; 3];
        let b = [bulk(0.27, 90.0), bulk(0.27, 70.0), bulk(0.27, 110.0)];
        resolve(&mut pos, &mut vel, &b);
        for i in 0..3 {
            for j in (i + 1)..3 {
                assert!(pos[i].dist(pos[j]) >= 0.54 - 0.005, "{i} {j}");
            }
        }
    }

    #[test]
    fn ten_in_a_pile_settle_inside_the_court() {
        let mut pos: Vec<Vec2> = (0..10)
            .map(|i| v(0.05 * i as f32 - 0.2, 0.4 + 0.03 * i as f32))
            .collect();
        let mut vel = vec![Vec2::ZERO; 10];
        let b = vec![bulk(0.27, 90.0); 10];
        for _ in 0..3 {
            resolve(&mut pos, &mut vel, &b);
        }
        for i in 0..10 {
            assert!(pos[i].y >= 0.27 - 1e-4 && pos[i].x.abs() <= 7.5);
            for j in (i + 1)..10 {
                assert!(pos[i].dist(pos[j]) >= 0.54 - 0.02, "{i} {j}");
            }
        }
    }

    #[test]
    fn same_input_same_output() {
        let start = [v(1.0, 5.0), v(1.1, 5.1), v(1.0, 5.05), v(7.4, 0.1)];
        let b = [
            bulk(0.27, 90.0),
            bulk(0.25, 80.0),
            bulk(0.3, 120.0),
            bulk(0.27, 90.0),
        ];
        let run = || {
            let (mut p, mut vl) = (start, [v(1.0, 0.0); 4]);
            resolve(&mut p, &mut vl, &b);
            (p, vl)
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn two_bodies_on_one_point_do_not_make_nan() {
        let mut pos = [v(2.0, 6.0), v(2.0, 6.0), v(2.0, 6.0)];
        let mut vel = [v(1.0, 1.0); 3];
        let b = [bulk(0.27, 90.0); 3];
        resolve(&mut pos, &mut vel, &b);
        for i in 0..3 {
            assert!(pos[i].x.is_finite() && pos[i].y.is_finite());
            assert!(vel[i].x.is_finite() && vel[i].y.is_finite());
            for j in (i + 1)..3 {
                assert!(pos[i].dist(pos[j]) > 0.5);
            }
        }
    }

    #[test]
    fn a_body_in_the_corner_does_not_leave_the_court() {
        let mut pos = [v(7.4, 27.9), v(7.3, 27.8)];
        let mut vel = [Vec2::ZERO; 2];
        resolve(&mut pos, &mut vel, &[bulk(0.27, 90.0), bulk(0.27, 90.0)]);
        for (p, r) in pos.iter().zip([0.27f32; 2]) {
            assert!(p.x <= 7.5 - r + 1e-4 && p.y <= 28.0 - r + 1e-4);
        }
        assert!(pos[0].dist(pos[1]) >= 0.54 - 0.005);
    }

    fn person(loyalty: u8, ambition: u8, leadership: u8) -> crate::player::Personality {
        crate::player::Personality {
            professionalism: 10,
            ambition,
            determination: 10,
            temperament: 10,
            leadership,
            consistency: 10,
            loyalty,
            presence: 10,
        }
    }

    #[test]
    fn taking_a_charge_needs_both_the_skill_and_the_will() {
        let team_man = person(18, 4, 15);
        let hired_gun = person(5, 17, 4);
        // Same head and legs, different character.
        assert!(charge_will(&team_man) > 0.9);
        assert!(charge_will(&hired_gun) < 0.05);
        assert!(charge_urge(0.6, &team_man) > 0.9);
        assert_eq!(charge_urge(0.6, &hired_gun), 0.0);
        // Same character, different skill: a poor defender stays home.
        assert_eq!(charge_urge(0.4, &team_man), 0.0);
        assert!(charge_urge(0.45, &team_man) < charge_urge(0.5, &team_man));
    }

    #[test]
    fn radius_follows_shoulders() {
        let mut b = Body {
            height_cm: 200.0,
            weight_kg: 95.0,
            wingspan_cm: 210.0,
            standing_reach_cm: 265.0,
            shoulder_width_cm: 46.0,
            hand_length_cm: 20.0,
            hand: crate::player::Hand::Right,
        };
        let small = radius(&b);
        b.shoulder_width_cm = 54.0;
        assert!(radius(&b) > small);
        assert!((0.22..=0.32).contains(&small));
    }
}
