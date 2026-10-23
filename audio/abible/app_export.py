"""Hand a rendered translation to Scriptorium's Listen feature (its docs/AUDIO.md):

  data/audio/[sideload/]<recording>.json  the reader, licence, credit, and each chapter's
                                           length and verse starts, in canonical order
  app/audio/<recording>/<BOOK>.<CH>.ogg   Ogg Opus, mono, speech-tuned, as the app's other
                                           recordings (16 kbps unless asked otherwise)

A recording in SIDELOAD goes in data/audio/sideload/, which Scriptorium builds in only when
SCRIPTORIUM_SIDELOAD_AUDIO=1 (sideload and development builds); store builds read
data/audio/ alone.

Nothing is written until every chapter passes `check`:
- it went through the hearing gate (abible.gate, run by abible.redo): redo.state has it
  done (its audio is the current plan, and every take has a verdict from the current
  checks with no fault left), its last redo didn't fail, and its words and voices reports
  (omissions.json, voicecheck.json) were made by the current checks (gate.CHECKS_VERSION);
- its MP3 was written to the end and is as long as its timing file says (RENDER_TOLERANCE);
- its verse labels are the KJV's verses of that chapter, each once and in order ("0" only
  for a Psalm's title);
- its verse starts are in order and inside the recording.
Any fault stops the export, listing every one. A deliberate partial export (`partial`,
--partial) lists the chapters not through the gate as warnings instead and ships them as
they are; every chapter must still be rendered. Verses that leave less than
MIN_SECONDS_PER_WORD a word before the next verse starts are listed but don't stop it: that
is a verse start stamped late or early (`abible.retime` mends it), not missing audio.

Then each chapter is encoded unless its Ogg file is newer than its MP3, was made at this
rate, and is as long as the MP3 (LENGTH_TOLERANCE); every Ogg file is then measured against
its MP3. The catalogue is written last, whole (a temporary file, then renamed), and only if
every chapter encoded, so the app never gets timings without their recordings. A run can be
repeated: it encodes only what changed. Don't run it while chapters are being rendered.

  python -m abible.app_export [translation] [--partial]   the checks alone; writes nothing
"""
import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from . import books, gate, kjv, paths, redo, render

RECORDINGS = {
    "kjv": {
        "id": "kjv-drama",
        "reader": "a full cast",
        "bibles": ["kjv"],
        "licence": "fish-audio",
        "credit": ("King James Version audio (1769 text, public domain outside the United Kingdom), dramatized: "
                   "the narrator, Jesus, and every other speaker each have a voice of their own. Voices generated "
                   "by AI with Fish Audio, each designed for this reading. Speaker assignments adapted from "
                   "Glyssen (SIL International and Faith Comes By Hearing), MIT licence, and checked against the "
                   "KJV's own wording and red letter."),
        "source": "https://github.com/Divhanthelion/KJV-Interlinear",
    },
}
# Kept out of Scriptorium's store builds until their terms for a published app are
# confirmed: the KJV was voiced with Fish's free model (decisions.md, open question 2)
SIDELOAD = {"kjv"}

# 16 kbps Ogg Opus, mono, in Opus's speech mode (-application voip, tuned for speech at low
# rates): the rate of the app's other recordings, and this reading is speech alone (no music
# or effects: decisions.md open 10). The whole KJV is then 0.58 GB, small enough to ship
# inside the app, where its 96 kbps MP3 masters are 3.3 GB (open 11).
KBPS = 16
# The most an MP3 may differ from its timing file's "duration", in seconds
RENDER_TOLERANCE = 0.5
# The most an encoded chapter may differ from its MP3, in seconds
LENGTH_TOLERANCE = 0.1
# A verse with fewer seconds than this a word before the next verse starts is listed; the
# cast reads about 0.32 s a word (185 a minute), its fastest readers about 0.25 s
MIN_SECONDS_PER_WORD = 0.15
# The Ogg tag that records the rate a file was encoded at
RATE_TAG = "encoded"


def duration(path: Path) -> float:
    out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
                         capture_output=True, text=True, check=True).stdout
    return float(out.strip())


def probe_mp3(path: Path) -> tuple[float, str]:
    """An MP3's length in seconds, and what ffprobe warns about it ("" when it is whole).
    ffmpeg writes the frame count into the first frame when it closes the file: one cut off
    mid-write has none, and ffprobe estimates the length from the bitrate; one cut short
    afterwards is shorter than its frame count says."""
    r = subprocess.run(["ffprobe", "-v", "warning", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
                       capture_output=True, text=True, check=True)
    warning = r.stderr.strip().splitlines()
    return float(r.stdout.strip()), warning[0].split("] ", 1)[-1] if warning else ""


def _ogg_rate(path: Path) -> str:
    out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", f"stream_tags={RATE_TAG}", "-of", "csv=p=0",
                          str(path)], capture_output=True, text=True).stdout
    return out.strip()


def _expected_labels(code: str) -> dict[int, list[str]]:
    """{chapter: its verse labels in order}, from the KJV text; "0" is a Psalm's title."""
    out: dict[int, list[str]] = {}
    for c, v in sorted(kjv.verses(code)):
        if v > 0 or code == "PSA":
            out.setdefault(c, []).append(str(v))
    return out


def _check_timing(code: str, ch: int, t: dict, labels: list[str]) -> tuple[dict, list[str], list[dict]]:
    """One chapter's catalogue entry, its faults, and its verses read too fast to be right."""
    key = f"{code}.{ch}"
    d = round(t["duration"], 2)
    v = [[label, round(start, 2)] for label, start in t["verses"]]
    faults, tight = [], []
    got = [label for label, _ in v]
    if got != labels:
        missing = [x for x in labels if x not in got]
        repeated = [x for i, x in enumerate(got) if x in got[:i]]
        foreign = [x for x in got if x not in labels]
        said = ([f"missing {', '.join(missing)}"] if missing else []) + \
               ([f"repeated {', '.join(repeated)}"] if repeated else []) + \
               ([f"not verses of the KJV's chapter: {', '.join(foreign)}"] if foreign else [])
        faults.append(f"{key}: verse labels {'; '.join(said) or 'out of order'}")
    if v and v[0][1] < 0:
        faults.append(f"{key}: verse {v[0][0]} starts before the recording ({v[0][1]} s)")
    for (a, sa), (b, sb) in zip(v, v[1:]):
        if sb < sa:
            faults.append(f"{key}: verse {b} starts at {sb} s, before verse {a} ({sa} s)")
    if v and v[-1][1] >= d:
        faults.append(f"{key}: verse {v[-1][0]} starts at {v[-1][1]} s, at or after the end ({d} s)")
    if not faults:
        text = kjv.verses(code)
        for i, (label, start) in enumerate(v):
            words = len(text[(ch, int(label))].split())
            seconds = (v[i + 1][1] if i + 1 < len(v) else d) - start
            if seconds < MIN_SECONDS_PER_WORD * words:
                tight.append({"chapter": key, "verse": label, "words": words, "seconds": round(seconds, 2)})
    return {"d": d, "v": v}, faults, tight


def _report(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}


def _gate(key: str, voices: dict, reports: dict) -> list[str]:
    """Why chapter `key` hasn't been through the hearing gate ([] when it has)."""
    why = []
    error = reports["redo.json"].get(key, {}).get("error")
    if error:
        why.append(f"its last redo failed ({error.splitlines()[0][:200]})")
    stale = [name for name in ("omissions.json", "voicecheck.json")
             if reports[name].get(key, {}).get("version") != gate.CHECKS_VERSION]
    if stale:
        why.append(f"{' and '.join(stale)} not made by the current checks")
    # by the same record redo.state goes by (redo.json), less the lines a person overruled
    wrong = redo.wrong_left(key, reports["redo.json"].get(key, {}).get("wrong_voice") or [])
    if wrong:
        why.append(f"{len(wrong)} line(s) still in another character's voice (the chapter's voice check)")
    if why:
        return why
    # Asked last: redo.state builds the chapter's requests again (about 0.2 s a chapter, 4
    # minutes for the whole Bible)
    try:
        return redo.state(key, voices, reports["redo.json"])
    except (Exception, SystemExit) as e:  # noqa: BLE001 - listed with the other faults
        return [f"redo.state failed ({type(e).__name__}: {str(e)[:200]})"]


def check(translation: str = "kjv", workers: int = 8, partial: bool = False) -> dict:
    """Every chapter's hearing-gate verdicts, timing file and MP3, before anything is written:
    {"chapters": the catalogue's, "mp3": {key: seconds}, "faults": [...], "ungated": [...],
    "tight": [...]}. "ungated" lists the chapters not through the gate; they are faults too
    unless `partial`. The catalogue is whole only when there are no faults."""
    out = paths.OUT / translation
    # A take's verdict counts only when the current checks made it (render._verdict); read
    # each report once (abible.redo replaces them whole as chapters finish)
    render.CHECKS_VERSION = gate.CHECKS_VERSION
    voices = render.voice_map()
    reports = {name: _report(out / name) for name in ("redo.json", "omissions.json", "voicecheck.json")}
    chapters, faults, ungated, tight, durations = {}, [], [], [], {}
    for code in books.CODES:
        for ch, labels in _expected_labels(code).items():
            key = f"{code}.{ch}"
            src, mp3 = out / f"{key}.json", out / f"{key}.mp3"
            if not (src.exists() and mp3.exists()):
                faults.append(f"{key}: not rendered (no {'timing file' if not src.exists() else 'MP3'})")
                continue
            why = _gate(key, voices, reports)
            if why:
                ungated.append(f"{key}: not through the hearing gate: {'; '.join(why)}")
                if not partial:
                    faults.append(ungated[-1])
            t = json.loads(src.read_text(encoding="utf-8"))
            chapters[key], f, tt = _check_timing(code, ch, t, labels)
            faults += f
            tight += tt
            durations[key] = t["duration"]
    with ThreadPoolExecutor(max_workers=workers) as pool:
        lengths = dict(zip(durations, pool.map(probe_mp3, (out / f"{k}.mp3" for k in durations))))
    for key, (length, warning) in lengths.items():
        if warning:
            faults.append(f"{key}: the MP3 isn't whole (ffprobe: {warning}; {length:.2f} s, "
                          f"the timing file says {durations[key]:.2f} s)")
        elif abs(length - durations[key]) > RENDER_TOLERANCE:
            faults.append(f"{key}: the MP3 is {length:.2f} s, the timing file says {durations[key]:.2f} s")
    return {"chapters": chapters, "mp3": {k: n for k, (n, _) in lengths.items()}, "faults": faults,
            "ungated": ungated, "tight": tight}


def _encode(src: Path, out: Path, length: float, meta: dict, kbps: int) -> tuple[str, int, float, bool]:
    """Encode one chapter unless its Ogg file is newer than the MP3, made at this rate, and as
    long as the MP3: (key, bytes, seconds off the MP3, whether it was encoded now)."""
    have = None
    if out.exists() and out.stat().st_mtime >= src.stat().st_mtime and _ogg_rate(out) == f"{kbps}k":
        have = duration(out)
    made = have is None or abs(have - length) > LENGTH_TOLERANCE
    if made:
        tmp = out.with_suffix(".tmp.ogg")
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(src), "-map_metadata", "-1", "-ac", "1",
                        "-c:a", "libopus", "-b:a", f"{kbps}k", "-application", "voip", "-vbr", "on",
                        "-compression_level", "10", "-metadata", f"title={out.stem}",
                        "-metadata", f"artist={meta['id']}", "-metadata", f"comment={meta['credit']}",
                        "-metadata", f"{RATE_TAG}={kbps}k", str(tmp)], check=True)
        tmp.replace(out)
        have = duration(out)
    return out.stem, out.stat().st_size, abs(have - length), made


def export(app_root: Path, translation: str = "kjv", kbps: int = KBPS, workers: int = 8,
           partial: bool = False) -> dict:
    meta = RECORDINGS[translation]
    recording = meta["id"]
    report = check(translation, workers, partial)
    if report["faults"]:
        raise SystemExit(f"{translation}: {len(report['faults'])} fault(s), nothing written:\n  "
                         + "\n  ".join(report["faults"]))
    ungated = report["ungated"]  # empty unless partial
    if ungated:
        print(f"{len(ungated)} chapter(s) not through the hearing gate, exported as they are (partial; "
              f"listed in the summary):", flush=True)
        for line in ungated[:20]:
            print(f"  {line}", flush=True)
    tight = report["tight"]
    if tight:
        print(f"{len(tight)} verse(s) with under {MIN_SECONDS_PER_WORD} s a word before the next starts "
              f"(listed in the summary; abible.retime mends a misplaced start):", flush=True)
        for t in sorted(tight, key=lambda t: t["seconds"] / t["words"])[:20]:
            print(f"  {t['chapter']}:{t['verse']}  {t['words']} words in {t['seconds']} s", flush=True)
    out_dir = app_root / "app" / "audio" / recording
    out_dir.mkdir(parents=True, exist_ok=True)
    # Left by an interrupted run; anything in the folder ships with the app
    for p in out_dir.glob("*.tmp.ogg"):
        p.unlink()
    total, worst, encoded, off = 0, (0.0, ""), 0, []
    with ThreadPoolExecutor(max_workers=workers) as pool:
        jobs = [pool.submit(_encode, paths.OUT / translation / f"{key}.mp3", out_dir / f"{key}.ogg",
                            report["mp3"][key], meta, kbps) for key in report["chapters"]]
        for i, job in enumerate(jobs, 1):
            key, size, diff, made = job.result()
            total += size
            encoded += made
            worst = max(worst, (diff, key))
            if diff > LENGTH_TOLERANCE:
                off.append(f"{key}: the Ogg file is {diff:.3f} s off its MP3")
            if i % 100 == 0:
                print(f"{i}/{len(jobs)}", flush=True)
    if off:
        raise SystemExit(f"{recording}: {len(off)} chapter(s) failed to encode, catalogue not written:\n  "
                         + "\n  ".join(off))
    audio = app_root / "data" / "audio"
    data = (audio / "sideload" if translation in SIDELOAD else audio) / f"{recording}.json"
    data.parent.mkdir(parents=True, exist_ok=True)
    tmp = data.with_name(data.name + ".tmp")
    tmp.write_text(json.dumps({**meta, "chapters": report["chapters"]}, ensure_ascii=False, separators=(",", ":")),
                   encoding="utf-8")
    tmp.replace(data)
    # A recording is built in from one place only: a copy left in the other would put a
    # sideload recording into store builds, or keep an old one
    other = (audio if translation in SIDELOAD else audio / "sideload") / f"{recording}.json"
    if other.exists():
        other.unlink()
        print(f"removed {other} (the catalogue is now {data})")
    summary = {"recording": recording, "catalogue": str(data), "files": len(jobs), "encoded": encoded,
               "gb": round(total / 1e9, 3), "kbps": kbps, "largest_length_difference_s": round(worst[0], 3),
               "at": worst[1], "checks": gate.CHECKS_VERSION, "ungated": ungated, "tight": tight}
    print(json.dumps(summary | {"ungated": len(ungated), "tight": len(tight)}))
    return summary


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    partial = "--partial" in sys.argv
    r = check(args[0] if args else "kjv", partial=partial)
    print(json.dumps({"chapters": len(r["chapters"]), "checks": gate.CHECKS_VERSION, "ungated": len(r["ungated"]),
                      "faults": r["faults"], "warnings": r["ungated"] if partial else [], "tight": r["tight"]},
                     indent=1))
    sys.exit(1 if r["faults"] else 0)
