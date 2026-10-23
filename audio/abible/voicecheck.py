"""Is every line in its own voice? Each line is compared (WeSpeaker embedding, CPU)
with the reference clip of every voice in its request, not with the other lines of the
take: in 1 Samuel 17, rendered as one request with nine voices, Fish gave nearly all of
David's lines Jesse's voice and David's voice to Goliath and the Israelites, which a
check against the take's own lines can't see (David's lines all sound alike, just not
like David). Run with the QA environment.

Calibrated on 2026-10-12 (1,112 lines of 25 clean chapters, and the 17 swapped lines of
the first 1 Samuel 17): a clean line's own similarity was 0.84 at the median and never
below 0.41; another voice beat it by at most 0.07, or 0.14 where the two voices'
reference clips are 0.6 alike or more; every swap was beaten by 0.12 or more.

- wrong voice: another voice in the request beats the line's own by more than MARGIN
  (MARGIN_CLOSE for voices whose clips are CLOSE_PAIR alike or more);
- listen: the line matches its own voice below FLOOR and nothing beats it (a blend or a
  drift, for a person's ear; single-voice requests are checked this way only);
- lines under MIN_SECONDS are scored on their own with SHORT_MARGIN, and pooled per voice
  as an extra check; lines with no timing or under MIN_EMBED are listed, never dropped;
- voices too alike to tell apart (CLOSE_PAIR) are kept out of one request by render.scenes;
  if a request holds two anyway, its lines are listed for a person's ear.
"""
import json
import sys

import numpy as np

from . import cast as castmod, paths, qa, spk

MARGIN, MARGIN_CLOSE, CLOSE_PAIR = 0.10, 0.16, 0.6
# A line under MIN_SECONDS: its embedding is noisier. Clean short tags score another voice
# 0.14 or more below their own; swapped ones 0.10 to 0.5 above (0.8 s of God's voice in a
# narrator's "And God said," in Genesis 1 scored +0.10)
SHORT_MARGIN = 0.05
FLOOR = 0.40
MIN_SECONDS = 1.2
MIN_EMBED = 0.3  # seconds: shorter than this can't be told apart
RATE = 16000
_refs = {}


def _clips() -> dict:
    """{voice id: clip}, for the cast's voices and the ones they replaced (a voice remade
    with an honest transcript, 2026-10-11: chapters rendered before still use the old id)."""
    out = {}
    for f in sorted(paths.CACHE.glob("cast.before-*.json")) + [castmod.CAST]:
        data = json.loads(f.read_text(encoding="utf-8"))
        for e in data.get("cast", data).values():
            v = e.get("voice") if isinstance(e, dict) else None
            if v and v.get("id"):
                out[v["id"]] = v["clip"]
    return out


def reference(voice_id: str) -> np.ndarray:
    """The embedding of a voice's own clip (the one Fish made it from)."""
    if voice_id not in _refs:
        v = spk.embed(qa.pcm16k(paths.VOICES / _clips()[voice_id]))
        _refs[voice_id] = v / np.linalg.norm(v)
    return _refs[voice_id]


def check_lines(pcm: np.ndarray, lines: list) -> dict:
    """lines: [{"voice", "start", "end", ...}] of one request (times in `pcm`, 16 kHz mono).
    Returns {"wrong": [...], "listen": [...], "checked": n, "unchecked": n}.

    Every line is scored on its own, short ones too: pooling short lines let a narrator's
    "And he said," in the character's voice hide among clean tags (up to three quarters of
    a pool could be wrong). The pool of a voice's short lines is an extra check. Lines with
    no timing, or too short to embed, are listed for a person's ear, never dropped."""
    voices = sorted({l["voice"] for l in lines})
    refs = {v: reference(v) for v in voices}
    wrong, listen, checked, unchecked = [], [], 0, 0
    close = len(voices) > 1 and any(float(refs[a] @ refs[b]) >= CLOSE_PAIR for a in voices for b in voices if a < b)
    pools = {}

    def about(l):
        return {k: l.get(k) for k in ("line", "v", "who", "key", "start", "end", "scene") if k in l}

    for l in lines:
        start, end = l.get("start"), l.get("end")
        seg = pcm[int(start * RATE):int(end * RATE)] if start is not None and end is not None and end > start else pcm[:0]
        if len(seg) < MIN_EMBED * RATE:
            unchecked += 1
            listen.append(about(l) | {"why": "not timed, or too short to tell its voice; not checked"})
            continue
        e = spk.embed(seg)
        e = e / np.linalg.norm(e)
        own = float(e @ refs[l["voice"]])
        checked += 1
        short = len(seg) < MIN_SECONDS * RATE
        if short:
            pools.setdefault(l["voice"], []).append((l, seg))
        if close:
            listen.append(about(l) | {"own": round(own, 2), "why": "its request holds two voices too alike to check"})
            continue
        others = [(float(e @ refs[v]), v) for v in voices if v != l["voice"]]
        if others:
            best_sim, best = max(others)
            margin = SHORT_MARGIN if short else (MARGIN_CLOSE if float(refs[l["voice"]] @ refs[best]) >= CLOSE_PAIR else MARGIN)
            if best_sim - own > margin:
                wrong.append(about(l) | {"own": round(own, 2), "sounds_like": best, "sounds_like_sim": round(best_sim, 2),
                                         "short": short})
                continue
        if own < FLOOR and not short:
            listen.append(about(l) | {"own": round(own, 2), "why": "unlike its own voice; nothing else matches better"})
    # the extra check: a voice's short lines together
    flagged = {(w.get("v"), w.get("start")) for w in wrong}
    for v, parts in pools.items():
        others = [u for u in voices if u != v]
        if len(parts) < 2 or not others or close:
            continue
        e = spk.embed(np.concatenate([seg for _, seg in parts]))
        e = e / np.linalg.norm(e)
        own = float(e @ refs[v])
        best_sim, best = max((float(e @ refs[u]), u) for u in others)
        if best_sim - own > MARGIN:
            for l, _ in parts:
                if (l.get("v"), l.get("start")) not in flagged:
                    wrong.append(about(l) | {"own": round(own, 2), "sounds_like": best, "sounds_like_sim": round(best_sim, 2),
                                             "pooled": True})
    return {"wrong": wrong, "listen": listen, "checked": checked, "unchecked": unchecked}


def check_chapter(key: str, translation: str = "kjv") -> dict:
    t = json.loads((paths.OUT / translation / f"{key}.json").read_text(encoding="utf-8"))
    pcm = qa.pcm16k(paths.OUT / translation / f"{key}.mp3")
    by_scene = {}
    for l in t["lines"]:
        by_scene.setdefault(l.get("scene", 0), []).append(l)
    out = {"checked": 0, "unchecked": 0, "wrong": [], "listen": [], "scenes": []}
    for n, lines in sorted(by_scene.items()):
        r = check_lines(pcm, lines)
        out["wrong"] += r["wrong"]
        out["listen"] += r["listen"]
        out["checked"] += r["checked"]
        out["unchecked"] += r["unchecked"]
        out["scenes"].append({"scene": n, "voices": len({l["voice"] for l in lines}), "lines": r["checked"],
                              "wrong": len(r["wrong"])})
    return out


def run(translation: str = "kjv", keys=None, again: bool = False) -> dict:
    from . import books, gate, script
    path = paths.OUT / translation / "voicecheck.json"
    report = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
    keys = keys or [f"{c}.{ch['chapter']}" for c in books.CODES for ch in script.load(c)["chapters"]]
    for i, key in enumerate(keys, 1):
        if key in report and report[key].get("version") == gate.CHECKS_VERSION and not again:
            continue
        report[key] = check_chapter(key, translation) | {"version": gate.CHECKS_VERSION}
        if i % 50 == 0:
            path.write_text(json.dumps(report, ensure_ascii=False), encoding="utf-8")
            print(i, "chapters", flush=True)
    path.write_text(json.dumps(report, ensure_ascii=False), encoding="utf-8")
    return report


if __name__ == "__main__":
    run(keys=[a for a in sys.argv[1:] if not a.startswith("--")] or None, again="--again" in sys.argv)
