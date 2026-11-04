#!/usr/bin/env python3
"""Score the daemon's live-paced transcription (or dictation) of a WAV against a reference file.

Usage:
  python3 -I score.py FILE.wav REFERENCE.txt (--engine parakeet|whisper-nb|turbo | --parakeet | --url URL | --fake TEXT)
                      [--model-dir DIR] [--parakeet-dir DIR] [--vad PATH] [--speaker-model PATH]
                      [--load-mode resident|unload] [--dictation] [--replay-log FILE]
                      [--model M] [--language L] [--glossary G]
  python3 -I score.py SET_DIR (--engine parakeet|whisper-nb|turbo | ...)

Reference (see ref.example.txt): `me: words` / `them: words` per utterance (prefix case-insensitive),
`[silence]` (ignored) or `[silence mm:ss-mm:ss]` (output lines inside the span are invented).
"""
from __future__ import annotations

import json
import math
import re
import statistics
import subprocess
import sys
import wave
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

SHORT_WORDS = {"ja", "nei", "takk", "greit"}

PHANTOMS = (
    "takk for at du så på",
    "teksting av ai-media",
    "undertekster av ai-media",
    "thank you for watching",
    "thanks for watching",
    "subtitles by the amara",
)

# Common English words for code-switch detection when a reference is not explicitly tagged.
CS_ENGLISH = {
    "account", "action", "agreement", "ai", "alliance", "analysis", "app", "application",
    "approach", "assessment", "authority", "background", "balance", "battery", "benchmark",
    "better", "black", "blue", "board", "border", "bottom", "brain", "brand", "break",
    "brief", "briefing", "broadband", "budget", "build", "building", "business", "call",
    "campaign", "capacity", "capital", "carbon", "care", "career", "case", "cash", "center",
    "centre", "ceo", "chain", "challenge", "change", "channel", "chat", "check", "chief",
    "child", "children", "choice", "city", "claim", "class", "clean", "clear", "climate",
    "close", "cloud", "club", "cluster", "co2", "coal", "coast", "code", "collaboration",
    "college", "committee", "common", "communication", "community", "company", "competition",
    "compliance", "computer", "concept", "concern", "conference", "conflict", "connect",
    "connection", "consensus", "content", "context", "contract", "control", "cooperation",
    "core", "corporate", "cost", "council", "country", "county", "couple", "course", "court",
    "cover", "coverage", "covid", "create", "credit", "crime", "crisis", "critical", "cross",
    "culture", "current", "customer", "cyber", "daily", "damage", "data", "database", "date",
    "deadline", "debate", "debt", "decade", "decision", "deep", "default", "defence",
    "defense", "degree", "delay", "delivery", "demand", "democracy", "democratic",
    "department", "design", "desktop", "detail", "development", "device", "digital",
    "direct", "director", "discussion", "disease", "display", "distance", "distribution",
    "district", "diversity", "doctor", "document", "dollar", "domain", "domestic", "double",
    "down", "draft", "drive", "driver", "drop", "duty", "early", "earth", "east", "easy",
    "economic", "economy", "edge", "education", "effect", "effective", "efficiency",
    "effort", "election", "electric", "electricity", "electronic", "element", "email",
    "emergency", "emission", "emissions", "employee", "employment", "energy", "engine",
    "engineering", "english", "enough", "enterprise", "environment", "environmental",
    "equal", "equipment", "equity", "error", "establish", "estate", "estimate", "europe",
    "european", "evaluation", "event", "evidence", "example", "exchange", "executive",
    "exercise", "existing", "expansion", "experience", "expert", "export", "extra",
    "extreme", "eye", "face", "facility", "fact", "factor", "factory", "failure", "fair",
    "family", "farm", "father", "feature", "federal", "fee", "feed", "feedback", "feel",
    "feeling", "field", "fight", "figure", "file", "final", "finance", "financial", "find",
    "fire", "firm", "first", "fit", "five", "fix", "fixed", "flag", "flat", "fleet",
    "flight", "floor", "flow", "focus", "follow", "food", "foot", "football", "force",
    "foreign", "forest", "format", "former", "forward", "foundation", "four", "frame",
    "framework", "free", "freedom", "french", "frequency", "fresh", "friend", "front",
    "fuel", "function", "fund", "funding", "future", "gain", "game", "gap", "gas", "gate",
    "general", "generation", "german", "germany", "giant", "gift", "girl", "give", "global",
    "goal", "gold", "good", "goods", "governance", "government", "grade", "grand", "grant",
    "great", "green", "grid", "ground", "group", "growth", "guarantee", "guard", "guide",
    "guideline", "guidelines", "half", "hall", "handle", "hard", "hardware", "head",
    "health", "healthcare", "heart", "heat", "heavy", "help", "high", "higher", "highway",
    "hill", "history", "hold", "holiday", "home", "hope", "hospital", "host", "hot",
    "hour", "house", "housing", "hub", "human", "hundred", "hydrogen", "idea", "identity",
    "image", "impact", "implementation", "import", "important", "improvement", "incentive",
    "incident", "income", "increase", "independent", "index", "individual", "industrial",
    "industry", "inflation", "info", "information", "infrastructure", "initial",
    "initiative", "innovation", "input", "inside", "insight", "instance", "institute",
    "institution", "insurance", "integration", "intelligence", "interest", "interface",
    "internal", "international", "internet", "interview", "investment", "investor",
    "island", "issue", "item", "job", "joint", "journal", "judge", "justice", "keep",
    "key", "kid", "kind", "king", "knowledge", "lab", "labor", "labour", "lack", "lake",
    "language", "laptop", "large", "late", "later", "launch", "law", "lawyer", "layer",
    "lead", "leadership", "learning", "least", "leave", "left", "legal", "legislation",
    "length", "less", "lesson", "letter", "level", "liberal", "library", "license", "life",
    "light", "limit", "limited", "line", "link", "list", "little", "live", "living", "load",
    "loan", "local", "location", "lock", "log", "logic", "long", "look", "loop", "loss",
    "lot", "love", "low", "lower", "machine", "magazine", "mail", "main", "maintenance",
    "major", "majority", "make", "maker", "making", "management", "manager",
    "manufacturing", "map", "margin", "mark", "market", "marketing", "marriage", "mass",
    "master", "match", "material", "matter", "maximum", "mayor", "meaning", "measure",
    "measurement", "media", "medical", "medicine", "medium", "meet", "meeting", "member",
    "membership", "memory", "mental", "message", "metal", "method", "middle", "might",
    "military", "mind", "mine", "minimum", "mining", "minister", "ministry", "minor",
    "minority", "minute", "mission", "mistake", "mix", "mobile", "mode", "modern",
    "moment", "money", "monitor", "monitoring", "month", "moral", "morning", "mother",
    "motion", "motor", "mountain", "mouse", "move", "movement", "movie", "multiple",
    "museum", "music", "name", "nation", "national", "native", "natural", "navy", "near",
    "necessary", "neck", "need", "negative", "network", "neutral", "never", "new", "news",
    "newspaper", "next", "nice", "night", "nine", "nobody", "node", "noise", "none",
    "north", "northern", "norway", "norwegian", "note", "nothing", "notice", "novel",
    "nuclear", "number", "nurse", "object", "objective", "obligation", "observation",
    "ocean", "off", "offer", "office", "officer", "official", "offshore", "oil", "okay",
    "old", "once", "one", "online", "only", "open", "operating", "operation",
    "operational", "operator", "opinion", "opportunity", "opposition", "option", "order",
    "ordinary", "organization", "original", "other", "otherwise", "outcome", "output",
    "outside", "overall", "owner", "ownership", "package", "page", "pain", "paint",
    "pair", "panel", "paper", "parent", "parliament", "participant", "particular",
    "partner", "partnership", "party", "passage", "passenger", "past", "path", "patient",
    "pattern", "pay", "payment", "peace", "peak", "penalty", "pension", "people",
    "percent", "percentage", "performance", "period", "permanent", "permission", "permit",
    "personal", "perspective", "phase", "phone", "photo", "physical", "pick", "picture",
    "piece", "pilot", "pipeline", "place", "planet", "planning", "plant", "plastic",
    "platform", "play", "player", "please", "plus", "pocket", "podcast", "point",
    "police", "policy", "political", "politics", "pollution", "pool", "poor", "popular",
    "population", "port", "portfolio", "portion", "position", "positive", "possibility",
    "possible", "potential", "pound", "poverty", "power", "practical", "practice",
    "president", "press", "pressure", "previous", "price", "primary", "prime",
    "principal", "principle", "print", "prior", "priority", "prison", "privacy", "prize",
    "procedure", "process", "processing", "produce", "producer", "product", "production",
    "professional", "professor", "profile", "profit", "program", "programme", "progress",
    "project", "promise", "promotion", "proof", "proper", "property", "proposal",
    "protection", "protocol", "public", "publication", "publish", "purchase", "purpose",
    "push", "quality", "quarter", "queen", "question", "quick", "quiet", "quite", "race",
    "radio", "rail", "railway", "rain", "raise", "range", "rank", "rapid", "rare", "rate",
    "rating", "ratio", "raw", "reach", "reaction", "read", "reader", "reading", "ready",
    "real", "reality", "reason", "reasonable", "receive", "recent", "recognition",
    "recommendation", "record", "recording", "recovery", "reduce", "reduction",
    "reference", "reform", "refugee", "regard", "region", "regional", "register",
    "regular", "regulation", "regulatory", "relation", "relationship", "relative",
    "release", "relevant", "relief", "religion", "religious", "remain", "remote",
    "renewable", "rent", "repair", "repeat", "replace", "replacement", "reply", "report",
    "reporting", "representative", "reputation", "request", "requirement", "rescue",
    "research", "reserve", "resident", "resolution", "resource", "respect", "response",
    "responsibility", "responsible", "rest", "restaurant", "result", "retail", "return",
    "revenue", "review", "revolution", "rich", "ride", "right", "rise", "risk", "river",
    "road", "roadmap", "rock", "role", "roll", "roof", "root", "round", "route",
    "routine", "row", "royal", "rule", "run", "running", "rural", "safe", "safety",
    "sale", "sales", "sample", "sanction", "sanctions", "satellite", "save", "saving",
    "savings", "scale", "scenario", "scene", "schedule", "scheme", "school", "science",
    "scientific", "scientist", "scope", "score", "screen", "script", "sea", "search",
    "season", "seat", "second", "secondary", "secret", "secretary", "section", "sector",
    "secure", "security", "see", "seed", "seek", "segment", "select", "selection",
    "self", "sell", "senate", "senior", "sense", "sensitive", "sentence", "separate",
    "sequence", "series", "serious", "server", "service", "session", "setting",
    "settlement", "seven", "several", "severe", "shadow", "shake", "share",
    "shareholder", "sharp", "sheet", "shell", "shelter", "shift", "shipping", "shock",
    "shoe", "shoot", "shop", "shopping", "short", "shot", "shoulder", "show", "shut",
    "sick", "sight", "sign", "signal", "significant", "silence", "silver", "similar",
    "simple", "simulation", "since", "single", "sister", "site", "situation", "six",
    "size", "skill", "skin", "sleep", "slice", "slide", "slight", "slow", "small",
    "smart", "smile", "smoke", "smooth", "snow", "social", "society", "soft", "software",
    "soil", "solar", "soldier", "solid", "solution", "solve", "somebody", "someone",
    "something", "sometimes", "somewhat", "somewhere", "song", "soon", "sorry", "soul",
    "sound", "source", "south", "southern", "space", "speak", "speaker", "special",
    "specialist", "species", "specific", "speech", "speed", "spend", "spending",
    "spirit", "split", "spokesman", "sport", "spot", "spread", "spring", "square",
    "stability", "stable", "staff", "stage", "stake", "stakeholder", "stand", "star",
    "startup", "state", "statement", "station", "statistics", "stay", "steady", "steal",
    "steel", "step", "stick", "still", "stock", "stomach", "stone", "storage", "store",
    "story", "straight", "strange", "strategic", "strategy", "stream", "street",
    "strength", "stress", "stretch", "strike", "string", "strip", "stroke", "strong",
    "structure", "struggle", "student", "studio", "study", "stuff", "style", "subject",
    "submit", "subsequent", "subsidy", "substance", "substantial", "success",
    "successful", "such", "sudden", "suffer", "sufficient", "sugar", "suggest",
    "suggestion", "suit", "summer", "summit", "super", "supply", "support", "supporter",
    "suppose", "supreme", "sure", "surface", "surgery", "surprise", "surround", "survey",
    "survival", "survive", "suspect", "sustainable", "sweet", "swim", "swing", "switch",
    "symbol", "symptom", "system", "table", "tackle", "tail", "take", "tale", "talent",
    "talk", "tall", "tank", "tape", "target", "task", "taste", "tax", "taxpayer", "tea",
    "teach", "teacher", "teaching", "team", "tear", "tech", "technical", "technique",
    "technology", "teen", "teenager", "telephone", "television", "tell", "temperature",
    "temporary", "tend", "tendency", "tennis", "tension", "term", "terms", "terrible",
    "territory", "terror", "terrorism", "terrorist", "testing", "text", "thank",
    "thanks", "theater", "theatre", "theme", "themselves", "theory", "therapy", "there",
    "therefore", "they", "thick", "thin", "thing", "think", "thinking", "third",
    "thirty", "this", "those", "though", "thought", "thousand", "threat", "threaten",
    "three", "throat", "through", "throughout", "throw", "thus", "ticket", "tie",
    "tight", "timeline", "tiny", "tire", "tired", "tissue", "title", "tobacco", "today",
    "toe", "together", "tomorrow", "tongue", "tonight", "too", "tool", "tooth", "top",
    "topic", "total", "totally", "touch", "tough", "tour", "tourist", "tournament",
    "toward", "towards", "tower", "town", "toy", "trace", "track", "trade", "trading",
    "tradition", "traditional", "traffic", "tragedy", "trail", "train", "trainer",
    "training", "transaction", "transfer", "transformation", "transition", "translate",
    "translation", "transmission", "transport", "transportation", "trap", "trash",
    "travel", "treat", "treatment", "treaty", "tree", "tremendous", "trend", "trial",
    "tribe", "trick", "trip", "troop", "trouble", "truck", "true", "truly", "trust",
    "truth", "try", "tube", "tunnel", "turn", "tv", "twelve", "twenty", "twice", "twin",
    "two", "typical", "typically", "ugly", "ukraine", "ukrainian", "ultimate",
    "ultimately", "unable", "uncle", "undergo", "understand", "understanding",
    "unfortunately", "uniform", "union", "unique", "unit", "united", "universal",
    "universe", "university", "unknown", "unless", "unlike", "unlikely", "until",
    "unusual", "update", "upon", "upper", "urban", "urge", "usa", "use", "used",
    "useful", "user", "usual", "usually", "utility", "vacation", "vaccine", "valley",
    "valuable", "value", "variable", "variation", "variety", "various", "vary", "vast",
    "vegetable", "vehicle", "venture", "version", "versus", "very", "vessel", "veteran",
    "via", "victim", "victory", "video", "view", "viewer", "village", "violate",
    "violation", "violence", "violent", "virtual", "virus", "visible", "vision", "visit",
    "visitor", "visual", "vital", "voice", "volume", "volunteer", "vote", "voter",
    "vulnerable", "wage", "wait", "wake", "walk", "wall", "want", "war", "warm", "warn",
    "warning", "wash", "waste", "watch", "water", "wave", "way", "weak", "wealth",
    "wealthy", "weapon", "wear", "weather", "web", "website", "wedding", "week",
    "weekend", "weekly", "weigh", "weight", "welcome", "welfare", "well", "west",
    "western", "wet", "what", "whatever", "wheel", "when", "whenever", "where",
    "whereas", "whether", "which", "while", "whisper", "white", "who", "whole", "whom",
    "whose", "why", "wide", "widely", "widespread", "wife", "wild", "will", "willing",
    "win", "window", "wine", "wing", "winner", "wire", "wisdom", "wise", "wish", "with",
    "withdraw", "within", "without", "witness", "woman", "wonder", "wonderful", "wood",
    "wooden", "word", "work", "worker", "working", "workflow", "workforce", "workshop",
    "world", "worldwide", "worry", "worse", "worth", "would", "wound", "wrap", "write",
    "writer", "writing", "wrong", "yard", "yeah", "year", "yellow", "yes", "yesterday",
    "yet", "yield", "you", "young", "your", "yours", "yourself", "youth", "zero", "zone",
}


def words(text: str) -> list[str]:
    """NPSC-style normalisation: lowercase, strip <...> tags, split to alphanumeric words."""
    clean = re.sub(r"<[^>]*>", " ", text.lower().replace("’", "'"))
    return re.findall(r"[^\W_]+(?:'[^\W_]+)?", clean)


def edit_distance(ref: list[str], hyp: list[str]) -> int:
    """Sequence Levenshtein edit distance (substitutions + deletions + insertions)."""
    n, m = len(ref), len(hyp)
    if n == 0:
        return m
    if m == 0:
        return n
    prev = list(range(m + 1))
    curr = [0] * (m + 1)
    for i in range(1, n + 1):
        curr[0] = i
        rw = ref[i - 1]
        for j in range(1, m + 1):
            cost = 0 if rw == hyp[j - 1] else 1
            curr[j] = min(
                prev[j] + 1,       # deletion
                curr[j - 1] + 1,   # insertion
                prev[j - 1] + cost # substitution / match
            )
        prev, curr = curr, prev
    return prev[m]


def percentile(sorted_vals: list[float], q: float) -> float:
    if not sorted_vals:
        return 0.0
    idx = min(len(sorted_vals) - 1, max(0, math.ceil(q * len(sorted_vals)) - 1))
    return sorted_vals[idx]


def parse_reference(text: str):
    me, them, silence = [], [], []
    turns = []
    tags = set()
    for n, line in enumerate(text.splitlines(), 1):
        line = line.strip()
        if not line:
            continue
        if line.startswith("#"):
            if "[codeswitch]" in line.lower():
                tags.add("codeswitch")
            if "[short_words]" in line.lower():
                tags.add("short_words")
            continue
        if m := re.fullmatch(r"\[silence(?:\s+(\d+):(\d\d)-(\d+):(\d\d))?\]", line, re.I):
            if m[1]:
                silence.append((int(m[1]) * 60 + int(m[2]), int(m[3]) * 60 + int(m[4])))
            else:
                tags.add("all_silence")
            continue
        who, sep, rest = line.partition(":")
        who = who.strip().lower()
        if not sep or who not in ("me", "them"):
            raise ValueError(f"reference line {n}: expected 'me:' or 'them:', got {line[:30]!r}")
        w = words(rest)
        (me if who == "me" else them).extend(w)
        turns.append((who, w))
    return me, them, silence, turns, tags


def wav_duration(path: str) -> float:
    try:
        with wave.open(path, "rb") as w:
            return w.getnframes() / max(1, w.getframerate())
    except Exception:
        return 0.0


def score_ext(records: list[dict], me: list[str], them: list[str], silence: list[tuple[float, float]], turns: list[tuple[str, list[str]]] | None = None, tags: set[str] | None = None, audio_duration: float = 0.0):
    turns = turns or []
    tags = tags or set()
    kept = [o for o in records if o.get("kind") == "line"]
    summary = next((o for o in reversed(records) if o.get("kind") == "summary"), {})
    duration = float(summary.get("duration") or audio_duration or 0.0)

    me_only, them_only = set(me) - set(them), set(them) - set(me)
    mic = sorted((o for o in kept if o["channel"] == 0), key=lambda o: o["start"])
    sys_lines = sorted((o for o in kept if o["channel"] == 1), key=lambda o: o["start"])

    remote = lambda o: sum(w in them_only for w in words(o["text"]))
    owner = lambda o: sum(w in me_only for w in words(o["text"]))
    bleed = [o for o in mic if remote(o) > owner(o)]
    clean = [o for o in mic if o not in bleed]

    mic_hyp = [w for o in mic for w in words(o["text"])]
    sys_hyp = [w for o in sys_lines for w in words(o["text"])]
    all_hyp = [w for o in kept for w in words(o["text"])]

    lost = Counter(me) - Counter(w for o in clean for w in words(o["text"]))
    remote_words = Counter(w for o in mic for w in words(o["text"]) if w in them_only)

    if silence:
        in_silence = [o for o in kept if any(o["start"] < b and o["end"] > a for a, b in silence)]
        silence_sec = sum(max(0.0, b - a) for a, b in silence)
    elif not me and not them:
        in_silence = list(kept)
        silence_sec = duration
    else:
        in_silence = []
        silence_sec = 0.0

    invented_per_hour = (len(in_silence) / (silence_sec / 3600.0)) if silence_sec > 0 else 0.0

    allref = set(me) | set(them)
    unknown = [
        o for o in kept
        if words(o["text"]) and sum(w in allref for w in words(o["text"])) / len(words(o["text"])) < 0.5
    ]

    # Phantom phrases anywhere in a line
    phantom_lines = [
        o for o in kept
        if any(p in o["text"].lower() for p in PHANTOMS)
    ]

    # Sequence edit-distance WER per channel
    edits = edit_distance(me, mic_hyp) + edit_distance(them, sys_hyp)
    ref_total = len(me) + len(them)
    wer = (edits / ref_total) if ref_total > 0 else (float(len(all_hyp)) if all_hyp else 0.0)

    # Short words kept ("ja", "nei", "takk", "greit", or short_words-tagged turns)
    short_ref_me = Counter(w for w in me if w in SHORT_WORDS)
    short_ref_them = Counter(w for w in them if w in SHORT_WORDS)
    short_kept_me = sum((short_ref_me & Counter(mic_hyp)).values())
    short_kept_them = sum((short_ref_them & Counter(sys_hyp)).values())
    short_total = sum(short_ref_me.values()) + sum(short_ref_them.values())
    short_kept = short_kept_me + short_kept_them
    if "short_words" in tags and short_total == 0 and turns:
        short_total = len(turns)
        short_kept = sum(1 for _, tw in turns if any(w in all_hyp for w in tw))

    # Code-switch error rate
    cs_turns_me = [w for who, tw in turns if who == "me" and ("codeswitch" in tags or any(x in CS_ENGLISH for x in tw)) for w in tw]
    cs_turns_them = [w for who, tw in turns if who == "them" and ("codeswitch" in tags or any(x in CS_ENGLISH for x in tw)) for w in tw]
    if "codeswitch" in tags:
        cs_edits = edits
        cs_ref_len = ref_total
    else:
        cs_en_ref = Counter(w for w in (me + them) if w in CS_ENGLISH)
        cs_en_hyp = Counter(w for w in all_hyp if w in CS_ENGLISH)
        cs_ref_len = sum(cs_en_ref.values())
        cs_edits = sum((cs_en_ref - cs_en_hyp).values())
    cs_wer = (cs_edits / cs_ref_len) if cs_ref_len > 0 else None

    requests = [float(o["latency"]) for o in records if o.get("kind") == "request"]
    s_lat = sorted(requests)
    latency_mean = sum(s_lat) / len(s_lat) if s_lat else 0.0
    latency_median = statistics.median(s_lat) if s_lat else 0.0
    latency_p95 = percentile(s_lat, 0.95) if s_lat else 0.0

    vm_hwm_kb = int(summary.get("vm_hwm_kb") or 0)
    vm_rss_kb = int(summary.get("vm_rss_kb") or 0)
    load_mode = summary.get("load_mode") or "resident"

    dictation_ok = bool(kept) and len(phantom_lines) == 0 and (ref_total == 0 or wer <= 0.5)

    return {
        "bleed": bleed,
        "lost": lost,
        "remote_words": remote_words,
        "in_silence": in_silence,
        "unknown": unknown,
        "latency_mean": latency_mean,
        "latency_median": latency_median,
        "latency_p95": latency_p95,
        "num_requests": len(requests),
        "num_kept": len(kept),
        "wer": wer,
        "edits": edits,
        "ref_total": ref_total,
        "invented_per_hour": invented_per_hour,
        "silence_sec": silence_sec,
        "phantom_lines": phantom_lines,
        "short_kept": short_kept,
        "short_total": short_total,
        "cs_wer": cs_wer,
        "cs_edits": cs_edits,
        "cs_ref_len": cs_ref_len,
        "vm_hwm_kb": vm_hwm_kb,
        "vm_rss_kb": vm_rss_kb,
        "load_mode": load_mode,
        "dictation_ok": dictation_ok,
        "duration": duration,
    }


def score(records, me, them, silence):
    """Backwards-compatible tuple return for existing callers."""
    m = score_ext(records, me, them, silence)
    return (
        m["bleed"],
        m["lost"],
        m["remote_words"],
        m["in_silence"],
        m["unknown"],
        m["latency_mean"],
        m["num_kept"],
    )


def run_replay(wav: str, rest: list[str], replay_log: Path | None = None) -> list[dict]:
    run = subprocess.run(
        ["cargo", "run", "-q", "--release", "--example", "accuracy_replay", "--", wav, *rest],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if run.returncode:
        sys.exit(run.stderr)
    if replay_log is not None:
        replay_log.parent.mkdir(parents=True, exist_ok=True)
        replay_log.write_text(run.stdout, encoding="utf-8")
    return [json.loads(l) for l in run.stdout.splitlines() if l.strip()]


def print_single_report(m: dict, is_dictation: bool = False) -> None:
    for name, items in (
        ("bleed", m["bleed"]),
        ("invented on silence", m["in_silence"]),
        ("phantom phrase", m["phantom_lines"]),
        ("unknown-word line", m["unknown"]),
    ):
        for o in items:
            print(f"{name}: [{o['start']:.1f}s] {o['text']}")
    print(f"word error rate (WER): {m['wer'] * 100:.1f}% ({m['edits']}/{m['ref_total']} words)")
    if m["cs_wer"] is not None:
        print(f"code-switch WER: {m['cs_wer'] * 100:.1f}% ({m['cs_edits']}/{m['cs_ref_len']} words)")
    print(f"lost owner words: {sum(m['lost'].values())} {dict(m['lost'])}")
    print(f"remote words on the mic channel: {sum(m['remote_words'].values())} {dict(m['remote_words'])}")
    print(f"bleed lines: {len(m['bleed'])}")
    print(
        f"invented lines on silence: {len(m['in_silence'])} "
        f"({m['invented_per_hour']:.2f}/h over {m['silence_sec']:.1f}s)"
    )
    print(f"phantom phrases: {len(m['phantom_lines'])}")
    if m["short_total"] > 0:
        print(
            f"short words kept: {m['short_kept']}/{m['short_total']} "
            f"({m['short_kept'] / m['short_total'] * 100:.1f}%)"
        )
    else:
        print("short words kept: 0/0 (n/a)")
    if is_dictation:
        status = "ok" if m["dictation_ok"] else "fail"
        print(f"dictation: {status} (wer={m['wer'] * 100:.1f}%, lines={m['num_kept']})")
    print(f"lines with mostly unknown words (second number): {len(m['unknown'])}")
    print(
        f"inference latency per chunk: {m['latency_mean']:.2f}s "
        f"(median={m['latency_median']:.2f}s, p95={m['latency_p95']:.2f}s) "
        f"over {m['num_requests']} requests ({m['num_kept']} lines kept)"
    )
    print(
        f"peak RSS (VmHWM): {m['vm_hwm_kb'] / (1024 * 1024):.2f} GB "
        f"({m['vm_hwm_kb']} kB, VmRSS={m['vm_rss_kb']} kB, load_mode={m['load_mode']})"
    )


def score_suite(suite_dir: Path, rest: list[str], replay_log_dir: Path | None = None) -> None:
    manifest_path = suite_dir / "manifest.json"
    if not manifest_path.is_file():
        sys.exit(f"missing {manifest_path}; run tools/accuracy/no_mix.py first")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    for set_name, entries in manifest.items():
        if not entries:
            continue
        tot_edits = tot_ref = tot_bleed = tot_lost = tot_invented = tot_phantoms = 0
        tot_short_kept = tot_short_ref = tot_cs_edits = tot_cs_ref = 0
        tot_silence_sec = 0.0
        dict_pass = 0
        all_latencies: list[float] = []
        max_hwm_kb = 0
        last_mode = "resident"
        for item in entries:
            wav = item["wav"]
            ref = item["txt"]
            is_dict = bool(item.get("dictation"))
            flags = list(rest)
            if is_dict and "--dictation" not in flags:
                flags.append("--dictation")
            log_file = (replay_log_dir / set_name / f"{item['name']}.jsonl") if replay_log_dir else None
            me, them, silence, turns, tags = parse_reference(Path(ref).read_text(encoding="utf-8"))
            records = run_replay(wav, flags, replay_log=log_file)
            m = score_ext(records, me, them, silence, turns=turns, tags=tags, audio_duration=float(item.get("duration") or 0.0))
            tot_edits += m["edits"]
            tot_ref += m["ref_total"]
            tot_bleed += len(m["bleed"])
            tot_lost += sum(m["lost"].values())
            tot_invented += len(m["in_silence"])
            tot_silence_sec += m["silence_sec"]
            tot_phantoms += len(m["phantom_lines"])
            tot_short_kept += m["short_kept"]
            tot_short_ref += m["short_total"]
            if m["cs_wer"] is not None:
                tot_cs_edits += m["cs_edits"]
                tot_cs_ref += m["cs_ref_len"]
            if m["dictation_ok"]:
                dict_pass += 1
            all_latencies.extend(float(o["latency"]) for o in records if o.get("kind") == "request")
            max_hwm_kb = max(max_hwm_kb, m["vm_hwm_kb"])
            last_mode = m["load_mode"]
        s_lat = sorted(all_latencies)
        med = statistics.median(s_lat) if s_lat else 0.0
        p95 = percentile(s_lat, 0.95) if s_lat else 0.0
        wer_pct = (tot_edits / tot_ref * 100.0) if tot_ref > 0 else 0.0
        inv_hr = (tot_invented / (tot_silence_sec / 3600.0)) if tot_silence_sec > 0 else 0.0
        short_str = f"{tot_short_kept}/{tot_short_ref}" if tot_short_ref > 0 else "n/a"
        cs_str = f"{tot_cs_edits / tot_cs_ref * 100:.1f}%" if tot_cs_ref > 0 else "n/a"
        print(
            f"[{set_name}] files={len(entries)} WER={wer_pct:.1f}% ({tot_edits}/{tot_ref}) "
            f"cs_WER={cs_str} bleed={tot_bleed} lost={tot_lost} "
            f"invented={tot_invented} ({inv_hr:.2f}/h) phantoms={tot_phantoms} "
            f"short={short_str} dict_ok={dict_pass}/{len(entries)} "
            f"median={med:.2f}s p95={p95:.2f}s VmHWM={max_hwm_kb / (1024 * 1024):.2f}GB ({last_mode})"
        )


def extract_replay_log_flag(args: list[str]) -> tuple[list[str], Path | None]:
    out = []
    log_path = None
    i = 0
    while i < len(args):
        if args[i] == "--replay-log":
            if i + 1 >= len(args):
                sys.exit("--replay-log needs a file or directory path")
            log_path = Path(args[i + 1])
            i += 2
        else:
            out.append(args[i])
            i += 1
    return out, log_path


def main(argv: list[str]) -> None:
    if len(argv) < 2:
        sys.exit(__doc__)
    cleaned, replay_log = extract_replay_log_flag(argv)
    first = Path(cleaned[0])
    if first.is_dir():
        score_suite(first.resolve(), cleaned[1:], replay_log_dir=replay_log)
        return
    if len(cleaned) < 3:
        sys.exit(__doc__)
    wav, ref, rest = str(first.resolve()), cleaned[1], cleaned[2:]
    me, them, silence, turns, tags = parse_reference(Path(ref).read_text(encoding="utf-8"))
    records = run_replay(wav, rest, replay_log=replay_log)
    m = score_ext(
        records, me, them, silence,
        turns=turns, tags=tags, audio_duration=wav_duration(wav),
    )
    print_single_report(m, is_dictation=("--dictation" in rest))


if __name__ == "__main__":
    main(sys.argv[1:])
