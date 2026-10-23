"""Words of the script that were never spoken, or spoken otherwise (run with the QA
environment).

Fish can leave text out and still return timestamps for it, so its own timestamps
can't show it (the render's coverage gate passed Romans 5, where verse 17 is missing).
Found in the KJV render (2026-10-10): asides in parentheses left out (Fish reads
"(...)" as a direction), short narrator lead-ins between two speakers ("And Ahab
said unto Obadiah,") skipped, stretches of long lists skipped, single words dropped
("And Jesus [said] unto him", John 9:37), and now and then a phrase spoken as
gibberish (John 7:22-23).

1. The take or chapter is transcribed (Whisper medium.en, batched, word timestamps;
   kept in .cache/asr/ for later checks) and aligned with the script word for word.
   Words are compared exactly, after the spelling table (qa.SAME) and true homophones
   (qa.HOMOPHONES); numbers by value ("four thousand and five hundred" is "4,500");
   only names may match by their sound skeleton.
2. Every difference is heard again by large-v3 from a clip around it, and settled only
   when the script's words are heard *in their place*: the clip must match more of the
   script around them with them than without them (in_place). The first versions
   accepted a word heard anywhere in the clip, then one joined to a neighbour; both let
   a dropped word pass on another copy of it nearby ("Verily, verily" said once; the
   first "not" of Matthew 25:45 dropped).
3. Words still not heard in place are heard once more primed with the script before
   them. What fits then is recorded for a person's ear ("listen"), not a fault; words
   with nothing heard in their place are never primed (a primed recognizer can supply
   words that were never said).
4. Words heard that the script doesn't have, including at the very start or end of a
   take and inside a stretch heard otherwise, are "added" when the clip matches more
   of the script with them inserted than without (a repeated "drink drink" too).

Each item says which line (index in the lines given), verse and speaker, what the script
has and what was heard. Faults are "missing", "changed" and "added"; "names" (a name
heard as another sound) and "listen" are for a person's ear. Writes
.cache/out/<translation>/omissions.json.
"""
import difflib
import json
import math
import re
import sys
from functools import lru_cache
from pathlib import Path

from . import books, paths, qa, render

ASR = paths.CACHE / "asr"
# The first-pass transcription's settings: a transcript cached under other settings is not reused
TRANSCRIBE = {"model": "medium.en", "batch_size": 16, "beam_size": 5, "language": "en"}
SETTINGS = "-".join(str(v) for v in TRANSCRIBE.values())
SECONDS_PER_WORD = 0.45
# Script words either side of a difference that the clip is compared over
NEIGHBOURS = 4
# A run of this many words or more may lose one in ten to the recognizer and still count
LONG_RUN = 5
# Words heard that the script doesn't have: a single one only if it could change the text
# (a lone "the" or "and" is the recognizer's habit far more often than Fish's)
FILLER = set("the a an and of to in that uh um oh ah #2 #4".split())


def _batched():
    if "batched" not in qa._models:
        from faster_whisper import BatchedInferencePipeline
        qa._models["batched"] = BatchedInferencePipeline(model=qa.model(TRANSCRIBE["model"]))
    return qa._models["batched"]


def heard(translation: str, key: str) -> list:
    """The chapter's transcript, cached (a chapter rendered again drops its cache)."""
    return transcript(paths.OUT / translation / f"{key}.mp3", ASR / translation / f"{key}.json")


def take_transcript(mp3: Path) -> list:
    """A take's transcript, cached by the take's key (whether it is <key>.mp3 or
    <key>.unheard.mp3) and the transcription settings."""
    key = Path(mp3).name.split(".")[0]
    cache = ASR / "renders" / SETTINGS / f"{key}.json"
    older = ASR / "renders" / f"{key}.json"  # made with these same settings before they were named
    if not cache.exists() and older.exists() and SETTINGS == "medium.en-16-5-en":
        return json.loads(older.read_text(encoding="utf-8"))
    return transcript(mp3, cache)


@lru_cache(maxsize=1)
def common_words() -> frozenset:
    """Every word the KJV writes in lower case somewhere: "and", "lord", "god", "i"...
    A capitalised word not in it is a name ("Eleazar", "Selah", "Job"). "O" is not."""
    out = set()
    for _, rel, _, _ in books.BOOKS:
        # whole hyphenated words: the "edom" of "Obed-edom" is not a lower-case word
        for w in re.findall(r"[A-Za-z’'][A-Za-z’'-]*", (paths.REPO / rel).read_text(encoding="utf-8")):
            if w[:1].islower():
                out.update(p.strip("’'") for p in w.lower().split("-") if p)
    out.update({"i", "o"})
    return frozenset(out)


def is_name(word: str) -> bool:
    """A word of the script that is a proper name (each part of a hyphenated one)."""
    parts = [p.strip(".,;:?!()‘’'\"") for p in word.split("-")]
    parts = [p for p in parts if p]
    return bool(parts) and parts[0][:1].isupper() and all(p.lower() not in common_words() for p in parts)


PIECE, STEP = 15.0, 12.0
JITTER = 0.25  # how far the two pieces' times for one word can differ


def clip_text(mp3, start: float, end: float) -> str:
    """What large-v3 hears from `start` to `end`, in overlapping 15-second pieces: given
    one long clip, Whisper sometimes writes its first sentence and stops (Lamentations 3
    and 1 John 1 looked empty after their announcements that way). Each word once: where
    two pieces overlap, each gives the words of the half further from its edge, and a word
    both give by the cut is kept once. Written twice, an overlap's words made a word Fish
    dropped there look heard ("able to go" said "able go" in Numbers 1:38)."""
    words, t = [], start
    while True:
        piece = qa.transcribe(mp3, "large-v3", t, min(PIECE, max(end - t, 3.0)))
        if words:
            cut = t + (PIECE - STEP) / 2
            words = [w for w in words if w["start"] < cut]
            tail = words[-4:]
            for w in piece:
                if w["start"] < cut - JITTER or any(
                        qa.norm(x["text"]) == qa.norm(w["text"]) and abs(x["start"] - w["start"]) < JITTER for x in tail):
                    continue
                words.append(w)
        else:
            words = list(piece)
        if t + PIECE >= end:
            return " ".join(w["text"] for w in words)
        t += STEP


def _script(lines: list):
    """The lines' comparison keys, each with the word it starts at, that word's position
    (line, word) and its line."""
    keys, source, place, owner = [], [], [], []
    for i, line in enumerate(lines):
        raw, raw_src = [], []
        for n, w in enumerate(line["text"].split()):  # a hyphenated name stays one word ("Maaleh-acrabbim")
            for t in qa._tokens(w):
                raw.append(t)
                raw_src.append((w, (i, n)))
        k, first = qa.comparable(raw)
        keys += k
        source += [raw_src[f][0] for f in first]
        place += [raw_src[f][1] for f in first]
        owner += [i] * len(k)
    return keys, source, place, owner


def _heard(words: list):
    """The transcript's comparison keys, each with the heard word it starts at."""
    raw, ref = [], []
    for w in words:
        for t in qa._tokens(w["text"]):
            raw.append(t)
            ref.append(w)
    k, first = qa.comparable(raw)
    return k, [ref[f] for f in first]


def _clip_keys(clip: str, ctx_keys: list, ctx_names: list) -> list:
    """The clip's comparison keys; a word that sounds like one of the context's names
    (same skeleton) takes that name's key, unless it is a word of the context or one the
    KJV writes in lower case: "is" is not "Jesus" (Matthew 1:16), nor "in" "John"
    (Matthew 3:1), nor "for" "Pharaoh", and taken as them it hid the name's loss and
    made the word itself unheard."""
    names = {qa._skeleton(k): k for k, n in zip(ctx_keys, ctx_names) if n}
    own, common = set(ctx_keys), common_words()
    raw = [t if not names or t in own or t in common else names.get(qa._skeleton(t), t) for t in qa._tokens(clip)]
    return qa.comparable(raw)[0]


def _matched(a: list, b: list) -> int:
    return sum(m.size for m in difflib.SequenceMatcher(None, a, b, autojunk=False).get_matching_blocks())


def in_place(script: list, names: list, lo: int, hi: int, clip: str, span: tuple | None = None,
             need: int | None = None) -> tuple:
    """Whether script[lo:hi] is heard in the clip in its place: the clip matches more of the
    script around it with those words than without them, by `need` (default all of them,
    or nine in ten for a long run, whose words the clip doesn't match audit then checks
    one run at a time). `span` is the stretch of script compared, which must cover all the
    clip holds: words from the clip's other copies of the same phrase must have their own
    place in it ("my son, my son" said once in 2 Samuel 18:33). Default: NEIGHBOURS words
    each side. Returns (heard, words matched)."""
    a0, a1 = span or (max(0, lo - NEIGHBOURS), min(len(script), hi + NEIGHBOURS))
    ctx = script[a0:a1]
    clip_keys = _clip_keys(clip, ctx, names[a0:a1])
    g0, g1 = lo - a0, hi - a0
    gain = _matched(ctx, clip_keys) - _matched(ctx[:g0] + ctx[g1:], clip_keys)
    n = g1 - g0
    if need is None:
        need = n if n < LONG_RUN else math.ceil(0.9 * n)
    return gain >= need, max(0, gain)


def _added(script: list, names: list, i1: int, extra: list, clip: str, span: tuple | None = None) -> str:
    """Words heard between script[i1-1] and script[i1] that the script doesn't have:
    "added" when the clip matches more of the script with them inserted there than
    without (at the start or end of a take too), "fine" when it matches no more,
    otherwise "unsure". `span` as for in_place."""
    a0, a1 = span or (max(0, i1 - NEIGHBOURS), min(len(script), i1 + NEIGHBOURS))
    ctx = script[a0:a1]
    clip_keys = _clip_keys(clip, ctx, names[a0:a1])
    at = i1 - a0
    gain = _matched(ctx[:at] + extra + ctx[at:], clip_keys) - _matched(ctx, clip_keys)
    if gain >= max(1, math.ceil(0.8 * len(extra))):
        return "added"
    return "fine" if gain <= 0 else "unsure"


def transcript(mp3: Path, cache: Path | None = None) -> list:
    """Whisper medium.en, batched, word timestamps; cached in `cache` when given."""
    if cache is not None and cache.exists():
        try:
            return json.loads(cache.read_text(encoding="utf-8"))
        except ValueError:
            pass  # cut short by an interrupted run: transcribe again
    segs, _ = _batched().transcribe(qa.pcm16k(mp3), batch_size=TRANSCRIBE["batch_size"], word_timestamps=True,
                                    language=TRANSCRIBE["language"], beam_size=TRANSCRIBE["beam_size"])
    words = [{"text": w.word.strip(), "start": round(float(w.start), 2), "end": round(float(w.end), 2)}
             for s in segs for w in s.words]
    if cache is not None:
        paths.ensure(cache.parent)
        tmp = cache.with_name(cache.name + ".partial")
        tmp.write_text(json.dumps(words, ensure_ascii=False), encoding="utf-8")
        tmp.replace(cache)
    return words


def chapter_spans(translation: str, code: str, chapter: int, lines: list) -> list:
    """Each spoken line's (start, end) in the chapter's recording, from its timing file
    (Fish's own word times; None for a line it has none for)."""
    timeline = json.loads((paths.OUT / translation / f"{code}.{chapter}.json").read_text(encoding="utf-8"))["lines"]
    spans, j = [], 0
    for line in lines:
        if j < len(timeline) and all(line.get(k) == timeline[j].get(k) for k in ("v", "kind", "key")):
            spans.append((timeline[j]["start"], timeline[j]["end"]))
            j += 1
        else:
            spans.append((None, None))
    return spans


def check_chapter(translation: str, code: str, chapter: int) -> dict:
    key = f"{code}.{chapter}"
    lines = render.spoken_lines(code, chapter)
    return audit(lines, paths.OUT / translation / f"{key}.mp3", heard(translation, key),
                 chapter_spans(translation, code, chapter, lines))


def _window(got, j1, j2, owner_lines, spans, n_words):
    """The stretch of audio a difference is heard in: the words either side of it by the
    transcript's times, and the lines concerned by Fish's (the batched transcript can drop
    a stretch and misplace what's around it: it had "Ecclesiastes, chapter 11" at 23.8 s,
    said at 0)."""
    before = got[j1 - 1]["start"] if j1 > 0 else 0.0
    after = got[j2]["end"] if j2 < len(got) else (got[-1]["end"] if got else 0.0)
    starts, ends = [before], [after]
    for i in owner_lines:
        if spans and spans[i][0] is not None:
            starts.append(spans[i][0])
            ends.append(spans[i][1])
    start = max(0.0, min(starts) - 1.5)
    end = max(max(ends) + 1.5, start + 3.0 + SECONDS_PER_WORD * n_words)
    return before, start, min(end, start + 120.0)


def _names_heard_otherwise(script, source, i1, i2, heard_keys) -> bool:
    """A stretch of names said otherwise, not shortened or lengthened: every script word a
    name, and the heard words sound like them all (joined consonant skeletons equal, so
    "Obed-edom" heard "Obeidodom" counts and a name dropped from a list doesn't)."""
    if not all(is_name(source[k]) for k in range(i1, i2)):
        return False
    names_said = len({source[k] for k in range(i1, i2)})
    if len(heard_keys) < names_said:
        return False  # fewer words heard than names: one may be missing
    a, b = _loose("".join(script[i1:i2])), _loose("".join(heard_keys))
    return a == b or difflib.SequenceMatcher(None, a, b, autojunk=False).ratio() >= 0.75


def _loose(word: str) -> str:
    """A name's consonants with voiced and voiceless pairs merged ("Ephes-dammim" heard
    "ephastamim")."""
    sk = qa._skeleton(word)
    return sk[:1] + sk[1:].translate(str.maketrans("dbgv", "tpkf"))


def audit(lines: list, mp3: Path, words: list, spans: list | None = None) -> dict:
    """{"missing", "changed", "added": faults; "names", "listen": for a person's ear}.
    `spans` is each line's (start, end) by Fish's timings, to place the clips."""
    script, source, place, owner = _script(lines)
    names = [is_name(w) for w in source]

    def words_of(i1, i2):  # the script's own words for keys i1..i2, each word once
        seen, out_ = set(), []
        for k in range(i1, i2):
            if place[k] not in seen:
                seen.add(place[k])
                out_.append(source[k])
        return " ".join(out_)
    toks, got = _heard(words)
    sm = difflib.SequenceMatcher(None, script, toks, autojunk=False)
    out = {"missing": [], "changed": [], "added": [], "names": [], "listen": []}
    clips = {}

    def heard_at(start, end):  # each clip heard once a take: a stretch's runs share its lines' clips
        key = (round(start, 2), round(end, 2))
        if key not in clips:
            clips[key] = clip_text(mp3, start, end)
        return clips[key]

    first_of, last_of = {}, {}
    for k, o in enumerate(owner):
        first_of.setdefault(o, k)
        last_of[o] = k + 1

    def span(i1, i2):  # the lines the clip covers, and a few words either side
        lo_line, hi_line = owner[min(i1, len(owner) - 1)], owner[min(max(i2 - 1, i1), len(owner) - 1)]
        return (max(0, first_of[lo_line] - NEIGHBOURS), min(len(script), last_of[hi_line] + NEIGHBOURS))

    def about(i):
        line = lines[owner[min(i, len(owner) - 1)]] if owner else {}
        return {"line": owner[min(i, len(owner) - 1)] if owner else None, "verse": line.get("v"), "who": line.get("who")}

    def check_added(i1, extra, j1, j2, clip=None):
        if len(extra) < 2 and all(t in FILLER for t in extra):
            return
        before, start, end = _window(got, j1, j2, [owner[min(i1, len(owner) - 1)]] if owner else [], spans, 0)
        clip = clip or heard_at(start, end)
        item = about(i1) | {"words": len(extra), "heard_extra": " ".join(extra), "heard_there": clip, "at": round(before, 2)}
        verdict = _added(script, names, i1, extra, clip, span(max(0, i1 - 1), i1 + 1))
        if verdict == "added":
            out["added"].append(item)
        elif verdict == "unsure":
            out["listen"].append(item | {"why": "words heard that the script doesn't have; the clip can't settle it"})

    def hear(i1, i2, clip, sp, need=None):
        """Whether script[i1:i2] is in place in the clip, or in a clip from each of its
        lines' own start: (heard, words matched, the clip that matched most)."""
        ok, n_hit = in_place(script, names, i1, i2, clip, sp, need)
        if not ok and spans:
            # Whisper can stop at a pause and write nothing after it ("... for ever. Amen."
            # and not Romans 16's colophon after it): hear again from each line's own start
            for i in sorted({owner[k] for k in range(i1, i2)}):
                s0, e0 = spans[i]
                if s0 is not None:
                    again = heard_at(max(0.0, s0 - 0.3), max(e0, s0) + 1.0)
                    ok2, n2 = in_place(script, names, i1, i2, again, sp, need)
                    if ok2 or n2 > n_hit:
                        clip, ok, n_hit = again, ok2, n2
                    if ok:
                        break
        return ok, n_hit, clip

    def unheard(i1, i2, clip, sp):
        """The runs of script[i1:i2] the clip doesn't match, each with the clip's words
        in their place."""
        a0, a1 = sp
        ctx = script[a0:a1]
        keys = _clip_keys(clip, ctx, names[a0:a1])
        runs = []
        for op, x1, x2, y1, y2 in difflib.SequenceMatcher(None, ctx, keys, autojunk=False).get_opcodes():
            k1, k2 = max(a0 + x1, i1), min(a0 + x2, i2)
            if op in ("replace", "delete") and k1 < k2:
                # of a difference reaching past the stretch, the clip's words for its part of it
                share = (y2 - y1) / (x2 - x1)
                runs.append((k1, k2, keys[y1 + round((k1 - a0 - x1) * share):y1 + round((k2 - a0 - x1) * share)]))
        return runs

    def when(k, i1, i2, before, end, j2):
        """About when script word k is said, inside a stretch heard from `before`: by its
        place in its line's span (Fish's timings), else by its share of the stretch up to
        the next word heard."""
        o = owner[k]
        if spans and spans[o][0] is not None:
            s0, e0 = spans[o]
            return s0 + (max(e0, s0) - s0) * (k - first_of[o]) / max(1, last_of[o] - first_of[o])
        nxt = got[j2]["start"] if j2 < len(got) else end
        return before + (max(nxt, before) - before) * (k - i1) / max(1, i2 - i1)

    def settle(item, i1, i2, spoken, clip, n_hit, before, start, end, sp, need=None, prime_first=False):
        """script[i1:i2], not heard in place: "missing" when nothing was heard there;
        otherwise heard once more, primed with the script before it, from a clip centred
        on it (the recognizers expect modern English: "every whit whole" is "every white
        hole" unprimed): a person's to hear if it fits then, else "changed". A gap at the
        start of a take is primed with its own line, the only text there is.
        `prime_first`: primed before it is called missing too."""
        item |= {"heard_there": clip, "heard_in_order": n_hit, "at": round(before, 2)}
        if not spoken and not prime_first:
            out["missing"].append(item)
            return
        context = " ".join(source[max(0, i1 - 30):i1]) or lines[owner[i1]]["text"]
        p0 = max(start, before - 6.0)
        segs, _ = qa.model("large-v3").transcribe(qa.pcm16k(mp3, p0, min(end - p0, PIECE)), beam_size=5,
                                                 language="en", initial_prompt=context or None,
                                                 condition_on_previous_text=False)
        primed = " ".join(x.text.strip() for x in segs)
        item["primed"] = primed
        if in_place(script, names, i1, i2, primed, sp, need)[0]:
            out["listen"].append(item | {"why": "heard in place only when primed with the text"})
        else:
            out["changed" if spoken else "missing"].append(item)

    for op, i1, i2, j1, j2 in sm.get_opcodes():
        if op == "equal":
            continue
        if op == "insert":
            check_added(i1, toks[j1:j2], j1, j2)
            continue
        if qa._joined(" ".join(script[i1:i2])) == qa._joined(" ".join(toks[j1:j2])):
            continue  # the same letters, split or joined otherwise ("to day", "today")
        item = about(i1) | {"to_verse": lines[owner[i2 - 1]].get("v"), "kind": lines[owner[i1]].get("kind"),
                            "words": i2 - i1, "script": words_of(i1, i2),
                            "heard_in_place": " ".join(toks[j1:j2])}
        if op == "replace" and _names_heard_otherwise(script, source, i1, i2, toks[j1:j2]):
            out["names"].append(item)  # a name said otherwise: pronunciation, for the name reel
            continue
        spoken_something = (j2 - j1) >= 0.5 * (i2 - i1)
        before, start, end = _window(got, j1, j2, sorted({owner[i1], owner[i2 - 1]}), spans, i2 - i1)
        sp = span(i1, i2)
        ok, n_hit, clip = hear(i1, i2, heard_at(start, end), sp)
        if ok:
            if n_hit < i2 - i1:
                # nine in ten of a long stretch heard in place: the recognizer can slip on a
                # word of a long clip, and Fish can drop one there too. The words the clip
                # doesn't match are each checked like any other difference, as if the first
                # pass had heard the rest
                runs = unheard(i1, i2, clip, sp)
                if sum(k2 - k1 for k1, k2, _ in runs) < (i2 - i1) - n_hit:
                    # fewer words unmatched inside than the clip falls short by: the clip
                    # matched one outside in their stead (the "Verily" before a stretch that
                    # starts with the one dropped from "Verily, verily"): a person's to hear
                    out["listen"].append(item | {"heard_there": clip, "heard_in_order": n_hit, "at": round(before, 2),
                                                 "why": "a long stretch heard in place but for words the clip can't place"})
                for k1, k2, there in runs:
                    sub = about(k1) | {"to_verse": lines[owner[k2 - 1]].get("v"), "kind": lines[owner[k1]].get("kind"),
                                       "words": k2 - k1, "script": words_of(k1, k2), "heard_in_place": " ".join(there)}
                    if there and _names_heard_otherwise(script, source, k1, k2, there):
                        out["names"].append(sub)
                        continue
                    ok_k, n_k, clip_k = hear(k1, k2, clip, sp, need=k2 - k1)
                    if not ok_k:
                        # Only large-v3 has heard these words: primed before they are called
                        # missing, a second opinion the first pass gives elsewhere
                        settle(sub, k1, k2, len(there) >= 0.5 * (k2 - k1), clip_k, n_k, when(k1, i1, i2, before, end, j2),
                               start, end, sp, need=k2 - k1, prime_first=True)
            # the script's words are there; anything heard beyond them in this stretch is
            # checked as words the script doesn't have
            if op == "replace" and j2 - j1 > i2 - i1:
                # one at a time: a heard word the script doesn't have shows as one more word
                # matched when it is put back at its place ("the LORD [God] spake", heard
                # "Lord God spoke": "spoke" is the recognizer's "spake", "God" is extra)
                a0, a1 = span(i1, i2)
                ctx = script[a0:a1]
                clip_keys = _clip_keys(clip, ctx, names[a0:a1])
                base = _matched(ctx, clip_keys)
                for t in dict.fromkeys(toks[j1:j2]):
                    if t in FILLER or t in script[i1:i2]:
                        continue
                    if any(_matched(ctx[:at - a0] + [t] + ctx[at - a0:], clip_keys) > base for at in range(i1, i2 + 1)):
                        out["added"].append(about(i1) | {"words": 1, "heard_extra": t, "heard_there": clip,
                                                         "at": round(got[j1]["start"], 2) if j1 < len(got) else 0.0})
            continue
        settle(item, i1, i2, spoken_something, clip, n_hit, before, start, end, sp)
    return out


def run(translation: str = "kjv", codes=None, again: bool = False) -> dict:
    report_path = paths.OUT / translation / "omissions.json"
    report = json.loads(report_path.read_text(encoding="utf-8")) if report_path.exists() else {}
    for code in codes or books.CODES:
        for p in sorted((paths.OUT / translation).glob(f"{code}.*.mp3"), key=lambda p: int(p.name.split(".")[1])):
            key = p.name[:-4]
            if key in report and report[key].get("version") == checks_version() and not again:
                continue
            report[key] = check_chapter(translation, code, int(key.split(".")[1])) | {"version": checks_version()}
            faults = report[key]["missing"] + report[key]["changed"] + report[key]["added"]
            if faults:
                print(key, [(o["verse"], o.get("script") or o.get("heard_extra")) for o in faults], flush=True)
            report_path.write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding="utf-8")
    return report


def checks_version() -> str:
    from . import gate
    return gate.CHECKS_VERSION


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    run(codes=args or None, again="--again" in sys.argv)
