#!/usr/bin/env python3
"""Drives the real daemon over its socket and checks every reply and event has
the shape shell/*.qml reads. usage: real-driver.py SOCKET [ollama_up 0|1]"""
import json, socket, sys, time

path, ollama = sys.argv[1], sys.argv[2] == "1"
s = socket.socket(socket.AF_UNIX)
s.connect(path)
f = s.makefile("rw", buffering=1)
events, bad, n = [], [], 0


def check(cond, what):
    if not cond:
        bad.append(what)
        print("BAD:", what)


def read_until(pred, timeout):
    """Read lines (events are stored) until pred(msg) or timeout; return the match."""
    s.settimeout(timeout)
    try:
        for line in f:
            m = json.loads(line)
            if "event" in m:
                events.append(m)
                check(isinstance(m["event"], str), f"event name: {m}")
                if m["event"] == "answer.delta":
                    check(isinstance(m.get("turn"), int) and isinstance(m.get("text"), str), f"answer.delta shape: {m}")
                if m["event"] == "answer.error":
                    check(isinstance(m.get("message"), str), f"answer.error shape: {m}")
            else:
                check(isinstance(m.get("ok"), bool) and "id" in m, f"reply shape: {m}")
                check(m.get("ok") is False and isinstance(m.get("error"), str) or m.get("ok") is True and "data" in m, f"reply data/error: {m}")
            if pred(m):
                return m
    except (TimeoutError, socket.timeout):
        pass
    return None


def call(cmd, args=None, timeout=15):
    global n
    n += 1
    my = n
    f.write(json.dumps({"id": my, "cmd": cmd, "args": args or {}}) + "\n")
    r = read_until(lambda m: m.get("id") == my and "event" not in m, timeout)
    check(r is not None, f"{cmd}: no reply")
    print(cmd, "->", (json.dumps(r)[:140] if r else None))
    return r or {"ok": False, "error": "timeout"}


def lst(cmd, args, keys):
    r = call(cmd, args)
    check(r["ok"] and isinstance(r["data"], list), f"{cmd} must return a list")
    for row in r.get("data") or []:
        check(isinstance(row, dict) and all(k in row for k in keys), f"{cmd} row {row}")
    return r.get("data") or []


r = call("state.get")
check(r["ok"] and isinstance(r["data"].get("recording"), bool) and isinstance(r["data"].get("mic"), bool), "state.get shape")
r = call("settings.get")
check(r["ok"] and isinstance(r["data"], dict) and "reduce_motion" not in r["data"] or r["ok"], "settings.get")
lst("settings.search", {"query": "model"}, ["key", "label", "value"])
lst("providers.list", {}, ["id", "name", "requires_key", "has_key"])
models = call("models.list", {"provider": "ollama"})
if ollama:
    check(models["ok"] and models["data"] and all(isinstance(m, str) for m in models["data"]), "models.list ollama")
check(call("session.new")["ok"], "session.new")
lst("session.list", {}, ["id", "kind", "title"])
lst("profiles.list", {}, ["id", "name"])
check(call("prompts.get", {"mode": "general", "kind": "answer"})["ok"], "prompts.get")

model = (models.get("data") or [""])[0] if ollama else "none"
call("settings.set", {"key": "chat_provider", "value": "ollama"})
call("settings.set", {"key": "chat_model", "value": sys.argv[3] if len(sys.argv) > 3 else model})
r = call("chat.send", {"text": "Reply with the single word: pong", "no_context": True})
check(r["ok"], "chat.send accepted")
end = read_until(lambda m: m.get("event") in ("answer.done", "answer.error"), 180)
check(end is not None, "chat.send: no answer.done/answer.error")
if end and ollama:
    check(end["event"] == "answer.done" and any(e["event"] == "answer.delta" for e in events), f"ollama is up but chat ended with {end}")
print("chat ended:", end)

r = call("record.start")
check(r["ok"] or (isinstance(r.get("error"), str) and r["error"]), "record.start must be ok or a clean error")
if r["ok"]:
    call("record.stop")
check(call("no.such.cmd")["ok"] is False, "unknown cmd must be a clean error")
sys.exit(1 if bad else 0)
