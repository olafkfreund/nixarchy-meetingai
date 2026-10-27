#!/usr/bin/env python3
"""Asks the REAL daemon (Interview mode, claude-sub haiku) "Explain k8s and how it is working?"
and saves the streamed answer and the suggestions as fixtures for fake-daemon.py --k8s.
usage: capture-k8s.py SOCKET OUT_DIR   (run a daemon on a throwaway XDG_RUNTIME_DIR/XDG_DATA_HOME first; keep a real `claude`
binary on its PATH, because the mise wrapper hangs when XDG_DATA_HOME is overridden)"""
import json, socket, sys

s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
f = s.makefile("rw", buffering=1)
out = sys.argv[2]
n = 0


def call(cmd, args):
    global n
    n += 1
    f.write(json.dumps({"id": n, "cmd": cmd, "args": args}) + "\n")
    for line in f:
        m = json.loads(line)
        if m.get("id") == n:
            return m


for k, v in [("meeting_mode", "interview"), ("chat_provider", "claude-sub"), ("chat_model", "haiku")]:
    assert call("settings.set", {"key": k, "value": v})["ok"]
assert call("chat.send", {"text": "Explain k8s and how it is working?"})["ok"]
answer, sugg, started = "", [], None
s.settimeout(120)
for line in f:
    m = json.loads(line)
    ev = m.get("event")
    if ev == "answer.started":
        started = m
    elif ev == "answer.delta":
        answer += m["text"]
    elif ev == "answer.error":
        sys.exit("answer.error: %s" % m)
    elif ev == "suggestion":
        sugg.append(m["text"])
        if len(sugg) == 3:
            break
    elif ev == "answer.done":
        s.settimeout(45)   # follow-ups come after done; fewer than 3 is fine
open(out + "/k8s-answer.md", "w").write(answer)
json.dump(sugg, open(out + "/k8s-suggestions.json", "w"), indent=2)
print("started:", started)
print("answer chars:", len(answer), "suggestions:", sugg)
