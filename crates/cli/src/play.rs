//! `basketman play`: one JSON request per line on stdin, one JSON response
//! per line on stdout.
//!
//! A request is an object with an `op` and that operation's arguments next
//! to it (or under `args`), plus an optional `id` that the response echoes:
//!
//! ```text
//! {"id": 1, "op": "new_game", "seed": 7, "clubs": 12, "club": 3}
//! {"id": 1, "ok": true, "result": {...}}
//! {"id": 2, "ok": false, "error": {"kind": "refused", "message": "..."}}
//! ```
//!
//! Malformed input gets an error response, never a crash. See
//! `docs/agents.md` for the operations.

use std::io::{BufRead, Write};

use serde_json::{Map, Value, json};

use crate::agent::{AgentError, Session};

/// Answer one request line.
pub fn respond(session: &mut Session, line: &str) -> Value {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return json!({"id": Value::Null, "ok": false,
                "error": {"kind": "parse_error", "message": format!("not JSON: {e}")}});
        }
    };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let result = handle(session, &req);
    match result {
        Ok(v) => json!({"id": id, "ok": true, "result": v}),
        Err(e) => json!({"id": id, "ok": false, "error": e.to_json()}),
    }
}

fn handle(session: &mut Session, req: &Value) -> Result<Value, AgentError> {
    let Some(obj) = req.as_object() else {
        return Err(AgentError::new(
            "bad_request",
            "a request is a JSON object with an op",
        ));
    };
    let Some(op) = obj.get("op").and_then(Value::as_str) else {
        return Err(AgentError::new("bad_request", "missing op (a string)"));
    };
    let args: Value = match obj.get("args") {
        Some(a @ Value::Object(_)) => a.clone(),
        Some(Value::Null) | None => {
            let rest: Map<String, Value> = obj
                .iter()
                .filter(|(k, _)| *k != "op" && *k != "id")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            Value::Object(rest)
        }
        Some(_) => return Err(AgentError::new("bad_request", "args must be an object")),
    };
    session.call(op, &args)
}

pub fn run() {
    // A panic message must not reach stdout; stderr only.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("basketman play: internal error: {info}")
    }));
    let mut session = Session::new();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let response = respond(&mut session, &line);
        if writeln!(out, "{response}").is_err() || out.flush().is_err() {
            break;
        }
    }
}
