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

Files: `engine/hands.rs` (the pure model), `engine/floor/hands_hooks.rs` (what each hand wants each tick, the dribble's bounce, what is recorded), `engine/floor/passing.rs` (passes, catches, deflections), `engine/floor/dribbling.rs` (pokes and strips), `lab_hands.rs` in the CLI (`basketman lab hands [games]`, `lab hands relations [games]`, `lab hands passes [games]`, `lab hands possession <set> <n>`, `HANDS_TRACE=game:time` prints the frames around a moment), `engine/hands_tests.rs`.

**Hands.** Each player has two hands (`hands::HandPos`: a point in 3D and a kind). A hand moves toward its goal at `4.5 + 0.18 * quickness + 0.14 * handling` metres a second (quickness is Handling for the side with the ball, Steal for the other) and is never further from its shoulder than his arm (`0.37 * standing reach`, a little more for a long wingspan). The shoulders are half a shoulder-width out from the body's centre, turned with the body (`Stance::body`), and rise with the jump. The recording (`Pose::hands`: offset from the body in centimetres, height, kind) is what the viewer draws; the ball in a dribble is drawn where it was recorded. The weak hand: `Player::off_hand` (1 to 20, innate, from his id and handling, filled in on load for old saves); the strong hand is the existing `Body::hand`. The recorded offsets are from the body's centre, so they include the shoulders rising in a jump and turning with the body (up to 18 m/s in the worst frames, takeoffs); what the model guarantees is the speed relative to the shoulder, and `a_hand_never_moves_faster_than_its_speed_from_its_shoulder` tests that on the model itself.

**The dribble.** While `Dribble::Live` the ball is out of his hand on a rhythm (`0.64 s - 0.025 s per m/s`, to 0.38 s): down to the floor in 42% of the bounce and back up. How far it goes out to the side is `0.42 - 0.012 * handling + 0.012 * speed` metres (the weak hand averages his handling and the off-hand rating), and the window in which it is more than 0.28 m below the hand is the exposed one. At the start of each bounce a defender within reach decides to go for it (`GO_BASE 0.32`, plus 0.02 a point of Steal, plus aggression; more against a weak handler), with an aim error from his rating. His hand then moves toward the ball at its limit; a hand within 20 cm of the ball in the window has touched it: arm (a foul through `call`), the ball taken (strip, `TAKE_BASE 0.26`), or knocked loose (a real loose ball under `ball/flight`, which the ten then chase). A try that comes close and finds nothing costs him balance and a moment, and may be a reach-in. The old dice for strips in `drive` and `drive_end` are gone; what is left in `drive()` is the reach-in at the first step (`0.030` of the old strip-and-foul dice, a quarter of the fouls).

**The pass.** `pass_kind` picks the kind that has the lowest `lane_risk` among those he can throw (chest always; bounce from Passing 6, overhead from 8, lob from 9 to a man more than 4.5 m from the basket and 7 m away, since a lob to the rim needs a catch above the head that is not modelled; one-hand push off the dribble; hand-off under 1.9 m; outlet over 13 m), and the odd kinds only when they beat the chest pass by a margin (`0.05`, `0.07`, `0.09`). `lane_risk` is the passer's reading of the lane and is what every decision uses as `pass_risk`: the ideal path (aimed ahead of the receiver, as far as he sees where he is going; a man walking to his spot is led further, `SPOT_LEAD 1.4`; a man sent on a cut is led by how fast he can get up to speed), each defender's reach (his hands' speed, reaction from Steal and IQ, how he faces), hands already in the way, bodies near the line, the glass (a ball that reaches the backboard bounces off it, so a passer who sees it in the line does not throw), the man's running. A passer with little vision reads it as clearer than it is. The throw takes the kind's time plus his turn and the travel of his hands, and a good passer is quicker (`spec_of`: speed and wind-up by Passing and IQ); at the release the ball leaves his hands (their positions then), with an error in metres at the target that grows with distance, the kind, pressure, balance, the weak hand, the stance factor and falls with Passing, then flies on a `flight::Path` (a bounce pass is two arcs). A hand in the way at the release changes the kind or holds the ball. The passer who cuts after a swing goes when the ball is let go (`cut_after`), not while he throws.

**The defense reads the pass.** A defender stays on the old picture of the ball until he has seen it go and worked out where it is going (`see_time` from Steal and IQ, how he faces, plus `REACT_READ 0.12 s`, longer against a good passer): `pre_aims`. Then he moves for the man who will catch it, at the speed the stance model gives him; a closeout costs what it costs.

**The catch and the deflection.** Every tick the ball's path is scanned in five fine steps against the hands (interpolated between ticks). The receiver comes to the ball so as to be there when it is, and stops there (a man on a cut runs through the place); his hand goes to a meeting point chosen on the side the ball comes from, at a comfortable reach (`catch_point`), kept once chosen. A hand of his within 33 cm catches it with a probability from his Handling and how the ball arrives (how far his arm is out, the ball's speed, whether he looks, his own speed, how far off the aim it was thrown); otherwise it is a bobble (a loose ball with the passer's team first at it). A catch on the hands is a shot that is ready: the receiver shoots sooner, and his jumper is the better for it (`catch_err`, `PASS_RHYTHM`). A defender's hand within 25 cm of the ball, whether it was sent there (a try: he saw it go, reaction from Steal and IQ, then his hand's speed) or was in the lane, tips it or takes it (from Steal and Handling, the ball's speed, how centrally). A tipped or fumbled ball is a loose ball: the ten chase it, whoever has it ends the scramble (a defender: a turnover and a steal; the offense: play goes on); out of bounds untouched is the passer's turnover, off a defender it is theirs to take in.

**The offense.** Swings go to the man whose look is best, weighted against distance, risk, men who have just had the ball (not the handler), men still running (`SWING_MOVER`) and a pull toward the man who runs the offence (`HANDLER_SWING`); the feed to the star in Isolation and to the post man are priors and not the rule; a man who has shot a lot (`Slot::heat`, +0.16 a shot, 8% fades a possession) draws a tighter look in his own reading and, a little less, in the real contest, and tires faster (`USAGE_TIRE`). A man whose defender overplays him on the ball side goes backdoor (`read_denial`), and a drive sends the men without the ball to the corner or along the arc (`relocate_for_drive`). A man who has camped in the lane is not thrown to, and a roller who has stood under the basket for two seconds goes out to the dunker spot (he used to camp: 5 three-second violations a hundred pick-and-roll possessions). The ball is brought up by the man who runs the offence in every set (it is then fed to the star in an isolation). Long threes are judged by their distance (`THREE_LONG`, `LONG_SLOPE`), which replaced the rule that nobody shoots from beyond the top of the arc. A beaten drive draws two helpers, the second a step behind the first, from the other side (`defense::help`).

**The shot clock.** It is the only thing that ends a possession by time. Offensive rebounds and fouls give a new clock inside the same possession, so how long a possession lasts means nothing (the 45 s rule that logged long possessions as violations was wrong, and counted all of them). `Recording::stalls` counts possessions the engine had to end because the clock had run more than four seconds past zero with nothing in the air (a fault; none in 20 000 games).  Violations are rare (0.05% of turnovers; real teams: about 3.7%, a violation or two a game): a man with the clock running out takes a forced shot at 3.2 s, always in time. A rule that made him sometimes hold it too long did not produce them (the forced shots are 2% of shots and the rule fired too seldom to show), so it is not in; the real cause, a possession that cannot get a shot off against pressure, is not modelled.

**What was not done.** Seals, screens with arms, box-outs with hands, tips, held balls, fakes, the shot and finish catalogue, signature moves as executions, jump catches (so no alley-oop), and the coaching instructions (deny or sag, pressure the ball, force the weak hand) are not built. A bobble is recovered by the nearest hand, not by a hand going to the floor. The release point of a pass is the passer's hand at the time, but his body does not pivot round a hand in his way.

**What ball movement pays, and what is still off.** See the numbers in the last section of this file.

## What ball movement pays (measured at the end of the stage)

`basketman lab hands passes 100` (the same world, both teams run the same set; per possession unless said otherwise):

| set | pts | passes | assists/game | turnovers | bad passes | completion | open at catch | after a pass: pts/shot | without a pass |
|---|---|---|---|---|---|---|---|---|---|
| Motion | 1.10 | 3.7 | 16.7 | .17 | .086 | 96.0% | 3.0 m | 0.99 | 0.75 |
| Pick and roll | 1.10 | 2.9 | 13.2 | .14 | .045 | 96.7% | 3.1 m | 0.98 | 0.80 |
| Post up | 1.12 | 5.0 | 19.2 | .16 | .086 | 96.7% | 2.8 m | 1.01 | 0.78 |
| Isolation | 1.07 | 3.3 | 15.5 | .15 | .056 | 96.6% | 3.0 m | 0.95 | 0.75 |
| Five-out | 1.12 | 4.2 | 17.8 | .16 | .086 | 96.3% | 3.1 m | 1.00 | 0.81 |

Why passing did not pay when the stage was first built (the diagnosis, with the lab columns that showed it):

- Shots after a pass were already worth about 0.25 points more than shots with no pass (1.0 against 0.75), so the shots were not the problem. Passing lost the possession on the way: 7% of passes did not reach a hand (bobbles 3.9%, an arm that was fully stretched at the catch because the receiver's hand went to the first point of the ball's path it could just touch, the ball arriving where the man was not when he was running), and a possession of 3.5 passes lost one in five.
- The defense had a perfect picture: it moved for the receiver as the ball left the passer's hand. Now it holds the old picture for the time it takes to see the ball go and read where it is going (the man who guards the receiver, half of it); the receiver is open by about 3 m at the catch and a defender closes 0.1 to 0.4 m of that while the ball flies.
- The offence passed on only when a shot cleared a bar that is high early in the possession (`threshold`: 1.45 at 20 s, 1.25 at 14 s, 1.1 at 8 s). Measured on 22 000 decisions: of the open catch-and-shoot threes (contest under 0.3, worth 1.10 points), 15 were taken with 14 to 19 s left, 1140 with 8 to 13 s left. So an open three was passed up for the first ten seconds of a possession and the ball moved on, to be lost on the way. Lowering the bar (1.08) made Motion better than Isolation but made the game too quick (pace 85 against a band of 70.5 to 75.5) and put 44 threes a game up (band 24 to 28). It was left where it was; what is left is a coupled choice (pace, three-point attempts and how early the open shot is taken) that has to be set together.
- Isolation was not an isolation: the star led the offence (he brought the ball up and stood in the lead's spot), and a star's usage went untaxed. The ball is now brought up by the handler in every set, a drive that beats a man draws two helpers, and a man who takes the load tires faster. The ablations: the star in the lead slot was worth 0.035 points a possession, the star's priority in `decide` another 0.04, and neither the drives (a sag or a closeout beaten) nor the heat on the star moved it.
- A pick and roll lost a possession in twenty to a roller camping under the basket (three seconds), which is fixed.

Where it landed (the tests): for a squad of good passers Motion 1.151 against Isolation 1.146 (24 worlds, thin), and for a star with four poor passers Isolation 1.233 against Motion 1.129.

## League numbers, main against the end of this stage (1500 games, 4 seasons, `basketman realism`)

Points a hundred possessions 101.6 -> 104.0 (target 113, band 109 to 117; the first hands build had 99.2); points a game 71.0 -> 73.7; pace 69.9 -> 70.8; FG% 43.1 -> 41.8; eFG% 50.4 -> 49.3; three-point attempts 23.1 -> 26.1 (inside); three-point % 35.2 -> 34.0; turnovers a hundred 20.0 -> 16.3 (inside); bad passes 53% of them (the first hands build 66%) -> 43%; steals a hundred 10.5 -> 9.9; assists a game 15.1 -> 14.7; assist leader 4.7 -> 3.5 (target 7.4); free-throw rate 33.9 -> 33.7; fouls a game 19.3 -> 19.5; top scorer 38.6 -> 27.4 points a game and 25.8% -> 20.0% of his team's points; shot-clock violations 0.09% of turnovers -> 0.05% (target 3.7%).

Still off target: points a hundred possessions (104 against 113), FG% and two-point % (the shooting model has its own calibration that was not touched), the assist leader (assists are spread over the five by slot, 23, 24, 22, 19, 11%; real teams have a point guard with 40%), 0 to 3 ft share (16% against 23%: a beaten drive meets two helpers), 3 to 10 ft accuracy (36% against 45.5%), corner threes (30% of threes against 20%), and-ones (10% of baskets against 6%), shot-clock violations, and the speed of the engine (see the report).

