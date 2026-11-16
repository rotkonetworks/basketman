#!/usr/bin/env python3
"""End-to-end test of `basketman mcp` over stdio. Run: python3 test_mcp.py [binary]"""
import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools"))
from client import default_binary  # noqa: E402

BIN = sys.argv[1] if len(sys.argv) > 1 else default_binary()
checks = 0


def check(cond, msg):
    global checks
    checks += 1
    if not cond:
        raise AssertionError(msg)


class Mcp:
    def __init__(self):
        self.proc = subprocess.Popen([BIN, "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, text=True, bufsize=1)
        self.n = 0
        self.stdout_lines = []

    def send(self, obj):
        self.proc.stdin.write((obj if isinstance(obj, str) else json.dumps(obj)) + "\n")
        self.proc.stdin.flush()

    def recv(self):
        line = self.proc.stdout.readline()
        if not line:
            raise AssertionError("server closed stdout")
        self.stdout_lines.append(line)
        return json.loads(line)

    def rpc(self, method, params=None):
        self.n += 1
        msg = {"jsonrpc": "2.0", "id": self.n, "method": method}
        if params is not None:
            msg["params"] = params
        self.send(msg)
        r = self.recv()
        check(r["jsonrpc"] == "2.0" and r["id"] == self.n, f"response id: {r}")
        return r

    def tool(self, name, **args):
        r = self.rpc("tools/call", {"name": name, "arguments": args})
        check("result" in r, f"{name}: {r}")
        res = r["result"]
        check(res["content"][0]["type"] == "text", "text content")
        return json.loads(res["content"][0]["text"]), res["isError"]

    def ok(self, name, **args):
        v, err = self.tool(name, **args)
        check(not err, f"{name} failed: {v}")
        return v


def main():
    m = Mcp()
    try:
        r = m.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                 "clientInfo": {"name": "test", "version": "0"}})
        res = r["result"]
        check(res["protocolVersion"] == "2025-06-18", "echoes a supported version")
        check("tools" in res["capabilities"] and res["serverInfo"]["name"] == "basketman", "capabilities and info")
        r2 = m.rpc("initialize", {"protocolVersion": "1999-01-01"})
        check(r2["result"]["protocolVersion"] == "2025-06-18", "unknown version gets the server's own")
        # A notification gets no answer: the next response is the ping's.
        m.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        check(m.rpc("ping")["result"] == {}, "ping")

        tools = m.rpc("tools/list")["result"]["tools"]
        names = {t["name"] for t in tools}
        for want in ["new_game", "load_game", "save_game", "view", "legal_actions", "apply_command", "advance",
                     "play_round", "set_instructions", "live_kickoff", "live_step", "live_feeds", "live_input", "live_finish"]:
            check(want in names, f"tool {want} listed")
        for t in tools:
            check(len(t["description"]) > 40, f"{t['name']} description")
            check(t["inputSchema"]["type"] == "object" and "properties" in t["inputSchema"], f"{t['name']} schema")

        # Errors.
        check(m.rpc("nope")["error"]["code"] == -32601, "unknown method")
        check(m.rpc("tools/call", {"name": "nope"})["error"]["code"] == -32602, "unknown tool")
        m.send("{not json")
        check(m.recv()["error"]["code"] == -32700, "parse error")
        v, err = m.tool("view", what="summary")
        check(err and v["kind"] == "no_game", "a tool error is isError, not a protocol error")

        # Play.
        s = m.ok("new_game", seed=5, clubs=8, club=1)
        check(s["name"], "new game")
        squad = m.ok("view", what="squad")["first_team"]
        check(len(squad) == 12, "squad")
        own = m.ok("view", what="own")
        t = own["playbook"][0]["tactics"]
        newpace = 6 if t["pace"] != 6 else 7
        m.ok("apply_command", command={"SetTactics": {"slot": 0, "tactics": dict(t, pace=newpace)}})
        check(m.ok("view", what="own")["playbook"][0]["tactics"]["pace"] == newpace, "tactic changed")
        v, err = m.tool("apply_command", command={"UseTactic": 9})
        check(err and v["kind"] == "out_of_range", "refusal is reported")
        legal = m.ok("legal_actions")
        check(legal["actions"] and "waiting" in legal, "legal actions")

        rounds0 = m.ok("view", what="summary")["calendar"]["round"]
        for _ in range(100):
            w = m.ok("view", what="summary")["waiting"]
            if w["kind"] == "match_day":
                break
            if w["kind"] == "offer":
                m.ok("apply_command", command={"AnswerOffer": {"offer": w["offer"], "accept": False}})
            elif w["kind"] == "press":
                m.ok("apply_command", command={"AnswerPress": {"question": w["question"], "choice": 1}})
            elif w["kind"] == "sponsor_offer":
                m.ok("apply_command", command={"AnswerSponsor": {"offer": w["offer"], "accept": True}})
            elif w["kind"] == "appearance":
                m.ok("apply_command", command={"AnswerAppearance": {"request": w["request"], "accept": True}})
            else:
                m.ok("advance")
        res = m.ok("play_round")
        check(res["played"], "round played")
        check(m.ok("view", what="summary")["calendar"]["round"] == rounds0 + 1, "a full round was played")
        table = m.ok("view", what="table")
        check(len(table) == 8 and sum(t["wins"] + t["losses"] for t in table) == 8, "table")

        # Money, fans, fame and media through the tools.
        fin = m.ok("view", what="finance")
        check(fin["price"]["min"] <= fin["price"]["in_force"] <= fin["price"]["max"], "price in range")
        check(fin["budgets"]["wage_budget"] > 0 and fin["debt"]["overdraft_limit"] > 0 and fin["tax"]["cap"] > 0, "budgets, debt, tax")
        check(len(fin["facilities"]) == 3 and fin["fans"]["base"] > 0, "facilities and fans")
        check(m.ok("view", what="names")["biggest"], "names")
        view_enum = next(t for t in tools if t["name"] == "view")["inputSchema"]["properties"]["what"]["enum"]
        check({"finance", "fans", "names", "appearances"} <= set(view_enum), "new views are in the schema")
        kinds = {a["kind"]: a for a in m.ok("legal_actions")["actions"]}
        for k in ["SetTicketPrice", "MoveBudget", "UpgradeFacility", "SetMedia"]:
            check(kinds[k]["available"], f"{k} available")
            m.ok("apply_command", command=kinds[k]["example"])
        check(m.ok("view", what="finance")["price"]["in_force"] != fin["price"]["in_force"], "price changed")
        media = [q for q in m.ok("view", what="squad")["first_team"] if q["media_work"] != "None"]
        check(media and "fame" in media[0], "media work shows on the player")
        m.ok("view", what="appearances")

        # Live through the tools.
        for _ in range(100):
            if m.ok("view", what="summary")["waiting"]["kind"] == "match_day":
                break
            w = m.ok("view", what="summary")["waiting"]
            if w["kind"] == "offer":
                m.ok("apply_command", command={"AnswerOffer": {"offer": w["offer"], "accept": False}})
            elif w["kind"] == "press":
                m.ok("apply_command", command={"AnswerPress": {"question": w["question"], "choice": 0}})
            elif w["kind"] == "sponsor_offer":
                m.ok("apply_command", command={"AnswerSponsor": {"offer": w["offer"], "accept": False}})
            elif w["kind"] == "appearance":
                m.ok("apply_command", command={"AnswerAppearance": {"request": w["request"], "accept": False}})
            else:
                m.ok("advance")
        st = m.ok("live_kickoff")
        st = m.ok("live_step", seconds=60)
        check(st["time"] > 0, "live step")
        feeds = m.ok("live_feeds", part="scores")
        check(feeds["games"] and "table" in feeds, "live_feeds scores")
        m.ok("live_input", input="Timeout")
        res = m.ok("live_finish")
        check(res["games"], "live finished")

        # Only protocol messages on stdout.
        for line in m.stdout_lines:
            o = json.loads(line)
            check(o["jsonrpc"] == "2.0" and ("result" in o or "error" in o), "stdout is only JSON-RPC")
        err = ""
    finally:
        m.proc.stdin.close()
        m.proc.wait(timeout=10)
        err = m.proc.stderr.read()
    check("ready" in err, "logging goes to stderr")
    print(f"mcp server: {checks} checks passed")


if __name__ == "__main__":
    main()
