#!/usr/bin/env python3
"""Stand-in for the nixarchy-meetingai daemon (stdlib only).

usage: fake-daemon.py SOCKET_PATH LOG_PATH [--auto]

Speaks the newline-delimited JSON protocol: replies to every request with
{"id","ok":true,"data":...}, appends each received cmd to LOG_PATH, and
streams a fake transcript and answer to each client once it asks state.get.
"""
import json
import os
import socketserver
import sys
import threading
import time

sock_path, log_path = sys.argv[1], sys.argv[2]
auto = "--auto" in sys.argv
state = {"recording": False, "since": 0, "mic": True, "system": True}
settings = {"auto_record_on_call": auto, "reduce_motion": False, "chat_provider": "ollama", "response_pace": "Balanced"}
DATA = {
    "providers.list": [{"id": "ollama", "name": "Ollama", "requires_key": False, "has_key": False}],
    "models.list": ["llama3", "qwen3"],
    "session.list": [{"id": 1, "kind": "meeting", "title": "Fake standup"}],
    "profiles.list": [],
    "settings.search": [{"key": "auto_record_on_call", "label": "Auto record on call", "value": auto}],
    "prompts.get": {"text": "fake prompt"},
    "notes.get": {"text": "## Summary\nfake"},
}
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
        self.send({"event": "transcript.final", "who": "system", "speaker": "Alex", "text": "What is the plan?", "t0": 0, "t1": 2})
        self.send({"event": "transcript.partial", "who": "mic", "speaker": "", "text": "We should", "t0": 2, "t1": 3})
        for i, word in enumerate(["Ship ", "it ", "**Friday**."]):
            self.send({"event": "answer.delta", "turn": 1, "text": word})
        self.send({"event": "answer.done", "turn": 1})
        self.send({"event": "suggestion", "text": "Who owns it?"})

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
            if cmd == "state.get":
                threading.Thread(target=self.stream, daemon=True).start()


class Server(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True


os.makedirs(os.path.dirname(sock_path), mode=0o700, exist_ok=True)
if os.path.exists(sock_path):
    os.unlink(sock_path)
Server(sock_path, Handler).serve_forever()
