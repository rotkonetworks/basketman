# Hands

Written 2026-11-16. A design for stage 2 of `docs/engine-physics.md` ("hands on everything"), widened after the owner's note: "we need way more actions for players to take with their hands to be realistic; basketball is all about hands, contact, body movement and legwork". Not built yet. It is to be built after body orientation and footwork (`engine/stance.rs`), fast breaks and referees have landed, because it rewrites the same code.

## The model

Each player has two hands. A hand has a position in 3D that follows from the shoulder, the arm's length (from wingspan), the body's direction and lean, and what the hand is doing. It moves at a limited speed, so reaching takes time and a hand that is up cannot also be down. A hand is strong or weak (handedness: a new innate trait, with an off-hand rating that training can raise).

A hand is always in one state. The state is simulated, recorded and drawn. Outcomes come from where hands and the ball actually are.

## What hands do

With the ball:
- **Dribble**: which hand, how high, how far from the body. Moves are hand actions with a cost and a window in which the ball is exposed: crossover, between the legs, behind the back, hesitation, in-and-out, spin, protect dribble with the body between. A defender's hand that reaches the ball in its exposed window pokes it loose.
- **Hold**: triple threat, ball high, ball on the hip, ripped through. Where it is held decides what can be reached and what he can do next quickly.
- **Pass**: chest, bounce, overhead, one-hand push, hook pass, wrap-around, lob, hand-off, outlet, no-look. Each has a release point, speed, a lane it needs clear, and a time to throw. The fake of each exists.
- **Shoot**: set shot, jumper, floater, runner, hook with either hand, finger roll, reverse, scoop, up-and-under, tip, dunk with one or two hands. The release hand and point decide what a defender can reach.
- **Fakes**: shot fake, pass fake, jab. A fake works on what the defender's hands and feet do: a hand that goes up for a fake is not down for the drive.

Without the ball, on offense:
- **Target hand** shown to the passer; **catch** with one or two hands, high or low, on the move; a bad catch is a bobble with the ball loose.
- **Seal** and hold position with an arm bar that is legal or not; **screen** with arms in; **swim** past a box-out.
- **Tip** an offensive rebound to keep it alive or in.

On defense:
- **Hand up** on the shooter (contest), **hand in the passing lane** (deny, deflect), **active hands** on the dribble (poke, strip from behind, swipe down, which risks the foul), **hand check** on the hip (a foul by the rulebook, called by degree), **arm bar** in the post, **verticality** (both hands straight up: legal contest at the rim), **wall up**, **trace the ball** on a held ball, **dig** from help and recover.
- **Block** by meeting the ball; **strip** on the way up; **tie-up** for a held ball (jump ball or possession arrow).
- **Box out** with the hips and forearm, **rebound** with two hands, one hand or a tip-out, **chin** the ball after.
- **Take a charge** with hands in; **foul on purpose** (wrap up) to stop a break or late in a game.

Loose balls: dive, scoop, tap ahead, save from going out of bounds.

## Rules that follow

Reach-in, hold, push, hand check, illegal use of hands, over the back, hooking, off-arm push by the dribbler, moving screen with arms, hit on the arm on a shot, a clean block against a foul on the hand, goaltending and basket interference, kicked ball, held ball, back-court and carrying/palming. Each is decided from what the hands and bodies did, then seen (or not) by the referees.

## Skill

Ratings decide choice, speed, accuracy and timing, not outcomes: handling (how exposed the ball is, off-hand), passing and vision (which passes he can make, how fast, how accurate), hands (catching, strips, deflections), steal and block (timing and anticipation), strength (holds, rips, seals), reach and hand size (what he can get to). Signature moves are particular executions.

## Coaching

Instructions the engine can then act on: pressure the ball or contain, deny or sag, hands active or stay down, go for strips or stay vertical, front the post, force him to his weak hand, foul or do not foul, crash or get back. Scouting can report a player's weak hand and favourite moves.

## Order of building

1. Hands as state with reach and speed; dribble as an exposed ball; passes as thrown balls with catches, deflections and interceptions; steals from pokes and strips.
2. Contests, verticality, blocks and strips at the rim; fakes; the foul rules above.
3. Off-ball hands: deny, seal, screens, box-outs, tips, loose balls, held balls.
4. Handedness, the pass and finish catalogue, signature moves as executions.

After each step: league statistics against `docs/realism.md`, the balance laboratory, and frame sequences looked at.

## Step 1 as built (the `hands` branch)

Files: `engine/hands.rs` (the pure model), `engine/floor/hands_hooks.rs` (what each hand wants each tick, the dribble's bounce, what is recorded), `engine/floor/passing.rs` (passes, catches, deflections), `engine/floor/dribbling.rs` (pokes and strips), `lab_hands.rs` in the CLI (`basketman lab hands [games]`, `lab hands relations [games]`, `HANDS_TRACE=game:time` prints the frames around a moment), `engine/hands_tests.rs`.

**Hands.** Each player has two hands (`hands::HandPos`: a point in 3D and a kind). A hand moves toward its goal at `4.5 + 0.18 * quickness + 0.14 * handling` metres a second (quickness is Handling for the side with the ball, Steal for the other) and is never further from its shoulder than his arm (`0.37 * standing reach`, a little more for a long wingspan). The shoulders are half a shoulder-width out from the body's centre, turned with the body (`Stance::body`), and rise with the jump. The recording (`Pose::hands`: offset from the body in centimetres, height, kind) is what the viewer draws; the ball in a dribble is drawn where it was recorded. The weak hand: `Player::off_hand` (1 to 20, innate, from his id and handling, filled in on load for old saves); the strong hand is the existing `Body::hand`.

**The dribble.** While `Dribble::Live` the ball is out of his hand on a rhythm (`0.64 s - 0.025 s per m/s`, to 0.38 s): down to the floor in 42% of the bounce and back up. How far it goes out to the side is `0.42 - 0.012 * handling + 0.012 * speed` metres (the weak hand averages his handling and the off-hand rating), and the window in which it is more than 0.28 m below the hand is the exposed one. At the start of each bounce a defender within reach decides to go for it (`GO_BASE 0.20`, plus 0.02 a point of Steal, plus aggression; more against a weak handler), with an aim error from his rating. His hand then moves toward the ball at its limit; a hand within 20 cm of the ball in the window has touched it: arm (a foul through `call`), the ball taken (strip), or knocked loose (a real loose ball under `ball/flight`, which the ten then chase). A try that comes close and finds nothing costs him balance and a moment, and may be a reach-in. The old dice for strips in `drive` and `drive_end` are gone.

**The pass.** `pass_kind` picks the kind that has the lowest `lane_risk` among those he can throw (chest always; bounce from Passing 6; overhead from 8; lob from 9 to a man near the rim or far away; one-hand push off the dribble; hand-off under 1.9 m; outlet over 13 m). `lane_risk` is the passer's reading of the lane and is what every decision uses as `pass_risk`: the ideal path (aimed ahead of the receiver, as far as he sees where he is going; a man sent on a cut is led by how fast he can get up to speed), each defender's reach (his hands' speed, reaction from Steal and IQ, how he faces), hands already in the way, bodies near the line, the glass; a passer with little vision reads it as clearer than it is. The throw takes the kind's time plus his turn and the travel of his hands; at the release the ball leaves his hands (their positions then), with an error in metres at the target that grows with distance, the kind, pressure, balance, the weak hand, the stance factor and falls with Passing, then flies on a `flight::Path` (a bounce pass is two arcs). A hand in the way at the release changes the kind or holds the ball.

**The catch and the deflection.** Every tick the ball's path is scanned in five fine steps against the hands (interpolated between ticks). The receiver's hand goes to a meeting point chosen on the path (kept once chosen); his body comes to the ball. A hand of his within 33 cm catches it with a probability from his Handling, how far he has to stretch, the ball's speed, whether he looks, his own speed and how far off the aim it was thrown; otherwise it is a bobble (a loose ball with the passer's team first at it). A defender's hand within 25 cm of the ball, whether it was sent there (a try: he saw it go, reaction from Steal and IQ, then his hand's speed) or was in the lane, tips it or takes it (from Steal and Handling, the ball's speed, how centrally). A tipped or fumbled ball is a loose ball: the ten chase it, whoever has it ends the scramble (a defender: a turnover and a steal; the offense: play goes on); out of bounds untouched is the passer's turnover, off a defender it is theirs to take in.

**The offense.** Passing frequency no longer depends on passing skill as much as it did, and swings go to the man whose look is best (`ev` at his own spot with the attention the defense has paid him), not at random by distance; the feed to the star in Isolation and to the post man are priors and not the rule; a man who has shot a lot (`Slot::heat`, +0.16 a shot, 8% fades a possession) draws a tighter look in his own reading and, a little less, in the real contest. A man whose defender overplays him on the ball side goes backdoor (`read_denial`), and a drive sends the men without the ball to the corner or along the arc (`relocate_for_drive`). A man who has camped in the lane is not thrown to.

**What was not done.** Seals, screens with arms, box-outs with hands, tips, held balls, fakes, the shot and finish catalogue, signature moves as executions, and the coaching instructions (deny or sag, pressure the ball, force the weak hand) are not built. A bobble is recovered by the nearest hand, not by a hand going to the floor. The release point of a pass is the passer's hand at the time, but his body does not pivot round a hand in his way. Isolation still beats the other sets in `basketman balance` (see `docs/balance.md`).
