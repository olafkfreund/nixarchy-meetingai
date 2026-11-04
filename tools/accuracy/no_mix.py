#!/usr/bin/env python3
"""Build Norwegian STT accuracy and #14 failure-case evaluation sets (#70 step 3).

Sources (kept in ~/.cache/meetingai-eval/no/, never in git):
- NPSC test split (NbAiLab/NPSC on Hugging Face, CC0; per sitting `data/test/<date>.json` + `<date>_16K_mp3.tar.gz`): ~2 h selected by fixed seed.
- FLEURS nb_no test (google/fleurs, CC BY 4.0): converted to 16 kHz s16le via ffmpeg -c:a pcm_s16le.
- MUSAN noise/ subset (OpenSLR 17, CC BY 4.0).
- Optional owner recordings folder (--owner-dir).

Writes stereo 16 kHz 16-bit PCM calls (and mono 16 kHz 16-bit PCM dictation clips) with
libri_mix.py / score.py reference .txt files (`me:`, `them:`, `[silence mm:ss-mm:ss]`):
  <out_dir>/
    npsc/             (~2 h conversational Norwegian calls)
    fleurs/           (FLEURS nb_no read-speech calls)
    silence/          (silence with [silence mm:ss-mm:ss])
    room_tone/        (quiet MUSAN room tone with [silence mm:ss-mm:ss])
    bursts/           (0.2-1.5 s MUSAN non-speech bursts with [silence mm:ss-mm:ss])
    remote_only/      (remote speech + 120 ms -12 dB mic bleed, no owner speech)
    double_talk/      (10-turn calls with 120 ms -12 dB bleed + owner double talk on turns 4, 8)
    short_words/      ("ja", "nei", "takk", "greit" segments)
    dictation_short/  (1-3 s mono dictation WAVs)
    dictation_long/   (40-60 s mono dictation WAVs)
    codeswitch/       (NPSC segments already containing English words, not spliced)
    owner/            (optional, when --owner-dir is provided)
    manifest.json
"""
from __future__ import annotations

import argparse
import array
import io
import json
import math
import os
import pathlib
import random
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import wave

RATE = 16_000
GAP = int(0.8 * RATE)
TAIL = int(4.0 * RATE)
DELAY = int(0.12 * RATE)
BLEED = 10 ** (-12 / 20)

SHORT_WORDS = {"ja", "nei", "takk", "greit"}

# Common English words used in Norwegian parliamentary/technical speech that are not
# homographs of ordinary Norwegian words.
ENGLISH_WORDS = {
    "account", "action", "agreement", "ai", "air", "airport", "alliance", "america",
    "american", "analysis", "app", "application", "approach", "arctic", "assessment",
    "Atlantic", "atlantic", "authority", "back", "background", "balance", "bank",
    "Banking", "based", "basic", "battery", "benchmark", "best", "better", "big",
    "bill", "black", "blue", "board", "bond", "book", "border", "bottom", "brain",
    "brand", "break", "brief", "briefing", "british", "broadband", "budget", "build",
    "building", "business", "call", "campaign", "cancer", "capacity", "capital",
    "carbon", "care", "career", "case", "cash", "center", "centre", "ceo", "chain",
    "challenge", "change", "channel", "charter", "chat", "check", "chief", "child",
    "children", "china", "choice", "city", "claim", "class", "clean", "clear",
    "climate", "close", "cloud", "club", "cluster", "co2", "coal", "coast", "code",
    "collaboration", "college", "committee", "common", "communication", "community",
    "company", "competition", "compliance", "computer", "concept", "concern",
    "conference", "conflict", "connect", "connection", "consensus", "content",
    "context", "contract", "control", "cooperation", "core", "corporate", "cost",
    "council", "country", "county", "couple", "course", "court", "cover", "coverage",
    "covid", "create", "credit", "crime", "crisis", "critical", "cross", "culture",
    "current", "customer", "cut", "cyber", "daily", "damage", "data", "database",
    "date", "dead", "deadline", "deal", "death", "debate", "debt", "decade",
    "decision", "deep", "default", "defence", "defense", "degree", "delay",
    "delivery", "demand", "democracy", "democratic", "department", "design",
    "desktop", "detail", "development", "device", "digital", "direct", "director",
    "discussion", "disease", "display", "distance", "distribution", "district",
    "diversity", "doctor", "document", "dollar", "domain", "domestic", "double",
    "down", "downtown", "draft", "drive", "driver", "drop", "drug", "duty", "early",
    "earth", "east", "easy", "economic", "economy", "edge", "education", "effect",
    "effective", "efficiency", "effort", "election", "electric", "electricity",
    "electronic", "element", "email", "emergency", "emission", "emissions",
    "employee", "employment", "end", "energy", "engine", "engineering", "english",
    "enough", "enterprise", "environment", "environmental", "equal", "equipment",
    "equity", "error", "establish", "estate", "estimate", "europe", "european",
    "evaluation", "event", "evidence", "example", "exchange", "executive",
    "exercise", "existing", "expansion", "experience", "expert", "export", "extra",
    "extreme", "eye", "face", "facility", "fact", "factor", "factory", "failure",
    "fair", "fall", "family", "far", "farm", "fast", "father", "feature", "federal",
    "fee", "feed", "feedback", "feel", "feeling", "field", "fight", "figure", "file",
    "film", "final", "finance", "financial", "find", "fine", "fire", "firm", "first",
    "fish", "fit", "five", "fix", "fixed", "flag", "flat", "fleet", "flight",
    "floor", "flow", "focus", "follow", "food", "foot", "football", "force",
    "foreign", "forest", "form", "format", "former", "forward", "foundation",
    "four", "frame", "framework", "free", "freedom", "french", "frequency", "fresh",
    "friend", "front", "fuel", "function", "fund", "funding", "future", "gain",
    "game", "gap", "gas", "gate", "general", "generation", "german", "germany",
    "giant", "gift", "girl", "give", "global", "goal", "gold", "good", "goods",
    "governance", "government", "grade", "grand", "grant", "Great", "great", "green",
    "grid", "ground", "group", "growth", "guarantee", "guard", "guide", "guideline",
    "guidelines", "half", "hall", "hand", "handle", "hard", "hardware", "head",
    "health", "healthcare", "heart", "heat", "heavy", "help", "high", "higher",
    "highway", "hill", "history", "hit", "hold", "holiday", "home", "hope",
    "hospital", "host", "hot", "hotel", "hour", "house", "housing", "hub", "human",
    "hundred", "hydrogen", "idea", "identity", "image", "impact", "implementation",
    "import", "important", "improvement", "incentive", "incident", "income",
    "increase", "independent", "index", "india", "indian", "individual",
    "industrial", "industry", "inflation", "info", "information", "infrastructure",
    "initial", "initiative", "innovation", "input", "inside", "insight", "instance",
    "institute", "institution", "insurance", "integration", "intelligence",
    "interest", "interface", "internal", "international", "internet", "interview",
    "investment", "investor", "ireland", "island", "issue", "item", "japan", "job",
    "joint", "journal", "judge", "justice", "keep", "key", "kid", "kind", "king",
    "knowledge", "lab", "labor", "labour", "lack", "lake", "land", "language",
    "laptop", "large", "last", "late", "later", "launch", "law", "lawyer", "layer",
    "lead", "leader", "leadership", "learning", "least", "leave", "left", "legal",
    "legislation", "length", "less", "lesson", "letter", "level", "liberal",
    "library", "license", "life", "light", "limit", "limited", "line", "link",
    "list", "little", "live", "living", "load", "loan", "local", "location", "lock",
    "log", "logic", "london", "long", "look", "loop", "loss", "lot", "love", "low",
    "lower", "machine", "magazine", "mail", "main", "maintenance", "major",
    "majority", "make", "maker", "making", "management", "manager", "manufacturing",
    "map", "margin", "mark", "market", "marketing", "marriage", "mass", "master",
    "match", "material", "matter", "maximum", "mayor", "meaning", "measure",
    "measurement", "media", "medical", "medicine", "medium", "meet", "meeting",
    "member", "membership", "memory", "mental", "message", "metal", "method",
    "middle", "might", "military", "million", "mind", "mine", "minimum", "mining",
    "minister", "ministry", "minor", "minority", "minute", "mission", "mistake",
    "mix", "mobile", "mode", "model", "modern", "moment", "money", "monitor",
    "monitoring", "month", "moral", "morning", "mother", "motion", "motor",
    "mountain", "mouse", "move", "movement", "movie", "multiple", "museum", "music",
    "name", "nation", "national", "native", "natural", "nature", "navy", "near",
    "necessary", "neck", "need", "negative", "network", "neutral", "never", "new",
    "news", "newspaper", "next", "nice", "night", "nine", "nobody", "node", "noise",
    "none", "normal", "north", "northern", "norway", "norwegian", "note", "nothing",
    "notice", "novel", "nuclear", "number", "nurse", "object", "objective",
    "obligation", "observation", "ocean", "off", "offer", "office", "officer",
    "official", "offshore", "oil", "okay", "old", "once", "one", "online", "only",
    "open", "operating", "operation", "operational", "operator", "opinion",
    "opportunity", "opposition", "option", "order", "ordinary", "organization",
    "original", "other", "otherwise", "outcome", "output", "outside", "overall",
    "owner", "ownership", "package", "page", "pain", "paint", "pair", "panel",
    "paper", "parent", "park", "parliament", "part", "participant", "particular",
    "partner", "partnership", "party", "pass", "passage", "passenger", "past",
    "path", "patient", "pattern", "pay", "payment", "peace", "peak", "penalty",
    "pension", "people", "percent", "percentage", "performance", "period",
    "permanent", "permission", "permit", "person", "personal", "perspective",
    "phase", "phone", "photo", "physical", "pick", "picture", "piece", "pilot",
    "pipeline", "place", "planet", "planning", "plant", "plastic", "platform",
    "play", "player", "please", "plus", "pocket", "podcast", "point", "police",
    "policy", "political", "politics", "pollution", "pool", "poor", "popular",
    "population", "port", "portfolio", "portion", "position", "positive",
    "possibility", "possible", "post", "potential", "Geir", "pound", "poverty",
    "power", "practical", "practice", "president", "press", "pressure", "previous",
    "price", "primary", "prime", "principal", "principle", "print", "prior",
    "priority", "prison", "privacy", "private", "prize", "problem", "procedure",
    "process", "processing", "produce", "producer", "product", "production",
    "professional", "professor", "profile", "profit", "program", "programme",
    "progress", "project", "promise", "promotion", "proof", "proper", "property",
    "proposal", "protection", "protocol", "public", "publication", "publish",
    "purchase", "purpose", "push", "quality", "quarter", "queen", "question",
    "quick", "quiet", "quite", "race", "radio", "rail", "railway", "rain", "raise",
    "range", "rank", "rapid", "rare", "rate", "rating", "ratio", "raw", "reach",
    "reaction", "read", "reader", "reading", "ready", "real", "reality", "reason",
    "reasonable", "receive", "recent", "recognition", "recommendation", "record",
    "recording", "recovery", "red", "reduce", "reduction", "reference", "reform",
    "refugee", "regard", "region", "regional", "register", "regular", "regulation",
    "regulatory", "relation", "relationship", "relative", "release", "relevant",
    "relief", "religion", "religious", "remain", "remote", "renewable", "rent",
    "repair", "repeat", "replace", "replacement", "reply", "report", "reporting",
    "representative", "reputation", "request", "requirement", "rescue", "research",
    "reserve", "resident", "resolution", "resource", "respect", "response",
    "responsibility", "responsible", "rest", "restaurant", "result", "retail",
    "return", "revenue", "review", "revolution", "rich", "ride", "right", "ring",
    "rise", "risk", "river", "road", "roadmap", "rock", "role", "roll", "roof",
    "room", "root", "round", "route", "routine", "row", "royal", "rule", "run",
    "running", "rural", "russia", "russian", "safe", "safety", "sale", "sales",
    "sample", "sanction", "sanctions", "satellite", "save", "saving", "savings",
    "scale", "scenario", "scene", "schedule", "scheme", "school", "science",
    "scientific", "scientist", "scope", "score", "screen", "script", "sea",
    "search", "season", "seat", "second", "secondary", "secret", "secretary",
    "section", "sector", "secure", "security", "see", "seed", "seek", "segment",
    "select", "selection", "self", "sell", "senate", "senior", "sense", "sensitive",
    "sentence", "separate", "sequence", "series", "serious", "server", "service",
    "session", "set", "setting", "settlement", "seven", "several", "severe", "sex",
    "shadow", "shake", "share", "shareholder", "sharp", "sheet", "shell", "shelter",
    "shift", "ship", "shipping", "shock", "shoe", "shoot", "shop", "shopping",
    "short", "shot", "shoulder", "show", "shut", "sick", "sight", "sign", "signal",
    "significant", "silence", "silver", "similar", "simple", "simulation", "since",
    "single", "sister", "site", "situation", "six", "size", "skill", "skin", "sky",
    "sleep", "slice", "slide", "slight", "slow", "small", "smart", "smile", "smoke",
    "smooth", "snow", "social", "society", "soft", "software", "soil", "solar",
    "soldier", "solid", "solution", "solve", "somebody", "someone", "something",
    "sometimes", "somewhat", "somewhere", "son", "song", "soon", "sorry", "sort",
    "soul", "sound", "source", "south", "southern", "space", "spanish", "speak",
    "speaker", "special", "Specialist", "specialist", "species", "specific",
    "speech", "speed", "spend", "spending", "spirit", "split", "spokesman", "sport",
    "spot", "spread", "spring", "square", "stability", "stable", "staff", "stage",
    "stake", "stakeholder", "stand", "standard", "star", "start", "startup",
    "state", "statement", "station", "statistics", "status", "stay", "steady",
    "steal", "steel", "step", "stick", "still", "stock", "stomach", "stone", "stop",
    "storage", "store", "storm", "story", "straight", "strange", "strategic",
    "strategy", "stream", "street", "strength", "stress", "stretch", "strike",
    "string", "strip", "stroke", "strong", "structure", "struggle", "student",
    "studio", "study", "stuff", "style", "subject", "submit", "subsequent",
    "subsidy", "substance", "substantial", "success", "successful", "such",
    "sudden", "suffer", "sufficient", "sugar", "suggest", "suggestion", "suit",
    "summer", "summit", "sun", "super", "supply", "support", "supporter", "suppose",
    "supreme", "sure", "surface", "surgery", "surprise", "surround", "survey",
    "survival", "survive", "suspect", "sustainable", "sweet", "swim", "swing",
    "switch", "symbol", "symptom", "system", "table", "tackle", "tail", "take",
    "tale", "talent", "talk", "tall", "tank", "tap", "tape", "target", "task",
    "taste", "tax", "taxpayer", "tea", "teach", "teacher", "teaching", "team",
    "tear", "tech", "technical", "technique", "technology", "teen", "teenager",
    "telephone", "television", "tell", "temperature", "temporary", "ten", "tend",
    "tendency", "tennis", "tension", "tent", "term", "terms", "terrible",
    "territory", "terror", "terrorism", "terrorist", "test", "testing", "text",
    "thank", "thanks", "theater", "theatre", "theme", "themselves", "theory",
    "therapy", "there", "therefore", "they", "thick", "thin", "thing", "think",
    "thinking", "third", "thirty", "this", "those", "though", "thought", "thousand",
    "threat", "threaten", "three", "throat", "through", "throughout", "throw",
    "thus", "ticket", "tie", "tight", "time", "timeline", "tiny", "tip", "tire",
    "tired", "tissue", "title", "tobacco", "today", "toe", "together", "tomorrow",
    "tone", "tongue", "tonight", "too", "tool", "tooth", "top", "topic", "total",
    "totally", "touch", "tough", "tour", "tourist", "tournament", "toward",
    "towards", "tower", "town", "toy", "trace", "track", "trade", "trading",
    "tradition", "traditional", "traffic", "tragedy", "trail", "train", "trainer",
    "training", "transaction", "transfer", "transformation", "transition",
    "translate", "translation", "transmission", "transport", "transportation",
    "trap", "trash", "travel", "treat", "treatment", "treaty", "tree", "tremendous",
    "trend", "trial", "tribe", "trick", "trip", "troop", "trouble", "truck", "true",
    "truly", "trust", "truth", "try", "tube", "tunnel", "turn", "tv", "twelve",
    "twenty", "twice", "twin", "two", "type", "typical", "typically", "ugly",
    "ukraine", "ukrainian", "ultimate", "ultimately", "unable", "uncle", "under",
    "undergo", "understand", "understanding", "unfortunately", "uniform", "union",
    "unique", "unit", "united", "universal", "universe", "university", "unknown",
    "unless", "unlike", "unlikely", "until", "unusual", "up", "update", "upon",
    "upper", "urban", "urge", "us", "usa", "use", "used", "useful", "user", "usual",
    "usually", "utility", "vacation", "vaccine", "valley", "valuable", "value",
    "variable", "variation", "variety", "various", "vary", "vast", "vegetable",
    "vehicle", "venture", "version", "versus", "very", "vessel", "veteran", "via",
    "victim", "victory", "video", "view", "viewer", "village", "violate",
    "violation", "violence", "violent", "virtual", "virus", "visible", "vision",
    "visit", "visitor", "visual", "vital", "voice", "volume", "volunteer", "vote",
    "voter", "vulnerable", "wage", "wait", "wake", "walk", "wall", "want", "war",
    "warm", "warn", "warning", "wash", "waste", "watch", "water", "wave", "way",
    "weak", "wealth", "wealthy", "weapon", "wear", "weather", "web", "website",
    "wedding", "week", "weekend", "weekly", "weigh", "weight", "welcome", "welfare",
    "well", "west", "western", "wet", "what", "whatever", "wheel", "when",
    "whenever", "where", "whereas", "whether", "which", "while", "whisper", "white",
    "who", "whole", "whom", "whose", "why", "wide", "widely", "widespread", "wife",
    "wild", "will", "willing", "win", "wind", "window", "wine", "wing", "winner",
    "winter", "wire", "wisdom", "wise", "wish", "with", "withdraw", "within",
    "without", "witness", "woman", "wonder", "wonderful", "wood", "wooden", "word",
    "work", "worker", "working", "workflow", "workforce", "workshop", "world",
    "worldwide", "worry", "worse", "worth", "would", "wound", "wrap", "write",
    "writer", "writing", "wrong", "yard", "yeah", "year", "yellow", "yes",
    "yesterday", "yet", "yield", "you", "young", "your", "yours", "yourself",
    "youth", "zero", "zone",
}

NORWEGIAN_HOMOGRAPHS = {
    "all", "alt", "an", "and", "are", "arm", "art", "as", "ask", "at", "av", "bad",
    "bak", "ball", "band", "bank", "bar", "bare", "barn", "base", "be", "bein",
    "best", "bil", "bit", "blant", "ble", "bli", "blir", "blod", "bok", "ul", "bord",
    "is", "in", "it", "if", "do", "by", "can", "come", "cut", "day", "deal", "del",
    "den", "der", "det", "din", "ul", "dra", "du", "dyr", "en", "end", "ene", "enn",
    "er", "et", "far", "fast", "feil", "fem", "fikk", "film", "fin", "fine", "fire",
    "fisk", "flere", "folk", "for", "form", "fort", "fra", "fram", "frem", "fri",
    "full", "før", "ga", "gal", "gammel", "gang", "gi", "gikk", "gir", "glad",
    "glass", "god", "gode", "godt", "grad", "gren", "grunn", "gul", "gull", "gå",
    "går", "ha", "had", "hadde", "halv", "ham", "han", "hand", "hans", "har", "hat",
    "hatt", "hav", "hel", "hele", "hell", "helt", "hen", "henne", "her", "hit",
    "hjem", "hjelp", "hos", "hotell", "ull", "hun", "hund", "hus", "hva", "hvem",
    "hver", "hvert", "hvit", "hvor", "i", "id", "igjen", "ikke", "ild", "inn",
    "inne", "is", "ja", "jeg", "jord", "jul", "kamp", "kan", "kant", "kap", "kar",
    "kart", "kast", "katt", "kjent", "klar", "klok", "klub", "knapp", "kom", "kort",
    "kost", "krav", "krig", "kropp", "kul", "kun", "kunst", "kurs", "kveld", "kvinne",
    "la", "lag", "land", "lang", "langs", "lant", "lar", "last", "lav", "led", "leder",
    "leg", "lett", "let", "lev", "lik", "like", "litt", "liv", "lov", "luft", "lukk",
    "lyd", "lys", "lyst", "liten", "lån", "lå", "lås", "løp", "løs", "makt", "mal",
    "man", "mann", "mat", "med", "meget", "mellom", "men", "mens", "mer", "mest",
    "meter", "mid", "midt", "mil", "mild", "million", "min", "mindre", "mine", "ul",
    "minst", "minutt", "mitt", "mod", "modell", "mor", "mot", "ul", "mye", "mål",
    "måte", "natt", "natur", "navn", "ned", "nei", "nest", "neste", "nett", "ni",
    "no", "noe", "noen", "nok", "nord", "normal", "norsk", "ny", "nye", "nytt",
    "nær", "nå", "når", "og", "også", "ok", "olje", "om", "ombord", "område", "ond",
    "opp", "oppe", "ord", "oss", "ost", "over", "par", "park", "part", "parti", "pass",
    "pen", "penger", "per", "person", "plan", "plass", "pol", "politisk", "post",
    "ul", "pris", "privat", "problem", "prosent", "punkt", "på", "rad", "rak", "ram",
    "rand", "rang", "rask", "red", "redd", "regel", "regn", "rein", "reise", "rekke",
    "ren", "rett", "rik", "ring", "ris", "ro", "rom", "rop", "rose", "ul", "rund",
    "rundt", "rygg", "råd", "sa", "sag", "sak", "sal", "salg", "salt", "sam", "samme",
    "sammen", "sand", "ul", "sang", "sann", "sat", "satt", "sau", "se", "seg", "seil",
    "seks", "selv", "sen", "send", "seng", "sent", "ser", "ul", "sett", "si", "side",
    "siden", "ul", "sin", "sine", "ul", "sist", "siste", "ul", "sitt", "sjø", "skal",
    "skatt", "ul", "ski", "ul", "skip", "skog", "skole", "skritt", "skudd", "sky",
    "skyld", "slag", "slags", "slik", "slipp", "slutt", "slå", "smak", "smal", "smil",
    "små", "snakke", "snart", "snø", "sol", "som", "sommer", "son", "sort", "sov",
    "spar", "spill", "spis", "spor", "språk", "spør", "stad", "ul", "stakk", "stand",
    "standard", "start", "stat", "status", "sted", "stein", "stell", "stemme",
    "sterk", "sti", "ig", "stille", "stjern", "sto", "stod", "stol", "stopp", "stor",
    "store", "stort", "storm", "straf", "straks", "strand", "ul", "strøm", "stund",
    "styr", "styre", "støtte", "stå", "står", "svar", "svart", "svak", "syk", "synd",
    "syn", "syv", "særlig", "sønn", "sør", "søster", "søt", "så", "sår", "ta", "tak",
    "takk", "tall", "tanke", "tap", "tar", "taus", "te", "tegn", "tekst", "tel",
    "telt", "tema", "tenk", "test", "tett", "ti", "tid", "tidlig", "til", "time",
    "ting", "to", "tog", "tok", "tom", "ton", "tone", "topp", "torg", "ul", "tre",
    "trekk", "ul", "trikk", "tro", "tross", "ul", "trygg", "trykk", "tung", "tur",
    "tusen", "tverr", "tvil", "tynn", "type", "tørr", "ul", "uke", "ul", "ulik", "ull",
    "ulv", "under", "ung", "unge", "unna", "ut", "ute", "uten", "utvikling", "uvær",
    "valg", "van", "vand", "vanlig", "vann", "vant", "var", "vare", "varm", "ved",
    "veg", "vegg", "vei", "vekt", "vel", "veldig", "velg", "venn", "vent", "verden",
    "ul", "verdi", "verk", "verre", "verst", "vert", "vest", "vet", "vi", "vid",
    "videre", "vik", "viktig", "vil", "vill", "ville", "vin", "vind", "vindu", "vinn",
    "vinter", "ul", "virkelig", "vis", "vise", "viss", "visst", "vit", "vite", "voksen",
    "vold", "ul", "vond", "ul", "vår", "våre", "vårt", "vær", "være", "vært", "yen",
    "yrke", "ytre", "ønske", "øy", "øye", "øst", "å", "åpen", "år", "åtte",
}

STRICT_ENGLISH = {w.lower() for w in ENGLISH_WORDS} - NORWEGIAN_HOMOGRAPHS


def norm_text(text: str) -> str:
    text = re.sub(r"<[^>]*>", " ", text.lower().replace("’", "'"))
    words = re.findall(r"[^\W_]+(?:'[^\W_]+)?", text)
    return " ".join(words)


def decode_pcm_bytes(raw: bytes) -> array.array:
    """Decode WAV/MP3/FLAC bytes to 16 kHz mono 16-bit PCM array('h')."""
    if raw[:4] == b"RIFF" and raw[8:12] == b"WAVE":
        fmt = data = None
        pos = 12
        while pos + 8 <= len(raw):
            cid = raw[pos : pos + 4]
            size = struct.unpack_from("<I", raw, pos + 4)[0]
            body = raw[pos + 8 : pos + 8 + size]
            if cid == b"fmt " and len(body) >= 16:
                fmt = struct.unpack_from("<HHIIHH", body)
            elif cid == b"data":
                data = body
            pos += 8 + size + (size & 1)
        if fmt is not None and data is not None:
            tag, channels, rate, _, _, bits = fmt
            if channels == 1 and rate == RATE and tag == 1 and bits == 16:
                out = array.array("h")
                out.frombytes(data[: len(data) // 2 * 2])
                return out
            if channels == 1 and rate == RATE and tag == 3 and bits == 32:
                floats = struct.unpack(f"<{len(data) // 4}f", data[: len(data) // 4 * 4])
                return array.array(
                    "h", (max(-32768, min(32767, int(v * 32767))) for v in floats)
                )
    proc = subprocess.run(
        [
            "ffmpeg", "-nostdin", "-loglevel", "error",
            "-i", "pipe:0",
            "-c:a", "pcm_s16le", "-ar", str(RATE), "-ac", "1", "-f", "s16le",
            "pipe:1",
        ],
        input=raw,
        capture_output=True,
        check=True,
    )
    out = array.array("h")
    out.frombytes(proc.stdout[: len(proc.stdout) // 2 * 2])
    return out


def read_wav_file(path: pathlib.Path) -> array.array:
    """Read a WAV file as 16 kHz mono 16-bit PCM (pure-Python RIFF s16/f32, or ffmpeg -c:a pcm_s16le)."""
    return decode_pcm_bytes(path.read_bytes())


def write_stereo_wav(path: pathlib.Path, left: list[float], right: list[float]) -> float:
    path.parent.mkdir(parents=True, exist_ok=True)
    n = max(len(left), len(right))
    if len(left) < n:
        left.extend([0.0] * (n - len(left)))
    if len(right) < n:
        right.extend([0.0] * (n - len(right)))
    clip = lambda v: max(-32768, min(32767, int(v * 32767)))
    pcm = array.array("h", (clip(v) for pair in zip(left, right) for v in pair))
    with wave.open(str(path), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm.tobytes())
    return n / RATE


def write_mono_wav(path: pathlib.Path, samples: array.array) -> float:
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(samples.tobytes())
    return len(samples) / RATE


def place(buf: list[float], at: int, samples: array.array, gain: float = 1.0) -> None:
    if len(buf) < at + len(samples):
        buf.extend([0.0] * (at + len(samples) - len(buf)))
    for i, s in enumerate(samples):
        buf[at + i] += gain * (s / 32768.0)


def fmt_mmss(seconds: float) -> str:
    sec = max(0, int(round(seconds)))
    return f"{sec // 60}:{sec % 60:02d}"


def load_fleurs(fleurs_dir: pathlib.Path, max_items: int | None = None) -> list[dict]:
    tsv = fleurs_dir / "test.tsv"
    if not tsv.is_file():
        return []
    audio_dir = fleurs_dir / "test"
    if not audio_dir.is_dir():
        audio_dir = fleurs_dir / "audio" / "test"
    if not audio_dir.is_dir():
        audio_dir = fleurs_dir
    tar_path = fleurs_dir / "test.tar.gz"
    items = []
    need_from_tar: dict[str, list[dict]] = {}
    for line in tsv.read_text(encoding="utf-8").splitlines():
        parts = line.split("\t")
        if len(parts) < 4:
            continue
        spk_id, wav_name, raw_t, norm_t = parts[0], parts[1], parts[2], parts[3]
        text = norm_text(norm_t or raw_t)
        if not text:
            continue
        w = text.split()
        is_special = any(x in SHORT_WORDS for x in w) or any(x in STRICT_ENGLISH for x in w)
        if max_items is not None and len(items) >= max_items and not is_special:
            continue
        dur = (int(parts[5]) / RATE) if len(parts) > 5 and parts[5].isdigit() else 0.0
        wav_path = audio_dir / wav_name
        entry: dict = {
            "id": wav_name[:-4],
            "speaker": f"fleurs_{spk_id}",
            "text": text,
            "duration": dur,
        }
        if wav_path.is_file():
            entry["wav_path"] = wav_path
            items.append(entry)
        elif tar_path.is_file():
            need_from_tar.setdefault(wav_name, []).append(entry)
            items.append(entry)
    if need_from_tar and tar_path.is_file():
        remaining = set(need_from_tar.keys())
        with tarfile.open(str(tar_path), "r:gz") as tf:
            for member in tf:
                if not member.isfile():
                    continue
                base = os.path.basename(member.name)
                if base in remaining:
                    fobj = tf.extractfile(member)
                    if fobj is not None:
                        raw = fobj.read()
                        for entry in need_from_tar[base]:
                            entry["audio_bytes"] = raw
                    remaining.discard(base)
                    if not remaining:
                        break
        items = [it for it in items if "wav_path" in it or "audio_bytes" in it]
    return items


def get_samples(item: dict) -> array.array:
    if "samples" not in item:
        if "wav_path" in item:
            item["samples"] = read_wav_file(item["wav_path"])
        elif "audio_bytes" in item:
            item["samples"] = decode_pcm_bytes(item["audio_bytes"])
        else:
            raise ValueError(f"item {item.get('id')} has no audio source")
    return item["samples"]


def load_npsc(npsc_dir: pathlib.Path, seed: int, target_hours: float) -> list[dict]:
    """NPSC as published: one JSONL file and one `<date>_16K_mp3.tar.gz` per sitting
    (`data/test/<date>.json`). Each sentence's MP3 is decoded once to 16 kHz s16 WAV."""
    import json
    import subprocess
    import tarfile

    sittings = sorted(npsc_dir.glob("*.json")) if npsc_dir.is_dir() else []
    if not sittings:
        return []
    wav_dir = npsc_dir / "wav"
    wav_dir.mkdir(exist_ok=True)
    rows = []
    for js in sittings:
        archive = npsc_dir / f"{js.stem}_16K_mp3.tar.gz"
        if not archive.is_file():
            continue
        lines = [json.loads(line) for line in js.read_text(encoding="utf-8").splitlines() if line.strip()]
        wanted = {}
        for r in lines:
            if r.get("sentence_language_code") != "nb-NO":
                continue
            text = norm_text(str(r.get("normsentence_text") or r.get("sentence_text") or ""))
            if text:
                wanted[pathlib.PurePosixPath(r["path"]).name] = (r, text)
        with tarfile.open(archive) as tar:
            for member in tar:
                stem = pathlib.PurePosixPath(member.name).stem
                if not member.isfile() or stem not in wanted:
                    continue
                r, text = wanted[stem]
                wav = wav_dir / f"{stem}.wav"
                if not wav.is_file():
                    data = tar.extractfile(member).read()
                    subprocess.run(
                        ["ffmpeg", "-loglevel", "error", "-y", "-i", "pipe:0",
                         "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", str(wav)],
                        input=data, check=True,
                    )
                rows.append({
                    "id": str(r["sentence_id"]),
                    "speaker": f"npsc_{r['meeting_date']}_{r['speaker_id']}",
                    "text": text,
                    "duration": (r["end_time"] - r["start_time"]) / 1000.0,
                    "wav_path": wav,
                })
    rng = random.Random(seed)
    indices = list(range(len(rows)))
    rng.shuffle(indices)
    target_sec = target_hours * 3600.0
    selected = []
    acc = 0.0
    # Keep all short words and code-switched segments plus up to target_sec of general segments
    for i in indices:
        row = rows[i]
        w = row["text"].split()
        is_short = len(w) <= 2 and any(x in SHORT_WORDS for x in w)
        is_cs = any(x in STRICT_ENGLISH for x in w)
        if acc < target_sec or is_short or is_cs:
            selected.append(row)
            acc += row["duration"]
    selected.sort(key=lambda r: r["id"])
    return selected


def load_musan_noise(musan_dir: pathlib.Path) -> list[pathlib.Path]:
    if not musan_dir.is_dir():
        return []
    noise_root = musan_dir / "noise" if (musan_dir / "noise").is_dir() else musan_dir
    return sorted(noise_root.rglob("*.wav"))


def build_calls(
    items: list[dict],
    out_dir: pathlib.Path,
    seed: int,
    max_calls: int | None = None,
    double_talk: bool = False,
    remote_only: bool = False,
) -> list[dict]:
    rng = random.Random(seed)
    pool = [it for it in items if it.get("text")]
    rng.shuffle(pool)
    by_spk: dict[str, list[dict]] = {}
    for it in pool:
        by_spk.setdefault(it["speaker"], []).append(it)
    speakers = [s for s, utts in sorted(by_spk.entries() if hasattr(by_spk, "entries") else by_spk.items()) if len(utts) >= 2]
    if len(speakers) < 2:
        speakers = ["spk_a", "spk_b"]
        by_spk = {"spk_a": pool[::2], "spk_b": pool[1::2]}

    manifest = []
    # Cutting audio while keeping the full reference text scores every cut word as an error, so
    # over-long sentences are left out instead (turns up to 14 s; overlaps up to 10 s).
    pool = [u for u in pool if u.get("duration", 0.0) <= 14.0]
    if not pool:
        return manifest
    turns_per_call = min(6 if remote_only else 10, len(pool))
    cursor = 0
    call_idx = 0
    while cursor + turns_per_call <= len(pool):
        if max_calls is not None and call_idx >= max_calls:
            break
        batch = pool[cursor : cursor + turns_per_call + (2 if double_talk else 0)]
        cursor += turns_per_call + (2 if double_talk else 0)
        if len(batch) < turns_per_call:
            break
        call_idx += 1
        left: list[float] = []
        right: list[float] = []
        ref: list[str] = []
        t = 0
        extra = batch[turns_per_call:]
        for turn in range(turns_per_call):
            u = batch[turn]
            s = get_samples(u)
            if remote_only or turn % 2 == 0:
                place(right, t, s, 1.0)
                if double_talk or remote_only:
                    place(left, t + DELAY, s, BLEED)
                ref.append(f"them: {u['text']}")
                if double_talk and turn in (4, 8) and extra:
                    over = extra.pop(0)
                    m = get_samples(over)
                    place(left, t + len(s) // 2, m, 1.0)
                    ref.append(f"me: {over['text']}")
                    t += max(0, len(s) // 2 + len(m) - len(s))
                t += len(s) + GAP
            else:
                place(left, t, s, 1.0)
                ref.append(f"me: {u['text']}")
                t += len(s) + GAP
        t += TAIL
        if len(left) < t:
            left.extend([0.0] * (t - len(left)))
        if len(right) < t:
            right.extend([0.0] * (t - len(right)))
        name = f"call_{call_idx:03d}"
        wav_path = out_dir / f"{name}.wav"
        txt_path = out_dir / f"{name}.txt"
        dur = write_stereo_wav(wav_path, left, right)
        txt_path.write_text("\n".join(ref) + "\n", encoding="utf-8")
        manifest.append({"name": name, "wav": str(wav_path), "txt": str(txt_path), "duration": dur})
    return manifest


def build_silence_and_noise(
    musan_files: list[pathlib.Path],
    out_dir: pathlib.Path,
    seed: int,
    duration_sec: float,
) -> dict[str, list[dict]]:
    rng = random.Random(seed)
    n_samples = int(duration_sec * RATE)
    results: dict[str, list[dict]] = {"silence": [], "room_tone": [], "bursts": []}

    # 1. Pure & near-zero silence
    sil_wav = out_dir / "silence" / "silence_001.wav"
    sil_txt = out_dir / "silence" / "silence_001.txt"
    dur = write_stereo_wav(sil_wav, [0.0] * n_samples, [0.0] * n_samples)
    sil_txt.write_text(f"[silence 0:00-{fmt_mmss(dur)}]\n", encoding="utf-8")
    results["silence"].append({"name": "silence_001", "wav": str(sil_wav), "txt": str(sil_txt), "duration": dur})

    # Gather noise samples from MUSAN (or deterministic fallback in slice mode)
    noise_pool: array.array = array.array("h")
    shuffled_musan = list(musan_files)
    rng.shuffle(shuffled_musan)
    for nf in shuffled_musan:
        noise_pool.extend(read_wav_file(nf))
        if len(noise_pool) >= n_samples * 2:
            break
    if len(noise_pool) < n_samples:
        fallback = array.array(
            "h",
            (int(rng.uniform(-0.35, 0.35) * 32767) for _ in range(n_samples * 2)),
        )
        noise_pool.extend(fallback)

    # 2. Quiet room tone (ambient background ~ -38 dBFS)
    room_left = [0.0] * n_samples
    for i in range(n_samples):
        room_left[i] = (noise_pool[i % len(noise_pool)] / 32768.0) * 0.04
    rt_wav = out_dir / "room_tone" / "room_tone_001.wav"
    rt_txt = out_dir / "room_tone" / "room_tone_001.txt"
    dur = write_stereo_wav(rt_wav, room_left, [0.0] * n_samples)
    rt_txt.write_text(f"[silence 0:00-{fmt_mmss(dur)}]\n", encoding="utf-8")
    results["room_tone"].append({"name": "room_tone_001", "wav": str(rt_wav), "txt": str(rt_txt), "duration": dur})

    # 3. Non-speech bursts of 0.2-1.5 s (above SOUND_LEVEL=0.008 so BatchChunks sends them)
    burst_left = [0.0] * n_samples
    at = RATE
    pool_at = 0
    while at + int(1.5 * RATE) < n_samples - TAIL:
        b_len = int(rng.uniform(0.22, 1.5) * RATE)
        seg = noise_pool[pool_at : pool_at + b_len]
        pool_at = (pool_at + b_len) % max(1, len(noise_pool) - b_len)
        peak = max((abs(s) for s in seg), default=1) / 32768.0
        gain = 0.25 / max(peak, 0.01)
        for i, s in enumerate(seg):
            burst_left[at + i] = max(-0.9, min(0.9, (s / 32768.0) * gain))
        at += b_len + int(rng.uniform(1.0, 1.8) * RATE)
    b_wav = out_dir / "bursts" / "bursts_001.wav"
    b_txt = out_dir / "bursts" / "bursts_001.txt"
    dur = write_stereo_wav(b_wav, burst_left, [0.0] * n_samples)
    b_txt.write_text(f"[silence 0:00-{fmt_mmss(dur)}]\n", encoding="utf-8")
    results["bursts"].append({"name": "bursts_001", "wav": str(b_wav), "txt": str(b_txt), "duration": dur})
    return results


def build_short_words(items: list[dict], out_dir: pathlib.Path, max_items: int = 20) -> list[dict]:
    short_utts = [
        it for it in items
        if 1 <= len(it["text"].split()) <= 3 and any(w in SHORT_WORDS for w in it["text"].split())
    ]
    if not short_utts:
        for it in items:
            words = it["text"].split()
            # Audio is cut at 4 s below, so a longer sentence would keep words that were never played.
            if any(w in SHORT_WORDS for w in words) and it.get("duration", 99.0) <= 4.0:
                short_utts.append(it)
            if len(short_utts) >= max_items:
                break
    if not short_utts and items:
        # In slice fallback when FLEURS has no standalone "ja"/"nei"/"takk"/"greit" clip,
        # include a 1.2 s spoken clip tagged with its first words so short-utterance survival is exercised.
        for it in items[:max_items]:
            s = get_samples(it)[: int(1.2 * RATE)]
            w = " ".join(it["text"].split()[:2])
            short_utts.append({"id": it["id"], "speaker": it["speaker"], "text": w, "samples": s})
    short_utts = short_utts[:max_items]
    if not short_utts:
        return []
    left: list[float] = []
    ref: list[str] = []
    t = int(0.5 * RATE)
    for it in short_utts:
        s = get_samples(it)
        if len(s) > 4 * RATE:
            s = s[: 4 * RATE]
        place(left, t, s, 1.0)
        ref.append(f"me: {it['text']}")
        t += len(s) + int(1.5 * RATE)
    t += TAIL
    right = [0.0] * t
    if len(left) < t:
        left.extend([0.0] * (t - len(left)))
    wav_path = out_dir / "short_words_001.wav"
    txt_path = out_dir / "short_words_001.txt"
    dur = write_stereo_wav(wav_path, left, right)
    txt_path.write_text("# [short_words]\n" + "\n".join(ref) + "\n", encoding="utf-8")
    return [{"name": "short_words_001", "wav": str(wav_path), "txt": str(txt_path), "duration": dur}]


def build_dictations(items: list[dict], out_dir: pathlib.Path, max_clips: int = 5) -> dict[str, list[dict]]:
    short_manifest = []
    long_manifest = []

    # 1. Short dictations (1-3 s)
    for it in items:
        if len(short_manifest) >= max_clips:
            break
        est_dur = float(it.get("duration") or 0.0)
        if est_dur > 0.0 and not (1.0 <= est_dur <= 3.5):
            continue
        s = get_samples(it)
        dur = len(s) / RATE
        if 1.0 <= dur <= 3.5:
            idx = len(short_manifest) + 1
            name = f"dict_short_{idx:03d}"
            wav_p = out_dir / "dictation_short" / f"{name}.wav"
            txt_p = out_dir / "dictation_short" / f"{name}.txt"
            write_mono_wav(wav_p, s)
            txt_p.write_text(f"me: {it['text']}\n", encoding="utf-8")
            short_manifest.append({
                "name": name, "wav": str(wav_p), "txt": str(txt_p),
                "duration": dur, "dictation": True,
            })
    if not short_manifest and items:
        s = get_samples(items[0])[: int(2.8 * RATE)]
        dur = len(s) / RATE
        name = "dict_short_001"
        wav_p = out_dir / "dictation_short" / f"{name}.wav"
        txt_p = out_dir / "dictation_short" / f"{name}.txt"
        write_mono_wav(wav_p, s)
        txt_p.write_text(f"me: {' '.join(items[0]['text'].split()[:6])}\n", encoding="utf-8")
        short_manifest.append({
            "name": name, "wav": str(wav_p), "txt": str(txt_p),
            "duration": dur, "dictation": True,
        })

    # 2. Long dictations (40-60 s)
    cursor = 0
    while len(long_manifest) < max_clips and cursor < len(items):
        combined = array.array("h")
        texts = []
        while cursor < len(items) and len(combined) / RATE < 42.0:
            it = items[cursor]
            cursor += 1
            s = get_samples(it)
            if len(combined) + len(s) > 59 * RATE and len(combined) >= 40 * RATE:
                break
            if combined:
                combined.extend([0] * int(0.4 * RATE))
            combined.extend(s)
            texts.append(it["text"])
        dur = len(combined) / RATE
        if 40.0 <= dur <= 60.0:
            idx = len(long_manifest) + 1
            name = f"dict_long_{idx:03d}"
            wav_p = out_dir / "dictation_long" / f"{name}.wav"
            txt_p = out_dir / "dictation_long" / f"{name}.txt"
            write_mono_wav(wav_p, combined)
            txt_p.write_text(f"me: {' '.join(texts)}\n", encoding="utf-8")
            long_manifest.append({
                "name": name, "wav": str(wav_p), "txt": str(txt_p),
                "duration": dur, "dictation": True,
            })

    return {"dictation_short": short_manifest, "dictation_long": long_manifest}


def build_codeswitch(items: list[dict], out_dir: pathlib.Path, seed: int, max_calls: int | None = None) -> list[dict]:
    cs_items = [
        it for it in items
        if any(w in STRICT_ENGLISH for w in it["text"].split())
    ]
    if not cs_items:
        return []
    calls = build_calls(cs_items, out_dir, seed=seed, max_calls=max_calls)
    for c in calls:
        txt_p = pathlib.Path(c["txt"])
        txt_p.write_text("# [codeswitch]\n" + txt_p.read_text(encoding="utf-8"), encoding="utf-8")
    return calls


def build_owner(owner_dir: pathlib.Path, out_dir: pathlib.Path) -> list[dict]:
    if not owner_dir or not owner_dir.is_dir():
        return []
    manifest = []
    target = out_dir / "owner"
    target.mkdir(parents=True, exist_ok=True)
    for wav_p in sorted(owner_dir.glob("*.wav")):
        txt_p = wav_p.with_suffix(".txt")
        if not txt_p.is_file():
            continue
        dst_wav = target / wav_p.name
        dst_txt = target / txt_p.name
        shutil.copy2(wav_p, dst_wav)
        shutil.copy2(txt_p, dst_txt)
        with wave.open(str(dst_wav), "rb") as w:
            dur = w.getnframes() / w.getframerate()
        manifest.append({"name": wav_p.stem, "wav": str(dst_wav), "txt": str(dst_txt), "duration": dur})
    return manifest


def main(argv: list[str] | None = None) -> None:
    cache_root = pathlib.Path(
        os.environ.get("MEETINGAI_EVAL_CACHE", os.path.expanduser("~/.cache/meetingai-eval"))
    ) / "no"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache-dir", type=pathlib.Path, default=cache_root)
    parser.add_argument("--npsc-dir", type=pathlib.Path, default=None)
    parser.add_argument("--fleurs-dir", type=pathlib.Path, default=None)
    parser.add_argument("--musan-dir", type=pathlib.Path, default=None)
    parser.add_argument("--owner-dir", type=pathlib.Path, default=None)
    parser.add_argument("--out-dir", type=pathlib.Path, default=None)
    parser.add_argument("--seed", type=int, default=70)
    parser.add_argument("--hours", type=float, default=2.0, help="NPSC target hours (default 2.0)")
    parser.add_argument("--slice", type=int, default=None, help="Build a small smoke slice of N calls per set")
    args = parser.parse_args(argv)

    base = args.cache_dir
    npsc_dir = args.npsc_dir or (base / "npsc")
    fleurs_dir = args.fleurs_dir or (base / "fleurs" if (base / "fleurs" / "test.tsv").is_file() else base / "clips")
    musan_dir = args.musan_dir or (base / "musan")
    out_dir = args.out_dir or (base / ("slice" if args.slice else "sets"))
    out_dir.mkdir(parents=True, exist_ok=True)

    fleurs_items = load_fleurs(fleurs_dir, max_items=(args.slice * 20 if args.slice else None))
    npsc_items = load_npsc(
        npsc_dir,
        seed=args.seed,
        target_hours=(0.05 * args.slice if args.slice else args.hours),
    )
    musan_files = load_musan_noise(musan_dir)

    if not args.slice:
        if not npsc_items:
            sys.exit(f"missing NPSC <date>.json + <date>_16K_mp3.tar.gz files in {npsc_dir}; see tools/accuracy/README.md")
        if not fleurs_items:
            sys.exit(f"missing FLEURS nb_no test.tsv in {fleurs_dir}; see tools/accuracy/README.md")
        if not musan_files:
            sys.exit(f"missing MUSAN noise WAVs in {musan_dir}; see tools/accuracy/README.md")

    primary = npsc_items if npsc_items else fleurs_items
    if not primary:
        sys.exit(f"no NPSC or FLEURS audio found under {base}")

    max_calls = args.slice
    manifest: dict[str, list[dict]] = {}

    if npsc_items:
        manifest["npsc"] = build_calls(npsc_items, out_dir / "npsc", seed=args.seed, max_calls=max_calls)
    if fleurs_items:
        manifest["fleurs"] = build_calls(fleurs_items, out_dir / "fleurs", seed=args.seed + 1, max_calls=max_calls)

    manifest["double_talk"] = build_calls(
        primary, out_dir / "double_talk", seed=args.seed + 2,
        max_calls=(max_calls or 10), double_talk=True,
    )
    manifest["remote_only"] = build_calls(
        primary, out_dir / "remote_only", seed=args.seed + 3,
        max_calls=(max_calls or 5), remote_only=True,
    )

    noise_dur = 20.0 if args.slice else 300.0
    manifest.update(build_silence_and_noise(musan_files, out_dir, seed=args.seed + 4, duration_sec=noise_dur))
    manifest["short_words"] = build_short_words(
        primary, out_dir / "short_words", max_items=(4 if args.slice else 20)
    )
    manifest.update(
        build_dictations(primary, out_dir, max_clips=(1 if args.slice else 5))
    )
    manifest["codeswitch"] = build_codeswitch(
        primary, out_dir / "codeswitch", seed=args.seed + 5, max_calls=(max_calls or 10)
    )
    if args.owner_dir:
        manifest["owner"] = build_owner(args.owner_dir, out_dir)

    (out_dir / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    for k, items_list in manifest.items():
        total_s = sum(x["duration"] for x in items_list)
        print(f"{k}: {len(items_list)} files, {total_s:.1f}s")


if __name__ == "__main__":
    main()
