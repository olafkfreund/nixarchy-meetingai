#!/usr/bin/env python3
"""Stand-in for the nixarchy-meetingai daemon (stdlib only).

usage: fake-daemon.py SOCKET_PATH LOG_PATH [--auto] [--live] [--k8s] [--scale=PERCENT]

--k8s is --live with one scene: Priya asks "Explain k8s and how it is working?", a real claude-sub haiku
answer (fixtures/k8s-answer.md) streams in, then its real suggestions. --scale sets the text_scale setting.

--live plays a paced meeting instead: recording on, words arriving as
partials, a question from the other side, a streamed answer (with a link), 3 suggestions, levels events, and a
typed question (answer.started with a question and automatic=false). chat.send in any mode answers the same way.

Speaks the newline-delimited JSON protocol: replies to every request with
{"id","ok":true,"data":...}, appends each received cmd to LOG_PATH, and
streams a fake transcript and answer to each client once it asks state.get.
"""
import json
import math
import os
import socketserver
import sys
import threading
import time

sock_path, log_path = sys.argv[1], sys.argv[2]
auto = "--auto" in sys.argv
k8s = "--k8s" in sys.argv
live = "--live" in sys.argv or k8s
scale = next((int(a.split("=")[1]) for a in sys.argv if a.startswith("--scale=")), 100)
FIXTURES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures")
state = {"recording": live, "since": int(time.time() * 1000) - 754_000 if live else 0, "mic": True, "system": True}
settings = {"auto_record_on_call": auto, "reduce_motion": False, "chat_provider": "claude-sub", "chat_model": "sonnet", "live_provider": "claude-sub", "live_model": "haiku", "response_pace": "Balanced", "text_scale": scale}
DATA = {
    # Mirrors the real daemon's defaults: frontier first, local last.
    "providers.list": [
        {"id": "claude-sub", "name": "Claude (subscription)", "requires_key": False, "has_key": False},
        {"id": "codex-sub", "name": "ChatGPT / Codex (subscription)", "requires_key": False, "has_key": False},
        {"id": "gemini", "name": "Google Gemini", "requires_key": True, "has_key": True},
        {"id": "ollama", "name": "Ollama (local)", "requires_key": False, "has_key": False},
    ],
    "models.list": ["sonnet", "opus", "haiku"],
    "session.list": [{"id": 1, "kind": "meeting", "title": "Fake standup"}],
    "profiles.list": [],
    "settings.search": [
        {"key": "auto_record_on_call", "label": "Take notes automatically when a call starts", "value": auto},
        {"key": "reduce_motion", "label": "Reduce motion", "value": False},
    ],
    "prompts.get": {"text": "fake prompt"},
    "notes.get": {"text": "## Summary\nfake"},
}
if live:
    settings["meeting_mode"] = "interview"
LOCK = threading.Lock()
PROVIDER = {"provider": "claude-sub", "model": "haiku"}


def log(line):
    with LOCK, open(log_path, "a") as f:
        f.write(line + "\n")


class Handler(socketserver.StreamRequestHandler):
    def send(self, obj):
        try:
            self.wfile.write((json.dumps(obj) + "\n").encode())
            self.wfile.flush()
        except OSError:
            pass

    def stream(self):
        time.sleep(0.3)
        self.send({"event": "settings", "settings": settings})
        self.send({"event": "session.reset"})
        self.send({"event": "history", "rows": [{"who": "question", "text": "old q"}, {"who": "answer", "text": "old a"}]})
        self.send({"event": "transcript.final", "who": "system", "speaker": "Alex", "text": "What is the plan?", "t0": 0, "t1": 2})
        self.send({"event": "transcript.partial", "who": "mic", "speaker": "", "text": "We should", "t0": 2, "t1": 3})
        for who, speaker, text in [
            ("mic", "", "I think we can ship this on Friday if QA signs off, but the migration needs another look."),
            ("system", "Sam", "Agreed. Last time the migration took two days longer than planned, so let us pad it."),
            ("mic", "", "Fine, let us say next Tuesday."),
        ]:
            self.send({"event": "transcript.final", "who": who, "speaker": speaker, "text": text, "t0": 3, "t1": 6})
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
        self.say("system", "Priya", "Thanks for joining. Can you walk us through how you would design the ingestion pipeline?", 740, 0.05)
        self.say("mic", "", "Sure. I would start with an event queue in front of the workers so producers never block.", 748, 0.05)
        self.say("system", "Priya", "Okay. And how would you handle a consumer that falls behind for an hour?", 756)
        time.sleep(0.6)  # Balanced pacing: answer after the question settles
        self.send({"event": "answer.started", "turn": 1, "question": "And how would you handle a consumer that falls behind for an hour?", "automatic": True, **PROVIDER})
        answer = ["**Talking points**\n\n", "- Lag is visible: alert on ", "consumer lag, not CPU.\n", "- Scale out consumers ", "per partition; ", "the queue keeps ", "ordering per key.\n", "- If it is a poison message, ", "park it in a ", "dead-letter queue and move on.\n\n", "*Mention:* you did this at your last job ", "with Kafka lag alerts. See the ", "[Kafka docs](https://kafka.apache.org/documentation/)."]
        for chunk in answer:
            self.send({"event": "answer.delta", "turn": 1, "text": chunk, **PROVIDER})
            time.sleep(0.25)
        self.send({"event": "answer.done", "turn": 1, **PROVIDER})
        for s in ["Give a concrete lag number", "Ask how they monitor today", "Explain back-pressure"]:
            self.send({"event": "suggestion", "text": s})
        self.say("mic", "", "Good question. First I would make the lag visible", 766, 0.3)
        time.sleep(1.2)
        self.typed("What should I say?")

    def stream_k8s(self):
        question = "Explain k8s and how it is working?"
        self.say("system", "Priya", question, 740, 0.12)
        time.sleep(0.6)
        provider = {"provider": "claude-sub", "model": "haiku"}
        self.send({"event": "answer.started", "turn": 1, "question": question, "automatic": True, **provider})
        answer = open(os.path.join(FIXTURES, "k8s-answer.md")).read()
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
        self.send({"event": "answer.started", "turn": 2, "question": question, "automatic": False, **typed_provider})
        for chunk in ["Say the number first: ", "**under a minute of lag** ", "is fine, past ten minutes ", "you page someone."]:
            self.send({"event": "answer.delta", "turn": 2, "text": chunk, **typed_provider})
            time.sleep(0.25)
        self.send({"event": "answer.done", "turn": 2, **typed_provider})
        for s in ["Who owns the alert?", "What is the retry policy?", "How do you replay?"]:
            self.send({"event": "suggestion", "text": s})

    def handle(self):
        for raw in self.rfile:
            req = json.loads(raw)
            cmd = req["cmd"]
            log(cmd)
            if cmd == "record.start":
                state.update(recording=True, since=int(time.time() * 1000))
            elif cmd == "record.stop":
                state["recording"] = False
            data = state if cmd == "state.get" else settings if cmd == "settings.get" else DATA.get(cmd, {})
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
                threading.Thread(target=self.stream_live if live else self.stream, daemon=True).start()


class Server(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True


os.makedirs(os.path.dirname(sock_path), mode=0o700, exist_ok=True)
if os.path.exists(sock_path):
    os.unlink(sock_path)
Server(sock_path, Handler).serve_forever()
