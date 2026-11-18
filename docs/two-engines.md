# Two engines

Written 2026-11-18, from the owner's decision. Not built yet. To be started once the physical-ball work has landed and its cost per game is measured.

## The decision

- **The division the manager plays in is simulated one way: full physics, every game.** His own games and the other clubs' games in his division use the same engine, so the table he competes in is fair.
- **The divisions he does not play in use a fast engine.**
- **No missing statistics.** The owner's complaint about Football Manager: leagues that are not played have almost no statistics. Here the fast engine produces the same record a physics game does: a full box score for every player, team totals, shot counts by zone, fouls, the score by period, injuries, minutes. Tables, leaders, player histories and scouting numbers in the other divisions are as complete as in his own.
- **Any game can be looked at afterwards.** A game missed live is still there in the film room.

## How each kind of game is kept

| Game | Engine | Result kept | Watching it later |
|---|---|---|---|
| Manager's own | physics | box score, events, recording | the recording |
| Other games in his division | physics | box score, events, the seed | re-simulated from its seed: the same game, exactly (the engine is deterministic), so no recording needs storing |
| A game a scout is sent to, any division | physics | box score, events, the scout's report | re-simulated from its seed |
| Other divisions | fast | box score in the same shape | a physics **rendition**: the same two line-ups and tactics played in the physics engine. It shows how the two teams play and is useful for tactics, but it is not the game that produced the box score, and the film room says so plainly |

Promotion and relegation move a club between the two kinds of division; nothing about the club changes.

## The fast engine

A possession-by-possession model fitted to the physics engine: for a line-up, tactics, fatigue and matchup it draws how a possession ends (who shoots from where, make or miss, foul, turnover by kind, rebound) from tables measured on many thousands of physics games, and writes the same `GameResult`. It keeps the game's own random numbers and the determinism rules.

It is a stand-in for the physics engine, never a second opinion. The rule:

- **Agreement is tested.** A standing check plays the same matchups in both engines and compares win probability, margin distribution, pace, every box-score line, shot zones, and the offense-by-defense matrix. If they diverge beyond noise the check fails.
- **It is refitted whenever the physics engine changes**, by a command, not by hand.
- The realism and balance laboratories measure the physics engine. The agreement check covers the fast one.

## Why

Physics per game is getting heavier (a season on one thread went from 18 s to 55 s in two days, before the physical ball everywhere and before a finer time step). With this split the cost that matters is one division's round: nine to eleven games, in parallel on a desktop and one after another in a browser. The other two divisions cost almost nothing, which leaves room for more divisions and countries later.

## Open questions

- Whether the manager may choose "full physics everywhere" on a fast machine (a setting).
- How much of the event log the fast engine writes (enough for the shot chart and the game-flow chart, or box score only).
- Reserve leagues: fast engine by default.
