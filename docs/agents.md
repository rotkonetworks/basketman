# Playing basketman as an agent

basketman can be played by a program. A program drives a club through a line-delimited JSON protocol (`basketman play`) or as a Model Context Protocol tool server (`basketman mcp`). Both are the same set of operations. Both are built on the same pieces in the core crate: a club's view of the world, a description of the legal actions, and standing match instructions.

The goal is a single interface that an online league can later use as its protocol. A server owns the world and runs the rounds at scheduled times. Each club is run by a human client, an agent over this protocol, or the club's standing instructions if nobody shows up for the game.

## Connecting

Build the binary with `cargo build --release -p basketman-cli`. The binary is `target/release/basketman`.

### Line protocol

`basketman play` reads one JSON object per line on stdin and writes one JSON object per line on stdout. A request has an `op`, the arguments of that operation next to it (or under `args`), and an optional `id` that the response echoes.

```
{"id": 1, "op": "new_game", "seed": 7, "clubs": 12, "club": 3}
{"id": 1, "ok": true, "result": { ... }}
{"id": 2, "op": "apply", "command": {"Bid": {"player": 140, "fee": 0}}}
{"id": 2, "ok": false, "error": {"kind": "refused", "message": "The squad is full."}}
```

Every response has `ok` and either a `result` or an `error` with a machine-readable `kind` and the game's own message. A line that is not JSON, or a request that is malformed, gets an error response with kind `parse_error` or `bad_request`. The server does not crash on bad input, and a panic inside the game is caught and reported as kind `internal`. The same requests always give the same responses: there is no clock and no randomness outside the seed, and JSON objects are written with sorted keys.

`tools/client.py` is a small Python client, and `tools/bot.py` is a complete reference manager built on it.

### MCP server

`basketman mcp` speaks JSON-RPC 2.0 over stdio, one message per line, as the MCP stdio transport specifies. Only protocol messages go to stdout; the server logs to stderr. It answers `initialize`, `notifications/initialized`, `ping`, `tools/list` and `tools/call`. A tool call returns one text content item holding JSON, with `isError` set when the operation failed (the text is then `{"kind": ..., "message": ...}`).

A sample client configuration entry, in the form most MCP hosts use:

```json
{
  "mcpServers": {
    "basketman": {
      "command": "/path/to/basketman",
      "args": ["mcp"]
    }
  }
}
```

The server offers protocol revision 2025-06-18. If the client asks for 2025-11-25, 2025-06-18, 2025-03-26 or 2024-11-05, the server answers with that version, since it uses nothing newer than the basic tools feature; for any other version it answers 2025-06-18. The revision strings and message shapes were written from memory (2025-11-25 in particular is unverified) and have been exercised only by the test script, not by a real MCP host.

### Tools

The MCP tool names and the line-protocol operations are the same set (the tool `legal_actions` is the operation `legal`, `load_game` is `load`, `save_game` is `save`, `live_kickoff` is `kickoff`, `command_log` is `log`; the others are named alike).

| Tool | What it does |
| --- | --- |
| `new_game` | seed, clubs (even, 2 to 20), club to manage. Starts a league and takes charge. |
| `load_game`, `save_game` | Read or write a career file at a path on the server machine. |
| `view` | The club's view of the world. `what` selects the part, see below. |
| `legal_actions` | What the game is waiting for, and every kind of command with its parameters and an example. |
| `apply_command` | Carry out one `Command`. |
| `advance` | Move the calendar to the next match day, new mail or the draft. |
| `play_round` | Play this round's games without coaching live, optionally with standing instructions. |
| `suggest_tactics` | A valid `Tactics` value with a fit rotation. |
| `trade_check` | Ask the other club's general manager about a deal without making it. |
| `set_instructions` | Store standing match instructions. |
| `live_kickoff`, `live_state`, `live_step`, `live_input`, `live_finish` | Coach today's game live. |
| `command_log` | Every command carried out so far. |

The line protocol also has `get_instructions` and `ping`.

## How to play

Start with `new_game` and `view` with `what=summary`. The summary has the calendar, the record, the budget, the next game and `waiting`, which says what the game needs from you next. Then look at `view` with `what=own` for the whole club and `what=inbox` for the mail.

`waiting.kind` is one of `draft_pick`, `offer`, `press`, `sponsor_offer`, `appearance`, `match_day` or `nothing`. An open transfer offer for one of your players lapses if you leave it. A sponsor's appearance request (`appearance`, with a `request` index) lapses after a few days; accepted, it pays the fee on the day of the event. A press question lapses after the next round. A draft pick holds the calendar: `advance` will not get past the draft until you pick, with `DraftPick` or `DraftAuto`. Answer these with `apply_command`, using the choices in `legal_actions`.

The loop is: answer what waits, adjust the club if you want to, `advance` until `stop` is `match_day`, then `play_round` or coach live, and repeat. When the season ends the draft opens, and when it is done the next season starts.

`legal_actions` lists each command kind with `available`, `why_not`, `params` (the choices and ranges) and an `example`, which is a command the game will accept right now. Copy the example and change what you mean to change. Commands are serde JSON: a bare string for commands with no parameters (`"Advance"`), or an object keyed by the variant name (`{"AnswerOffer": {"offer": 0, "accept": false}}`). The flat form `{"type": "Bid", "player": 140, "fee": 0}` is accepted too, and for single-value variants `{"type": "UseTactic", "value": 1}`. A refused command changes nothing and is not logged.

## The views

All views are built by `World::view*` in `crates/core/src/view.rs`.

`summary`: calendar (season, day, date, round, whether a game is today, what `advance` will stop at), record, place, budget, the board's expected place, the dressing room, the match tactic and its familiarity, the next game, unread mail, `waiting`, injuries.

`own`: the managed club in full. The first team and the reserves, each player with attributes, overall, a range for potential, condition, sharpness, mood, the one-line character, the medical note, injury record, contract, training, his place in the rotation and his season stats. The playbook with familiarity, the training schedule and the assistant's notes, staff (including the press officer, with a `media` rating), the general manager, the books (budget, wages, payroll against the cap, this season's lines, what renovation and expansion would cost and whether they are blocked), the arena and sponsor deals, open sponsor proposals, open appearance requests, `finance` (see below), the shortlist, the draft picks held, and the standing instructions.

`player`: one player. For your own players the figures are exact. For others they are ranges.

`club`: another club as anyone sees it: record, place, points scored and allowed, the names of its offense, defense and press, its general manager and staff, its arena and sponsor boards, its draft picks, and its squad with scouted figures.

`finance`: the Finances screen as data. `price`: the ticket price in force, the hall's reference price, the allowed range and a `preview` of crowd and gate for the next home game at one step below, at and one step above the price in force, with the price that would take most (`best_price`). `budgets`: the board's wage and transfer budgets, what is committed and left, what the board first set, and the most `MoveBudget` can move each way. `debt`: the state in a word, the overdraft limit, whether spending is frozen and next round's interest. `tax`: payroll against the cap, the year's luxury tax at this payroll, per checkpoint, checkpoints left and the marginal rate. `facilities`: training ground, medical centre and youth setup with level, effect in words, upgrade cost, why it is blocked, and any build in progress. `projection`: the rest of the season by category and the budget at its end. `merchandise_per_round` and `top_sellers` (player, fame, shirt money a round and this season). `fans`: see below. The same object is `own.finance`.

`fans`: the fan base in people, its size next to the hall (50 is ordinary), this season's trend in percent, what moved it this season and last by cause (results, finish and playoffs, stars on the team, media work, ticket prices, style of play, signings and sales), the base at the end of each past season, and the support of the town. The arena card of every club (`club` view) also shows the price in force, the reference price and the fan base; other clubs' money shows only as a word (`money`).

`names`: the biggest names still playing, with age, club, fame, fame in words and whether this is his farewell season, and the hall of fame (retired greats, with awards). Also in `league`. `appearances`: sponsors' requests for a player's time that can still be answered. In `inbox` the message carrying one has an `appearance` object.

Fame, media and retirement on players. Every player view has `fame` (0 to 100), `fame_words`, `farewell` and, for a retired player, `retired_after_season`. Fame is public: everybody knows who is famous. A retired player is off every club, list and market but his page still opens. For your own players the view adds `media_work` (None, Some or A lot), `manner_in_public` (how he comes across, in words as the profile page says them) and shirt sales. A player's presence is never a number: for other clubs' players only the one-line character shows, as with other traits, and it can read "Showman" or "Loose tongue".

`inbox`: arrived mail, newest first, with the offer, press question or sponsor proposal attached to the message that carries it, and your pending bids.

`table`, `fixtures`, `box_score`, `leaders`, `calendar`: public league data. `market`: free agents, your shortlist and the staff looking for work. `draft`: the order and the prospects while a draft is held. `league`: the table, all fixtures with scores, playoffs, past seasons, leaders, every club and the box scores of your games and the latest round. `full`: everything. It is large; prefer the narrow views.

Match recordings are never in a view. Only box scores are.

## What is hidden, and why

An agent gets the same information a human manager gets from the interface, and no more. The human interface hides some things the world holds, and so does the view.

- Other clubs' players are known only as ranges for each of the twenty-one attributes (Footwork is the newest: balance and feet), for overall and for potential. The ranges narrow as the scouts watch a shortlisted player play. The true value is always inside the range. Body measurements, contract, age, season stats and injuries are public.
- A player's character (a one-line label such as "Leader" or "Volatile"), his condition and his mood are shown for other clubs' players only once he is scouted to 60 percent. Max vertical and touch need 95 percent. Signature moves of another club's player are listed once he is scouted to 60 percent, the same bar as character, or once a scout report has listed them; whether his body can still do a move (`can_still_do`) needs 95 percent. The medical note ("Injury prone", "heals fast") needs 50 percent. Below that the note says there is not enough information and the injury record is what there is to go on.
- The numbers behind personality, and the hidden proneness, recovery and exact potential, are never shown, not even for your own players. You see the labels and the notes the interface shows. For your own players, potential is a range of plus or minus one around the truth, as on the profile page.
- The numbers behind how a player comes across in public (presence) are hidden like the other character numbers. Other clubs' budgets (a word for their money state is shown), tactic sliders, rotations, playbooks, training and reserves are not shown. You see only the names of the offense, defense and press they run, which the club page shows.
- The market value of a player, shown on the bid page, is in `market_value` for other clubs' players. The interface shows it, so the view does too. Be aware that it is computed by the game from true ability, so it carries some information about the hidden numbers. The same goes for the verdicts of `trade_check`.
- During a live game you see your own players' energy, fouls and mood and the opponent's box lines, and the opponent's current system. You do not see the opponent's energy.

Signature moves. A player may have up to three innate signature moves (a one in eight chance, rising with potential). They cannot be taught or changed. Every player view has a `signatures` list, left out when empty: each entry has `id`, `name`, `group`, `description`, `answer` (what a coach can do about it) and, for your own players and fully scouted ones, `can_still_do` (false when his body is no longer up to it). Your own players' signatures are all listed. For other clubs' players only revealed ones appear, and an empty or missing list says nothing about the ones you have not seen. The assistant's pre-game mail names the next opponent's known signatures with the answer to each, and the biggest-names list carries a `signatures` list of the names known. Signatures do not yet change match results.

The view is built from the same functions the interface uses (`Scouting::estimate`, `medical`, `Personality::kind` and the same thresholds), so it cannot drift from what the human sees. Tests in `view.rs` check the fog: ranges contain the truth and are not collapsed for unscouted players, no true attribute or potential appears anywhere in another player's data, hidden keys appear nowhere, and other clubs show no budget or tactic details.

A world has one scouting department and one desk, which belong to the managed club. `view(club)` therefore only answers for the managed club.

## Commands added with the finance system, fame and media

`SetTicketPrice` (an integer within `finance.price.min` to `max`; dragging it is one logged command), `MoveBudget` (`to_wages` true or false, `amount`), `UpgradeFacility` (`"Training"`, `"Medical"` or `"Youth"`), `Renovate` and `Expand` for the hall, `SetMedia` (`{"player": id, "level": "None" | "Some" | "ALot"}`, your first-team players), `AnswerAppearance` (`{"request": n, "accept": bool}`) and `Hire` or `Dismiss` for the press officer (`"Press"`). All are in `legal_actions` with an example the game accepts. Media work raises the player's fame a little and the club's fan base, teaches a poor talker to talk (faster with a good press officer) and costs training gain and freshness. A poor talker, a hot temper or low professionalism can produce a gaffe: a hit to the dressing room, a teammate's mood, an annoyed sponsor and a story in the inbox. A good talker may earn a glowing piece instead. Selling, trading or benching a favourite of the supporters costs fan base and support, and signing a star lifts them. A famous player who decides to retire announces a farewell season (mail, fuller halls and a send-off at his last home game) and retires after it.

## Standing match instructions

Standing instructions are rules a manager leaves for a game he is not coaching live. They are stored on the club and set with the `SetInstructions` command (or the `set_instructions` tool). `PlayWithInstructions` (or `play_round` with `instructions=true`) plays the club's game by stepping it and applying the rules. With no rules the assistant coaches the game exactly as `PlayRound` would, and the log gets `PlayRound`. With rules the game is played live and logged as `PlayLive` with the inputs the rules produced, so the log replays to the same world without the rules.

A rule has `when` (all conditions must hold), `then` (actions in order), `max_fires` (1 to 10 per game) and `cooldown_secs` (game seconds before the rule may fire again). There are at most 16 rules, 8 conditions and 4 actions each.

Conditions: `{"MarginAtMost": -8}`, `{"MarginAtLeast": 10}`, `{"Period": {"from": 3, "to": 4}}`, `{"PeriodClockLeftAtMost": 120}`, `{"ElapsedAtLeast": 600}`, `{"OpponentRunAtLeast": 8}` (unanswered points), `{"PlayerEnergyBelow": {"player": null, "percent": 40}}`, `{"PlayerFoulsAtLeast": {"player": null, "fouls": 4}}` (a null player means any of yours on the floor), `{"TeamFoulsAtLeast": 4}`, and `{"OpponentDefenseIs": "Zone23"}`, `{"OpponentOffenseIs": ...}`, `{"OpponentPressIs": ...}`.

Actions: `"Timeout"`, `{"Swap": {"off": "Triggering", "on": "BestRested"}}` (a player can also be `{"Id": 12}`), `{"SetOrders": {"defense": "PackLine", "pace": 8}}` (only the named parts change), `{"Shout": {"shout": "CalmDown", "target": null}}`, `{"AutoSubs": true}`.

Details worth knowing: the rules look at the game about once per second of play and act at the next dead ball. Time is game time from the minutes played, not the replay clock. A swap turns the assistant's substitutions off, so after a rule's swap has been made the coach gives them back. A timeout is skipped if none is left or one is already asked for. How well the squad knows a new system is worked out by the game from the playbook; neither a rule nor a client can supply it, also in `live_input`. A game coached through the live interface, or by rules, does not get the engine's own automatic timeouts and system changes that an unattended assistant-coached side gets, so with rules you may want one that calls timeouts for a run.

A bench with a person or rules on it is different from the assistant's bench in that way, which is why an empty rule set plays through the plain `PlayRound` path.

## Live matches

`live_kickoff` starts today's game with you on the bench. After that the world is locked (other operations return `live_match_open`) until `live_finish`. A started game cannot be abandoned. `live_state` shows the score, game time, your players, the opponent, timeouts, team fouls, queued swaps and the orders. `live_step` with `seconds` plays about that many seconds of replay time, stopping earlier at a timeout, the end of a period or full time. Without `seconds` it plays to the next such pause. `live_input` takes `"Timeout"`, `{"AutoSubs": false}`, `{"Swap": {"off": id, "on": id}}`, `{"CancelSwap": n}`, `{"Shout": {"shout": "Praise", "target": id}}` or `{"Orders": {"orders": {...}}}`. `live_kickoff` with `instructions=true` applies your standing rules while you step.

## Mapping to an online league

The design assumes the server owns the world. The command log is the authoritative record: a career is its seed plus its log, and replaying the log into a fresh world gives an identical world (there is a test for exactly that, including games played live and games played by rules).

A scheduled game would work like this. The server keeps the world and a clock. Between games each club is driven by whatever is connected to it: a human client, or an agent speaking this protocol. Each issues commands, which the server validates with the same `apply` and appends to the log. At the scheduled time the server plays the round. For each club it checks who is present. A club with a live coach gets a live match, over `kickoff` and the live operations, with the same `LiveInput` set. A club with an agent that did not ask to coach live gets its standing instructions applied by `PlayWithInstructions`. A club with nobody gets the same thing, since an empty rule set is the assistant's game. Nothing in the protocol needs a person to be present, and the result is the same deterministic function of the log either way.

What this repository does not yet have for that is several managed clubs in one world. Today there is one `watch` club with one desk and one scouting department, and the views and commands act for it. An online version would need a desk and a scouting department per club and a club id on every command. The views take the club as a parameter for that reason.
