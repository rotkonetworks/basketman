# A physics-first match engine

Written 2026-11-16. This is the plan for replacing the way the match engine decides what happens. It is a design, not a description of the code.

## Why

The engine today is a two-dimensional tactical simulation. Ten points move on a floor, and at each decisive moment (a shot, a pass, a drive, a rebound) a dice roll weighted by the players' ratings decides the outcome. The 3D view is drawn afterwards from positions and an event log. Because the outcome is decided first and the picture is made up to fit it, the picture does not match: a ball is "caught" by a player whose hands are nowhere near it, a made shot does not visibly go through the ring, a rebound appears in someone's hands.

The owner's direction is the opposite order: the match is a 3D world first, and what happens in that world is the result. A shot scores because the ball went through the ring. A pass is caught because a hand got to it. That is what makes a football game like FIFA convincing, and it is also what direct control of a player needs.

Body collision and contact already work this way (see `engine/contact.rs`): bodies have size and weight, and who moves whom comes out of that. This plan extends the same idea to the ball and the hands.

## What stays

- **The brain.** How a team decides where to stand and what to do: offensive sets, defensive schemes, the tactics board, roles and instructions, reads off screens, help defense, substitutions, timeouts, the coach's orders. This is the tactical value of the game and it is kept. It now issues intentions (move there, pass to him, shoot, jump, reach) instead of outcomes.
- **Determinism.** Same seed and same inputs give the same game on every platform: fixed time step, the game's own random numbers, no `powf`/`exp`/`ln`/trig in simulation code.
- **The outside of the engine.** `Lineup` in, `GameResult` and `Recording` out, `LiveMatch` for coached games, the command log. Everything outside `engine/` keeps working.
- **Measured realism.** The `basketman lab` runs (league statistics, the offense-by-defense matrix, fouls, screens, post, charges) are the acceptance test. A physics engine that produces 30% shooting is not realistic, however good it looks.

## What changes

### 1. The world is 3D and steps finely

- A fixed step of 1/30 s for everything, watched or not. (1/60 s if 1/30 proves too coarse for the ball at the rim; decide by test.) The brain does not think every step: decisions stay on their own slower cadence.
- **Ball**: position, velocity and spin in three dimensions. Gravity, air drag small enough to ignore at first, bounce on the floor, the backboard (a plane with restitution) and the rim (a ring: collision with a torus of the right radius, the source of every rattle-in and rim-out). The net only slows the ball. Sweep the collision test along the step so a fast ball cannot pass through the rim or a hand.
- **Bodies**: the existing circle on the floor with mass, plus height state (on the ground or in a jump with take-off velocity from the player's leap and how well he gathered), a facing with a turning rate, and momentum (acceleration and deceleration limits from speed, strength and weight, so nobody stops dead or turns on the spot at a sprint).
- **Hands**: each player has two hands with a reachable volume that follows from his real measurements (standing reach, wingspan, hand length), his jump, and where he is facing. A hand has a target (the ball, a passing lane, up in a shooter's face) and gets there at a limited speed. Hands are the only things that can hold, catch, deflect or block the ball.

### 2. Skill becomes error and timing, not a percentage

A player's ratings decide how well he executes a physical action, and physics decides what that execution achieves.

- **Shot**: the shooter chooses a release (point, time). The ideal launch velocity to drop the ball through the ring is computed; his actual launch is the ideal plus an error in direction and speed. The error grows with distance, movement at release, fatigue, poor balance, a hand near the release, pressure and low composure, and shrinks with shooting skill. The ball then flies. It goes in or it does not.
- **Pass**: the same, with an aim point ahead of the receiver. Passing skill and vision set the error and how early the pass is thrown. A defender's hand in the lane that reaches the ball deflects or takes it.
- **Catch**: a ball that arrives within a ready hand's reach is caught with a chance that depends on handling, the ball's speed and whether he saw it coming. Otherwise it is fumbled into a loose ball that obeys physics.
- **Dribble**: the ball is a physical thing leaving and returning to the hand. While it is out of the hand a defender's hand can reach it. Handling decides how long it is exposed and how far from the body.
- **Block and contest**: a hand that meets the ball changes its velocity (a block). A hand close to the release without touching enlarges the shooter's error (a contest).
- **Rebound**: the ball comes off the rim where physics sends it. Players read it (rebounding skill is anticipation of the landing point and timing of the jump), fight for position with their bodies, jump, and the first hand to reach it with control has it. Tips are natural.
- **Layup and dunk**: a shot released close to the rim from a moving, jumping body. A dunk is a hand carrying the ball above and through the ring, possible when reach plus jump clears it.

### 3. Outcomes are read from the world

- A score is the ball passing down through the ring. Two or three points by where the shooter's feet were. The scorer is whoever released it.
- A rebound, a steal, a block, a turnover out of bounds, a shot-clock violation are all detected from what the ball and hands did. The event log is written by observers of the world, not by the code that used to roll the dice.
- Fouls keep their existing contact-based rules (who was set, who displaced whom, the referee's whistle), now with hands included: a hand on the shooter's arm, a reach that hits the body.

### 4. The recording is the world

A frame stores the ball's 3D position and velocity, each body's position, facing and height, each hand's position, and what each player is doing. The viewer draws exactly that: the arm goes where the hand is. It invents nothing. The viewer interpolates between frames only for smoothness.

## Calibration

The error models have free parameters. They are tuned until the league's numbers match targets taken from real basketball at this level and from the current engine: possessions per game, points per 100 possessions, shooting percentage at the rim, from mid-range and from three, the share of shots from each area, free throws, fouls, turnovers by kind, offensive-rebound rate, assists per basket, blocks, steals, and the relations in the offense-by-defense matrix. Each rating must keep its meaning: a better shooter shoots better, measured on otherwise identical players.

## Cost

About 86,000 steps a game at 1/30 s. With ten bodies, twenty hands and one ball, a step has to stay around a quarter of a microsecond per body-pair check to simulate a game in a few tens of milliseconds. A twelve-club season with reserve games is then a few seconds. The lab's thousand-game runs become minutes instead of seconds. Recording is only made for watched games.

## Stages

Each stage ends with the lab numbers inside their bands, the determinism tests passing, and contact sheets from the 3D view that show the thing working.

1. **Ball and rim.** The 3D ball with floor, board and rim collisions. Shots resolved by launch error and flight. Rebounds from the real bounce, with jumps and hands. Scores detected from the ball. Fine time step. Everything else (passes, dribble, steals) stays as it is for now, with the ball carried in a hand.
2. **Hands on everything.** Passes and catches, deflections and interceptions, the dribble as an exposed ball, steals, blocks and contests by hands, loose balls.
3. **Bodies with momentum.** Acceleration, turning, balance and jumping as physics. Dribble moves and footwork act on the defender's balance. Signature moves become particular executions (a hook released above the defender's reach, a step back that creates distance).
4. **Controllers.** Each player's intentions come from a controller: the built-in brain, a human, or an agent. Directing players with the mouse and, later, direct control.

## Stage 1 as built (shots, blocks, rebounds)

- `engine/ball.rs`: the ball in 3D with gravity, the ring as a torus (radius 0.2286 m, iron 0.0095 m), the glass as a slab with edges, the floor. Contacts are found by bisecting the step (3 ms near the basket), so nothing passes through iron or glass. The result is a chain of exact parabolas (`flight::Path`), recorded in `Recording::ball_arcs` and drawn by the viewer as it was.
- `engine/shot.rs`: a shot is the ideal launch (through the middle of the ring, off the glass from the side) plus an error in direction, speed and elevation. `floor.rs::shot_errors` sizes it from skill, distance, speed at release, the contest (distance, reach, whether his hand is up), pressure, rhythm and contact. A score is the ball going down through the ring.
- Blocks: a defender's arm (shoulder, arm length, the jump he is in, timed by his Block rating) meets the ball while it is still rising.
- Rebounds: everyone reads the loose ball (error in where, delay and timing by rebounding rating), moves, jumps; the first hand in reach takes it, or tips it and it stays live. Out of bounds ends the play (a ball last touched by a defender goes back to the attackers).
- Known gaps: the whole match still steps at 0.1 s (only the ball is finer); the net only slows the ball once; spin is not modelled; passes, dribbles and steals are unchanged.

## Stage 3, first part: stance (body, eyes, legwork, balance, dribble)

- `engine/stance.rs` is the pure model; `engine/floor/stance_hooks.rs` is the game's side (it reads the private state of `floor.rs`). Per player and per tick there are three directions: the **body** (hips and chest, turns at a limited rate by agility and footwork, slower at speed and off balance), the **eyes** (fast, never more than 80 degrees from the body) and the **movement** (the velocity). No trigonometry from the standard library: angles come from a polynomial arctangent, turns are fixed small rotations by polynomial sine and cosine.
- What the body wants to face is decided by role in `stance_wants` (shooter to the rim, hook side-on, post player back to the basket, on-ball defender square to the handler, help defender open to ball and man, boxing out toward the rim, closing out at the handler, receiver square to the passer, screener to the defender).
- The legwork is the angle between movement and body: run, slide, backpedal, crossover, pivot, drop step, closeout and chop, jump stop. The top speed in a direction is a share of the man's top speed (`move_cap`): all of it forward, less sideways, least backward; footwork and agility raise the limits. A defender whose hips must turn to chase a drive loses the time the turn takes.
- Balance (0 to 1) falls with hard acceleration, fast turns at speed, contact and unsteady footing, and comes back with Footwork and Strength.
- Shots: the shooter squares his shoulders before going up (the turn is time, `plan_shot`), and the launch error is multiplied by `shot_multiplier` (angle off the rim, sideways and backward movement, balance), less the matching skill. A hook is side-on, a turnaround starts with the back to the rim, a pull-up with a hand in the face may become a fadeaway, a shot at the buzzer is a flagged heave. `EventKind::Shot::how` says which, for `lab stats`.
- Passes thrown across or behind the body leave late and are riskier, a receiver not looking or running away from the ball is harder to catch for, defenders whose head is turned read a pass later. A defender with his back to a shooter cannot contest or block.
- The ball handler has a real dribble state (`Dribble`: not yet, live with a hand, picked up). Once picked up he may only pivot (his position is held to a radius), pass or shoot. The picture draws one hand or two from this state, never from speed (the old viewer drew two hands whenever he slowed down).
- Known gaps: no five-second or travelling turnovers, no closely-guarded rule; signature moves are not yet wired to the shot penalties; a finish starts earlier than the two steps of a real gather.

## What it replaces and what it risks

- The dice in `engine/floor.rs` for shots, passes, steals, blocks and rebounds go. The tactical decision code that leads up to them stays.
- The risk is calibration: an emergent simulation can drift in ways a table of probabilities cannot. The contact work showed this (zones stopped giving up threes until the cause was found). The defence is the same as there: measure after every step, and never loosen a test to get past a number.
- Until a stage passes, the game keeps running on the previous engine.
