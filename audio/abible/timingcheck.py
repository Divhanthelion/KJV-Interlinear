"""Do a chapter's files agree with each other and with what was heard? On the CPU, from
the transcripts already cached (no recognizer is run, nothing is changed):

    python -m abible.timingcheck                       both checks, every chapter
    python -m abible.timingcheck --integrity [KEY...]  only the files
    python -m abible.timingcheck --timing [KEY...]     only the verse starts (KEY: JOB.39)

1. Integrity (integrity.json): every MP3 is read to its end (ffprobe counts its frames,
   ffmpeg decodes it) and its length compared with its timing file's "duration", and
   that with the takes the chapter was joined from. A redo run stopped while ffmpeg was
   writing left Exodus 29 cut at 327.65 s, beside the timing file of the whole 477.70 s
   (render.render_chapter writes the MP3, then the timing file). For a chapter whose
   timing file records its "scenes", each take's MP3 less what was cut from either end
   must be the length it was joined at; a scene with no cut_tail recorded (before
   2026-10-13) is listed as not checked. Every verse of the KJV text must have a line in
   the timing file, and the verse starts must be in order and inside the recording.
2. Timing (timingcheck.json): every verse start is compared with where the recognizer
   heard the verse's first words. The script is aligned (difflib, qa._tokens) with the
   chapter's transcript (.cache/asr/kjv/, omissions) or, for a chapter joined from
   several takes, with each take's transcript (.cache/asr/renders/, the hearing gate)
   moved to where the take starts in the chapter (offset_check_s: how far Fish's first
   line of a take is from where the takes' measured lengths put it). A verse whose start
   is more than OFF from the span of its first heard word is reported. The recognizer
   has its own faults (words it misplaces or stretches over what it didn't transcribe),
   so a word places a verse only when it looks sound (see MAX_WORD); when the first word
   doesn't, the verse must start between the word heard before it and the end of the
   first of its words that does. Each verse reported carries the other transcript's view
   where there is one (the same model on nearly the same audio, so it confirms little)
   and how many words Fish stamped at one instant near it (Fish's own failure, and the
   usual cause).
"""
import concurrent.futures as cf
import difflib
import json
import statistics
import subprocess
import sys

from . import books, kjv, paths, qa, render, script

OUT = paths.OUT / "kjv"
ASR = paths.CACHE / "asr"
TOLERANCE = 0.5  # seconds an MP3 may differ from its timing file
TAKES_TOLERANCE = 0.05  # the takes and their gaps add up to the timing file's duration to the millisecond
# A take as joined ("seconds") is its MP3 less what was cut at either end, to the millisecond
# (ffprobe's length and the decoded samples agree within 1 ms: Genesis 11 and 12, 2026-10-13)
SCENE_TOLERANCE = 0.01
GAP = int(44100 * 0.45) / 44100  # the breath render.render_chapter puts between takes
OFF = 1.0  # a verse start further than this from its first heard word is reported
FIRST_WORDS = 5  # a verse none of whose first five words was heard soundly is not placed
WRITTEN_AFTER = 60.0  # an MP3 newer than its timing file by more than this: they may not belong together


def chapters(keys=None) -> list:
    if keys:
        return [(k.split(".")[0], int(k.split(".")[1])) for k in keys]
    return [(code, ch["chapter"]) for code in books.CODES for ch in script.load(code)["chapters"]]


def probe(path) -> dict:
    """An MP3's length by its header and by counting its frames (a file cut short can keep
    a header that claims the whole), or why ffprobe could not read it."""
    r = subprocess.run(["ffprobe", "-v", "error", "-count_packets", "-show_entries",
                        "format=duration:stream=nb_read_packets,sample_rate", "-of", "json", str(path)],
                       capture_output=True, text=True)
    try:
        info = json.loads(r.stdout)
        stream = info["streams"][0]
        rate = int(stream["sample_rate"])
        out = {"header_s": round(float(info["format"]["duration"]), 3),
               "frames_s": round(int(stream["nb_read_packets"]) * (1152 if rate >= 32000 else 576) / rate, 3)}
    except (ValueError, KeyError, IndexError, TypeError):
        return {"error": r.stderr.strip() or f"ffprobe exit {r.returncode}, no duration"}
    if r.stderr.strip():
        out["error"] = r.stderr.strip()
    return out


def decode_errors(path) -> str:
    """What ffmpeg reports decoding the whole file (empty: none)."""
    r = subprocess.run(["ffmpeg", "-v", "error", "-i", str(path), "-f", "null", "-"], capture_output=True, text=True)
    return (r.stderr.strip() or ("" if r.returncode == 0 else f"ffmpeg exit {r.returncode}"))[:500]


def take_lengths(renders: list) -> list:
    return [probe(paths.RENDERS / f"{k}.mp3").get("header_s") for k in renders]


def integrity_one(code: str, ch: int) -> tuple[str, dict, list]:
    """(key, measurements, problems) for one chapter."""
    key = f"{code}.{ch}"
    mp3, tf = OUT / f"{key}.mp3", OUT / f"{key}.json"
    m, problems = {}, []
    if not tf.exists():
        return key, m, ["no timing file"]
    try:
        t = json.loads(tf.read_text(encoding="utf-8"))
    except ValueError as e:
        return key, m, [f"timing file unreadable: {e}"]
    d = m["timing_s"] = t["duration"]
    if not mp3.exists():
        problems.append("no MP3")
    else:
        m |= probe(mp3)
        errors = decode_errors(mp3)
        if errors:
            m["decode_errors"] = errors
        if "header_s" not in m:
            problems.append(f"ffprobe can't read the MP3: {m.get('error')}")
        else:
            for what, s in (("header", m["header_s"]), ("frames", m["frames_s"])):
                if abs(s - d) > TOLERANCE:
                    problems.append(f"MP3 {s:.2f} s by its {what}, timing file {d:.2f} s ({s - d:+.2f})")
            if m.get("error"):
                problems.append(f"ffprobe: {m['error'][:200]}")
        if errors:
            problems.append(f"decoding: {errors[:200]}")
        newer = mp3.stat().st_mtime - tf.stat().st_mtime
        if newer > WRITTEN_AFTER:
            m["mp3_newer_s"] = round(newer)
            problems.append(f"MP3 written {newer / 3600:.1f} h after its timing file")
    lengths = take_lengths(t["renders"])
    scenes = t.get("scenes")
    if scenes and [x["render"] for x in scenes] != t["renders"]:
        problems.append("its scenes and its takes disagree")
    elif None in lengths:
        problems.append(f"take(s) missing or unreadable: {[k for k, s in zip(t['renders'], lengths) if s is None]}")
    else:
        if scenes:  # takes trimmed at the joins (render.EDGE) and GAP between them
            m["takes_s"] = round(sum(x["seconds"] for x in scenes) + t["edges"]["gap"] * (len(scenes) - 1), 3)
            # each take as joined: its MP3 less what was cut from either end
            off = [f"scene {n}: take {s:.3f} s less {x['cut_lead']} + {x['cut_tail']} s cut is "
                   f"{s - x['cut_lead'] - x['cut_tail']:.3f} s, joined at {x['seconds']} s"
                   for n, (x, s) in enumerate(zip(scenes, lengths))
                   if "cut_tail" in x and abs(s - x["cut_lead"] - x["cut_tail"] - x["seconds"]) > SCENE_TOLERANCE]
            if off:
                problems.append("takes not as joined: " + "; ".join(off))
            uncut = [n for n, x in enumerate(scenes) if "cut_tail" not in x]
            if uncut:  # timing files written before render.render_chapter recorded it
                m["unchecked"] = f"scene(s) {uncut}: no cut_tail recorded, their takes can't be checked against the join"
        else:
            m["takes_s"] = round(sum(lengths) + GAP * (len(lengths) - 1), 3)
        if abs(m["takes_s"] - d) > TAKES_TOLERANCE:
            problems.append(f"its takes add up to {m['takes_s']:.2f} s, the timing file says {d:.2f} s")
    # Every verse of the text spoken, the verse starts in order and inside the recording
    text = sorted(v for (c, v) in kjv.verses(code) if c == ch)
    timed = {l["v"] for l in t["lines"] if l.get("v") is not None}
    in_script = {l["v"] for c in script.load(code)["chapters"] if c["chapter"] == ch for l in c["lines"]
                 if l.get("v") is not None}
    missing = [v for v in text if v not in timed]
    if missing:
        m["verses_without_lines"] = missing
        problems.append(f"verses with no line in the timing file: {missing}"
                        + ("" if all(v in in_script for v in missing) else
                           f" (not in the script: {[v for v in missing if v not in in_script]})"))
    extra = sorted(timed - set(text))
    if extra:
        problems.append(f"lines for verses the text doesn't have: {extra}")
    labels = [label for label, _ in t["verses"]]
    if sorted(labels, key=int) != [str(v) for v in sorted(timed)]:
        problems.append("the verse list and the lines disagree on which verses there are")
    starts = [s for _, s in t["verses"]]
    if starts != sorted(starts):
        problems.append("verse starts out of order: " + ", ".join(
            f"v{a[0]} {a[1]} > v{b[0]} {b[1]}" for a, b in zip(t["verses"], t["verses"][1:]) if a[1] > b[1]))
    if starts and (min(starts) < 0 or max(starts) > d):
        problems.append(f"verse starts outside the recording (0-{d} s): "
                        f"{[(label, s) for label, s in t['verses'] if s < 0 or s > d]}")
    past = [l["v"] for l in t["lines"] if l["end"] > d + TOLERANCE]
    if past:
        m["lines_past_end"] = past
    if max((l.get("scene", 0) for l in t["lines"]), default=0) + 1 != len(t["renders"]):
        problems.append("scene numbers and takes disagree")
    return key, m, problems


def integrity(keys=None, workers: int = 12) -> dict:
    todo = chapters(keys)
    found, problems = {}, {}
    with cf.ThreadPoolExecutor(workers) as ex:  # each job is ffprobe and ffmpeg, in their own processes
        for key, m, p in ex.map(lambda item: integrity_one(*item), todo):
            found[key] = m
            if p:
                problems[key] = p
    unchecked = {k: m["unchecked"] for k, m in found.items() if m.get("unchecked")}
    report = {"checked": len(todo), "tolerance_s": TOLERANCE, "with_problems": len(problems),
              "problems": problems, "with_unchecked": len(unchecked), "unchecked": unchecked, "chapters": found}
    if not keys:
        (OUT / "integrity.json").write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding="utf-8")
    return report


def scene_offsets(t: dict) -> list:
    """Where each take starts in the chapter: render.render_chapter joins the takes with GAP
    between them, so take n starts after the lengths of the takes before it and n gaps. (Fish
    stamps a take's first word at 0.0, so this is where Fish's timings start scene n's first
    line: they agree to the millisecond in 1 Kings 11, 1 Corinthians 15 and 1 Chronicles 12.)"""
    if t.get("scenes"):  # recorded by render.render_chapter: take time t is at offset + t - cut_lead
        return [x["offset"] - x["cut_lead"] for x in t["scenes"]]
    out, at = [], 0.0
    for s in take_lengths(t["renders"]):
        if s is None:
            raise FileNotFoundError("a take is missing")
        out.append(at)
        at += s + GAP
    return out


def measured_offsets(t: dict) -> tuple[list | None, str | None]:
    """(where each take of a chapter with "scenes" starts in the chapter as scene_offsets has
    it, rebuilt from the take MP3s' lengths less what was cut from them rather than from the
    recorded offsets; or None and why it can't be). A take's cut_tail is recorded from
    2026-10-13 on; the last take's doesn't move any."""
    lengths = take_lengths(t["renders"])
    if None in lengths:
        return None, "a take is missing or unreadable"
    uncut = [n for n, x in enumerate(t["scenes"][:-1]) if "cut_tail" not in x]
    if uncut:
        return None, f"no cut_tail recorded for scene(s) {uncut}"
    gap = int(44100 * t["edges"]["gap"]) / 44100  # whole samples of silence, as render.render_chapter puts them
    out, at = [], 0.0
    for x, s in zip(t["scenes"], lengths):
        out.append(at - x["cut_lead"])
        at += s - x["cut_lead"] - x.get("cut_tail", 0.0) + gap
    return out, None


def _take_transcript(k: str):
    """A take's cached transcript: the hearing gate's, under its transcription settings
    (omissions.take_transcript), or the older file beside them."""
    from . import omissions
    named = ASR / "renders" / omissions.SETTINGS / f"{k}.json"
    return named if named.exists() else ASR / "renders" / f"{k}.json"


def transcripts(key: str, t: dict) -> tuple[list, float | str | None]:
    """The chapter's transcripts in chapter time, [(name, words)], the one to judge by first,
    and the largest difference between where a take starts by the lengths of the takes
    before it (measured_offsets) and Fish's start for its first line, or why that can't be
    found (None: no transcripts of the takes). A chapter of one take is judged by its own
    transcript (omissions) unless that is older than the MP3 (of a version since replaced);
    a chapter of several takes by its takes' (the hearing gate's), each moved to where the
    take starts. The other, where there is one, is a second opinion."""
    mp3, own = OUT / f"{key}.mp3", ASR / "kjv" / f"{key}.json"
    renders = t["renders"]
    out, check = [], None
    if own.exists() and own.stat().st_mtime >= mp3.stat().st_mtime:
        out.append(("chapter", json.loads(own.read_text(encoding="utf-8"))))
    takes = [_take_transcript(k) for k in renders]
    if all(f.exists() for f in takes):
        offsets = [0.0] if len(renders) == 1 else scene_offsets(t)
        backup = OUT / f"{key}.fish-timing.json"
        fish = json.loads(backup.read_text(encoding="utf-8")) if backup.exists() else t
        firsts = [next((l["start"] for l in fish["lines"] if l.get("scene", 0) == n), None) for n in range(len(renders))]
        # Fish's first line of a take starts where the take starts after what was cut from its
        # lead (render.render_chapter: max(0, t - cut_lead) + offset, Fish's first word at 0.0)
        measured, check = (offsets, None) if len(renders) == 1 or not t.get("scenes") else measured_offsets(t)
        if measured is not None:
            leads = [x["cut_lead"] for x in t["scenes"]] if t.get("scenes") else [0.0] * len(renders)
            check = round(max((abs(f - off - lead) for f, off, lead in zip(firsts, measured, leads) if f is not None),
                              default=0.0), 3)
        words = [dict(w, start=round(w["start"] + off, 3), end=round(w["end"] + off, 3))
                 for f, off in zip(takes, offsets) for w in json.loads(f.read_text(encoding="utf-8"))]
        out.insert(0 if len(renders) > 1 else len(out), ("takes" if len(renders) > 1 else "take", words))
    if not out:
        raise FileNotFoundError(f"{key}: no transcript of the chapter or of all its takes")
    return out, check


# A heard word placed by the recognizer this long or longer took in a stretch it didn't
# transcribe ("wrote" 227.27-242.97 s in Acts 23:25), or started early after a pause; one
# with no length sits where the recognizer misplaced a run of words (Leviticus 24:13's
# "the LORD spake unto Moses, saying" all at 138.25 s). Neither places a verse; nor does a
# word whose next script word is such a word, or wasn't heard next, or JUMP or more after it.
MAX_WORD, MIN_WORD, JUMP = 1.2, 0.02, 3.0


def _span(start: float, lo: float, hi: float) -> float:
    """How far `start` is outside [lo, hi] (negative: before it)."""
    return 0.0 if lo <= start <= hi else (start - hi if start > hi else start - lo)


def place(toks: list, by_verse: dict, words: list, verses: list) -> dict:
    """{verse label: where its start is against what was heard, or None (not placed)}."""
    got = [w for w in words for _ in qa._tokens(w["text"])]
    heard_toks = [x for w in words for x in qa._tokens(w["text"])]
    block = [None] * len(toks)
    for op in difflib.SequenceMatcher(None, toks, heard_toks, autojunk=False).get_opcodes():
        for i in range(op[1], op[2]):
            block[i] = op

    def heard_at(x: int, in_place: bool):
        op, i1, _, j1, _ = block[x]
        if op == "equal":
            return j1 + x - i1
        if in_place and op == "replace" and i1 == x:
            return j1  # heard as another word, in its place ("Nashon" for Nahshon)
        return None

    def plausible(w: dict) -> bool:
        return MIN_WORD <= w["end"] - w["start"] < MAX_WORD

    def reliable(x: int, j: int) -> bool:
        """A word placed with sense, and its next script word heard right after it: an "and"
        matched to the "an" of "an Ammonitess" two words before 1 Kings 14:22 is not."""
        w = got[j]
        if not plausible(w):
            return False
        if x + 1 == len(toks):
            return True
        j2 = heard_at(x + 1, True)  # the next word as itself, or as another word in its place
        if j2 is None:
            return False
        n = got[j2]
        if n is w:  # one heard word, several script words ("4,500")
            return True
        return j2 == j + 1 and plausible(n) and 0 <= n["start"] - w["start"] <= JUMP

    out = {}
    for label, start in verses:
        vt = by_verse.get(label)
        if not vt:
            out[label] = None
            continue
        found = None
        for k, x in enumerate(vt[:FIRST_WORDS]):
            j = heard_at(x, k == 0)
            if j is not None and reliable(x, j):
                found = k, got[j]
                break
        if found is None:
            out[label] = None
            continue
        k, w = found
        if k == 0:
            how, lo = ("first word" if block[vt[0]][0] == "equal" else "first word heard as another"), w["start"]
        else:
            # The verse starts after the word heard before its first word, and before the end
            # of the first of its words heard reliably
            op, i1, _, j1, _ = block[vt[0]]
            before = (j1 + vt[0] - i1 if op == "equal" else j1) - 1
            how, lo = f"first placed by word {k + 1}", (got[before]["end"] if before >= 0 else 0.0)
        out[label] = {"off_s": round(_span(start, lo, w["end"]), 2), "heard": w["start"], "heard_end": w["end"],
                      "word": w["text"], "how": how, "window": [round(lo, 2), round(w["end"], 2)]}
    return out


def fish_words(t: dict) -> list:
    """Fish's own word timings for the chapter, in chapter time (phoneme tags folded)."""
    offsets = [0.0] if len(t["renders"]) == 1 else scene_offsets(t)
    out = []
    for k, off in zip(t["renders"], offsets):
        meta = json.loads((paths.RENDERS / f"{k}.json").read_text(encoding="utf-8"))
        out += [dict(w, start=w["start"] + off, end=w["end"] + off) for w in render.collapse_phonemes(meta["words"])]
    return out


def stamped_together(fish: list, at: float, around: float = 3.0) -> int:
    """The most words Fish stamped within 50 ms of the one before, in a row, near `at`: Fish's
    timings fail that way (a stretch of words at one instant, often by a phoneme-tagged name),
    and the verse starts taken from them with it."""
    best = run = 0
    near = [w for w in fish if abs(w["start"] - at) <= around]
    for a, b in zip(near, near[1:]):
        run = run + 1 if b["start"] - a["start"] < 0.05 else 0
        if run:
            best = max(best, run + 1)
    return best


def timing_one(code: str, ch: int) -> dict:
    key = f"{code}.{ch}"
    t = json.loads((OUT / f"{key}.json").read_text(encoding="utf-8"))
    found, offset_check = transcripts(key, t)
    lines = next(c for c in script.load(code)["chapters"] if c["chapter"] == ch)["lines"]
    toks, by_verse = [], {}
    for line in lines:
        for w in qa._tokens(line["text"]):
            if line.get("v") is not None:
                by_verse.setdefault(str(line["v"]), []).append(len(toks))
            toks.append(w)
    (source, words), second = found[0], (found[1] if len(found) > 1 else None)
    placed = place(toks, by_verse, words, t["verses"])
    again = place(toks, by_verse, second[1], t["verses"]) if second else {}
    off, gaps, fish, by_second = [], [], None, []
    for label, start in t["verses"]:
        p, q, judge = placed[label], again.get(label), source
        if p is None and q is not None:  # placed by the other transcript only
            p, q, judge = q, None, second[0]
            by_second.append(label)
        if p is None:
            continue
        if p["how"] == "first word":
            gaps.append(abs(start - p["heard"]))
        if abs(p["off_s"]) > OFF:
            fish = fish if fish is not None else fish_words(t)
            other = second[0] if second and judge == source else None
            off.append({"chapter": key, "v": label, "start": start} | p |
                       {"transcript": judge, "second_opinion": other,
                        "second_off_s": q["off_s"] if other and q else None,
                        "fish_stamped_together": stamped_together(fish, start)})
    not_placed = [label for label, _ in t["verses"] if placed[label] is None and label not in by_second]
    out = {"source": source, "second_opinion": second[0] if second else None, "verses": len(t["verses"]),
           "checked": len(t["verses"]) - len(not_placed), "placed_by_second": by_second, "off": off,
           "not_placed": not_placed, "median_s": round(statistics.median(gaps), 3) if gaps else None,
           "worst_s": max((abs(o["off_s"]) for o in off), default=0.0)}
    if isinstance(offset_check, str) and len(t["renders"]) > 1:
        out["offset_check"] = f"not checked: {offset_check}"
    elif offset_check is not None and len(t["renders"]) > 1:
        out["offset_check_s"] = offset_check
    return out


def _timing_job(item):
    code, ch = item
    try:
        return f"{code}.{ch}", timing_one(code, ch)
    except (OSError, ValueError, KeyError, StopIteration) as e:
        return f"{code}.{ch}", {"error": f"{type(e).__name__}: {e}"}


def timing(keys=None, workers: int = 8) -> dict:
    todo = chapters(keys)
    by = {}
    with cf.ProcessPoolExecutor(workers) as ex:  # difflib over whole chapters: one process each
        for key, r in ex.map(_timing_job, todo, chunksize=8):
            by[key] = r
    offs = sorted((o for r in by.values() for o in r.get("off", [])), key=lambda o: -abs(o["off_s"]))
    second = [o for o in offs if o["second_opinion"]]
    report = {"chapters": len(by), "verses": sum(r.get("verses", 0) for r in by.values()),
              "checked": sum(r.get("checked", 0) for r in by.values()), "off_s_over": OFF,
              "off": len(offs), "chapters_with_off": sum(1 for r in by.values() if r.get("off")),
              # of those, how the other transcript (where there is one) places the same verse
              "second_opinion": {"agrees": sum(1 for o in second if o["second_off_s"] is not None
                                               and abs(o["second_off_s"]) > OFF),
                                 "disagrees": sum(1 for o in second if o["second_off_s"] is not None
                                                  and abs(o["second_off_s"]) <= OFF),
                                 "not_placed": sum(1 for o in second if o["second_off_s"] is None),
                                 "none": len(offs) - len(second)},
              "not_placed": sum(len(r.get("not_placed", [])) for r in by.values()),
              "errors": {k: r["error"] for k, r in by.items() if "error" in r},
              "worst": offs[:50], "by_chapter": by}
    if not keys:
        (OUT / "timingcheck.json").write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding="utf-8")
    return report


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("--")] or None
    both = "--integrity" not in sys.argv and "--timing" not in sys.argv
    if both or "--integrity" in sys.argv:
        r = integrity(args)
        print(f"integrity: {r['checked']} chapters, {r['with_problems']} with problems, "
              f"{r['with_unchecked']} with takes that can't be checked against their joins")
        for k, p in r["problems"].items():
            print(" ", k, "|", "; ".join(p))
        for k, why in r["unchecked"].items():
            print(" ", k, "| not checked:", why)
    if both or "--timing" in sys.argv:
        r = timing(args)
        print(f"timing: {r['verses']} verses in {r['chapters']} chapters, {r['checked']} placed by the transcript, "
              f"{r['off']} more than {OFF:.0f} s off in {r['chapters_with_off']} chapters "
              f"(second opinion: {r['second_opinion']}), {r['not_placed']} not placed, {len(r['errors'])} errors")
        for o in r["worst"][:20]:
            print(f"  {o['chapter']}:{o['v']} start {o['start']} heard {o['heard']}-{o['heard_end']} "
                  f"{o['word']!r} off {o['off_s']:+} ({o['how']}; {o['second_opinion']}: {o['second_off_s']}; "
                  f"Fish stamped {o['fish_stamped_together']} words together)")
