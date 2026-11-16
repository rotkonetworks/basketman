//! Finishing near the rim: what a layup, a floater or a dunk is worth when the
//! man comes in at speed, off one foot, with a defender at his side or
//! behind him (`docs/realism.md`: the engine let open finishes go in nearly
//! every time and made everything just outside them too hard).
//!
//! Three things are decided here and the rest stays in `floor.rs`:
//!
//! * which hand he finishes with, and what that costs (a right-hander
//!   going up with his left, the hand toward the man who is going to
//!   contest it, going across his body to the far side of the board);
//! * how he arrives: speed at release, the angle to the board, the base line;
//!   one multiplier on the launch error, shrunk by his finishing and footwork;
//! * the floater, a high short shot over a protected rim, with its own error
//!   model (it is not stopped by an arm the way a layup is, and it is a
//!   harder shot to make clean).
//!
//! Everything is applied once, in `shot_errors`, to the same launch error the
//! stance multiplier and the officials' disturbed-shot term act on.

use super::*;

/// What each m/s of speed above `FIN_SPEED_FROM` at the release adds to a
/// finish's error (shrunk by finishing and footwork). Replaces the older
/// break-only sprint term: a driver at full speed has the same problem.
const FIN_SPEED: f32 = 0.18;
const FIN_SPEED_FROM: f32 = 2.2;
/// Coming across the face of the board, per m/s across the line to the rim.
const FIN_ACROSS: f32 = 0.16;
/// Finishing from behind the plane of the rim, at its largest.
const FIN_BASELINE: f32 = 0.32;
/// Going up with the hand he does not write with, with the hand that is
/// across his body from the side he came, and with the hand toward a man
/// who is at him.
const FIN_OFFHAND: f32 = 0.36;
const FIN_REVERSE: f32 = 0.16;
const FIN_TOWARD: f32 = 0.27;
/// A dunk that is not clean: a bump, losing the legs, a jump that took him
/// further from the rim than the dunk wants. All in multiples of the dunk's
/// very small base error.
pub(super) const DUNK_BUMP: f32 = 7.0;
const DUNK_BALANCE: f32 = 9.0;
const DUNK_FAR: f32 = 4.0;
/// The floater (base error in `shot::base`): what a hand in the face adds
/// (less than at the rim: the ball goes over it), its scale over the base
/// and its share of the arrival penalties.
pub(super) const FLOATER_SCALE: f32 = 0.55;
const FLOATER_CONTEST: f32 = 0.25;
const FLOATER_ARRIVAL: f32 = 0.25;

impl Game {
    /// 0 to 1: how much his finishing and footwork take off what coming in
    /// fast or awkward costs.
    fn finish_control(&self, h: usize) -> f32 {
        ((0.5 * (self.oe(h, Attr::CloseShot) + self.oe(h, Attr::Footwork)) - 8.0) / 12.0).clamp(0.0, 1.0)
    }

    /// The way he goes: his velocity, or the line to the rim when he stands.
    fn finish_heading(&self, h: usize) -> Vec2 {
        let o = self.play.o;
        let m = &self.m[o][h];
        if m.vel.len() > 1.0 {
            m.vel.norm()
        } else {
            (HOOP - m.pos).norm()
        }
    }

    /// Which hand he finishes with, 1 right and -1 left, and decided as he
    /// gathers: the hand on the side he comes from (the right hand from the
    /// right of the rim), the other one when a defender is at him on that
    /// side and he has the touch to go across.
    pub(super) fn finish_hand(&self, h: usize) -> i8 {
        let o = self.play.o;
        let p = self.m[o][h].pos;
        let fwd = self.finish_heading(h);
        let to_rim = HOOP - p;
        let across = fwd.x * to_rim.y - fwd.y * to_rim.x;
        let dom: i8 = match self.off().slots[self.off().on[h]].p.body.hand {
            crate::player::Hand::Right => 1,
            crate::player::Hand::Left => -1,
        };
        // The rim to his left: the right hand is the outside one.
        let natural: i8 = if across.abs() < 0.12 {
            dom
        } else if across > 0.0 {
            1
        } else {
            -1
        };
        let (dk, dist) = self.nearest_defender(p);
        if dist > 2.0 || self.finish_control(h) < 0.35 {
            return natural;
        }
        let q = self.dp(dk) - p;
        let side = fwd.x * q.y - fwd.y * q.x;
        if side.abs() < 0.15 {
            return natural;
        }
        // A defender to the left is best avoided with the right hand.
        let away: i8 = if side > 0.0 { 1 } else { -1 };
        away
    }

    /// The multiplier a finish's launch error takes from how he arrives and
    /// which hand he uses, as a number to add to 1.
    pub(super) fn finish_arrival(&self, h: usize, hand: i8, near: Option<usize>, c: f32) -> f32 {
        let o = self.play.o;
        let m = &self.m[o][h];
        let ctrl = self.finish_control(h);
        // The speed he came in at, not what he has left as he gathers: a man
        // who ran the length of the floor has it in his legs at the rim.
        let v = 0.5 * (m.vel.len() + self.st[o][h].peak);
        let to = (HOOP - m.pos).norm();
        let speed = FIN_SPEED * (v - FIN_SPEED_FROM).max(0.0) * (1.0 - 0.5 * ctrl);
        let across = (m.vel.x * to.y - m.vel.y * to.x).abs();
        let angle = FIN_ACROSS * across * (1.0 - 0.4 * ctrl);
        let behind = ((HOOP.y - m.pos.y) / 0.6).clamp(0.0, 1.0);
        let base = FIN_BASELINE * behind * (1.0 - 0.5 * ctrl);
        let mut hand_cost = 0.0;
        if hand != 0 {
            let dom: i8 = match self.off().slots[self.off().on[h]].p.body.hand {
                crate::player::Hand::Right => 1,
                crate::player::Hand::Left => -1,
            };
            if hand != dom {
                hand_cost += FIN_OFFHAND * (1.0 - 0.7 * ctrl);
            }
            let fwd = self.finish_heading(h);
            let to_rim = HOOP - m.pos;
            let across_rim = fwd.x * to_rim.y - fwd.y * to_rim.x;
            let natural: i8 = if across_rim.abs() < 0.12 {
                hand
            } else if across_rim > 0.0 {
                1
            } else {
                -1
            };
            if hand != natural {
                hand_cost += FIN_REVERSE * (1.0 - 0.6 * ctrl);
            }
            // The hand toward his man is the hand that gets blocked.
            if let Some(dk) = near {
                let q = self.dp(dk) - m.pos;
                let side = fwd.x * q.y - fwd.y * q.x;
                let toward = (hand as f32) * side < 0.0;
                if toward && q.len() < 2.2 {
                    hand_cost += FIN_TOWARD * c.min(1.0) * (1.0 - 0.4 * ctrl);
                }
            }
        }
        speed + angle + base + hand_cost
    }

    /// The part of a dunk's error that is not the contest: a bump, a man who
    /// has lost his legs, a jump that left him too far from the rim.
    pub(super) fn dunk_trouble(&self, h: usize, to_rim: f32, bumped: bool) -> f32 {
        let o = self.play.o;
        let bal = self.st[o][h].balance;
        let v = self.m[o][h].vel.len();
        let ctrl = self.finish_control(h);
        let mut t = DUNK_BALANCE * (1.0 - bal).max(0.0) + DUNK_FAR * (to_rim - 0.7).max(0.0);
        t += FIN_SPEED * 3.0 * (v - 4.0).max(0.0) * (1.0 - 0.5 * ctrl);
        if bumped {
            t += DUNK_BUMP * (1.0 - 0.4 * ctrl);
        }
        t
    }

    /// The floater's launch error multiplier.
    pub(super) fn floater_error(&self, h: usize, hand: i8, c: f32, near: Option<usize>) -> f32 {
        let touch = 0.5 * self.oe(h, Attr::CloseShot) + 0.3 * self.oe(h, Attr::MidRange) + 0.2 * self.oe(h, Attr::Handling);
        let guard = near.map_or(10.0, |dk| 0.5 * self.de(dk, Attr::InteriorD) + 0.5 * self.de(dk, Attr::Block));
        (1.0 - SKILL_SLOPE * 1.4 * (touch - 10.0)).clamp(0.45, 2.0)
            * (1.0 + FLOATER_CONTEST * c * (1.0 + GUARD_SLOPE * (guard - 10.0)).max(0.3))
            * (1.0 + FLOATER_ARRIVAL * self.finish_arrival(h, hand, near, c))
    }

    /// A man going to the rim with the ball on a drive or a break, or gathering
    /// for a finish: the best placed of the others, who is not his man, steps
    /// into the lane to meet him at the rim if he can get there in time. He
    /// goes up straight (squared to the shooter, set) to contest without a
    /// foul, or he arrives late and moving, and the whistle is likelier.
    pub(super) fn rim_rotation(&mut self, on_ball: Option<usize>) {
        let o = self.play.o;
        let d = 1 - o;
        let Some(h) = self.play.holder else { return };
        let me = self.m[o][h];
        let going = match self.play.phase {
            Phase::Drive { .. } => true,
            Phase::Gather { .. } => self
                .play
                .shot
                .is_some_and(|s| matches!(s.style, Style::Layup | Style::Dunk | Style::Floater)),
            Phase::Advance => self.play.brk && me.vel.dot((HOOP - me.pos).norm()) > 3.0,
            _ => false,
        };
        let to_rim = me.pos.dist(HOOP);
        if !going || to_rim > RIM_HELP_RANGE || me.vel.dot((HOOP - me.pos).norm()) < 1.0 {
            return;
        }
        // When he is there: the time left to the rim, at the speed he has.
        let left = to_rim / me.vel.len().max(3.0);
        let spot = HOOP.toward(me.pos, 1.0);
        let mut best: Option<(f32, usize)> = None;
        for dk in 0..5 {
            if Some(dk) == on_ball
                || self.play.stun[dk] > 0.0
                || self.play.hunt.is_some_and(|(hk, _, _)| hk == dk)
            {
                continue;
            }
            let q = self.m[d][dk].pos;
            // Nobody who is closer to his own man than to the lane leaves him
            // for a ball that is not coming his way, and nobody who is already
            // behind the play.
            let t = q.dist(spot) / (self.cache.top[d * 5 + dk] * 0.95).max(1.0);
            if t > left + RIM_HELP_SLACK || q.dist(HOOP) > RIM_HELP_RANGE + 1.5 {
                continue;
            }
            if best.is_none_or(|b| t < b.0) {
                best = Some((t, dk));
            }
        }
        if let Some((_, dk)) = best {
            self.m[d][dk].target = spot;
            self.m[d][dk].pace = 1.0;
        }
    }

    /// Whether a man going to the rim takes a floater: a defender is waiting
    /// at the rim (the shot would be a contested layup), the man is neither
    /// a dunker with room nor a post player, and he has the touch. The
    /// choice is made from what he sees, with no dice but his own skill:
    /// the better the touch, the lower the contest at which he gives up
    /// the layup.
    pub(super) fn takes_floater(&self, h: usize, d: f32, c: f32) -> bool {
        if !(FLOATER_MIN..=FLOATER_MAX).contains(&d) {
            return false;
        }
        let o = self.play.o;
        let source = self.play.source;
        if matches!(source, Source::Post | Source::Putback | Source::Forced) {
            return false;
        }
        let touch = (0.5 * self.oe(h, Attr::CloseShot) + 0.3 * self.oe(h, Attr::MidRange) + 0.2 * self.oe(h, Attr::Handling) - 8.0) / 12.0;
        // A man who can finish over the top does not need it.
        let lift = Pose::max_lift(self.off().slots[self.off().on[h]].p.attrs[Attr::Vertical]);
        let reach = self.off().slots[self.off().on[h]].p.body.standing_reach_cm / 100.0;
        let strong = (reach + lift - RIM_HEIGHT) * 2.0;
        let _ = o;
        let threshold = FLOATER_AT - 0.25 * touch.clamp(-0.5, 1.0) - 0.15 * strong.clamp(-1.0, 1.0);
        c >= threshold
    }
}

/// How far from the rim a drive or a break draws a helper over, and how many
/// seconds late he may be and still try.
const RIM_HELP_RANGE: f32 = 6.0;
const RIM_HELP_SLACK: f32 = 0.35;

/// Distances from the rim, in metres, between which a floater is taken.
const FLOATER_MIN: f32 = 1.3;
const FLOATER_MAX: f32 = 3.2;
/// The contest at which an average man gives up the layup for it.
const FLOATER_AT: f32 = 0.55;

