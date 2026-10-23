"""Verse timings where Fish's own timestamps went wrong. Now and then Fish stamps a
stretch of words all at one moment (Romans 5:18, Daniel 5:25-28 in the KJV), so a line
seems to end before it starts or a verse to start after the next one. Or it stamps the
words after a long one late and catches up with a run of words at one instant, so the
lines between start 1.5-4 s late and run too fast (Genesis 12:17: sixteen words in
2.18 s, starting 3.82 s after it is spoken). The chapter's MP3 is transcribed with word
timestamps, the script aligned with what was heard, and each line whose Fish timings
look wrong (suspect) re-timed from the recording itself. The audio is not touched.

A line's start is where the voice resumes after the longest pause from just before its
first heard word to that word's end (onset: the recognizer's starts after a pause aren't
reliable, nor its word ends where a pause follows).
A verse Fish left out altogether is marked where the next verse starts.

A new time is kept only when it is plausible: the recognizer can skip words too, and a
line placed by the ones it heard can land far from where it is spoken (Job 39:4, on
2026-10-11: the first fourteen of its twenty words were not heard, at the edge of the
recognizer's 30-second window, and the verse was moved from Fish's right 24.475 s to
30.18 s, ending 1.16 s later; Acts 23:25, on 2026-10-13: the recognizer missed verses 23
and 24 and stretched "wrote" over them, 227.27-242.97 s, and the verse was moved from
Fish's right 248.634 s to 226.95 s, verses 23 and 24 piled onto it). The line's first
word must have been heard (as itself, or as another word in its place), it and the
line's last heard word must look sound (timingcheck.MIN_WORD to MAX_WORD long), the new
start must be within NEAR of the first, and the line must run no faster than
MIN_SECONDS_PER_WORD and no slower than MAX_SECONDS_PER_WORD. Nor may a new start fall
before an earlier line left alone or after a later one. Changes turned down are listed
in the report and kept in <translation>/retime-rejected.json; verses that still start
after the next one are listed in the report.

Re-timing starts from Fish's own timings, kept in <chapter>.fish-timing.json once a
chapter has been re-timed, so running it again does not build on an earlier re-timing.
Run with the QA environment; with --cached it uses the chapter's cached transcript
(.cache/asr/kjv/, or its takes' in .cache/asr/renders/ moved to where each take starts:
timingcheck.transcripts) and runs on the CPU.
"""
import difflib
import json
import os
import sys

import numpy as np

from . import paths, qa, render, timingcheck

# A line is re-timed when Fish's times are inconsistent, or its start is further than this from
# the heard one. Fish's failures are far off (10 s and more); the recognizer's start for the
# first word after a pause can be 1-3 s early (Psalm 119's letters), so smaller gaps are left.
FAR = 5.0
# Fish speaks a word in 0.3-0.4 s: a line given less than this a word was placed by too few of its words
MIN_SECONDS_PER_WORD = 0.15
# Fish gives no line of the KJV more than 1.1 s a word (2026-10-13): a line given more was
# placed by a word the recognizer stretched over what it didn't transcribe
MAX_SECONDS_PER_WORD = 1.0
NEAR = 1.0  # seconds a new start may be from the span of the line's first word as heard
# Words Fish stamped within 50 ms of each other, in a row, near a line's start
# (timingcheck.stamped_together): 7 at the start of Genesis 11:17 (2.2 s late), 8 where
# Genesis 12:17 (3.8 s late) ends and 18 starts
STAMPED = 4


def suspect(lines: list, texts: list | None = None, fish: list | None = None) -> dict:
    """{line index: why} for the lines whose Fish timings can't be right: ending before they
    start or after the next line starts, or starting before the line ahead of them; with
    `texts` (each line's script text) running faster than MIN_SECONDS_PER_WORD; with `fish`
    (Fish's words in chapter time, timingcheck.fish_words) starting next to STAMPED or more
    words Fish stamped at one instant."""
    bad = {}
    for i, t in enumerate(lines):
        nxt = lines[i + 1] if i + 1 < len(lines) else None
        words = len(qa._tokens(texts[i])) if texts else 0
        together = timingcheck.stamped_together(fish, t["start"]) if fish else 0
        if t["end"] < t["start"]:
            bad[i] = "ends before it starts"
        elif i and t["start"] < lines[i - 1]["start"]:
            bad[i] = "starts before the line ahead of it"
        elif nxt and nxt["start"] < t["start"]:
            bad[i] = "starts after the next line"
        elif nxt and t["end"] > nxt["start"]:
            bad[i] = f"ends {t['end'] - nxt['start']:.2f} s after the next line starts"
        elif t["end"] - t["start"] < MIN_SECONDS_PER_WORD * words:
            bad[i] = f"{words} words in {t['end'] - t['start']:.2f} s"
        elif together >= STAMPED:
            bad[i] = f"next to {together} words Fish stamped together"
    return bad


def onset(pcm: np.ndarray, rate: int, heard_start: float, word_end: float) -> float:
    """Where the voice resumes after the longest pause (50 ms or more well below the
    chapter's speech level) from 0.3 s before the recognizer's start for the first heard
    word to that word's end; the recording's start counts as the longest. The last pause
    before the word's end, taken until 2026-10-13, is often inside the word or the one
    after it, which the recognizer gives to the word ("Psalm" heard 0.0-0.58 s in Psalm 58,
    its 60 ms pause before "fifty" at 0.51 s, the line spoken from 0.06 s); and the
    recognizer's start after a pause is often early, inside the pause or the last sound of
    the word before ("And" at 148.49 s in Genesis 12:17, spoken from 148.72 s). Against
    Fish's starts where they are within 0.5 s of the recognizer's (11,198 lines in 362
    chapters): 1.9% more than 0.15 s apart and 0.2% more than 0.3 s, against 6.1% and 2.8%
    by the last pause."""
    hop = rate // 100
    frames = len(pcm) // hop
    rms = np.sqrt(np.mean(pcm[: frames * hop].reshape(frames, hop) ** 2, axis=1) + 1e-12)
    quiet = rms < 0.1 * np.median(rms[rms > np.percentile(rms, 30)])
    lo, hi = max(0, int((heard_start - 0.3) * 100)), min(int(word_end * 100), frames - 1)
    best, i = None, lo  # (frames, last frame) of the longest pause so far; the later of two as long
    while i <= hi:
        if not quiet[i]:
            i += 1
            continue
        j = i
        while j < hi and quiet[j + 1]:
            j += 1
        if j - i + 1 >= 5 and (best is None or (float("inf") if i == 0 else j - i + 1) >= best[0]):
            best = (float("inf") if i == 0 else j - i + 1), j
        i = j + 1
    return (best[1] + 1) / 100 if best else heard_start


def align(texts: list, heard: list) -> dict:
    """The script's lines aligned with a transcript: {line index: {"any": its first word heard
    as itself, "last": its last word heard as itself, "end": that word's end, "first": the
    word heard in place of its first word (itself or another; None when nothing was),
    "words": its word count}} for every line with a word heard as itself."""
    script, owner, first_token = [], [], {}
    for i, text in enumerate(texts):
        for w in qa._tokens(text):
            first_token.setdefault(i, len(script))
            script.append(w)
            owner.append(i)
    got = [w for w in heard for _ in qa._tokens(w["text"])]
    toks = [t for w in heard for t in qa._tokens(w["text"])]
    lines = {}
    for op, i1, i2, j1, _ in difflib.SequenceMatcher(None, script, toks, autojunk=False).get_opcodes():
        if op == "equal":
            for k in range(i2 - i1):
                i, w = owner[i1 + k], got[j1 + k]
                h = lines.setdefault(i, {"any": None, "first": None})
                if h["any"] is None:
                    h["any"] = w
                if first_token[i] == i1 + k:
                    h["first"] = w
                h["last"], h["end"] = w, w["end"]
        elif op == "replace" and first_token[owner[i1]] == i1:
            lines.setdefault(owner[i1], {"any": None, "first": None})["first"] = got[j1]
    words = {i: len(qa._tokens(text)) for i, text in enumerate(texts)}
    return {i: h | {"words": words[i]} for i, h in lines.items() if h["any"] is not None}


def _sound(w: dict) -> bool:
    """A heard word of a length the recognizer gives a word it placed with sense: not one
    stretched over what it didn't transcribe, nor one of a run it stamped at one instant."""
    return timingcheck.MIN_WORD <= w["end"] - w["start"] < timingcheck.MAX_WORD


def implausible(start: float, end: float, words: int, first: dict | None, last: dict | None = None) -> str | None:
    """Why a line's new start and end can't be right, or None. `first`: the word that places
    the start (heard in place of the line's first word); `last`: the word that places the end."""
    if first is None:
        return "its first word was not heard"
    for what, w in (("first", first), ("last", last)):
        if w is not None and not _sound(w):
            return f"its {what} word as heard, {w['text']!r}, lasts {w['end'] - w['start']:.2f} s ({w['start']}-{w['end']})"
    if end - start < MIN_SECONDS_PER_WORD * words or end - start > MAX_SECONDS_PER_WORD * words:
        return f"{words} words in {end - start:.2f} s"
    off = start - first["end"] if start > first["end"] else (first["start"] - start if start < first["start"] else 0.0)
    if off > NEAR:
        return f"{off:.2f} s from its first word as heard ({first['start']}-{first['end']})"
    return None


def out_of_place(lines: list, new: dict) -> dict:
    """{line index: why} for the new starts (`new`: {line index: start}) that fall before the
    start of an earlier line left alone or after a later one's."""
    alone = [j for j in range(len(lines)) if j not in new]
    out = {}
    for i, start in new.items():
        before = [j for j in alone if j < i and lines[j]["start"] > start]
        after = [j for j in alone if j > i and lines[j]["start"] < start]
        if before or after:
            b = lines[max(before, key=lambda j: lines[j]["start"]) if before else min(after, key=lambda j: lines[j]["start"])]
            out[i] = (f"starts {'before an earlier' if before else 'after a later'} line left alone "
                      f"(v{b['v']} {b['who']}, {b['start']} s)")
    return out


def plan(timeline: list, texts: list, heard: list, pcm: np.ndarray, rate: int = 16000,
         fish: list | None = None) -> dict:
    """The re-timing of a chapter's lines, from Fish's timeline, each line's script text, a
    transcript of the chapter, its audio (mono, `rate`) and Fish's words in chapter time
    (for suspect): {"lines": the timeline with the changes, "changed", "rejected",
    "not_heard", "heard_verses", "agree" (how far the lines left alone are from where they
    were heard)}."""
    lines = [dict(line) for line in timeline]
    by = align(texts, heard)
    bad = suspect(lines, texts, fish)
    items, new, why_not, agree = {}, {}, {}, []
    for i, line in enumerate(lines):
        h = by.get(i)
        if h is None:
            continue
        off = h["any"]["start"] - line["start"]
        if i not in bad and abs(off) <= FAR:
            agree.append(abs(off))
            continue
        w = h["first"] or h["any"]
        start, end = round(onset(pcm, rate, w["start"], w["end"]), 3), round(h["end"], 3)
        items[i] = {"v": line["v"], "who": line["who"], "suspect": bad.get(i, f"starts {-off:+.2f} s from where it was heard"),
                    "fish": [line["start"], line["end"]], "heard": [start, end]}
        why = implausible(start, end, h["words"], h["first"], h["last"])
        if why:
            why_not[i] = why
        else:
            new[i] = start, end
    # A line placed by words heard elsewhere can land past lines whose Fish timings stand
    # (Acts 23:25 at 226.95 s, before verse 23's 229.254 s). Each change undone leaves
    # another line alone, which can bound the others: again until none moves
    while undo := out_of_place(lines, {i: s for i, (s, _) in new.items()}):
        for i, why in undo.items():
            why_not[i] = why
            del new[i]
    for i, (start, end) in new.items():
        lines[i]["start"], lines[i]["end"] = start, end
    # A line next to a moved one keeps Fish's end, which ran to the late start: it ends where
    # the next line now starts (Luke 23:40, the narrator's line 2.09 s into the criminal's)
    for i in sorted(new):
        for j, k in ((i - 1, i), (i, i + 1)):
            if 0 <= j and k < len(lines) and lines[j]["end"] > lines[k]["start"]:
                items.setdefault(i, {}).setdefault("ends_trimmed", []).append(
                    {"v": lines[j]["v"], "who": lines[j]["who"], "end": [lines[j]["end"], max(lines[j]["start"], lines[k]["start"])]})
                lines[j]["end"] = max(lines[j]["start"], lines[k]["start"])
    return {"lines": lines, "changed": [items[i] for i in sorted(new)],
            "rejected": [items[i] | {"why": why_not[i]} for i in sorted(why_not)], "agree": agree,
            "not_heard": [line["v"] for i, line in enumerate(lines) if i not in by],
            "heard_verses": {str(line["v"]) for i, line in enumerate(lines) if i in by and line.get("v") is not None}}


def first_starts(timeline: list) -> list:
    """[[verse, the start of its first line]], in the timeline's order."""
    verses, seen = [], set()
    for line in timeline:
        if line.get("v") is not None and line["v"] not in seen:
            seen.add(line["v"])
            verses.append([str(line["v"]), line["start"]])
    return verses


def verse_starts(timeline: list) -> tuple[list, list]:
    """(verses as the timing file has them, the verses moved): a verse that starts after the
    next one is marked where the next one starts (a verse Fish left out has its start from
    words of it Fish matched elsewhere)."""
    verses = first_starts(timeline)
    moved = []
    for k in range(len(verses) - 2, -1, -1):
        if verses[k][1] > verses[k + 1][1]:
            moved.append([verses[k][0], verses[k][1]])
            verses[k][1] = verses[k + 1][1]
    return verses, moved[::-1]


def _write(path, text: str) -> None:
    """Write next to `path` and move into place whole: a run stopped mid-write leaves the
    file as it was (render._write_checked)."""
    tmp = path.with_name(path.name + ".partial")
    tmp.write_text(text, encoding="utf-8")
    os.replace(tmp, path)


def _record_rejected(out, key: str, rejected: list) -> None:
    path = out / "retime-rejected.json"
    kept, damaged = {}, False
    if path.exists():
        try:
            kept = json.loads(path.read_text(encoding="utf-8"))
        except ValueError as e:  # cut short when it was written in place (before 2026-10-13)
            damaged = True
            print(f"{path.name} unreadable ({e}): written again with this chapter only", file=sys.stderr)
        if not isinstance(kept, dict):
            if not damaged:
                print(f"{path.name} is not a chapter map: written again with this chapter only", file=sys.stderr)
            kept, damaged = {}, True
    if rejected:
        kept[key] = rejected
    elif kept.pop(key, None) is None and not damaged:
        return
    _write(path, json.dumps(kept, ensure_ascii=False, indent=1))


def _texts(code: str, chapter: int, timeline: list) -> list:
    """Each timeline line's script text: the timeline is the spoken lines that were heard in
    Fish's timestamps, in order."""
    texts, j = [], 0
    for line in render.spoken_lines(code, chapter):
        if j < len(timeline) and all(line.get(k) == timeline[j].get(k) for k in ("v", "kind", "key")):
            texts.append(line["text"])
            j += 1
    if j != len(timeline):
        raise SystemExit(f"{code} {chapter}: the script no longer matches the timeline")
    return texts


def _fish_words(t: dict) -> tuple:
    """(Fish's words in chapter time, None), or (None, why) when a take's timings are gone:
    the other checks still stand."""
    try:
        return timingcheck.fish_words(t), None
    except (OSError, ValueError, KeyError) as e:
        return None, f"{type(e).__name__}: {e}"


def needs(code: str, chapter: int, translation: str = "kjv") -> dict:
    """suspect() on a chapter's timing file as it stands, with every trigger (each line's
    text, Fish's words): {line index: why}, empty when nothing needs re-timing."""
    t = json.loads((paths.OUT / translation / f"{code}.{chapter}.json").read_text(encoding="utf-8"))
    return suspect(t["lines"], _texts(code, chapter, t["lines"]), _fish_words(t)[0])


def retime(code: str, chapter: int, translation: str = "kjv", write: bool = True, heard: list | None = None) -> dict:
    """`heard`: a transcript of the chapter (word dicts with start and end) to use instead of
    transcribing it now."""
    out = paths.OUT / translation
    path = out / f"{code}.{chapter}.json"
    backup = out / f"{code}.{chapter}.fish-timing.json"
    t = json.loads(path.read_text(encoding="utf-8"))
    fish = json.loads(backup.read_text(encoding="utf-8")) if backup.exists() else t
    if fish.get("renders") != t.get("renders"):
        raise SystemExit(f"{code} {chapter}: {backup.name} is of another render")
    timeline = fish["lines"]
    texts = _texts(code, chapter, timeline)
    fish_words, no_words = _fish_words(fish)
    mp3 = out / f"{code}.{chapter}.mp3"
    if heard is None:
        heard = qa.transcribe(mp3)
    p = plan(timeline, texts, heard, qa.pcm16k(mp3), fish=fish_words)
    starts = [s for _, s in first_starts(p["lines"])]  # before verse_starts puts them in order
    verses, moved = verse_starts(p["lines"])
    at = dict(verses)
    agree = p["agree"]
    report = {"chapter": f"{code} {chapter}", "changed": p["changed"], "rejected": p["rejected"],
              # every verse that starts after the next one, marked where that starts; of those,
              # the ones not heard at all either (as when Fish left one out)
              "out_of_order": [{"v": v, "start": s, "marked_at": at[v], "heard": v in p["heard_verses"]}
                               for v, s in moved],
              "unheard_verses": [v for v, _ in moved if v not in p["heard_verses"]],
              "lines_not_heard": p["not_heard"], "in_order": starts == sorted(starts),
              "agreement_s": {"lines": len(agree), "median": round(float(np.median(agree)), 3) if agree else None,
                              "p90": round(float(np.percentile(agree, 90)), 3) if agree else None}}
    if no_words:
        report["stamped_together_not_checked"] = no_words
    if write:
        # A chapter re-timed before is written again from Fish's timings, even with nothing
        # to change now: an earlier change this run turns down is undone
        if p["changed"] or "retimed" in t:
            if not backup.exists():
                _write(backup, path.read_text(encoding="utf-8"))
            t["lines"], t["verses"] = p["lines"], verses
            if p["changed"]:
                t["retimed"] = {"how": "whisper medium.en word ends, voice onset after the longest pause",
                                "lines": [c["v"] for c in p["changed"]]}
            else:
                t.pop("retimed", None)
            _write(path, json.dumps(t, ensure_ascii=False, indent=0))
        _record_rejected(out, f"{code}.{chapter}", p["rejected"])
    return report


def cached(key: str) -> list:
    """A KJV chapter's cached transcript: its own (omissions) when it is not older than the
    MP3, else its takes' (the hearing gate's) moved to where each take starts."""
    t = json.loads((timingcheck.OUT / f"{key}.json").read_text(encoding="utf-8"))
    found = dict(timingcheck.transcripts(key, t)[0])
    return found.get("chapter") or found.get("takes") or found["take"]


if __name__ == "__main__":
    for arg in (a for a in sys.argv[1:] if not a.startswith("--")):
        code, ch = arg.split(".")
        print(json.dumps(retime(code, int(ch), write="--dry" not in sys.argv,
                                heard=cached(arg) if "--cached" in sys.argv else None),
                         ensure_ascii=False, indent=1))
