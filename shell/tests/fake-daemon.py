#!/usr/bin/env python3
"""Stand-in for the nixarchy-meetingai daemon (stdlib only).

usage: fake-daemon.py SOCKET_PATH LOG_PATH [--auto] [--live] [--k8s] [--team] [--interviewer] [--first-run] [--error] [--drop] [--stale] [--call] [--providers] [--locked] [--scale=PERCENT] [--size=WxH]

--k8s is --live with one scene: Priya asks "Explain k8s and how it is working?", a real claude-sub haiku
answer (fixtures/k8s-live-answer.md if present, else k8s-answer.md) streams in, then its real suggestions. --scale sets the text_scale setting.

--team is a team meeting in a series: items (with drafts), one earlier_item event, last_time. It runs on --live's paced stream.

--interviewer is the interviewer-mode protocol: a candidate with documents and 8 questions in mixed statuses,
record.start refused with consent_required until it carries consent:true, then a transcript and one probe (automatic answer).

--providers: the real provider list, providers.status and a models.list per provider (replies carry `provider`; claude-sub answers a second late,
out of order, and opencode fails), for the SCENARIO=providers switch in live-shots.sh.

--call writes a running Zoom call ("Weekly sync", pid of this process) to ../meeting-mode.json beside the socket's directory and
stays idle (not recording), so the bar shows the outlined dot and the "Take notes?" chip. record.start is logged with its title.

--drop cuts the first connection mid-answer. Like the real daemon, the answer goes on for whoever is connected, and a client that
(re)connects gets nothing replayed: it must ask transcript.get and now.get for what it missed.

--live plays a paced meeting instead: recording on, words arriving as
partials, a question from the other side, a streamed answer (with a link), 3 suggestions, levels events, and a
typed question (answer.started with a question and automatic=false). chat.send in any mode answers the same way.

Speaks the newline-delimited JSON protocol: replies to every request with
{"id","ok":true,"data":...}, appends each received cmd to LOG_PATH, and
streams a fake transcript and answer to each client once it asks state.get.
"""
import itertools
import json
import math
import os
import socket
import socketserver
import sys
import threading
import time

sock_path, log_path = sys.argv[1], sys.argv[2]
auto = "--auto" in sys.argv
k8s = "--k8s" in sys.argv
team = "--team" in sys.argv
iv = "--interviewer" in sys.argv
dropped = [False]
first_run = "--first-run" in sys.argv   # no provider, speech not ready: the Setup card
err = "--error" in sys.argv          # a typed question starts, waits, then fails (Writing bar, then Retry)
stale = "--stale" in sys.argv        # turn 2 starts, then the late tail of turn 1 arrives: the client must drop it
drop = "--drop" in sys.argv          # the first connection is cut mid-answer; the client reconnects and asks transcript.get / now.get
providers = "--providers" in sys.argv
call = "--call" in sys.argv
slow = "--slow-settings" in sys.argv   # settings.get answers after 2.5 s and no settings event is pushed: the client must wait for them
locked = "--locked" in sys.argv       # settings fixed by Nix: chat_provider, meeting_mode, text_scale
voice_mode = "--voice" in sys.argv     # transcript.get room row, transcript.relabel, direct room final, and voice.* commands
live = "--live" in sys.argv or k8s or team or iv or err or drop
scale = next((int(a.split("=")[1]) for a in sys.argv if a.startswith("--scale=")), 100)
FIXTURES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures")
state = {"recording": live, "since": int(time.time() * 1000) - 754_000 if live else 0, "mic": True, "system": True}
if first_run:
    state.update(ready={"chat": False, "speech": False}, hints={"chat": "No AI provider is set up. Open Providers to add a key, or use a subscription CLI or Ollama.", "speech": "Speech-to-text is not ready. Check the Parakeet model, or add a Deepgram key in Providers."})
MODELS = {   # models.list per provider; a string is the daemon's error
    "claude-sub": ["sonnet", "opus", "haiku"],
    "agy-sub": ["gemini-3.8-flash-low", "gemini-3.8-pro"],
    "opencode": "OpenCode is not signed in. Run `opencode auth login`.",
    "codex-sub": ["default", "gpt-5.2-codex", "gpt-5.2"],
    "ollama-cloud": ["gpt-oss:120b", "qwen3-coder:480b"],
    "ollama": ["qwen3.8:27b", "llama4:8b"],
}
SPOKEN = []   # the final transcript lines so far, for transcript.get
ANSWER = {"question": "", "body": "", "streaming": False, "suggestions": []}   # the answer in flight, for now.get
NOTICED = []
TURNS = itertools.count(2)
HANDLERS = set()   # connected clients: --drop's answer goes to whoever is connected, as the real daemon's does
VOICE = {"enrolled": False, "model": "", "created": 0}
settings = {"auto_record_on_call": auto, "reduce_motion": False, "chat_provider": "claude-sub", "chat_model": "sonnet", "live_provider": "claude-sub", "live_model": "haiku", "response_pace": "Balanced", "text_scale": scale, "voice_check_paused": False}


if voice_mode:
    state["meeting"] = "m1"


def voice_status():
    return {"enrolled": VOICE["enrolled"], "model": VOICE["model"], "created": VOICE["created"], "paused": bool(settings.get("voice_check_paused", False))}
DATA = {
    # The real daemon's providers (src/ai/catalog/providers.rs).
    "providers.list": [
        {"id": "claude-sub", "name": "Claude (subscription)", "requires_key": False, "has_key": False},
        {"id": "agy-sub", "name": "Antigravity (subscription)", "requires_key": False, "has_key": False},
        {"id": "opencode", "name": "OpenCode", "requires_key": False, "has_key": False},
        {"id": "codex-sub", "name": "ChatGPT / Codex (subscription)", "requires_key": False, "has_key": False},
        {"id": "ollama-cloud", "name": "Ollama Cloud", "requires_key": True, "has_key": False},
        {"id": "ollama", "name": "Ollama (local)", "requires_key": False, "has_key": False},
    ],
    "providers.status": [
        {"id": "claude-sub", "name": "Claude (subscription)", "state": "ready", "login": "claude", "cli": "claude"},
        {"id": "agy-sub", "name": "Antigravity (subscription)", "state": "not_logged_in", "login": "agy", "cli": "agy"},
        {"id": "opencode", "name": "OpenCode", "state": "not_logged_in", "login": "opencode auth login", "cli": "opencode"},
        {"id": "codex-sub", "name": "ChatGPT / Codex (subscription)", "state": "not_installed", "login": "codex login", "cli": "codex"},
        {"id": "ollama-cloud", "name": "Ollama Cloud", "state": "no_key", "login": "set OLLAMA_API_KEY in the service environment, or add the key here", "cli": None},
        {"id": "ollama", "name": "Ollama (local)", "state": "ready", "login": "ollama serve", "cli": None},
    ],
    "session.list": [{"id": 1, "kind": "meeting", "title": "Fake standup"}],
    "settings.search": [
        {"key": "overlay_w", "label": "Overlay width", "value": -1},
        {"key": "auto_record_on_call", "label": "Take notes automatically when a call starts", "value": auto},
        {"key": "reduce_motion", "label": "Reduce motion", "value": False},
    ],
    "series.list": [],
    "candidates.list": [],
    "items.list": [],
    "notes.get": {"text": "## Summary\nfake"},
}
if locked:
    settings["locked"] = ["chat_provider", "meeting_mode", "text_scale"]
settings.update(overlay_x=-1, overlay_y=-1, overlay_w=-1, overlay_h=-1)
size = next((a.split("=")[1] for a in sys.argv if a.startswith("--size=")), "")   # --size=WxH: start with a non-default card
if size:
    settings.update(overlay_w=int(size.split("x")[0]), overlay_h=int(size.split("x")[1]))
# Round 5 protocol: in-memory profiles with documents.
profiles = [
    {"name": "Backend interview", "selected": True, "personal_context": "Senior backend engineer, 8 years of Rust and Kafka.", "meeting_context": "Interview for a staff platform role at Acme; expect system design questions.",
     "documents": [{"id": 1, "name": "cv.pdf", "chars": 5210}, {"id": 2, "name": "job-description.md", "chars": 2380}]},
    {"name": "Weekly standup", "selected": False, "personal_context": "", "meeting_context": "", "documents": []},
]
next_doc = [3]
DEFAULT_PROMPTS = {"answer": "default answer prompt", "live_answer": "default live answer prompt", "follow_up": "default follow-up prompt", "recap": "default recap prompt"}
prompts = {}
PRESETS = [
    {"preset": "openai", "name": "OpenAI", "kind": "chat", "endpoint": "https://api.openai.com/v1/chat/completions", "model": "gpt-4.1-mini", "needs_key": True, "local": False},
    {"preset": "anthropic", "name": "Anthropic", "kind": "chat", "endpoint": "https://api.anthropic.com/v1/messages", "model": "claude-sonnet-5-5", "needs_key": True, "local": False},
    {"preset": "lm-studio", "name": "LM Studio", "kind": "chat", "endpoint": "http://127.0.0.1:1234/v1/chat/completions", "model": "local-model", "needs_key": False, "local": True},
    {"preset": "openai-speech", "name": "OpenAI", "kind": "speech", "endpoint": "https://api.openai.com/v1/audio/transcriptions", "model": "whisper-1", "needs_key": True, "local": False},
    {"preset": "groq-speech", "name": "Groq", "kind": "speech", "endpoint": "https://api.groq.com/openai/v1/audio/transcriptions", "model": "whisper-large-v3-turbo", "needs_key": True, "local": False},
]
CUSTOM_PROVIDERS = []
next_custom = [1]
if "--locked-use" in sys.argv:   # a custom provider is the Nix-fixed chat provider: Use and Remove must be off
    CUSTOM_PROVIDERS.append({"id": "custom-9", "name": "Fixed one", "kind": "chat", "custom": True, "model": "m", "endpoint": "https://example.test/v1", "has_key": True, "requires_key": True})
    settings.update(chat_provider="custom-9", locked=["chat_provider"])


def endpoint_ok(url):
    return url.startswith("https://") or url.startswith(("http://127.0.0.1", "http://localhost", "http://[::1]"))


def find_preset(slug, endpoint=""):
    matches = [p for p in PRESETS if p["preset"] == slug]
    if not matches:
        return None
    return next((p for p in matches if endpoint and p["endpoint"] == endpoint), matches[0])


def providers_cmd(cmd, a):
    """Returns (ok, data_or_error, changed_bool, extra_events) for presets.list and providers.add/update/remove/test."""
    if cmd == "presets.list":
        return True, PRESETS, False, []
    if cmd == "providers.list":
        return True, DATA["providers.list"] + CUSTOM_PROVIDERS, False, []
    if cmd == "providers.test":
        has_p, has_i = bool(a.get("preset")), bool(a.get("id"))
        if has_p == has_i:
            return False, "Specify either preset or id.", False, []
        ep = (a.get("endpoint") or "").strip()
        if ep and not endpoint_ok(ep):
            return True, {"ok": False, "message": "Endpoint must use https://, or http:// on localhost."}, False, []
        if has_i:
            row = next((c for c in CUSTOM_PROVIDERS if c["id"] == a["id"]), None)
            if row is None:
                return False, "Unknown provider.", False, []
            needs_key = row["requires_key"]
            has_key = bool(a.get("api_key")) or row["has_key"]
        else:
            pr = find_preset(a["preset"], ep)
            if pr is None:
                return False, "Unknown preset.", False, []
            needs_key = pr["needs_key"]
            has_key = bool(a.get("api_key"))
        if needs_key and not has_key:
            return True, {"ok": False, "message": "This authentication mode needs an API key."}, False, []
        return True, {"ok": True, "message": "Connection successful."}, False, []
    if cmd == "providers.add":
        ep_in = (a.get("endpoint") or "").strip()
        pr = find_preset(a.get("preset"), ep_in)
        if pr is None:
            return False, "Unknown preset.", False, []
        name = (a.get("name") or pr["name"]).strip() or pr["name"]
        model = (a.get("model") or pr["model"]).strip() or pr["model"]
        endpoint = ep_in or pr["endpoint"]
        if not endpoint_ok(endpoint):
            return False, "Endpoint must use https://, or http:// on localhost.", False, []
        has_key = bool(a.get("api_key"))
        if pr["needs_key"] and not has_key:
            return False, "API key is required.", False, []
        cid = "custom-%d" % next_custom[0]
        next_custom[0] += 1
        row = {"id": cid, "name": name, "kind": pr["kind"], "custom": True, "model": model, "endpoint": endpoint, "has_key": has_key, "requires_key": pr["needs_key"]}
        CUSTOM_PROVIDERS.append(row)
        MODELS[cid] = [model]
        return True, {"id": cid}, True, []
    if cmd == "providers.update":
        row = next((c for c in CUSTOM_PROVIDERS if c["id"] == a.get("id")), None)
        if row is None:
            return False, "Unknown provider.", False, []
        for field, label in (("name", "Enter a provider name."), ("model", "A custom chat provider needs a model."), ("endpoint", "Enter a valid endpoint.")):
            if field in a and not (a[field] or "").strip() and (field != "model" or row["kind"] == "chat"):
                return False, label, False, []
        if "endpoint" in a:
            ep = a["endpoint"].strip()
            if not endpoint_ok(ep):
                return False, "Endpoint must use https://, or http:// on localhost.", False, []
            row["endpoint"] = ep
            row["requires_key"] = not ep.startswith(("http://127.0.0.1", "http://localhost", "http://[::1]"))
        if "name" in a:
            row["name"] = a["name"].strip()
        if "model" in a and a["model"].strip():
            row["model"] = a["model"].strip()
            MODELS[row["id"]] = [row["model"]]
        if "api_key" in a:
            if a["api_key"] is None:
                row["has_key"] = False
            elif isinstance(a["api_key"], str) and a["api_key"] != "":
                row["has_key"] = True
        if row["requires_key"] and not row["has_key"]:
            return False, "This authentication mode needs an API key.", False, []
        return True, {}, True, []
    if cmd == "providers.remove":
        row = next((c for c in CUSTOM_PROVIDERS if c["id"] == a.get("id")), None)
        if row is None:
            return False, "Unknown provider.", False, []
        cid = row["id"]
        lck = settings.get("locked", [])
        for k in ("chat_provider", "live_provider", "speech_provider"):
            if settings.get(k) == cid and k in lck:
                return False, f"Set in Nix: '{k}' is fixed by programs.nixarchy-meetingai.settings.", False, []
        evs = []
        reset = False
        for k, dflt in (("chat_provider", "claude-sub"), ("live_provider", "claude-sub"), ("speech_provider", "parakeet-local")):
            if settings.get(k) == cid:
                settings[k] = dflt
                reset = True
        if reset:
            evs.append({"event": "settings", "settings": settings})
            evs.append({"event": "notice", "kind": "providers", "text": f"Removed {row['name']}; switched back to the default provider."})
        CUSTOM_PROVIDERS.remove(row)
        MODELS.pop(cid, None)
        return True, {}, True, evs
    return False, "bad request", False, []


def profile(name):
    return next((p for p in profiles if p["name"] == name), None)


def profiles_cmd(cmd, a):
    """Returns (ok, data_or_error) for profiles.* and prompts.* commands."""
    if cmd == "profiles.list":
        return True, profiles
    if cmd == "prompts.get":
        return True, {"text": prompts.get((a.get("mode"), a.get("kind")), DEFAULT_PROMPTS.get(a.get("kind"), ""))}
    if cmd == "prompts.set":
        prompts[(a.get("mode"), a.get("kind"))] = a.get("text", "")
        return True, {}
    if cmd == "prompts.reset":
        prompts.pop((a.get("mode"), a.get("kind")), None)
        return True, {}
    p = profile(a.get("name"))
    if cmd == "profiles.save":
        if p is None:
            p = {"name": a["name"], "selected": False, "personal_context": "", "meeting_context": "", "documents": []}
            profiles.append(p)
        for k in ("personal_context", "meeting_context"):
            if k in a:
                p[k] = a[k]
        return True, {}
    if p is None:
        return False, "no such profile"
    if cmd == "profiles.select":
        for q in profiles:
            q["selected"] = q is p
    elif cmd == "profiles.rename":
        p["name"] = a["new_name"]
    elif cmd == "profiles.delete":
        profiles.remove(p)
    elif cmd == "profiles.attach":
        p["documents"].insert(0, {"id": next_doc[0], "name": os.path.basename(a["path"]), "chars": 1000})
        next_doc[0] += 1
    elif cmd == "profiles.detach":
        p["documents"] = [d for d in p["documents"] if d["id"] != a["id"]]
    return True, {}


if live:
    settings["meeting_mode"] = "team_meeting" if team else "interviewer" if iv else "interview"
if iv:
    state.update(recording=False, since=0, candidate="c1", consent=False)
    CANDS = {"c1": {"row": {"id": "c1", "name": "Dana Whitfield", "local_only": False, "meeting_id": None, "created_at": "2026-10-27 09:00:00"},
                    "docs": [{"id": "d1", "kind": "cv", "name": "dana-cv.pdf", "chars": 5210}, {"id": "d2", "kind": "job", "name": "staff-platform.md", "chars": 2380}],
                    "qs": [{"id": "q%d" % n, "text": t, "source": src, "competency": c, "status": st, "note": note, "position": n}
                           for n, (t, src, c, st, note) in enumerate([
                               ("Walk me through the Kafka migration you led.", "Led the migration of 40 services to Kafka", "Technical depth", "covered", "Cut lag from 9 min to 40 s; owned the rollout plan."),
                               ("What was your part in the on-call redesign?", "Redesigned the on-call rotation", "Ownership", "asked", ""),
                               ("How did you decide what to leave out of the first release?", "Scoped the v1 release", "Prioritisation", "asked", "Dropped multi-region; shipped single-region first."),
                               ("Tell me about a disagreement with another team.", "Worked across four product teams", "Collaboration", "skipped", ""),
                               ("How do you measure the reliability of a platform?", "Defined SLOs for the ingestion platform", "Technical depth", "planned", ""),
                               ("What did you mentor others on?", "Mentored three junior engineers", "Leadership", "planned", ""),
                               ("Describe an incident you handled end to end.", "Incident commander for two sev-1 outages", "Ownership", "planned", ""),
                               ("What would you change in this role's first 90 days?", "Own the platform roadmap", "Role fit", "planned", ""),
                           ])]}}
    DATA["candidates.list"] = [CANDS["c1"]["row"]]
    DATA["items.list"] = []
if team:
    state["series"] = "s1"
    state["meeting"] = "m2"
    DATA["series.list"] = [{"id": "s1", "name": "Weekly platform sync", "created_at": "2026-10-01 09:00:00"}]
    ITEMS = [
        {"id": "i1", "meeting_id": "m2", "series_id": "s1", "kind": "action", "text": "Pad the migration by two days", "owner": "Sam", "due": "Tuesday", "status": "open", "source_line": 3, "t0": 125.0, "origin_item": "e1", "origin_text": "Write the migration runbook", "draft": 1, "created_at": "2026-10-27 10:02:00"},
        {"id": "i2", "meeting_id": "m2", "series_id": "s1", "kind": "decision", "text": "Ship next Tuesday if QA signs off", "owner": None, "due": None, "status": "open", "source_line": 2, "t0": 70.0, "origin_item": None, "draft": 1, "created_at": "2026-10-27 10:03:00"},
        {"id": "i3", "meeting_id": "m2", "series_id": "s1", "kind": "question", "text": "Who owns the lag alert?", "owner": None, "due": None, "status": "open", "source_line": 5, "t0": 190.5, "origin_item": None, "draft": 0, "created_at": "2026-10-27 10:04:00"},
        {"id": "i4", "meeting_id": "m2", "series_id": "s1", "kind": "action", "text": "Share the QA checklist", "owner": "Alex", "due": None, "status": "done", "source_line": 4, "t0": 160.0, "origin_item": None, "draft": 0, "created_at": "2026-10-27 10:05:00"},
    ]
    DATA["items.list"] = ITEMS
    DATA["last_time.get"] = {"meeting": "m1", "title": "Platform sync, 20 Oct", "started_at": "2026-10-20 10:00:00", "recap": "## Recap\nAgreed to **review the Kafka migration** this week.", "items": [{"id": "e1", "kind": "action", "text": "Write the migration runbook", "status": "open"}]}
LOCK = threading.Lock()
PROVIDER = {"provider": "claude-sub", "model": "haiku"}


def iv_cmd(cmd, a):
    """Returns (ok, data_or_error, candidate_to_announce) for candidate.*, candidates.* and questions.*."""
    c = CANDS.get(a.get("id") or a.get("candidate"))
    q = next((q for k in CANDS.values() for q in k["qs"] if q["id"] == a.get("id")), None)
    if cmd == "candidate.create":
        cid = "c%d" % (len(CANDS) + 1)
        CANDS[cid] = {"row": {"id": cid, "name": a["name"], "local_only": False, "meeting_id": None, "created_at": "2026-10-27 11:00:00"}, "docs": [], "qs": []}
        DATA["candidates.list"] = [k["row"] for k in CANDS.values()]
        return True, CANDS[cid]["row"], None
    if cmd == "candidates.list":
        return True, DATA["candidates.list"], None
    if c is None and cmd.startswith("candidate"):
        return False, "no such candidate", None
    if cmd == "candidate.get":
        return True, {"candidate": c["row"], "documents": c["docs"], "questions": sorted(c["qs"], key=lambda x: x["position"])}, None
    if cmd == "candidate.attach":
        d = {"id": "d%d" % next_doc[0], "kind": a["kind"], "name": os.path.basename(a["path"]), "chars": 1000}
        next_doc[0] += 1
        c["docs"] = [x for x in c["docs"] if x["kind"] != a["kind"]] + [d]
        return True, d, None
    if cmd == "candidate.local_only":
        c["row"]["local_only"] = a["on"]
        return True, c["row"], None
    if cmd == "candidates.delete":
        del CANDS[a["id"]]
        DATA["candidates.list"] = [k["row"] for k in CANDS.values()]
        return True, {}, None
    if cmd == "questions.generate":
        return True, {"questions": c["qs"], "dropped": 2}, c["row"]["id"]
    if cmd == "questions.add":
        q = {"id": "q%d" % next_doc[0], "text": a["text"], "source": a.get("source", ""), "competency": a.get("competency"), "status": "planned", "note": "", "position": len(c["qs"])}
        next_doc[0] += 1
        c["qs"].append(q)
        return True, q, c["row"]["id"]
    owner = next((k for k in CANDS.values() if q in k["qs"]), None)
    if cmd == "questions.update" and q:
        q.update({k: v for k, v in a.items() if k != "id"})
        return True, q, owner["row"]["id"]
    if cmd == "questions.delete" and q:
        owner["qs"].remove(q)
        return True, {}, owner["row"]["id"]
    if cmd == "questions.reorder":
        for n, i in enumerate(a["ids"]):
            next(x for x in c["qs"] if x["id"] == i)["position"] = n
        return True, {}, c["row"]["id"]
    return False, "bad request", None


def log(line):
    with LOCK, open(log_path, "a") as f:
        f.write(line + "\n")


def cast(obj):
    for h in list(HANDLERS):
        h.send(obj)


class Handler(socketserver.StreamRequestHandler):
    def setup(self):
        super().setup()
        HANDLERS.add(self)

    def finish(self):
        HANDLERS.discard(self)
        super().finish()

    def send(self, obj):
        try:
            self.wfile.write((json.dumps(obj) + "\n").encode())
            self.wfile.flush()
        except OSError:
            pass

    def stream_stale(self):
        time.sleep(0.3)
        self.send({"event": "answer.started", "turn": 1, "question": "old?", "automatic": False, **PROVIDER})
        self.send({"event": "answer.started", "turn": 2, "question": "new?", "automatic": False, **PROVIDER})
        self.send({"event": "answer.delta", "turn": 2, "text": "NEW", **PROVIDER})
        self.send({"event": "answer.delta", "turn": 1, "text": "OLD", **PROVIDER})
        self.send({"event": "answer.error", "turn": 1, "message": "late failure", **PROVIDER})
        self.send({"event": "answer.done", "turn": 1, **PROVIDER})

    def stream(self):
        time.sleep(0.3)
        if not slow:
            self.send({"event": "settings", "settings": settings})
        self.send({"event": "session.reset"})
        self.send({"event": "history", "rows": [{"who": "question", "text": "old q"}, {"who": "answer", "text": "old a"}]})
        self.send({"event": "transcript.final", "who": "system", "speaker": "Alex", "text": "What is the plan?", "t0": 0, "t1": 2})
        self.send({"event": "transcript.partial", "who": "mic", "speaker": "", "text": "We should", "t0": 2, "t1": 3})
        for who, speaker, text, t0, t1 in [
            ("mic", "", "I think we can ship this on Friday if QA signs off, but the migration needs another look.", 3, 6),
            ("system", "Sam", "Agreed. Last time the migration took two days longer than planned, so let us pad it.", 6, 9),
            ("mic", "", "Fine, let us say next Tuesday.", 9, 11),
            ("mic", "", "Coffee is ready in the kitchen.", 11, 13),
        ]:
            self.send({"event": "transcript.final", "who": who, "speaker": speaker, "text": text, "t0": t0, "t1": t1})
        self.send({"event": "transcript.relabel", "meeting": state.get("meeting", ""), "t0": 11, "t1": 13, "who": "room"})
        self.send({"event": "notice", "text": "An agent started recording"})
        if providers and not NOTICED:   # the daemon tells its first client, once
            NOTICED.append(1)
            self.send({"event": "notice", "kind": "providers", "text": "Removed the stored Gemini API key; Gemini now runs through Antigravity (sign in with `agy`)."})
        self.send({"event": "answer.started", "turn": 1, "question": "What is the plan?", "automatic": True, **PROVIDER})
        for word in ["Ship ", "it ", "**next Tuesday**.\n\n", "- QA sign-off first\n", "- Pad the migration by two days\n"]:
            self.send({"event": "answer.delta", "turn": 1, "text": word, **PROVIDER})
        self.send({"event": "answer.done", "turn": 1, **PROVIDER})
        self.send({"event": "suggestion", "text": "Who owns it?"})

    def say(self, who, speaker, text, t0, pace=0.12):
        words = text.split()
        for n in range(1, len(words)):
            self.send({"event": "transcript.partial", "who": who, "speaker": speaker, "text": " ".join(words[:n]), "t0": t0, "t1": t0 + n * 0.4})
            time.sleep(pace)
        self.send({"event": "transcript.final", "who": who, "speaker": speaker, "text": text, "t0": t0, "t1": t0 + len(words) * 0.4})
        SPOKEN.append({"line": len(SPOKEN) + 1, "who": who, "speaker": speaker, "text": text, "t0": t0, "t1": t0 + len(words) * 0.4})

    def levels(self):
        n = 0
        while state["recording"]:
            n += 1
            self.send({"event": "levels", "mic": abs(math.sin(n * 0.7)) * 0.8, "system": abs(math.sin(n * 0.3 + 1)) * 0.6})
            time.sleep(0.2)

    def stream_live(self):
        time.sleep(0.3)
        self.send({"event": "settings", "settings": settings})
        self.send({"event": "state", **state})
        threading.Thread(target=self.levels, daemon=True).start()
        if k8s:
            return self.stream_k8s()
        if team:
            return self.stream_team()
        if iv:
            return   # the call starts with record.start
        self.say("system", "Priya", "Thanks for joining. Can you walk us through how you would design the ingestion pipeline?", 740, 0.05)
        self.say("mic", "", "Sure. I would start with an event queue in front of the workers so producers never block.", 748, 0.05)
        self.say("system", "Priya", "Okay. And how would you handle a consumer that falls behind for an hour?", 756)
        time.sleep(0.6)  # Balanced pacing: answer after the question settles
        emit = cast if drop else self.send
        ANSWER.update(question="And how would you handle a consumer that falls behind for an hour?", body="", streaming=True)
        emit({"event": "answer.started", "turn": 1, "question": ANSWER["question"], "automatic": True, **PROVIDER})
        answer = ["**Talking points**\n\n", "- Lag is visible: alert on ", "consumer lag, not CPU.\n", "- Scale out consumers ", "per partition; ", "the queue keeps ", "ordering per key.\n", "- If it is a poison message, ", "park it in a ", "dead-letter queue and move on.\n\n", "*Mention:* you did this at your last job ", "with Kafka lag alerts. See the ", "[Kafka docs](https://kafka.apache.org/documentation/)."]
        for n, chunk in enumerate(answer):
            ANSWER["body"] += chunk
            emit({"event": "answer.delta", "turn": 1, "text": chunk, **PROVIDER})
            time.sleep(0.25)
            if drop and not dropped[0] and n == 3:   # mid-answer: cut this connection; the answer goes on
                dropped[0] = True
                self.request.shutdown(socket.SHUT_RDWR)
        ANSWER["streaming"] = False
        emit({"event": "answer.done", "turn": 1, **PROVIDER})
        ANSWER["suggestions"] = ["Give a concrete lag number", "Ask how they monitor today", "Explain back-pressure"]
        for s in ANSWER["suggestions"]:
            emit({"event": "suggestion", "text": s})
        if drop:
            return
        self.say("mic", "", "Good question. First I would make the lag visible", 766, 0.3)
        time.sleep(1.2)
        self.typed("What should I say?")

    def stream_resume(self):
        """After a dropped connection: like the real daemon, send nothing but the settings and state. The client asks transcript.get and now.get itself."""
        time.sleep(0.3)
        self.send({"event": "settings", "settings": settings})
        self.send({"event": "state", **state})

    def stream_voice(self):
        time.sleep(0.3)
        self.send({"event": "settings", "settings": settings})
        self.send({"event": "state", **state})
        self.send({"event": "transcript.final", "who": "mic", "speaker": "", "text": "My own first line", "t0": 5.5, "t1": 8.0})
        self.send({"event": "transcript.final", "who": "mic", "speaker": "", "text": "Someone else in the room", "t0": 5.5, "t1": 8.0})
        self.send({"event": "transcript.relabel", "meeting": "m1", "t0": 5.5, "t1": 8.0, "who": "room"})
        self.send({"event": "transcript.final", "who": "room", "speaker": "", "text": "Direct room line", "t0": 8.5, "t1": 10.0})

    def stream_iv_call(self):
        """After record.start: the candidate answers, then one automatic probe grounded in a quote."""
        self.send({"event": "state", **state})
        self.say("system", "Dana", "I led the Kafka migration, forty services over six months, and cut consumer lag from nine minutes to forty seconds.", 12, 0.02)
        self.say("mic", "", "What was your part in the cutover itself?", 20, 0.02)
        time.sleep(0.4)
        self.send({"event": "answer.started", "turn": 1, "question": "", "automatic": True, **PROVIDER})
        for chunk in ["**Probes**\n\n", "- \u201ccut consumer lag from nine minutes to forty seconds\u201d: what did you change to get that?\n", "- \u201cforty services over six months\u201d: how did you order the rollout?\n"]:
            self.send({"event": "answer.delta", "turn": 1, "text": chunk, **PROVIDER})
            time.sleep(0.1)
        self.send({"event": "answer.done", "turn": 1, **PROVIDER})

    def stream_team(self):
        self.send({"event": "items.updated", "meeting": "m2"})
        self.say("system", "Priya", "Did anyone finish the migration runbook from last week?", 740, 0.05)
        self.say("mic", "", "Almost, I will send it today.", 748, 0.05)
        time.sleep(0.5)
        self.send({"event": "earlier_item", "item": {"id": "e1", "kind": "action", "text": "Write the migration runbook", "created_at": "2026-10-20 10:00:00", "status": "open"}, "quote": "I will send it today.", "line": 2, "meeting": "m2", "new_item": "i1"})

    def stream_k8s(self):
        question = "Explain k8s and how it is working?"
        self.say("system", "Priya", question, 740, 0.12)
        time.sleep(0.6)
        provider = {"provider": "claude-sub", "model": "haiku"}
        self.send({"event": "answer.started", "turn": 1, "question": question, "automatic": True, **provider})
        answer = open(next(f for f in (os.path.join(FIXTURES, n) for n in ("k8s-live-answer.md", "k8s-answer.md")) if os.path.exists(f))).read()
        for i in range(0, len(answer), 24):   # about 8 s end to end, like a streaming model
            self.send({"event": "answer.delta", "turn": 1, "text": answer[i:i + 24], **provider})
            time.sleep(0.08)
        self.send({"event": "answer.done", "turn": 1, **provider})
        for text in json.load(open(os.path.join(FIXTURES, "k8s-suggestions.json"))):
            self.send({"event": "suggestion", "text": text})

    def typed(self, question):
        """A typed question: started carries it, then deltas, done, and at most 3 follow-ups."""
        time.sleep(0.2)
        typed_provider = {"provider": "claude-sub", "model": "sonnet"}
        turn = next(TURNS)   # like the daemon: every question is a new turn
        self.send({"event": "answer.started", "turn": turn, "question": question, "automatic": False, **typed_provider})
        if err:
            time.sleep(4)
            return self.send({"event": "answer.error", "turn": turn, "message": "claude: the provider did not answer within 30 s", **typed_provider})
        for chunk in ["Say the number first: ", "**under a minute of lag** ", "is fine, past ten minutes ", "you page someone."]:
            self.send({"event": "answer.delta", "turn": turn, "text": chunk, **typed_provider})
            time.sleep(0.25)
        self.send({"event": "answer.done", "turn": turn, **typed_provider})
        for s in ["Who owns the alert?", "What is the retry policy?", "How do you replay?"]:
            self.send({"event": "suggestion", "text": s})

    def handle(self):
        try:
            self.serve()
        except OSError:   # the client went away
            pass

    def serve(self):
        for raw in self.rfile:
            req = json.loads(raw)
            cmd = req["cmd"]
            log(cmd + (" " + str(req.get("args", {}).get("key")) if cmd == "settings.set" else "") + (" title=" + str(req.get("args", {}).get("title", "")) + (" new" if req.get("args", {}).get("new") else "") if cmd == "record.start" else ""))
            if cmd == "record.start" and iv and not (state["consent"] or req.get("args", {}).get("consent")):
                self.send({"id": req["id"], "ok": False, "error": "consent_required: Tell the candidate this interview is being recorded and transcribed."})
                continue
            if cmd == "record.start":
                state.update(recording=True, since=int(time.time() * 1000))
                if call:
                    cast({"event": "state", **state})
                if iv:
                    state["consent"] = True
                    threading.Thread(target=self.stream_iv_call, daemon=True).start()
            elif cmd == "record.stop":
                state["recording"] = False
            elif cmd in ("screenshot.capture", "capture.clear", "chat.send"):   # a screenshot waits for the next question
                state["capture"] = cmd == "screenshot.capture"
                self.send({"event": "state", **state})
            data = state if cmd == "state.get" else settings if cmd == "settings.get" else DATA.get(cmd, {})
            if slow and cmd == "settings.get":
                threading.Timer(2.5, self.send, args=({"id": req["id"], "ok": True, "data": settings},)).start()
                continue
            if cmd == "models.list":   # the reply carries the provider it answers, so a client can drop a stale one
                p = req.get("args", {}).get("provider")
                models = MODELS.get(p, "unknown provider")
                reply = {"id": req["id"], "provider": p, **({"ok": True, "data": models} if isinstance(models, list) else {"ok": False, "error": models})}
                if providers and p == "claude-sub":   # slow and out of order: a client that already moved on must drop it
                    threading.Timer(1.0, self.send, args=(reply,)).start()
                else:
                    self.send(reply)
                continue
            if drop and cmd == "transcript.get":
                data = {"meeting": "m1", "lines": [l for l in SPOKEN if l["line"] > req.get("args", {}).get("after_line", 0)]}
            elif voice_mode and cmd == "transcript.get":
                data = {"meeting": "m1", "lines": [{"line": 1, "who": "room", "speaker": "", "text": "Earlier room remark", "t0": 1.0, "t1": 2.5}] if req.get("args", {}).get("after_line", 0) == 0 else []}
            elif drop and cmd == "now.get":
                data = {"status": "writing" if ANSWER["streaming"] else "ready" if ANSWER["body"] else "idle", "question": ANSWER["question"], "answer": ANSWER["body"], "provider": PROVIDER["provider"], "model": PROVIDER["model"], "suggestions": ANSWER["suggestions"]}
            if cmd == "voice.status":
                self.send({"id": req["id"], "ok": True, "data": voice_status()})
                continue
            if cmd == "voice.enroll":
                if state["recording"]:
                    self.send({"id": req["id"], "ok": False, "error": "Stop recording before enrolling your voice."})
                else:
                    VOICE.update(enrolled=True, model="3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx", created=1761700000000)
                    threading.Timer(0.3, self.send, args=({"id": req["id"], "ok": True, "data": voice_status()},)).start()
                continue
            if cmd == "voice.delete":
                VOICE.update(enrolled=False, model="", created=0)
                self.send({"id": req["id"], "ok": True, "data": {}})
                continue
            if iv and cmd.startswith(("candidate", "questions.")):
                ok, data, announce = iv_cmd(cmd, req.get("args", {}))
                self.send({"id": req["id"], "ok": ok, **({"data": data} if ok else {"error": data})})
                if announce:
                    self.send({"event": "questions.updated", "candidate": announce})
                continue
            if iv and cmd == "session.new":
                state["candidate"] = req.get("args", {}).get("candidate")
                self.send({"id": req["id"], "ok": True, "data": {}})
                self.send({"event": "state", **state})
                continue
            if cmd.startswith(("profiles.", "prompts.")):
                ok, data = profiles_cmd(cmd, req.get("args", {}))
                self.send({"id": req["id"], "ok": ok, **({"data": data} if ok else {"error": data})})
                continue
            if cmd in ("presets.list", "providers.list", "providers.add", "providers.update", "providers.remove", "providers.test"):
                ok, data, changed, evs = providers_cmd(cmd, req.get("args", {}))
                self.send({"id": req["id"], "ok": ok, **({"data": data} if ok else {"error": data})})
                for ev in evs:
                    cast(ev)
                if changed:
                    cast({"event": "providers.changed"})
                continue
            if cmd == "settings.set":
                k, val = req["args"]["key"], req["args"].get("value")
                if k in ("chat_provider", "live_provider", "speech_provider") and isinstance(val, str) and val.startswith("custom-"):
                    want_kind = "speech" if k == "speech_provider" else "chat"
                    if not any(c["id"] == val and c["kind"] == want_kind for c in CUSTOM_PROVIDERS):
                        self.send({"id": req["id"], "ok": False, "error": "This provider no longer exists."})
                        continue
            self.send({"id": req["id"], "ok": True, "data": data})
            if cmd == "settings.set":
                settings[req["args"]["key"]] = req["args"]["value"]
                self.send({"event": "settings", "settings": settings})
            elif cmd == "session.restore":
                self.send({"event": "session.reset"})
                self.send({"event": "history", "rows": [{"who": "question", "text": "old q"}, {"who": "answer", "text": "old a"}]})
            if cmd == "chat.send":
                threading.Thread(target=self.typed, args=(req["args"].get("text", ""),), daemon=True).start()
            if cmd == "state.get":
                threading.Thread(target=self.stream_stale if stale else self.stream_voice if voice_mode else self.stream_resume if drop and dropped[0] else self.stream_live if live else self.stream, daemon=True).start()


class Server(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True


os.makedirs(os.path.dirname(sock_path), mode=0o700, exist_ok=True)
if call:
    now_ms = int(time.time() * 1000)
    with open(os.path.join(os.path.dirname(os.path.dirname(sock_path)), "meeting-mode.json"), "w") as f:
        json.dump({"active": True, "reason": "Zoom", "since": now_ms - 90_000, "updated": now_ms, "pid": os.getpid(), "call": "call", "callApp": "Zoom",
                   "callTitle": "Weekly sync", "callSince": now_ms - 90_000, "callDetected": True}, f)
if os.path.exists(sock_path):
    os.unlink(sock_path)
Server(sock_path, Handler).serve_forever()
