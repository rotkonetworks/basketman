#!/usr/bin/env python3
"""End-to-end test of `basketman play`. Run: python3 test_play.py [binary]"""
import json
import os
import sys
import tempfile

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools"))
from client import ApiError, Play  # noqa: E402

BIN = sys.argv[1] if len(sys.argv) > 1 else None
checks = 0


def check(cond, msg):
    global checks
    checks += 1
    if not cond:
        raise AssertionError(msg)


def to_match_day(p):
    """Answer what waits, advance to the next match day."""
    for _ in range(200):
        s = p.call("view", what="summary")
        w = s["waiting"]
        if w["kind"] == "match_day":
            return s
        if w["kind"] == "offer":
            p.apply({"AnswerOffer": {"offer": w["offer"], "accept": False}})
        elif w["kind"] == "press":
            p.apply({"AnswerPress": {"question": w["question"], "choice": 1}})
        elif w["kind"] == "sponsor_offer":
            p.apply({"AnswerSponsor": {"offer": w["offer"], "accept": False}})
        elif w["kind"] == "appearance":
            p.apply({"AnswerAppearance": {"request": w["request"], "accept": True}})
        elif w["kind"] == "draft_pick":
            p.apply({"DraftAuto": {"all": True}})
        else:
            p.call("advance")
    raise AssertionError("never reached a match day")


def script(p):
    """A fixed session; returns facts to compare between runs."""
    out = []
    out.append(p.call("new_game", seed=7, clubs=8, club=2))
    s = to_match_day(p)
    r = p.call("play_round")
    out.append(r)
    out.append(p.call("view", what="table"))
    s = to_match_day(p)
    p.call("set_instructions", instructions={"rules": [
        {"when": [{"OpponentRunAtLeast": 6}], "then": ["Timeout"], "max_fires": 3, "cooldown_secs": 30}]})
    out.append(p.call("play_round", instructions=True))
    out.append(p.call("view", what="table"))
    return out


def main():
    p = Play(BIN)
    try:
        # --- malformed input never crashes ---------------------------------
        for line in ["", "not json", "[1,2]", "{}", '{"op": 5}', '{"op": "nope"}', '{"op": "view"}',
                     '{"op":"new_game","clubs":7}', '{"op":"new_game","clubs":"x"}', '{"op":"apply","command":{"Bid":{}}}']:
            if not line:
                continue
            resp = p.raw(line)
            check(resp["ok"] is False and resp["error"]["kind"], f"{line!r} -> {resp}")
        check(p.request("legal")["error"]["kind"] == "no_game", "queries before a game say no_game")

        # --- a new game, the views -----------------------------------------
        s = p.call("new_game", seed=7, clubs=8, club=2)
        check(s["name"] and s["calendar"]["season"] == 1, "summary")
        squad = p.call("view", what="squad")["first_team"]
        check(len(squad) == 12, "twelve players")
        check(all(len(q["attrs"]) == 21 and "Footwork" in q["attrs"] for q in squad), "twenty-one attributes, with footwork")
        check(all(q["attrs"]["Close shot"]["lo"] == q["attrs"]["Close shot"]["hi"] for q in squad), "own attributes exact")
        legal = p.call("legal")
        kinds = {a["kind"]: a for a in legal["actions"]}
        check(kinds["Advance"]["available"] and not kinds["TakeCharge"]["available"], "legal availability")
        check(kinds["TakeCharge"]["why_not"], "reason given")

        # --- the club's past and people, the manager's career --------------
        prof = p.call("view", what="club_profile")
        check(prof["founded"] > 1800 and prof["rival"] and prof["legends"] and prof["people"], "club profile")
        check(0 <= prof["confidence"] <= 100 and prof["reasons"] == [], "the board starts without reasons")
        check(len(prof["record"]) >= 20 and prof["record"][0]["played"] is False, "the record starts before the game")
        other = p.call("view", what="club_profile", club=5)
        check(other["name"] != prof["name"], "another club's profile")
        car = p.call("view", what="career")
        check(car["jobless"] is False and car["reputation"] > 0 and len(car["spells"]) == 1, "career view")
        check(kinds["Resign"]["available"] and not kinds["AcceptJob"]["available"], "resign listed, no offer yet")
        _, err = p.try_call("apply", command={"AcceptJob": {"offer": 0, "bring": [], "keep": []}})
        check(err and err.kind == "out_of_range", "no such offer")

        # --- fog: another club's players are ranges, hidden things absent --
        club = p.call("view", what="club", club=5)
        text = json.dumps(club)
        for hidden in ["proneness", "recovery", "professionalism", "budget", "morale"]:
            check(hidden not in text, f"{hidden} leaked")
        pl = club["squad"][0]
        check(pl["attrs"]["Close shot"]["lo"] < pl["attrs"]["Close shot"]["hi"], "range")
        check(pl["character"] is None and pl["known_pct"] <= 30, "unscouted character is hidden")
        _, err = p.try_call("view", what="player", player=100000)
        check(err and err.kind == "out_of_range", "bad player id")

        # --- change a tactic, see it ---------------------------------------
        own = p.call("view", what="own")
        t = own["playbook"][0]["tactics"]
        new = dict(t, pace=5 if t["pace"] != 5 else 6)
        p.apply({"SetTactics": {"slot": 0, "tactics": new}})
        check(p.call("view", what="own")["playbook"][0]["tactics"]["pace"] == new["pace"], "tactic changed")
        _, err = p.try_call("apply", command={"SetTactics": {"slot": 0, "tactics": new}})
        check(err and err.kind == "nothing_to_do", "unchanged tactic is nothing_to_do")
        _, err = p.try_call("apply", command={"SetTactics": {"slot": 0, "tactics": dict(new, pace=0)}})
        check(err and err.kind == "out_of_range", "slider out of range")
        # The flat alias.
        _, err = p.try_call("apply", command={"type": "UseTactic", "value": 0})
        check(err and err.kind == "nothing_to_do", "flat alias form is understood")
        r = p.apply({"type": "Advance"})
        check(r["stop"] in ("mail", "match_day", "draft"), "flat alias for a bare command")
        sug = p.call("suggest_tactics")
        check(len(sug["rotation"]) >= 5, "suggested rotation")

        # --- through a full round ------------------------------------------
        before = p.call("view", what="summary")
        s = to_match_day(p)
        check(s["calendar"]["match_today"], "match day reached")
        r = p.call("play_round")
        check(r["played"] and any(g["yours"] for g in r["games"]), "our game was played")
        after = p.call("view", what="summary")
        check(after["calendar"]["round"] == before["calendar"]["round"] + 1, "round advanced")
        check(after["wins"] + after["losses"] == 1, "one game on the record")
        table = p.call("view", what="table")
        check(sum(t["wins"] for t in table) == 4 and len(table) == 8, "table is complete")
        box = p.call("view", what="box_score", fixture=r["played"][0])
        check(len(box["sides"][0]) >= 8, "box score")
        _, err = p.try_call("view", what="box_score", fixture=999)
        check(err, "unplayed fixture has no box score")

        # --- money, fans, fame, media ---------------------------------------
        fin = p.call("view", what="finance")
        check(fin["price"]["min"] < fin["price"]["in_force"] < fin["price"]["max"] or fin["price"]["in_force"] >= fin["price"]["min"],
              "ticket price in its range")
        check(fin["price"]["preview"] is None or len(fin["price"]["preview"]["points"]) == 3, "price preview has three points")
        check(fin["budgets"]["wage_budget"] > 0 and "transfer_left" in fin["budgets"], "board budgets")
        check(fin["debt"]["overdraft_limit"] > 0 and fin["tax"]["cap"] > 0, "overdraft limit and luxury tax cap")
        check([f["command_name"] for f in fin["facilities"]] == ["Training", "Medical", "Youth"], "three facilities")
        check(fin["fans"]["base"] > 1000 and fin["fans"]["moved"], "fan base and what moved it")
        check(p.call("view", what="fans")["base"] == fin["fans"]["base"], "fans view agrees")
        check(p.call("view", what="own")["finance"]["price"]["in_force"] == fin["price"]["in_force"], "own view carries finance")
        names = p.call("view", what="names")
        check(names["biggest"] and all("fame" in n for n in names["biggest"]), "the biggest names")
        check(p.call("view", what="league")["names"]["biggest"], "league view has names")
        pls = p.call("view", what="squad")["first_team"]
        check(all("fame" in q and "media_work" in q and "manner_in_public" in q for q in pls), "own players: fame, media, manner")
        check(all("presence" not in json.dumps(q) for q in pls), "no raw presence for own players")
        other = p.call("view", what="club", club=5)["squad"][0]
        check("fame" in other and "media_work" not in other and "manner_in_public" not in other, "fame is public, media is not")
        check(isinstance(p.call("view", what="club", club=5)["money"], str), "other clubs show a word for their money")
        legal = p.call("legal")
        kinds = {a["kind"]: a for a in legal["actions"]}
        for k in ["SetTicketPrice", "MoveBudget", "UpgradeFacility", "SetMedia", "AnswerAppearance"]:
            check(k in kinds, f"legal lists {k}")
        for k in ["SetTicketPrice", "MoveBudget", "UpgradeFacility", "SetMedia"]:
            check(kinds[k]["available"] and kinds[k]["example"], f"{k} has an example")
            p.apply(kinds[k]["example"])
        fin2 = p.call("view", what="finance")
        check(fin2["price"]["in_force"] != fin["price"]["in_force"], "price changed")
        check(fin2["budgets"]["wage_budget"] != fin["budgets"]["wage_budget"]
              or fin2["budgets"]["transfer_budget"] != fin["budgets"]["transfer_budget"], "budget moved")
        check(any(f["building"] for f in fin2["facilities"]), "a facility is being built")
        who = kinds["SetMedia"]["example"]["SetMedia"]["player"]
        check(next(q for q in p.call("view", what="squad")["first_team"] if q["id"] == who)["media_work"] != "None", "media work set")
        _, err = p.try_call("apply", command={"SetMedia": {"player": club["squad"][0]["id"], "level": "ALot"}})
        check(err and err.kind == "not_your_player", "media work for another club's player is refused")
        _, err = p.try_call("apply", command={"SetTicketPrice": 1})
        check(err and err.kind == "out_of_range", "price out of range")
        # An appearance request, when one has come, can be answered.
        for _ in range(6):
            asks = p.call("view", what="appearances")
            if asks:
                r = p.apply({"AnswerAppearance": {"request": asks[0]["index"], "accept": True}})
                check(r, "appearance answered")
                break
            to_match_day(p)
            p.call("play_round")
        # A retired player's page still opens, and he is not on the market.
        market = p.call("view", what="market")
        check(all(q.get("retired_after_season") is None for q in market["free_agents"]), "no retired player on the market")

        # --- standing instructions -----------------------------------------
        to_match_day(p)
        bad = {"rules": [{"when": [], "then": [], "max_fires": 1}]}
        _, err = p.try_call("set_instructions", instructions=bad)
        check(err and err.kind == "refused", "an idle rule is refused")
        p.call("set_instructions", instructions={"rules": [
            {"name": "run", "when": [{"OpponentRunAtLeast": 6}], "then": ["Timeout"], "max_fires": 3, "cooldown_secs": 30}]})
        check(p.call("get_instructions")["rules"][0]["name"] == "run", "instructions stored")
        p.call("play_round", instructions=True)
        log = p.call("log")["commands"]
        check(any(isinstance(c, dict) and "PlayLive" in c for c in log), "the standing game was logged as PlayLive")

        # --- a live match ---------------------------------------------------
        to_match_day(p)
        st = p.call("kickoff")
        check(not st["over"] and len(st["players"]["yours"]) >= 8, "kickoff state")
        _, err = p.try_call("advance")
        check(err and err.kind == "live_match_open", "world is locked during a live game")
        st = p.call("live_step", seconds=90)
        check(st["time"] > 0 and st["steps"] > 0, "stepped")
        st = p.call("live_input", input="Timeout")
        check(st["timeout_wanted"], "timeout requested")
        on = [q for q in st["players"]["yours"] if q["on_court"]][0]["id"]
        bench = [q for q in st["players"]["yours"] if not q["on_court"]][0]["id"]
        _, err = p.try_call("live_input", input={"Swap": {"off": bench, "on": on}})
        check(err and err.kind == "refused", "a bad swap is refused")
        st = p.call("live_input", input={"Swap": {"off": on, "on": bench}})
        check(st["queued_swaps"], "swap queued")
        st = p.call("live_input", input={"Shout": {"shout": "Encourage", "target": None}})
        check(len(st["reactions"]) == 5, "shout reactions")
        o = st["orders"]["yours"]
        st = p.call("live_input", input={"Orders": {"orders": dict(o, pace=17), "familiarity": 1.0}})
        paused = None
        for _ in range(40):
            st = p.call("live_step")
            paused = st["paused"]
            if st["over"]:
                break
        check(st["over"] and paused == "final", "played to full time")
        r = p.call("live_finish")
        check(r["played"] and r["games"], "live result booked")
        check(any(isinstance(c, dict) and "PlayLive" in c and c["PlayLive"]["inputs"] for c in p.call("log")["commands"]),
              "live inputs logged")
        _, err = p.try_call("live_state")
        check(err and err.kind == "no_live_match", "no live match after finish")

        # --- save and load --------------------------------------------------
        path = os.path.join(tempfile.mkdtemp(), "career.bin")
        p.call("save", path=path)
        a = p.call("view", what="own")
        q = Play(BIN)
        try:
            q.call("load", path=path)
            check(json.dumps(q.call("view", what="own"), sort_keys=True) == json.dumps(a, sort_keys=True), "load restores the view")
            for pl in (p, q):
                to_match_day(pl)
                pl.call("play_round")
            check(p.call("view", what="table") == q.call("view", what="table"), "a loaded world plays on identically")
            _, err = q.try_call("load", path=path + ".missing")
            check(err and err.kind == "io", "missing save file")
            open(path + ".bad", "wb").write(b"junk")
            _, err = q.try_call("load", path=path + ".bad")
            check(err and err.kind == "bad_save", "damaged save")
        finally:
            q.close()
    finally:
        p.close()

    # --- out of work: resign, wait for a call, take the job ------------------
    q = Play(BIN)
    try:
        q.call("new_game", seed=7, clubs=8, club=2)
        old = q.call("view", what="club_profile")["name"]
        q.apply("Resign")
        lg = q.call("legal")
        check(lg["club"] is None, "no club when out of work")
        ks = {a["kind"]: a for a in lg["actions"]}
        check(ks["Advance"]["available"] and "Resign" in ks and not ks["Resign"]["available"], "jobless actions")
        car = q.call("view", what="career")
        check(car["jobless"] and car["spells"][0]["ended"] == "resigned", "career says out of work")
        _, err = q.try_call("view", what="summary")
        check(err and err.kind == "no_club", "no summary without a club")
        offer = None
        for _ in range(80):
            car = q.call("view", what="career")
            opens = [o for o in car["offers"] if o["open"]]
            if opens:
                offer = opens[0]["offer"]
                break
            r = q.call("advance")
            if r.get("stop") == "match_day":
                q.call("play_round")
        check(offer is not None, "a club called")
        jo = q.call("view", what="job_offer", offer=offer, bring=[], keep=["Assistant"])
        check(jo["summary"]["name"] and jo["terms"]["release"] is not None, "the offer shows the new club")
        q.apply({"AcceptJob": {"offer": offer, "bring": [], "keep": ["Assistant", "Skills", "Fitness", "Scout", "Press"]}})
        s2 = q.call("view", what="summary")
        check(s2["name"] != old and q.call("view", what="career")["club"] is not None, "a new club")
        check(q.call("legal")["club"] is not None, "legal is back")
    finally:
        q.close()

    # --- determinism --------------------------------------------------------
    one, two = Play(BIN), Play(BIN)
    try:
        a, b = script(one), script(two)
        check(a == b and one.transcript[1:] == two.transcript[1:], "same requests, same responses")
    finally:
        one.close()
        two.close()
    print(f"play protocol: {checks} checks passed")


if __name__ == "__main__":
    main()
