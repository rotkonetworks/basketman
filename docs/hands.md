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
