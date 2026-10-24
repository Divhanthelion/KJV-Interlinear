"""Voice one chapter: script lines -> Fish requests -> MP3 + timings.

A chapter is one request when its voices fit (Fish takes about three minutes
of reference audio per request) and it is not huge; otherwise it is split at
narrator lines into scenes. One request keeps every voice conditioned on what
came before (Fish's condition_on_previous_chunks): no cold starts, no joins.

Requests are cached by a hash of everything that changes the audio, so only
changed scenes are voiced again. Each cached render keeps Fish's word timings,
from which every line and verse gets its start and end.
"""
import hashlib
import json
import re
import subprocess

from . import books, cast as castmod, fish, lexicon, paths, script

SPEED = 1.0
MAX_CHARS = 14000  # about 15 minutes of speech: one request (measured 36 chars/s rendering)
# Voices one request may carry (None: as many as fit). With many, Fish loses track of who
# is who: 1 Samuel 17 as one request of nine voices gave David's lines Jesse's voice
MAX_VOICES = 2  # 2026-10-12, whole KJV: lines in another voice by voices a request: 1: 0%, 2: 0.4%, 3: 3.8%, 4-5: 2-3%, 6-10: 7-16%


def voice_map(placeholders: dict | None = None) -> dict:
    """{cast key: (Fish voice id, reference seconds, age)} for every key."""
    entries = castmod.load()
    out = {}
    for key in entries:
        base, aged = castmod.resolve(key, entries)
        if placeholders is not None:  # {key: (library voice id, its reference seconds)}, for tests
            vid, secs = placeholders.get(base) or placeholders.get(key) or (None, 0.0)
        else:
            v = entries.get(base, {}).get("voice") or {}
            vid, secs = v.get("id"), v.get("seconds", 12.0)
        out[key] = (vid, secs, aged)
    return out


AGE_CUE = {"old": "[elderly, slower, a little frail]"}


def spoken_lines(code: str, chapter: int) -> list:
    """The chapter's lines with their cast key resolved, in order."""
    book = script.load(code)
    ch = next(c for c in book["chapters"] if c["chapter"] == chapter)
    return [dict(line, key=castmod.voice_key(line, code)) for line in ch["lines"]]


def _close_pairs() -> set:
    """Voice pairs too alike for the voice check to tell apart (reference clips 0.6 alike or
    more: the narrator and Elisha, Daniel, Stephen, Hezekiah, the Holy Spirit, Cain), from
    .cache/voices/close-pairs.json (written with abible.voicecheck's embeddings). They are
    never put in one request: each speaks in a request of its own, where Fish can't swap them."""
    global _CLOSE
    if _CLOSE is None:
        f = paths.VOICES / "close-pairs.json"
        _CLOSE = {frozenset(p[:2]) for p in json.loads(f.read_text(encoding="utf-8"))["pairs"]} if f.exists() else set()
    return _CLOSE


_CLOSE = None


def _too_many(seen: set, vid: str) -> bool:
    """Whether a request already holding voices `seen` can't take voice `vid` too."""
    if vid in seen:
        return False
    return (MAX_VOICES and len(seen) >= MAX_VOICES) or any(frozenset((vid, u)) in _close_pairs() for u in seen)


def scenes(lines: list, voices: dict) -> list:
    """Split lines into requests within Fish's reference-audio limit and MAX_CHARS.
    A chapter is cut only where the narrator speaks: it is grouped into beats
    (a narrator line and the speeches after it), and beats are packed in order."""
    beats = []
    for line in lines:
        if voices[line["key"]][0] is None:
            raise SystemExit(f"no voice for {line['key']!r} yet (cast it first, or use placeholders)")
        if not beats or line["who"] == "narrator" and beats[-1][-1]["who"] != "narrator":
            beats.append([])
        beats[-1].append(line)
    if MAX_VOICES:
        # A beat with more voices than a request may carry ("And they said ... And he
        # said ..." with no narrator between) is cut where the next voice would be one
        # too many: 118 requests carried 3 or 4 voices before this
        split = []
        for beat in beats:
            part, seen = [], set()
            for line in beat:
                vid = voices[line["key"]][0]
                if part and _too_many(seen, vid):
                    split.append(part)
                    part, seen = [], set()
                part.append(line)
                seen.add(vid)
            split.append(part)
        beats = split
    out, cur, ids, chars = [], [], {}, 0
    for beat in beats:
        need = {voices[l["key"]][0]: voices[l["key"]][1] for l in beat}
        if sum(need.values()) > fish.MAX_REFERENCE_SECONDS:
            raise SystemExit(f"one beat needs {sum(need.values()):.0f} s of reference audio: "
                             f"{[l['key'] for l in beat]}")
        merged = {**ids, **need}
        size = sum(len(l["text"]) for l in beat)
        if cur and (sum(merged.values()) > fish.MAX_REFERENCE_SECONDS or chars + size > MAX_CHARS
                    or (MAX_VOICES and len(merged) > MAX_VOICES)
                    or any(_too_many(set(ids), v) for v in need)):
            out.append(cur)
            cur, ids, chars = [], {}, 0
            merged = dict(need)
        cur += beat
        ids, chars = merged, chars + size
    if cur:
        out.append(cur)
    return out


def speakable(text: str) -> str:
    """Fish reads words in parentheses as a direction, not as text, and leaves them out
    (Deuteronomy 6:15, Hebrews 10:23 in the first KJV render): the brackets go, and a
    comma marks the aside where nothing else does."""
    if "(" not in text and ")" not in text:
        return text
    text = re.sub(r"(?<=[\w’'])\s*\(", ", ", text)  # "Casluhim (of whom" -> "Casluhim, of whom"
    text = re.sub(r"\)(?=\s*[\w‘'])", ",", text)    # "Philistines) and" -> "Philistines, and"
    text = text.replace("(", "").replace(")", "")
    text = re.sub(r",(\s*[,;:.?!])", r"\1", text)    # a comma next to other stops: keep the other
    text = re.sub(r"([;:.?!])\s*,", r"\1", text)
    return re.sub(r"\s{2,}", " ", text).strip()


def request_text(lines: list, voices: dict, deliveries=True):
    """The text Fish reads (speaker tags, delivery cues, phoneme tags) and the voice id list."""
    order, parts, prev = [], [], None
    for line in lines:
        vid, _, aged = voices[line["key"]]
        if vid not in order:
            order.append(vid)
        cue = []
        if deliveries and delivery_cue(line.get("delivery")):
            cue.append(delivery_cue(line["delivery"]))
        if aged in AGE_CUE:
            cue.append(AGE_CUE[aged])
        text = lexicon.apply(speakable(line["text"]))
        tag = f"<|speaker:{order.index(vid)}|>" if vid != prev or cue else ""
        parts.append(tag + "".join(cue) + (" " if cue else "") + text)
        prev = vid
    text = " ".join(parts)
    if len(order) == 1:  # one voice: no speaker tags (measured harmless, but not needed)
        text = re.sub(r"<\|speaker:\d+\|>", "", text)
    return text, order


def body_for(text, ids):
    # normalize_loudness (the S2 default, stated so it can't change under us) evens out each
    # request's level; quality-guard is Fish's own check on a synthesis (free, no slower: 2026-10-09).
    return {"text": text, "reference_id": ids if len(ids) > 1 else ids[0], "format": "mp3", "mp3_bitrate": 128,
            "latency": "normal", "normalize": True, "chunk_length": 300, "temperature": 0.7, "top_p": 0.7,
            "prosody": {"speed": SPEED, "normalize_loudness": True}, "features": ["quality-guard"]}


def cache_key(body, attempt=0) -> str:
    raw = json.dumps({"model": fish.MODEL, "body": body, "attempt": attempt}, sort_keys=True)
    return hashlib.sha256(raw.encode()).hexdigest()[:24]


def as_sent(text: str) -> str:
    """A line as Fish is sent it (respellings, no parentheses), with each phoneme-tagged
    word as a bare mark: Fish's timestamps give those as "phoneme start ... end", which
    collapse_phonemes folds to the same mark, and norm() drops it on both sides."""
    return re.sub(r"<\|phoneme_start\|>.*?<\|phoneme_end\|>", " • ", lexicon.apply(speakable(text)))


def coverage(lines, words) -> float:
    """Share of the script's words that Fish's timed words account for."""
    import difflib
    # hyphenated names ("Beth-dagon") come back from Fish as two words: compare word parts
    a = [norm(w) for line in lines for w in re.split(r"[\s\-]+", as_sent(line["text"])) if norm(w)]
    b = [norm(p) for w in words for p in re.split(r"[\s\-]+", w["text"]) if norm(p)]
    matched = sum(n for _, _, n in difflib.SequenceMatcher(None, a, b, autojunk=False).get_matching_blocks())
    return matched / max(1, len(a))


MIN_COVERAGE = 0.97  # a stream can end early with no error: once, 50 words of a 500-word scene

# The hearing gate (abible.gate, QA environment): a function (lines, mp3, words, voices) ->
# verdict {"faults", "names", "listen", "voices"}; CHECKS_VERSION is the gate's fingerprint.
# When set, a take is accepted only with no faults; one with faults is kept as
# <key>.unheard.mp3 with its verdict, and Fish is asked for another take, up to ATTEMPTS.
# Fish's word timestamps can't show a skipped word (it stamps those too), so only a
# recognizer can. Words heard as the same other words in two takes are no slip a new take
# would mend (the recognizer's blind spot, "every whit whole" -> "white hole", or Fish's
# own reading of a word: a lexicon matter): they don't turn a take down, and stay in its
# verdict as "persistent" for a person to hear. The two takes may be of the scene a halved
# scene came from: a half keeps that evidence (render_scenes). A verdict made by other
# checks than the current ones is no verdict: the take, accepted or turned down, is heard
# again.
HEARING = None
CHECKS_VERSION = None
ATTEMPTS = 2


def _fault_id(f: dict) -> tuple:
    return f.get("fault"), f.get("verse"), f.get("script"), f.get("heard_in_place"), f.get("heard_extra")


def _write_json(path, value):
    """Write next to `path`, then move into place: a run killed mid-write leaves the old file."""
    tmp = path.with_name(path.name + ".partial")
    tmp.write_text(json.dumps(value, ensure_ascii=False, indent=1), encoding="utf-8")
    tmp.replace(path)


def _verdict(path) -> dict | None:
    """A take's verdict if the current checks made it (one cut short is no verdict)."""
    if not path.exists():
        return None
    try:
        v = json.loads(path.read_text(encoding="utf-8"))
    except ValueError:
        return None
    return v if v.get("version") == CHECKS_VERSION else None


def _hear(lines, mp3, words, voices, path) -> dict:
    v = HEARING(lines, mp3, words, voices) | {"version": CHECKS_VERSION}
    _write_json(path, v)
    return v


# Faults a new take may not mend: the same words heard the same other way in two takes
# (a recognizer's blind spot, or the voice's own reading), or the same extra words heard
# twice (a recognizer's habit at a pause, or Fish's own). Accepted, and listed for an ear.
PERSISTENT_KINDS = ("changed", "added")


def _acceptable(verdict: dict, seen: set) -> list | None:
    """The faults to accept as persistent if every fault is one, else None."""
    faults = verdict["faults"]
    persistent = [f for f in faults if f.get("fault") in PERSISTENT_KINDS and _fault_id(f) in seen]
    return persistent if len(persistent) == len(faults) else None


def render_scene(lines, voices, deliveries=True, attempts=None, seen=None):
    """A take of these lines that passes the hearing gate: (cache key, mp3, words).
    `seen` holds the faults of the takes turned down before (this scene's, and those of
    the scene it was halved from); the faults of every take turned down here are added to
    it, so the caller can hand them on to the halves (render_scenes)."""
    text, ids = request_text(lines, voices, deliveries)
    body = body_for(text, ids)
    paths.ensure(paths.RENDERS)
    seen = set() if seen is None else seen
    for attempt in range(attempts or ATTEMPTS):
        key = cache_key(body, attempt)
        mp3, meta = paths.RENDERS / f"{key}.mp3", paths.RENDERS / f"{key}.json"
        failed = paths.RENDERS / f"{key}.incomplete.mp3"
        unheard = paths.RENDERS / f"{key}.unheard.mp3"
        verdict_path = paths.RENDERS / f"{key}.heard.json"
        if failed.exists():
            continue  # this take came back short; don't ask again
        if unheard.exists():
            if HEARING is None:
                continue
            words = collapse_phonemes(json.loads(meta.read_text(encoding="utf-8"))["words"])
            # turned down by other checks: hear it again with these. Turned down by these,
            # it is weighed again all the same: the takes before it may now hold the same
            # faults (the scene it was halved from), which make them persistent
            verdict = _verdict(verdict_path) or _hear(lines, unheard, words, voices, verdict_path)
            persistent = _acceptable(verdict, seen)
            if persistent is not None:  # the same rule as a take heard the first time
                if persistent and verdict.get("persistent") != persistent:
                    _write_json(verdict_path, verdict | {"persistent": persistent})
                unheard.rename(mp3)
                return key, mp3, words
            seen.update(_fault_id(f) for f in verdict["faults"])
            continue
        if not mp3.exists():
            audio, words = fish.tts_with_timestamps(body)
            _write_json(meta, {"words": words, "text": text, "ids": ids})
            tmp = mp3.with_name(mp3.name + ".partial")
            tmp.write_bytes(audio)
            tmp.replace(mp3)
        words = collapse_phonemes(json.loads(meta.read_text(encoding="utf-8"))["words"])
        if coverage(lines, words) < MIN_COVERAGE:
            mp3.rename(failed)  # keep the evidence; the next attempt has its own key
            continue
        if HEARING is None:
            return key, mp3, words
        verdict = _verdict(verdict_path) or _hear(lines, mp3, words, voices, verdict_path)
        persistent = _acceptable(verdict, seen)
        if persistent is not None:
            if persistent and verdict.get("persistent") != persistent:
                _write_json(verdict_path, verdict | {"persistent": persistent})
            return key, mp3, words
        seen.update(_fault_id(f) for f in verdict["faults"])
        mp3.rename(unheard)
    raise Incomplete(f"no take of a scene starting {lines[0]['text'][:40]!r} passed the gate or came back whole")


class Incomplete(fish.FishError):
    pass


def collapse_phonemes(words):
    """Fish's timings list a phoneme-tagged word as 'phoneme start EH1 ... phoneme end':
    fold those back into one word."""
    out, i = [], 0
    while i < len(words):
        w = words[i]
        if w["text"].lower() == "phoneme" and i + 1 < len(words) and words[i + 1]["text"].lower() == "start":
            j = i + 2
            while j + 1 < len(words) and not (words[j]["text"].lower() == "phoneme" and words[j + 1]["text"].lower() == "end"):
                j += 1
            out.append({"text": "•", "start": w["start"], "end": words[min(j + 1, len(words) - 1)]["end"]})
            i = j + 2
        else:
            out.append(w)
            i += 1
    return out


def norm(w: str) -> str:
    """A word for comparing the script with Fish's timed words; a phoneme-tagged word is
    "•" on both sides (as_sent, collapse_phonemes), so a line of one ("Selah.") is timed."""
    return "•" if "•" in w else re.sub(r"[^a-z0-9]", "", w.lower())


def line_times(lines, words):
    """Start and end of each line: the script's words are aligned with Fish's
    timed words (they differ now and then: hyphens, phoneme tags), and each
    line spans its first to last aligned word."""
    import difflib
    script_words, owner = [], []
    for i, line in enumerate(lines):
        for w in re.split(r"[\s\-]+", as_sent(line["text"])):
            if norm(w):
                script_words.append(norm(w))
                owner.append(i)
    timed = [(norm(p), w) for w in words for p in re.split(r"[\s\-]+", w["text"]) if norm(p)]
    words = [w for _, w in timed]
    sm = difflib.SequenceMatcher(None, script_words, [t for t, _ in timed], autojunk=False)
    first, last = {}, {}
    for a, b, n in sm.get_matching_blocks():
        for k in range(n):
            i = owner[a + k]
            first.setdefault(i, words[b + k]["start"])
            last[i] = words[b + k]["end"]
    out = []
    for i in range(len(lines)):
        out.append((first.get(i), last.get(i)))
    return out


# Glyssen's delivery notes, as Fish cues: only clear acted moments (decision D24). Laments
# and songs are read as heartfelt speech, not sobbed or sung; notes about manner or staging
# ("rebuking", "preaching", "to crowd") get no cue, the words carry them. Each note is
# looked up whole: the first table searched inside words, so "sing" in "blessing" gave
# God's blessing in Genesis 1:28 "[singing softly]" and "rage" in "discouraged" made John
# the Baptist's question angry. A note not listed gets no cue.
_CUES = {
    "[weeping]": ["weeping", "wailing", "crying", "sobbing", "weeping, pleading"],
    "[calling out loudly]": ["shouting", "calling out", "calling out from the sky", "calling out in anguish", "loud",
                             "loud voice", "loud (thundering)", "screaming", "shrieking"],
    "[whispering]": ["whispering"],
    "[prayerful]": ["praying", "prayer", "interceding"],
    "[angry]": ["angry", "furious", "outraged", "quarreling", "angry, quarreling"],
    "[mocking]": ["mocking", "sneering", "insulting", "taunting", "jeering"],
    "[trembling, afraid]": ["afraid", "terrified", "fearful", "frightened", "frightened (probably drunk)",
                            "trembling", "trembled"],
    "[pleading]": ["pleading", "begging", "desperate", "entreating"],
    "[laughing]": ["laughing"],
    "[quietly, to himself]": ["thought", "thinking", "thinking or talking to himself", "said in his heart",
                              "said to himself", "speaking or thinking to himself", "say in your heart", "to themselves"],
    "[amazed]": ["amazed", "astonished", "surprised"],
    "[weak, strained]": ["critically wounded", "dying", "faint"],
}
DELIVERY = {note: cue for cue, notes in _CUES.items() for note in notes}


def delivery_cue(note):
    return DELIVERY.get(note.strip().lower()) if note else None


def decode(mp3, rate=44100):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(mp3), "-f", "s16le", "-ac", "1", "-ar", str(rate), "-"],
                         capture_output=True, check=True).stdout
    return raw


class Unsplittable(fish.FishError):
    pass


def split_scene(scene: list) -> list:
    """Two halves, cut before the narrator line nearest the middle; in a scene with no
    such line (a psalm, one speaker throughout), at the line nearest the middle."""
    if len(scene) < 2:
        raise Unsplittable(f"a scene of one line keeps failing: {scene[0]['text'][:60]!r}")
    cuts = [i for i in range(1, len(scene)) if scene[i]["who"] == "narrator" and scene[i - 1]["who"] != "narrator"]
    cuts = cuts or list(range(1, len(scene)))
    mid = min(cuts, key=lambda i: abs(i - len(scene) / 2))
    return [scene[:mid], scene[mid:]]


MAX_HALVINGS = 2  # then the fault is the voice's or the text's, not the take's (Revelation 16:7's altar)


def render_scenes(scene, voices, deliveries=True, depth=0, seen=frozenset()):
    """Render a scene; when Fish refuses its references as too long, or keeps
    stopping early (both seen with library voices, whose real reference audio
    is longer than it looks), or no take passes the hearing gate, halve it and
    try each half, at most MAX_HALVINGS times.

    Each half starts from the faults heard in the takes of the scene it came from
    (`seen`, which has no line numbers, so a fault is the same in a half): a word heard
    the same other way in a take of the whole and again in a take of the half is as
    persistent as one heard so in two takes of the half. 1 Chronicles 4:10-43 (some 60
    rare names) failed for want of it: "Shiphi" heard as "shippi" in all four takes of
    the scene and in the first take of its half, which was that take's only fault. Each
    half gets its own copy: what one half's takes heard is no evidence for the other."""
    own = set(seen)
    try:
        return [(scene, render_scene(scene, voices, deliveries=deliveries, seen=own))]
    except fish.FishError as e:
        if "Reference audio too long" not in str(e) and not isinstance(e, Incomplete):
            raise
        if depth >= MAX_HALVINGS:
            raise Unsplittable(f"still failing after {depth} halvings: {e}") from e
        out = []
        for half in split_scene(scene):
            out += render_scenes(half, voices, deliveries, depth + 1, frozenset(own))
        return out


MAX_VOICE_GAIN_DB = 12.0


def level_voices(pcm: bytes, timeline: list, rate=44100):
    """Bring every voice to the chapter's common speech level, so no character
    is quieter than another. Each voice gets one gain per scene (Fish request),
    from the median loudness of its lines (speech frames only), so a character
    still whispers and shouts within that level. Gains are capped at 12 dB and
    eased in and out over 30 ms at line edges. Returns (pcm, {voice: dB})."""
    import numpy as np

    x = np.frombuffer(pcm, dtype=np.int16).astype(np.float32)
    frame = rate // 100  # 10 ms

    def speech_level(a, b):
        seg = x[int(a * rate):int(b * rate)]
        if len(seg) < frame * 10:
            return None
        rms = np.sqrt(np.mean(seg[: len(seg) // frame * frame].reshape(-1, frame) ** 2, axis=1))
        loud = rms[rms > 0.1 * rms.max()]
        return float(np.median(loud)) if len(loud) else None

    # one gain per voice per scene: each Fish request is levelled on its own, so the same
    # voice can sit a little higher in one scene than in the next
    by_voice = {}
    for t in timeline:
        lvl = speech_level(t["start"], t["end"])
        if lvl:
            by_voice.setdefault((t.get("scene", 0), t["voice"]), []).append(lvl)
    if len(by_voice) < 2:
        return pcm, {}  # one voice in one scene: nothing to balance
    voice_level = {v: float(np.median(ls)) for v, ls in by_voice.items()}
    target = float(np.median([l for ls in by_voice.values() for l in ls]))
    gains = {v: float(np.clip(20 * np.log10(target / l), -MAX_VOICE_GAIN_DB, MAX_VOICE_GAIN_DB))
             for v, l in voice_level.items()}
    env = np.ones_like(x)
    for t in timeline:
        g = 10 ** (gains.get((t.get("scene", 0), t["voice"]), 0.0) / 20)
        env[int(t["start"] * rate):int(t["end"] * rate)] = g
    ramp = int(0.03 * rate)
    env = np.convolve(env, np.ones(ramp) / ramp, mode="same")  # no clicks at line edges
    y = np.clip(x * env, -32768, 32767).astype(np.int16)
    return y.tobytes(), {k: round(g, 1) for k, g in gains.items()}


# Between two requests: each take's own silence at either end is cut to EDGE, and GAP put
# between. With up to 2 voices a request, a dialogue chapter has many joins; untrimmed, a
# join was a 0.65 s pause (median) against 0.30 s for a change of speaker inside a request.
EDGE, GAP = 0.12, 0.35


def _edges(pcm: bytes, rate=44100) -> tuple:
    """Seconds of silence (10 ms frames under -45 dBFS) at the start and the end of a take."""
    import array
    samples = array.array("h")
    samples.frombytes(pcm)
    frame = rate // 100
    quiet = 32768 * 10 ** (-45 / 20)

    def loud(i):
        chunk = samples[i * frame:(i + 1) * frame]
        return chunk and (sum(x * x for x in chunk) / len(chunk)) ** 0.5 > quiet

    n = len(samples) // frame
    lead = next((i for i in range(n) if loud(i)), n)
    tail = next((i for i in range(n) if loud(n - 1 - i)), n)
    return lead / 100, tail / 100


def _write_checked(path, data: bytes | None = None, encode_from: bytes | None = None, seconds: float | None = None):
    """Write next to `path` and move into place only when whole: a run killed mid-write
    (the machine slept at 2:33 on 2026-10-12) left Exodus 29 cut at 5:28 of 7:58."""
    tmp = path.with_name(path.stem + ".partial" + path.suffix)
    if encode_from is not None:
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "s16le", "-ar", "44100", "-ac", "1", "-i", "-",
                        "-af", "loudnorm=I=-18:TP=-1.5:LRA=11", "-ar", "44100", "-c:a", "libmp3lame", "-b:a", "96k",
                        str(tmp)], input=encode_from, check=True)
        got = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0",
                                    str(tmp)], capture_output=True, text=True, check=True).stdout.strip())
        if abs(got - seconds) > 0.2:
            raise RuntimeError(f"{tmp.name}: {got:.2f} s encoded, {seconds:.2f} s expected")
    else:
        tmp.write_bytes(data)
    tmp.replace(path)


def plan_id(code: str, chapter: int, voices: dict | None = None) -> str:
    """A fingerprint of the chapter's plan: its requests' text and voices as scenes() and
    request_text() give them now. A chapter rendered from this plan, halved scenes and
    all, carries it; one rendered from another (lexicon, cues, voices, grouping) doesn't."""
    voices = voices or voice_map()
    plan = [request_text(sc, voices) for sc in scenes(spoken_lines(code, chapter), voices)]
    return hashlib.sha256(json.dumps(plan, ensure_ascii=False).encode("utf-8")).hexdigest()[:16]


def render_chapter(code: str, chapter: int, placeholders: dict | None = None, deliveries=True, tag="") -> dict:
    voices = voice_map(placeholders)
    lines = spoken_lines(code, chapter)
    parts, timeline, offset, placed = [], [], 0.0, []
    gap = b"\x00\x00" * int(44100 * GAP)
    renders = []
    done = [pair for sc in scenes(lines, voices) for pair in render_scenes(sc, voices, deliveries)]
    for n, (scene, (key, mp3, words)) in enumerate(done):
        renders.append(key)
        pcm = decode(mp3)
        lead, tail = _edges(pcm)
        cut_lead = max(0.0, lead - EDGE) if n > 0 else 0.0
        cut_tail = max(0.0, tail - EDGE) if n < len(done) - 1 else 0.0
        a, b = int(cut_lead * 44100) * 2, len(pcm) - int(cut_tail * 44100) * 2
        pcm = pcm[a:b]
        # where the take sits in the chapter: a time t in the take is offset + t - cut_lead
        placed.append({"render": key, "offset": round(offset, 3), "cut_lead": round(cut_lead, 3),
                       "cut_tail": round(cut_tail, 3), "seconds": round(len(pcm) / 2 / 44100, 3),
                       "verses": [line.get("v") for line in scene]})
        for line, t in zip(scene, line_times(scene, words)):
            if t[0] is None:
                continue
            timeline.append({k: line.get(k) for k in ("v", "kind", "who", "key")} |
                            {"voice": voices[line["key"]][0], "scene": n,
                             "start": round(max(0.0, t[0] - cut_lead) + offset, 3),
                             "end": round(max(0.0, t[1] - cut_lead) + offset, 3)})
        parts.append(pcm)
        offset += len(pcm) / 2 / 44100
        parts.append(gap)
        offset += len(gap) / 2 / 44100
    out_dir = paths.OUT / ("kjv" + tag)
    paths.ensure(out_dir)
    name = f"{code}.{chapter}"
    pcm, gains = level_voices(b"".join(parts[:-1]), timeline)
    for t in timeline:
        t["gain_db"] = gains.get((t.get("scene", 0), t["voice"]), 0.0)
    duration = round(len(pcm) / 2 / 44100, 3)
    _write_checked(out_dir / f"{name}.mp3", encode_from=pcm, seconds=duration)
    verses, seen = [], set()
    for t in timeline:
        if t.get("v") is not None and t["v"] not in seen:
            seen.add(t["v"])
            verses.append([str(t["v"]), t["start"]])
    timing = {"verses": verses, "duration": duration, "lines": timeline, "renders": renders,
              "title": books.announcement(code, chapter), "scenes": placed, "edges": {"edge": EDGE, "gap": GAP},
              "plan": plan_id(code, chapter, voices)}
    _write_checked(out_dir / f"{name}.json", data=json.dumps(timing, ensure_ascii=False, indent=0).encode("utf-8"))
    return timing
