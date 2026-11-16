# Balance

The game is meant to be won by coaching: tactics, matchups, benching, reading the opponent. If one
tactic is simply best, or one cheap trick beats everything, there is no game. `basketman balance`
measures that. It does not tune anything; it says how far the engine is from the targets below and
where to look.

## What balance means here, with targets

The targets are provisional, chosen so that the rest of the design (scouting, training, online play)
still has something to do. Each is one line of the scorecard at the end of the report and one test in
`crates/cli/src/balance.rs`.

| id | property | target | why |
|---|---|---|---|
| no-dominated | no system is dominated by another | at most 2 of 25 | a dominated choice is a trap, not a choice |
| no-dominant | no system is a safe pick against everything | none even-or-better against all; none above 58% mean | otherwise scouting is pointless |
| bounded | a system is worth a bounded share | none below 42% mean | a wrong pick should hurt, not decide |
| symmetric-support | equilibrium of the full game mixes systems | at least 6 of 25 with 5% each | counters must form a web, not a ladder |
| defense-has-answer / offense-has-answer | every defense is beaten by some offense and the reverse | best edge at least 0.03 ppp (about 2 points a game) | rock-paper-scissors needs a "scissors" for each |
| support-offense / support-defense | equilibrium of the offense-vs-defense game | at least 3 sets and 3 schemes | the manager's key choice should be a real mix |
| every-system-countered | every system has a counter on fresh games | 90% of systems | the manager must be able to answer what he sees |
| fit-offense / fit-defense / fit-regret | the best tactic depends on the squad | 3 different best offenses and defenses over 8 squad types; the generic best costs half the specialised squads 3+ points | the reason to build a squad before picking tactics |
| no-always-best-setting / sliders-matter | no slider is best at an extreme for every system; sliders move results | none; at least 3 of 4 sliders move 3+ points | a slider with an optimum at 20 is not a slider |
| coach-worth | a counter-picking coach beats the default at equal talent | 60-80% | tactics real but bounded |
| talent-matters-small / large | talent must matter | d = 0.5 wins 60%+; d = 3 wins 90%+ | d is attributes added to every player |
| tactics-beat-talent | the best counter makes up a real gap | worth 0.5 to 2.0 attribute points (0.75 to 3 league sd) | the better squad with the worse plan can lose |
| adapt-at-half | switching to the right system at half-time pays | +5 win points | "spot what he does and answer it" |
| familiarity | an undrilled system costs a bounded share | 5 to 25 points at familiarity 0 | drills matter, but not absolutely |
| no-exploit / no-absurd-lines | no combination of settings beats the field | best found at most 4 points above the best ordinary system, at most 62%; stat lines within 0.7-1.4 of league | online play will find them |
| season-quality | the table follows quality but not too tightly | r between 0.55 and 0.85 | coaching irrelevant above, a lottery below |

## Running it

```sh
cargo run --release -p basketman-cli -- balance                 # quick mode, 25 systems
cargo run --release -p basketman-cli -- balance --full          # 75 systems (press too), 3x the games
cargo run --release -p basketman-cli -- balance --sections matrix,fit --games 10 --out /tmp/b
cargo test --release -p basketman-cli balance                   # instrument tests (seconds)
cargo test --release -p basketman-cli balance -- --ignored      # the balance properties (minutes)
```

Options: `--games N` units per matrix cell (a unit is four games), `--seed S`, `--teams N`,
`--sections matrix,fit,levers,coach,search,season`, `--out DIR`. It writes `balance-report.txt` and
`balance.json` (every matrix, curve and the scorecard) into DIR and prints the report. Threads: half
the cores, or `RAYON_NUM_THREADS` if smaller, or `BALANCE_THREADS`; the output does not depend on
it (tested). Quick mode is about 3700 CPU-seconds (a few minutes on 16 free cores); full mode is
roughly six times that. A full run was **not** made for this first reading (see Limits).

How it measures. A *strategy* is everything a manager sets before tip-off. A *unit* is four games of X
against Y: X on each of two squads, at home and away, so squad strength and home court cancel. All
numbers are from X's side. "±" is a 95% interval from the spread of the units. A single matrix cell
at quick settings is about ±14 win points; a mean over 24 opponents about ±4; do not read single cells.
Where something is picked as the best of many noisy numbers it is replayed on fresh games before it
is believed (counters, squad fit, the exploit).

## Reading the report

1. Matrix: win % of every system against every other; strength; dominance; the equilibrium mix of the
   symmetric game (value is 0 by symmetry, so the support is what matters) and the pooled
   offense-by-defense ppp table with its own equilibrium. Counters are the offense with the best ppp
   against the opponent's defense and the defense allowing the least to his offense (±0.006 ppp, far
   sharper than one cell), replayed on fresh games.
2. Squad fit: eight squad types (shooters, bigs, athletes, slow veterans, star plus role players,
   top-heavy, deep bench, balanced) against generic opponents. Lifts are over the squad's own average.
3. Levers: tempo, three-point bias, crash, aggression, press, briefs, primary option's role, minutes
   plan, each on five base systems against five opponents.
4. Coaching: equal squads; informed against naive; the talent gap tactics can overcome; familiarity;
   minutes plans; the staff coach; live levers (half-time switch, timeouts, shouts, no substitutions)
   as paired games identical until the lever is used.
5. Search: random plus hill climbing over every setting including a hand-drawn set, scored against all
   25 systems, winners replayed on fresh seeds, ablated setting by setting, then searched again for a
   counter.
6. Seasons: win % against squad quality in generated leagues.

## First reading (quick mode, seed 1, 16-team league)

23 checks, 16 fail. Ranked by damage to the game.

### 1. One hand-drawn set beats everything (no-exploit, no-absurd-lines)

A random search over all settings found, within a minute, **Post up with the big man's spot moved from
(2.7, 2.9) to (1.1, 1.7) m** (the stock spot is `offense.rs:37`; the hoop is at (0, 1.575),
`court.rs:16`) plus two drawn runs. Against the 25 neutral systems: **88.5 ± 2.2% win**, the best ordinary
system (Iso/Pack) 59.4 ± 3.2%. Ablation on fresh games: with everything else neutral (no press, sliders
10) moving only the big man's spot wins **89.8 ± 2.1%**; only the guard's or runs alone do nothing
(55.5, 51.5); sliders, press and briefs are worth about 0 (all within ±3). Stat line: 110.7 points to
85.1, 31.5 free throws (league 14.9), 13 steals, opponents 20.7 turnovers and 34.6 threes (league 28.8,
they chase the game), 13.9 offensive rebounds.
Cause, from reading the engine (not tested further): a man standing a metre from the rim is a rim
shot (`shot_kind`, `floor.rs:216`) with a base 83% finish (`floor.rs:2204`) and the contest weight
reaches only 1.7 m (`floor.rs:2157-2190`); the `drawn` flag also switches off the zone flash rule
(`floor.rs:1178`) and nothing models defending a man glued to the rim or a three-second count for a
player who simply stands there (the only three-second logic is for a rolling player,
`floor.rs:2370`). Entering the ball to him is almost free (`floor.rs:2497`, passes to the post at
80%). A counter does exist (a second search found PnR/3-2 with full-court press at 60.4 ± 11.0% against
it, borderline), so it is not yet a pure strategy, but one nudge of one spot should not be worth 30
points. Proposal: (a) clamp drawn spots to legal stock-like regions (no spot inside the restricted
area, 4.3 m rule of `in_post`), or charge for it; (b) model the offensive three seconds for anyone
standing in the lane and the defender's ability to front or help on a man at the rim; (c) make the
search a permanent regression test: `search_finds_no_exploit`.

### 2. Iso/Pack dominates; most systems are dominated (no-dominated, no-dominant, supports)

Strength (mean win % against the other 24): Iso/Pack **58.4 ± 2.1**, Motion/Pack 57.4, Iso/3-2 56.1,
Post/Pack 55.9 ... PnR/Switch 43.8, 5out/Switch 44.0. Spread 14.6 points against noise ±2.1: real.
Iso/Pack is within one se of even against all 24 others. **21 of 25 systems have another that is at
least as good against 85% of opponents and never significantly worse.** The equilibrium of the
symmetric game is Iso/Pack 50%, Iso/3-2 40%, Iso/Man 10% (effective 2.4 systems). In the pooled ppp table
(each cell ±0.006): Isolation has the best ppp against all five defenses (1.048 to 1.085) and Pack line
allows the least to every offense but one; the offense equilibrium is Isolation 100% and the defense
equilibrium Pack line 100%. The best offense edge available against Pack line is +0.009 ppp, and the
largest ppp an offense gains by choosing its defense opponent is 0.015 (targets 0.03): defenses and
offenses barely answer each other.
Cause: `OffenseProfile`/`DefenseProfile` in `tactics.rs` (the multipliers the descriptions quote:
Zone 2-3 "-3% rim", Pack "0.82 rim frequency") are **not read anywhere**; `profile()` has no callers.
What a set does is its spot template (`offense.rs`) and the branch in `decide` (`floor.rs:2498-2560`).
Isolation lets the star take up to three drives and always passes to him when open
(`floor.rs:2546-2560`), concentrating shots in the best player, which nothing in the defense taxes;
Pack line sits in the driving gap (`defense.rs:112-117`) and goes under screens
(`floor.rs:3156`), which gives up threes the engine rates 37% + touch (`floor.rs:2214`) while the
drive is the main way the other sets create shots. Proposal: make the descriptions true in the engine
(zones should be punished by shooters, Pack by shooters and by pull-ups, Isolation by help and by a
star's fatigue); then re-run. The pooled table is the target: every row and column needs a +/- 0.03 ppp
swing.

### 3. Three sliders are best at 20 for every base system (no-always-best-setting)

Lift over neutral, five bases, ±5.5 at quick: tempo 20 **+11.3**, three-point bias 20 **+12.2**, crash
20 **+9.8**; best value per base is 20 for all five (crash: 15 or 20). Pace feeds `threshold`
(`floor.rs:2274`, -0.012 per point off the bar a shot has to clear) and walking speed (`floor.rs:1667`)
while its price, fatigue (`mod.rs:447`, load 0.8 + pace/50) is paid back by the substitution rule
(`mod.rs:388`). Three-point bias raises the taste for threes (`floor.rs:2251`) but the engine's
three is worth more than the two it replaces. Crash costs "transition defense" in the description but
the glass team simply sends up to four (`floor.rs:3650`) and the missing getting-back is not punished.
Aggression has an interior optimum at 5 (+7.5), and the press is best at full-court for four of five
bases (+5.5): also a weak edge. Briefs: "Look for his shot" +7.3 and "Attack the rim" +7.7, "Move the
ball" -11.5. The primary option's role is flat (±5.4); the minutes plan has no benefit at all
for stars (see below). Proposal: price each slider where it is paid (fatigue at tempo 20 must show
in the box score, transition defense for crash, turnovers for pace), then require an interior optimum
that depends on the squad.

### 4. Familiarity is too expensive and adapting in play does not pay (familiarity, adapt-at-half)

Familiarity 1.0 / 0.75 / 0.5 / 0.25 / 0: 47.9 / 37.5 / 33.3 / 20.0 / 18.8 win % (mirror matchups, ±5):
an undrilled system costs **31 points**, about what a talent gap of d = 1 attribute point (two league sd, 76% win) is worth.
Switching in play with the game's own familiarity rule costs **-17.2 ± 6.7** win points (from a bad
system to the best counter), and a drilled change (familiarity 0.8) gains +1.2 ± 7.0, i.e. nothing
measurable; changing away from the best counter costs -12.0 ± 5.8; timeouts +0.8 ± 6.6, shouts all
within noise, and switching the assistant off (no substitutions) costs **-30.4 ± 7.7** points. So the
live levers a human has are, in effect, "do not touch anything" and "keep subs on". Effects of
`unfamiliar` are applied at `floor.rs:1183, 1468, 2079, 2235, 2315, 2439, 2682, 3746`: eight
separate penalties stacked. Proposal: cap the stacked penalty (a target of 10-15 points at 0) and make
the cost of a change per possession fade in over a few minutes rather than apply instantly; then
a prepared alternative at half-time becomes worth switching to.

### 5. Counters exist for two thirds of systems, but "scout and counter" is worth nothing over "play Iso/Pack" (coach-worth, every-system-countered)

Structural counters validated on fresh games: **16 of 25** systems have one that wins at 95%
one-sided; the rest are Pack-based systems and Iso/Pack, whose own best reply is itself. At equal
talent a counter-picking coach wins 60.1 ± 2.9% against the 25 systems, the best fixed system
(Iso/Pack) 61.4 ± 2.8%, the default Motion/Man 51.0 ± 3.0%, the worst reply 43.3 ± 2.7%. Since the
counter to most things is Iso/Pack or Motion/Pack, scouting adds -1.3 ± 4 points over a fixed choice.
Same cause as finding 2.

### 6. Tactics are worth little against talent, but talent works (tactics-beat-talent; talent passes)

A naive Motion/Man coach with an attribute gap d against the best counter: 43.8 / 48.4 / 50.0 / 73.4 / 85.9
% at d = 0 / .25 / .5 / 1 / 1.5. A line through d <= 1.5 gives an even bet at **d = 0.31** attribute
points, **0.5 league standard deviations** (teams in a generated league differ by sd 0.67, range 2.22);
target 0.5 to 2.0 attribute points. Talent itself: d = 0.5 wins 67.2%, d = 3 wins 96.9% (both pass).
Bounds: the best counter against the default at equal talent is +9 points (60.1 vs 51.0).

### 7. Squad fit exists but is mostly the same answer (fit-defense, fit-regret)

Best offense over eight squads: Motion for Balanced, Iso for Shooters/Slow veterans/Star/Top-heavy, Post
for Bigs/Athletes/Deep: three different, the pattern is sensible (Bigs Post +13.7, Star Iso +21.0, Post
-14.2; ±3.9). Best defense is Pack for six and 3-2 for one or two (Pack lifts +3 to +9 everywhere);
Switch and 2-3 are never in the top two. Using the Balanced squad's choice (Motion/Pack) instead of
the fit system on fresh games costs Bigs 7.8 ± 6.2, Star 12.2 ± 6.2 points, the rest within noise
(2 of 7, target 4). Shooters prefer Isolation, 5out (the "answer for shooters") is never in the top two
for any squad. Proposal: fix finding 2 first; squad fit is then a matter of Five out and zones paying
for shooters.

### 8. Minutes plans: no reason to play stars more (not scored)

Mirror against the standard 30/10 plan: deep (20 each) 52.9 ± 5.4, starters 36/4 25.4 ± 5.0, starters
only 19.6 ± 4.9. On a top-heavy squad (starters +2, bench -3) the standard plan still scores best
(45.0 control; deep 25.0, heavy 36.7, only 33.3). Fatigue (`mod.rs:442-452`, `need` at `mod.rs:379`)
punishes more minutes harder than the bench's quality costs. Proposal: calibrate fatigue so that
stars' minutes pay when the bench is worse.

### 9. Seasons follow quality (season-quality passes)

Five leagues of 16: win % against squad quality r = 0.79 (R^2 0.63; within a league rank correlation
0.77), against the coach's system strength r = 0.16, together R^2 0.66. Spread of win % 0.200 against
0.091 for pure luck; the best squad is top two in 80% of leagues. Inside the target (0.55-0.85), but
the system contributes almost nothing beyond quality (R^2 0.63 to 0.66): once finding 2 is fixed this
should rise. Not measured: AI managers adapting during a season (they do not).

### Noise, plainly

The quick run's single-cell intervals are ±14 points and the live-lever intervals ±6 to 7 over 250
paired games, so any single live lever below about 8 points is invisible; only large effects are claimed
above (no substitutions, improvised switching, familiarity, the exploit). Every number marked "±" is a
95% interval; a difference inside it is not a finding. Findings 3 (pace, bias, crash: +10 to +12 against
±5.5) and 1 are far outside it; the individual slider bests per base (20, 20, 20, 20, 20) are not by
themselves significant.

## Limits and what is not measured

* No full-mode run was made for this reading (the machine was being shared; full needs about an hour of
  free cores). Full mode was smoke-tested at `--games 1` for every section. Re-run it before tuning.
* Quick-mode squads come from one generated league (seed 1); `--seed` repeats it elsewhere.
* The AI managers' own tactics/adjustments in a season (`instructions.rs` rules, `floor.rs:922 adapt`) are
  not used in the matrix; the staff coach's adjustments were tested alone and are within noise
  (44.2 / 44.2 / 45.0 at ratings 0 / 10 / 20).
* Roles other than the primary option, per-player briefs beyond "all five", positions, and
  the training/scouting layers are not explored. Drawn sets only by random jitter of spots and runs.
* The equilibrium solver reads noisy matrices; supports are upper bounds.
* Hidden causes in findings 1 to 4 are readings of the code, not experiments.
