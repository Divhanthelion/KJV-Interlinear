"""Bring chapters to the current plan through the hearing gate, until every word is
heard in its place and every line is in its own voice (run with the QA environment).

    python -m abible.redo                  every chapter that isn't done (see state())
    python -m abible.redo DEU.17 ROM.5     these chapters
    python -m abible.redo --list           only show what would be done and why

A chapter is done when its audio is the current plan (the scenes and text render.scenes
and request_text give now: lexicon, delivery cues, voices, at most 2 voices a request)
and every take in it has a verdict from the current checks (gate.CHECKS_VERSION) with no
fault left. Otherwise it is rendered: a take already in the cache whose verdict is
missing or stale is heard again (no request to Fish); a take with a fault is turned down
and Fish asked for another, up to ATTEMPTS; a scene that keeps failing is halved at most
render.MAX_HALVINGS times. The version a chapter replaces is kept in .cache/out/kjv/
replaced/. Timings Fish stamped wrongly are re-timed, and the chapter's voices and sound
are checked whole. Results go to redo.json; what needs a person's ear (names, words heard
in place only when primed, words read the same way in two takes, lines unlike their own
voice) goes to omissions.json for the listening guide.
"""
import concurrent.futures as cf
import json
import shutil
import sys
import threading
import time
import traceback

from . import books, gate, paths, qa, render, retime, script, voicecheck

OUT = paths.OUT / "kjv"
ATTEMPTS = 4  # takes a scene before it is halved (render.MAX_HALVINGS times)
ROUNDS = 3  # renders of a chapter while its own voice check finds a wrong voice
_lock = threading.Lock()


def _keys() -> list:
    return [f"{c}.{ch['chapter']}" for c in books.CODES for ch in script.load(c)["chapters"]]


def _meta(key: str) -> dict | None:
    p = paths.RENDERS / f"{key}.json"
    return json.loads(p.read_text(encoding="utf-8")) if p.exists() else None


def _read(path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def state(key: str, voices=None, results: dict | None = None) -> list:
    """Why chapter `key` isn't done ([] when it is): its audio must be the current plan
    (render.plan_id: scenes, text and voices; a halved scene still counts as its plan),
    every take must have a verdict from the current checks with no fault left, and the
    steps after the render (re-timing, the chapter's voice and sound checks, the reports)
    must have finished for these takes (redo.json, written last)."""
    voices = voices or render.voice_map()
    code, ch = key.split(".")
    timing_path = OUT / f"{key}.json"
    timing = _read(timing_path) if timing_path.exists() else None
    if timing is None or not (OUT / f"{key}.mp3").exists():
        return ["not rendered" if timing_path.exists() is False else "timing file unreadable"]
    why = []
    if timing.get("plan") != render.plan_id(code, int(ch), voices):
        why.append("the text or grouping changed since it was rendered (lexicon, cues, voices, 2 voices a request)")
    verdicts = [render._verdict(paths.RENDERS / f"{k}.heard.json") for k in timing["renders"]]
    if any(v is None for v in verdicts):
        why.append("not heard by the current checks")
    elif any(render._acceptable(v, set()) is None and not _accepted(v) for v in verdicts):
        why.append("a take has a fault")
    results = results if results is not None else (_read(OUT / "redo.json") or {})
    done = results.get(key) or {}
    if not why and (done.get("version") != gate.CHECKS_VERSION or done.get("error")
                    or done.get("renders") != timing["renders"]):
        why.append("the steps after rendering (re-timing, chapter checks, reports) haven't finished")
    elif not why and wrong_left(key, done.get("wrong_voice") or []):
        # the chapter's own voice check still found one after ROUNDS renders: the next run
        # turns its take down again (Fish's takes run out, then the scene is halved)
        why.append("a line is still in another character's voice")
    return why


def heard_ok(key: str) -> set:
    """The lines of chapter `key` a person has listened to and found in their own voice,
    as (verse, who), from heard-ok.json ({"GEN.1": [{"v": 3, "who": "God", "by": ...,
    "note": ...}]}): the chapter's voice check is overruled for them, which a false alarm
    it keeps raising otherwise never lets the chapter be done."""
    entries = (_read(OUT / "heard-ok.json") or {}).get(key, [])
    return {(str(x.get("v")), x.get("who")) for x in entries}


def wrong_left(key: str, wrong: list) -> list:
    """The wrong-voice items a person hasn't overruled (heard_ok)."""
    ok = heard_ok(key)
    return [w for w in wrong if (str(w.get("v")), w.get("who")) not in ok]


def _accepted(verdict: dict) -> bool:
    """A take accepted with faults the gate treats as persistent."""
    accepted = {render._fault_id(f) for f in verdict.get("persistent", [])}
    return all(render._fault_id(f) in accepted for f in verdict["faults"])


def _log(line: str):
    stamp = time.strftime("%Y-%m-%d %H:%M:%S ")
    for attempt in range(5):
        try:
            with _lock, open(OUT / "redo.log", "a", encoding="utf-8") as f:
                f.write(stamp + line + "\n")
            return
        except OSError:
            time.sleep(0.5 * (attempt + 1))


def _save(results: dict):
    with _lock:
        tmp = OUT / "redo.json.partial"
        tmp.write_text(json.dumps(results, ensure_ascii=False, indent=1), encoding="utf-8")
        tmp.replace(OUT / "redo.json")


def _merge_report(name: str, key: str, value: dict):
    with _lock:
        path = OUT / name
        report = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
        report[key] = value
        tmp = path.with_suffix(".json.partial")
        tmp.write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding="utf-8")
        tmp.replace(path)


def _placed(items: list, timing: dict, n: int) -> list:
    """A take's items in this chapter: scene n, and the verse of the line it names (a take
    shared by two chapters carries the first one's verses)."""
    verses = (timing.get("scenes") or [{}] * (n + 1))[n].get("verses") or []
    out = []
    for x in items:
        y = x | {"scene": n}
        if isinstance(x.get("line"), int) and x["line"] < len(verses):
            y["verse"] = y["v"] = verses[x["line"]]
        out.append(y)
    return out


def _turn_down(take: str, wrong: list):
    """A take the chapter's own voice check found a wrong voice in: added to its verdict as
    a fault and set aside, so the next render asks Fish for another take."""
    path = paths.RENDERS / f"{take}.heard.json"
    v = _read(path) or {"faults": [], "version": gate.CHECKS_VERSION}
    v["faults"] = v["faults"] + [{"fault": "wrong voice", "found_by": "chapter check"} | w for w in wrong]
    v.pop("persistent", None)
    render._write_json(path, v)
    mp3 = paths.RENDERS / f"{take}.mp3"
    if mp3.exists():
        mp3.rename(paths.RENDERS / f"{take}.unheard.mp3")


def redo_chapter(key: str, why: list) -> dict:
    code, ch = key.split(".")
    ch = int(ch)
    kept = OUT / "replaced"
    paths.ensure(kept)
    for ext in ("mp3", "json", "qa.json"):
        src = OUT / f"{key}.{ext}"
        if src.exists() and not (kept / f"{key}.{ext}").exists():
            shutil.copy2(src, kept / f"{key}.{ext}")
    before = (_read(OUT / f"{key}.json") or {}).get("renders", [])
    turned = []
    for round_ in range(ROUNDS):  # a wrong voice only the whole chapter's check finds: render again
        timing = render.render_chapter(code, ch)
        (OUT / f"{key}.fish-timing.json").unlink(missing_ok=True)  # belonged to the version replaced
        (paths.CACHE / "asr" / "kjv" / f"{key}.json").unlink(missing_ok=True)
        # Fish's timestamps went wrong where they were inconsistent, too fast, stamped
        # together, or around phoneme tags (Romans 5, Daniel 5, Genesis 12:17, Numbers 13:9):
        # retime decides line by line
        lines = render.spoken_lines(code, ch)
        needs_retime = bool(retime.needs(code, ch)) or any("•" in render.as_sent(l["text"]) for l in lines)
        if needs_retime:
            with gate._gpu:
                timed = retime.retime(code, ch)
        else:
            timed = {"changed": [], "rejected": []}
        timing = json.loads((OUT / f"{key}.json").read_text(encoding="utf-8"))
        voices_ = voicecheck.check_chapter(key)
        left = wrong_left(key, voices_["wrong"])
        voices_["overruled_by_ear"] = [w for w in voices_["wrong"] if w not in left]
        voices_["wrong"] = left
        if not voices_["wrong"] or round_ == ROUNDS - 1:
            break
        by_take = {}
        for w in voices_["wrong"]:
            by_take.setdefault(timing["renders"][w.get("scene", 0)], []).append(w)
        for take, ws in by_take.items():
            _turn_down(take, ws)
            turned.append(take)
    mp3 = OUT / f"{key}.mp3"
    sound = qa.sound_check(mp3)
    verdicts = [json.loads((paths.RENDERS / f"{k}.heard.json").read_text(encoding="utf-8")) for k in timing["renders"]]
    # Every take passed the gate, and joining takes can't drop a word: the chapter's words
    # are its takes' verdicts. What they leave for a person's ear goes on the guide.
    words = {"missing": [], "changed": [], "added": [], "by": "gate", "version": gate.CHECKS_VERSION,
             "persistent": [x for n, v in enumerate(verdicts) for x in _placed(v.get("persistent", []), timing, n)],
             "names": [x for n, v in enumerate(verdicts) for x in _placed(v.get("names", []), timing, n)],
             "listen": [x for n, v in enumerate(verdicts) for x in _placed(v.get("listen", []), timing, n)]}
    take_voice = [x for n, v in enumerate(verdicts) for x in _placed(v.get("voice_listen", []), timing, n)]
    seen_v = {(x.get("v"), x.get("who")) for x in voices_["listen"]}
    voices_["listen"] = voices_["listen"] + [x for x in take_voice if (x.get("v"), x.get("who")) not in seen_v]
    _merge_report("omissions.json", key, words)
    _merge_report("voicecheck.json", key, voices_ | {"version": gate.CHECKS_VERSION})
    (OUT / f"{key}.qa.json").write_text(json.dumps(
        {"chapter": f"{code} {ch}", "words": words, "voices": voices_, "sound": sound},
        indent=1, ensure_ascii=False), encoding="utf-8")
    return {"why": why, "version": gate.CHECKS_VERSION, "renders_before": before, "renders": timing["renders"],
            "duration": timing["duration"], "retimed": [c["v"] for c in timed["changed"]],
            "rejected_retimes": timed.get("rejected", []), "turned_down_by_chapter_check": turned,
            "wrong_voice": voices_["wrong"], "voice_listen": len(voices_["listen"]),
            "unchecked_short_lines": voices_["unchecked"],
            "sound": {"loud": len(sound.get("loud", [])), "babble": len(sound.get("babble", []))},
            "turned_down": turned_down(timing["renders"]), "persistent": words["persistent"],
            "names": len(words["names"]), "listen": len(words["listen"])}


def turned_down(renders: list) -> list:
    """The takes turned down before each accepted one (same request, earlier attempts),
    with why."""
    out = []
    for k in renders:
        meta = _meta(k)
        if not meta:
            continue
        body = render.body_for(meta["text"], meta["ids"])
        for attempt in range(ATTEMPTS):
            ak = render.cache_key(body, attempt)
            if ak == k:
                break
            v = paths.RENDERS / f"{ak}.heard.json"
            if (paths.RENDERS / f"{ak}.unheard.mp3").exists() and v.exists():
                faults = json.loads(v.read_text(encoding="utf-8"))["faults"]
                out.append({"take": ak, "faults": [f"{f['fault']} v{f.get('verse', f.get('v'))}" for f in faults]})
    return out


def _stay_awake():
    """A run takes hours: ask Windows not to idle-sleep while this process runs (a power
    request like a media player's; it ends with the process, no setting is changed). On
    2026-10-12 the machine slept at 2:33 and took a run with it."""
    if sys.platform == "win32":
        import ctypes
        ES_CONTINUOUS, ES_SYSTEM_REQUIRED = 0x80000000, 0x00000001
        ctypes.windll.kernel32.SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)


def redo(keys: list | None = None, workers: int = 5, attempts: int = ATTEMPTS) -> dict:
    _stay_awake()
    render.HEARING = gate.faults
    render.CHECKS_VERSION = gate.CHECKS_VERSION
    render.ATTEMPTS = attempts
    path = OUT / "redo.json"
    results = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
    voices = render.voice_map()
    todo = {}
    for k in keys or _keys():
        try:
            why = state(k, voices, results)
        except Exception as e:  # noqa: BLE001 - a file cut short or the like: name it, redo the chapter
            why = [f"its state couldn't be read: {type(e).__name__}: {e}"]
        if why:
            todo[k] = why
    _log(f"start: {len(todo)} chapters not done, {workers} at a time, up to {attempts} takes a scene, "
         f"checks {gate.CHECKS_VERSION}")
    with cf.ThreadPoolExecutor(workers) as ex:
        futures = {ex.submit(redo_chapter, k, v): k for k, v in todo.items()}
        for fut in cf.as_completed(futures):
            key = futures[fut]
            try:
                results[key] = fut.result()
                r = results[key]
                _log(f"{key}: {len(r['renders'])} request(s), {len(r['turned_down'])} take(s) turned down; "
                     f"wrong voice {len(r['wrong_voice'])}, listen {r['listen'] + r['voice_listen']}, "
                     f"persistent {len(r['persistent'])}, sound {r['sound']}")
            except (Exception, SystemExit) as e:  # noqa: BLE001 - logged per chapter; run again to retry
                results[key] = {"why": todo[key], "error": f"{type(e).__name__}: {e}", "version": gate.CHECKS_VERSION}
                _log(f"{key}: FAILED {type(e).__name__}: {str(e)[:200]}")
                with open(OUT / "redo.trace", "a", encoding="utf-8") as f:
                    f.write(f"== {key}\n{traceback.format_exc()}\n")
            _save(results)
    _log("end")
    return results


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if "--list" in sys.argv:
        voices = render.voice_map()
        render.CHECKS_VERSION = gate.CHECKS_VERSION
        results = _read(OUT / "redo.json") or {}
        n = 0
        for k in args or _keys():
            why = state(k, voices, results)
            if why:
                n += 1
                print(k, "|", "; ".join(why))
        print(n, "chapters not done")
    else:
        redo(args or None)
