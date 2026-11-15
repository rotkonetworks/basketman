//! `basketman mcp`: a Model Context Protocol server on stdio.
//!
//! Newline-delimited JSON-RPC 2.0, as the stdio transport specifies. Only
//! protocol messages go to stdout; logging goes to stderr. Implemented by
//! hand: `initialize`, `notifications/initialized`, `ping`, `tools/list`,
//! `tools/call`. The tools are the operations of `agent.rs`.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::agent::{AgentError, Session};

/// The revision this server offers when the client asks for one it does
/// not know: the newest one the author is sure exists. 2025-11-25 is also
/// accepted when asked for, but is from memory (see docs/agents.md).
const LATEST: &str = "2025-06-18";
const SUPPORTED: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "You manage a basketball club in a deterministic league sim. Start with new_game (or load_game). \
Then call view with what=summary: it says what the game is waiting for. Read the inbox (view what=inbox): offers for your players, press questions, \
sponsor proposals, appearance requests and the draft need answers; the finance view has the ticket price (SetTicketPrice), budgets (MoveBudget) and facilities (UpgradeFacility); SetMedia gives players media work for fame and fans; use legal_actions to see exactly which commands are valid and their choices, then apply_command. \
Move time with advance (stops at match day, new mail or the draft), continue (one day) and play_round on match days. Leave standing_rules with set_instructions \
if you do not want to coach live. Other clubs' players are shown as ranges that narrow when you shortlist them.";

struct Tool {
    name: &'static str,
    op: &'static str,
    description: &'static str,
    schema: fn() -> Value,
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": required, "additionalProperties": false})
}

fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "new_game",
            op: "new_game",
            description: "Start a new league and take charge of one club. The same seed gives the same league and the same results for the same decisions. Returns the summary view. Call this first unless you load a save.",
            schema: || {
                obj(
                    json!({
                        "seed": {"type": "integer", "minimum": 0, "description": "Random seed, default 1."},
                        "clubs": {"type": "integer", "minimum": 2, "maximum": 20, "description": "Number of clubs, even, default 12."},
                        "club": {"type": "integer", "minimum": 0, "description": "Which club (0-based) you manage, default 0."}
                    }),
                    &[],
                )
            },
        },
        Tool {
            name: "load_game",
            op: "load",
            description: "Load a saved career from a file path on the machine running this server.",
            schema: || obj(json!({"path": {"type": "string"}}), &["path"]),
        },
        Tool {
            name: "save_game",
            op: "save",
            description: "Save the career to a file path on the machine running this server.",
            schema: || obj(json!({"path": {"type": "string"}}), &["path"]),
        },
        Tool {
            name: "view",
            op: "view",
            description: "Look at the world exactly as your club's manager sees it. Start with what=summary (calendar, record, budget, next game, and what the game is waiting for). Other views: own (your whole club: squad with attributes, condition, contracts, tactics playbook, training, staff, finances, arena, sponsors, standing instructions), squad, player (needs player id; for other clubs' players the figures are ranges that narrow when you shortlist him), club (needs club id: public facts, scouted squad), inbox (messages, with offers and questions that need answers; unread_only optional), table, fixtures (optional round, club), box_score (needs fixture index), market (free agents, shortlist, staff for hire), draft (prospects and order while a draft is on), calendar, leaders, league (with the biggest names and the hall of fame), finance (ticket price and a preview of crowd and gate one step either side, board budgets, overdraft, luxury tax, facilities, projection, shop and top shirt sellers, fan base), fans, names (biggest names and hall of fame; n optional), appearances (sponsors' requests for a player's time), full (everything; large). Fame is public for every player; media work and manner in public are shown for your own players only.",
            schema: || {
                obj(
                    json!({
                        "what": {"type": "string", "enum": ["summary", "own", "squad", "player", "club", "inbox", "table", "fixtures", "box_score", "market", "draft", "calendar", "leaders", "league", "finance", "fans", "names", "appearances", "full"]},
                        "player": {"type": "integer", "description": "Player id, for what=player."},
                        "club": {"type": "integer", "description": "Club id, for what=club or a fixtures filter."},
                        "fixture": {"type": "integer", "description": "Fixture index, for what=box_score."},
                        "round": {"type": "integer"},
                        "day": {"type": "integer", "description": "For what=calendar: list everything on this day of the season, across the league."},
                        "from": {"type": "integer", "description": "For what=calendar: first day of a range of events (with to; club filters)."},
                        "to": {"type": "integer"},
                        "unread_only": {"type": "boolean"},
                        "n": {"type": "integer"}
                    }),
                    &["what"],
                )
            },
        },
        Tool {
            name: "legal_actions",
            op: "legal",
            description: "List what you can do right now: what the game is waiting for (a draft pick, an open offer, a press question, a sponsor proposal, a sponsor's appearance request, a match day, or nothing) and every kind of command with whether it is available, why not, its parameters and valid choices, and a concrete example command that will be accepted. Use it to build a valid command without guessing.",
            schema: || obj(json!({}), &[]),
        },
        Tool {
            name: "apply_command",
            op: "apply",
            description: "Carry out one manager command. `command` is a Command as JSON: a bare string for commands without parameters (\"Advance\", \"PlayRound\", \"Renovate\"), or an object with the variant name as its key, e.g. {\"AnswerOffer\": {\"offer\": 0, \"accept\": false}}, {\"AnswerPress\": {\"question\": 0, \"choice\": 1}}, {\"Bid\": {\"player\": 12, \"fee\": 0}}, {\"SetTraining\": {\"player\": 3, \"focus\": \"Shooting\", \"intensity\": \"Normal\"}}, {\"UseTactic\": 1}, {\"DraftPick\": 140}. A flat form {\"type\": \"Bid\", \"player\": 12, \"fee\": 0} is also accepted. A refused command changes nothing and tells you why. Get valid choices from legal_actions.",
            schema: || {
                obj(
                    json!({"command": {"description": "A Command, as serde JSON."}}),
                    &["command"],
                )
            },
        },
        Tool {
            name: "advance",
            op: "advance",
            description: "Let the calendar run until the next thing that needs you: a game today (stop=match_day), new mail (stop=mail) or the draft (stop=draft). Answer open offers and questions first: unanswered ones lapse. During the draft it stops until you pick (DraftPick or DraftAuto through apply_command).",
            schema: || obj(json!({}), &[]),
        },
        Tool {
            name: "continue",
            op: "continue",
            description: "Pass one day. Whatever is scheduled for it happens: other clubs' games, reserve games, preseason friendlies, camp training. Returns the new date and what is on it for your club (`today`), and `stop` if the new day holds mail, a game for you (match_day) or the draft. If you have a game today, nothing is passed (stop=match_day): play it first. Use advance to jump to the next thing that needs you.",
            schema: || obj(json!({}), &[]),
        },
        Tool {
            name: "play_round",
            op: "play_round",
            description: "Play this round's games on a match day without coaching live. By default the whole round (Friday to Sunday) is played at once. With today=true only today's games are played (yours with your standing instructions or the assistant) and the rest of the weekend is played as the days pass. With instructions=true your game is coached by your standing instructions (see set_instructions); otherwise your assistant coach runs it. Returns the scores.",
            schema: || obj(json!({"instructions": {"type": "boolean"}, "today": {"type": "boolean"}}), &[]),
        },
        Tool {
            name: "suggest_tactics",
            op: "suggest_tactics",
            description: "A valid Tactics value for your club: the current sliders with a rotation the assistant picks from your fit squad. Use it as the base for a SetTactics command, especially after the squad changed.",
            schema: || obj(json!({}), &[]),
        },
        Tool {
            name: "trade_check",
            op: "trade_check",
            description: "Ask the other club's general manager whether he would accept a trade, without making it. `deal` is {club, give: [your player ids], get: [their player ids], give_picks: [pick indices], get_picks: [...], cash}. Make it with apply_command ProposeTrade.",
            schema: || obj(json!({"deal": {"type": "object"}}), &["deal"]),
        },
        Tool {
            name: "set_instructions",
            op: "set_instructions",
            description: "Leave standing match instructions: if-then rules the game applies when it plays your game for you (play_round with instructions=true, or live stepping with standing rules, or a server when you are absent). `instructions` is {rules: [{name, when: [conditions], then: [actions], max_fires, cooldown_secs}]}. Conditions: {\"MarginAtMost\": -8}, {\"MarginAtLeast\": 10}, {\"Period\": {\"from\": 3, \"to\": 4}}, {\"PeriodClockLeftAtMost\": 120}, {\"ElapsedAtLeast\": 600}, {\"OpponentRunAtLeast\": 8}, {\"PlayerEnergyBelow\": {\"player\": null, \"percent\": 40}}, {\"PlayerFoulsAtLeast\": {\"player\": null, \"fouls\": 4}}, {\"TeamFoulsAtLeast\": 4}, {\"OpponentDefenseIs\": \"Zone23\"}, {\"OpponentOffenseIs\": ...}, {\"OpponentPressIs\": ...}. Actions: \"Timeout\", {\"Swap\": {\"off\": \"Triggering\", \"on\": \"BestRested\"}}, {\"SetOrders\": {\"defense\": \"PackLine\", \"pace\": 8}}, {\"Shout\": {\"shout\": \"CalmDown\", \"target\": null}}, {\"AutoSubs\": true}. At most 16 rules, 8 conditions and 4 actions per rule, max_fires 1 to 10. An empty rules list leaves the game to the assistant.",
            schema: || {
                obj(
                    json!({"instructions": {"type": "object"}}),
                    &["instructions"],
                )
            },
        },
        Tool {
            name: "live_kickoff",
            op: "kickoff",
            description: "Coach today's game yourself, live. Only on a match day. After this the world is locked until live_finish: use live_state, live_step and live_input, then live_finish. Set instructions=true to have your standing rules applied automatically while you step. Set today=true to hold back only today's games instead of the whole round. You cannot abort a started game.",
            schema: || obj(json!({"instructions": {"type": "boolean"}, "today": {"type": "boolean"}}), &[]),
        },
        Tool {
            name: "live_state",
            op: "live_state",
            description: "The live game as a coach sees it: score, game time in seconds, your players (on court, energy %, fouls, stats, mood), the opponent's players' box lines, timeouts and team fouls, queued swaps, your orders and the opponent's current system.",
            schema: || obj(json!({}), &[]),
        },
        Tool {
            name: "live_step",
            op: "live_step",
            description: "Advance the live game. With seconds=N, play about N seconds of replay time (stops earlier at a timeout, the end of a period or full time); without it, play to the next such pause. Returns the new state and why it paused. Substitutions, timeouts and new orders take effect at the next dead ball.",
            schema: || obj(json!({"seconds": {"type": "number", "minimum": 0}}), &[]),
        },
        Tool {
            name: "live_input",
            op: "live_input",
            description: "Do something as the coach. `input` is one of: \"Timeout\" (asks for one at the next dead ball; again to wave it off), {\"AutoSubs\": false}, {\"Swap\": {\"off\": id on court, \"on\": id on bench}} (turns the assistant's substitutions off until you give them back with {\"AutoSubs\": true}), {\"CancelSwap\": index}, {\"Shout\": {\"shout\": \"Encourage|DemandMore|CalmDown|Focus|Praise\", \"target\": id or null}}, {\"Orders\": {\"orders\": {offense, defense, press, pace, three_bias, crash_boards, aggression}}} (how well the squad knows the new system is worked out by the game from your playbook).",
            schema: || {
                obj(
                    json!({"input": {"description": "A LiveInput."}}),
                    &["input"],
                )
            },
        },
        Tool {
            name: "live_finish",
            op: "live_finish",
            description: "Finish the live game (playing out any time left without further changes from you), book the result, play the other games of the round, and return the scores. Unlocks the world.",
            schema: || obj(json!({}), &[]),
        },
        Tool {
            name: "command_log",
            op: "log",
            description: "The list of every command carried out so far, in order. The log plus the seed is the whole career.",
            schema: || obj(json!({"from": {"type": "integer", "minimum": 0}}), &[]),
        },
    ]
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_result(r: Result<Value, AgentError>) -> Value {
    match r {
        Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}], "isError": false}),
        Err(e) => {
            json!({"content": [{"type": "text", "text": e.to_json().to_string()}], "isError": true})
        }
    }
}

/// Answer one JSON-RPC message; `None` for notifications.
pub fn handle(session: &mut Session, msg: &Value) -> Option<Value> {
    let Some(o) = msg.as_object() else {
        return Some(error(&Value::Null, -32600, "invalid request"));
    };
    let id = o.get("id").cloned();
    let Some(method) = o.get("method").and_then(Value::as_str) else {
        // A response or garbage: nothing to answer.
        return id.map(|id| error(&id, -32600, "invalid request"));
    };
    let params = o.get("params").cloned().unwrap_or(Value::Null);
    // Notifications carry no id and get no answer.
    let id = id?;
    let result = match method {
        "initialize" => {
            let want = params.get("protocolVersion").and_then(Value::as_str);
            let version = want.filter(|v| SUPPORTED.contains(v)).unwrap_or(LATEST);
            json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "basketman", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS,
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools": tools().iter().map(|t| json!({
            "name": t.name, "description": t.description, "inputSchema": (t.schema)()})).collect::<Vec<_>>()}),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error(&id, -32602, "tools/call needs a tool name"));
            };
            let Some(tool) = tools().into_iter().find(|t| t.name == name) else {
                return Some(error(&id, -32602, &format!("unknown tool {name}")));
            };
            let args = match params.get("arguments") {
                Some(a @ Value::Object(_)) => a.clone(),
                None | Some(Value::Null) => json!({}),
                Some(_) => return Some(error(&id, -32602, "arguments must be an object")),
            };
            tool_result(session.call(tool.op, &args))
        }
        _ => return Some(error(&id, -32601, &format!("method not found: {method}"))),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

pub fn run() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("basketman mcp: internal error: {info}")
    }));
    eprintln!("basketman mcp: ready (stdio, protocol {LATEST})");
    let mut session = Session::new();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Err(e) => Some(error(&Value::Null, -32700, &format!("parse error: {e}"))),
            Ok(Value::Array(batch)) => {
                let replies: Vec<Value> = batch
                    .iter()
                    .filter_map(|m| handle(&mut session, m))
                    .collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            Ok(msg) => handle(&mut session, &msg),
        };
        if let Some(r) = reply
            && (writeln!(out, "{r}").is_err() || out.flush().is_err())
        {
            break;
        }
    }
}
