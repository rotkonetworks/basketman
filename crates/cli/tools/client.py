"""A tiny client for `basketman play` (line-delimited JSON over stdio)."""
import json
import os
import subprocess


def default_binary():
    here = os.path.dirname(os.path.abspath(__file__))
    for p in (
        os.environ.get("BASKETMAN"),
        "/steam/rotko/basketman/target/agentws/release/basketman",
        os.path.join(here, "../../../target/release/basketman"),
    ):
        if p and os.path.exists(p):
            return p
    raise SystemExit("set BASKETMAN to the basketman binary")


class ApiError(Exception):
    def __init__(self, error):
        super().__init__(f"{error.get('kind')}: {error.get('message')}")
        self.kind = error.get("kind")
        self.message = error.get("message")


class Play:
    def __init__(self, binary=None):
        self.proc = subprocess.Popen(
            [binary or default_binary(), "play"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self.next_id = 1
        self.transcript = []

    def raw(self, line):
        self.proc.stdin.write(line + "\n")
        self.proc.stdin.flush()
        out = self.proc.stdout.readline()
        if not out:
            raise RuntimeError("server closed: " + self.proc.stderr.read())
        self.transcript.append(out)
        return json.loads(out)

    def request(self, op, **args):
        """The whole response object."""
        req = dict(args, op=op, id=self.next_id)
        self.next_id += 1
        resp = self.raw(json.dumps(req))
        assert resp["id"] == req["id"], resp
        return resp

    def call(self, op, **args):
        """The result, or ApiError."""
        resp = self.request(op, **args)
        if not resp["ok"]:
            raise ApiError(resp["error"])
        return resp["result"]

    def try_call(self, op, **args):
        try:
            return self.call(op, **args), None
        except ApiError as e:
            return None, e

    def apply(self, command):
        return self.call("apply", command=command)

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.wait(timeout=10)
        except Exception:
            self.proc.kill()
