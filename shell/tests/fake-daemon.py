#!/usr/bin/env python3
"""Stand-in for the nixarchy-meetingai daemon (stdlib only).

usage: fake-daemon.py SOCKET_PATH LOG_PATH [--auto] [--live]

--live plays a paced meeting instead: recording on, words arriving as
partials, a question from the other side, a streamed answer (with a link), suggestions, and levels events.

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
live = "--live" in sys.argv
state = {"recording": live, "since": int(time.time() * 1000) - 754_000 if live else 0, "mic": True, "system": True}
settings = {"auto_record_on_call": auto, "reduce_motion": False, "chat_provider": "ollama", "chat_model": "llama3", "response_pace": "Balanced"}
DATA = {
    "providers.list": [{"id": "ollama", "name": "Ollama", "requires_key": False, "has_key": False}],
    "models.list": ["llama3", "qwen3"],
    "session.list": [{"id": 1, "kind": "meeting", "title": "Fake standup"}],
    "profiles.list": [],
    "settings.search": [
        {"key": "auto_record_on_call", "label": "Take notes automatically when a call starts", "value": auto},
        {"key": "reduce_motion", "label": "Reduce motion", "value": False},
        {"key": "chat_model", "label": "Chat model", "value": "llama3"},
    ],
    "prompts.get": {"text": "fake prompt"},
    "notes.get": {"text": "## Summary\nfake"},
}
if live:
    settings["meeting_mode"] = "interview"
LOCK = threading.Lock()


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
        for word in ["Ship ", "it ", "**next Tuesday**.\n\n", "- QA sign-off first\n", "- Pad the migration by two days\n"]:
            self.send({"event": "answer.delta", "turn": 1, "text": word})
        self.send({"event": "answer.done", "turn": 1})
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
        self.say("system", "Priya", "Thanks for joining. Can you walk us through how you would design the ingestion pipeline?", 740, 0.05)
        self.say("mic", "", "Sure. I would start with an event queue in front of the workers so producers never block.", 748, 0.05)
        self.say("system", "Priya", "Okay. And how would you handle a consumer that falls behind for an hour?", 756)
        time.sleep(0.6)  # Balanced pacing: answer after the question settles
        answer = ["**Talking points**\n\n", "- Lag is visible: alert on ", "consumer lag, not CPU.\n", "- Scale out consumers ", "per partition; ", "the queue keeps ", "ordering per key.\n", "- If it is a poison message, ", "park it in a ", "dead-letter queue and move on.\n\n", "*Mention:* you did this at your last job ", "with Kafka lag alerts. See the ", "[Kafka docs](https://kafka.apache.org/documentation/)."]
        for chunk in answer:
            self.send({"event": "answer.delta", "turn": 1, "text": chunk})
            time.sleep(0.25)
        self.send({"event": "answer.done", "turn": 1})
        for s in ["Give a concrete lag number", "Ask how they monitor today", "Explain back-pressure"]:
            self.send({"event": "suggestion", "text": s})
        self.say("mic", "", "Good question. First I would make the lag visible", 766, 0.3)

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
            if cmd == "state.get":
                threading.Thread(target=self.stream_live if live else self.stream, daemon=True).start()


class Server(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True


os.makedirs(os.path.dirname(sock_path), mode=0o700, exist_ok=True)
if os.path.exists(sock_path):
    os.unlink(sock_path)
Server(sock_path, Handler).serve_forever()
