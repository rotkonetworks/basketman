#!/usr/bin/env python3
"""A rule-based reference manager for `basketman play`.

It runs one club through whole seasons, draft included: answers every
question that waits for it, keeps a fit rotation, picks a tactic from its
squad, leaves standing match instructions, and keeps moving the calendar.

    python3 bot.py [--seed 7] [--clubs 12] [--club 0] [--seasons 1]

Prints the final table position of each season. Exits non-zero if the game
ever stops progressing.
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import ApiError, Play  # noqa: E402

MIN_ROSTER = 9
STEP_LIMIT = 4000


def mid(r):
    return (r["lo"] + r["hi"]) / 2


class Manager:
    def __init__(self, play, log=print):
        self.p = play
        self.log = log
        self.rules_set = False
        self.refused = 0

    # ---- helpers -----------------------------------------------------------
    def summary(self):
        return self.p.call("view", what="summary")

    def squad(self):
        return self.p.call("view", what="squad")["first_team"]

    def try_apply(self, command):
        try:
            return self.p.apply(command)
        except ApiError as e:
            self.refused += 1
            self.log(f"  refused {list(command)[0] if isinstance(command, dict) else command}: {e}")
            return None

    # ---- the decisions -----------------------------------------------------
    def set_tactics(self, fresh=False):
        """A system that suits the squad, with a fit rotation."""
        own = self.p.call("view", what="own")
        squad = [p for p in own["squad"] if not p["injury"]]
        top = sorted(squad, key=lambda p: -p["overall"]["lo"])[:8]

        def avg(name):
            return sum(p["attrs"][name]["lo"] for p in top) / len(top)

        sug = self.p.call("suggest_tactics")
        slot = own["playbook"][[t["active"] for t in own["playbook"]].index(True)]["slot"]
        current = own["playbook"][slot]["tactics"]
        t = dict(current, rotation=sug["rotation"])
        if fresh:
            # Keep the system the squad already knows (changing offense,
            # defense or press costs familiarity, and in test runs the
            # bot did worse when it did); tune only the shot mix.
            lean = (avg("Three-point") - avg("Close shot")) / 2
            t["three_bias"] = int(max(6, min(15, round(11 + lean))))
        if t != current:
            self.try_apply({"SetTactics": {"slot": slot, "tactics": t}})

    def leave_rules(self):
        rules = {
            "rules": [
                {
                    "name": "stop the run",
                    "when": [{"OpponentRunAtLeast": 8}],
                    "then": ["Timeout", {"Shout": {"shout": "Focus", "target": None}}],
                    "max_fires": 3,
                    "cooldown_secs": 90,
                },
                {
                    "name": "rest the tired",
                    "when": [{"PlayerEnergyBelow": {"player": None, "percent": 35}}],
                    "then": [{"Swap": {"off": "Triggering", "on": "BestRested"}}],
                    "max_fires": 6,
                    "cooldown_secs": 120,
                },
                {
                    "name": "foul trouble",
                    "when": [
                        {"PlayerFoulsAtLeast": {"player": None, "fouls": 4}},
                        {"Period": {"from": 1, "to": 3}},
                    ],
                    "then": [{"Swap": {"off": "Triggering", "on": "BestRested"}}],
                    "max_fires": 3,
                    "cooldown_secs": 120,
                },
                {
                    "name": "protect a big lead",
                    "when": [{"MarginAtLeast": 15}, {"Period": {"from": 4, "to": 9}}],
                    "then": [{"SetOrders": {"pace": 7}}],
                    "max_fires": 1,
                },
                {
                    "name": "chase",
                    "when": [{"MarginAtMost": -10}, {"Period": {"from": 4, "to": 9}}],
                    "then": [{"SetOrders": {"pace": 16, "press": "ThreeQuarter"}}],
                    "max_fires": 1,
                },
            ]
        }
        self.try_apply({"SetInstructions": rules})
        self.rules_set = True

    def answer_offer(self, offer):
        """Sell only a man outside our best seven, for at least his worth."""
        squad = self.squad()
        ranked = sorted(squad, key=lambda p: -p["overall"]["lo"])
        keep = {p["id"] for p in ranked[:7]}
        sell = offer["player"] not in keep and len(squad) > MIN_ROSTER + 1
        self.try_apply({"AnswerOffer": {"offer": offer["index"], "accept": bool(sell)}})

    def answer_mail(self, pending):
        kind = pending["kind"]
        if kind == "offer":
            inbox = self.p.call("view", what="inbox")
            for m in inbox["messages"]:
                if m["offer"] and m["offer"]["index"] == pending["offer"]:
                    self.answer_offer(m["offer"])
                    return
            self.try_apply({"AnswerOffer": {"offer": pending["offer"], "accept": False}})
        elif kind == "press":
            self.try_apply({"AnswerPress": {"question": pending["question"], "choice": 1}})
        elif kind == "sponsor_offer":
            legal = self.p.call("legal")
            spec = next(a for a in legal["actions"] if a["kind"] == "AnswerSponsor")
            offers = spec["params"]["offer"]["choices"]
            best = max(offers, key=lambda o: o["fee_per_season"])
            self.try_apply({"AnswerSponsor": {"offer": best["offer"], "accept": True}})
            for o in offers:
                if o["offer"] != best["offer"]:
                    self.try_apply({"AnswerSponsor": {"offer": o["offer"], "accept": False}})

    def draft(self):
        d = self.p.call("view", what="draft")
        if not d or not d["ours_on_clock"]:
            return
        best = max(
            d["prospects"],
            key=lambda x: mid(x["overall"]) * 0.45 + mid(x["potential"]) * 0.55,
        )
        self.try_apply({"DraftPick": best["id"]})

    def fill_roster(self):
        squad = self.squad()
        if len(squad) >= MIN_ROSTER + 1:
            return
        market = self.p.call("view", what="market")
        pool = sorted(market["free_agents"], key=lambda x: -mid(x["overall"]))
        for cand in pool[: MIN_ROSTER + 1 - len(squad)]:
            self.try_apply({"Bid": {"player": cand["id"], "fee": 0}})

    # ---- the loop ----------------------------------------------------------
    def run(self, seasons=1):
        start = self.summary()["calendar"]["season"]
        self.set_tactics(fresh=True)
        self.leave_rules()
        results = []
        seen_history = 0
        steps = 0
        idle, last = 0, None
        while len(results) < seasons:
            steps += 1
            if steps > STEP_LIMIT:
                raise RuntimeError("stuck: step limit reached")
            s = self.summary()
            w = s["waiting"]["kind"]
            if w == "draft_pick":
                self.draft()
            elif w in ("offer", "press", "sponsor_offer"):
                self.answer_mail(s["waiting"])
            elif w == "match_day":
                inj = [p for p in self.squad() if p["injury"] and p.get("rotation")]
                if inj:
                    self.set_tactics()
                self.p.call("play_round", instructions=True)
            else:
                self.fill_roster()
                r = self.p.call("advance")
                if r.get("stop") == "draft":
                    self.log(f"  the draft is on (day {r['day']})")
            fingerprint = (s["calendar"]["day"], s["calendar"]["round"], s["calendar"]["season"], w, s["unread"])
            idle = idle + 1 if fingerprint == last else 0
            last = fingerprint
            if idle > 6:
                raise RuntimeError(f"stuck at {fingerprint}")
            # A season has finished when history grew.
            h = self.p.call("view", what="league")["history"]
            if len(h) > seen_history:
                seen_history = len(h)
                row = h[-1]
                results.append(row)
                self.log(
                    f"season {row['season']}: finished {row['our_place']} in the table, champion club {row['champion']}"
                )
                self.set_tactics()
        return results


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--clubs", type=int, default=12)
    ap.add_argument("--club", type=int, default=0)
    ap.add_argument("--seasons", type=int, default=1)
    ap.add_argument("--binary")
    a = ap.parse_args()
    play = Play(a.binary)
    try:
        summary = play.call("new_game", seed=a.seed, clubs=a.clubs, club=a.club)
        print(f"managing {summary['name']} (club {a.club}), board expects place {summary['board_expects_place']}")
        bot = Manager(play)
        results = bot.run(a.seasons)
        for r in results:
            print(f"RESULT seed={a.seed} season={r['season']} place={r['our_place']} of {a.clubs}")
        print(f"commands refused: {bot.refused}")
        log = play.call("log")
        print(f"command log length: {log['length']}")
    finally:
        play.close()


if __name__ == "__main__":
    main()
