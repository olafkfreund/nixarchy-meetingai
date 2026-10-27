#!/usr/bin/env python3
"""Ask the real daemon one question per provider/model; print first-token and total time.

Usage: provider-probe.py SOCKET [--models] PROVIDER=MODEL ...
  SOCKET   the daemon socket of a THROWAWAY daemon (XDG_RUNTIME_DIR/nixarchy-meetingai/daemon.sock)
  --models print models.list for each PROVIDER instead of asking a question
  example: provider-probe.py $RUN/nixarchy-meetingai/daemon.sock codex-sub= claude-sub=haiku ollama=qwen2.5:7b
"""
import json, socket, sys, time
sock_path, cases = sys.argv[1], [a for a in sys.argv[2:] if a != "--models"]
models_only = "--models" in sys.argv[2:]
def conn():
    global s, f
    s = socket.socket(socket.AF_UNIX); s.connect(sock_path); f = s.makefile("rw")
conn()
seq = 0
def req(cmd, args):
    global seq; seq += 1; f.write(json.dumps({"id": seq, "cmd": cmd, "args": args}) + "\n"); f.flush()
    while True:
        m = json.loads(f.readline())
        if m.get("id") == seq: return m
for case in cases:
    conn()
    prov, model = case.split("=", 1)
    if models_only:
        r = req("models.list", {"provider": prov}); print(f"{prov}: {json.dumps(r.get('result', r))[:600]}"); continue
    for k, v in (("chat_provider", prov), ("chat_model", model)):
        r = req("settings.set", {"key": k, "value": v})
        if not r.get("ok"): print(f"{case}: settings.set {k} failed: {r.get('error')}"); break
    req("session.new", {})
    t0 = time.time(); r = req("chat.send", {"text": "In one sentence: what is Kubernetes?"})
    if not r.get("ok"): print(f"{case}: chat.send failed: {r.get('error')}"); continue
    first = None; body = ""; used = ""
    s.settimeout(150)
    try:
        while True:
            m = json.loads(f.readline()); ev = m.get("event")
            if ev == "answer.delta":
                first = first or time.time() - t0; body += m.get("text", ""); used = f"{m.get('provider','')}/{m.get('model','')}"
            elif ev == "answer.done":
                used = f"{m.get('provider', '')}/{m.get('model', '')}" or used; break
            elif ev == "answer.error":
                body = "ERROR: " + str(m.get("message")); break
    except socket.timeout:
        body = "TIMEOUT"
    total = time.time() - t0
    print(f"{case:42s} first={first and round(first,1)}s total={round(total,1)}s used={used} :: {body.strip()[:90]!r}")
