"""Making the cast's voices: Voice Design -> candidates -> one chosen -> a
private Fish voice from that one clip (about 10 s, so 20 voices fit in a
request; see docs/fish-audio.md).

Every candidate reads a line the character really says in the KJV (150
characters at most), so the audition is in character. Candidates are kept in
.cache/voices/<slug>/ with what produced them. Choosing:
  - principal characters (hand-written briefs): the owner listens and picks
    (`voices audition` writes the page; `voices choose KEY N` records it);
  - everyone else: automatically, the candidate that said its line correctly
    and is least like every voice already cast (speaker embeddings), with a
    hard floor so no two voices in one book are near-identical.
"""
import base64
import json
import re

from . import cast as castmod, fish, paths

# Speaker-embedding similarity (cosine, WeSpeaker). Measured 2026-10-08 on the
# 496 candidates of the 124 principal characters: two renders of one voice
# 0.78-0.85; four candidates of one brief, median 0.51 (0.33-0.67); candidates
# of different characters, median 0.26, 99th percentile 0.65. A pair can score
# high across genders (a 'male' candidate at 219 Hz scored 0.82 with Eve), so
# screen.py also checks pitch.
MAX_SIM_SAME_BOOK = 0.60
MAX_SIM_ANYWHERE = 0.72

FALLBACK_LINES = {
    "woman": "And she said, Behold, here am I; for thou didst call me.",
    "man": "And he said, Behold, here am I; for thou didst call me.",
}


def slug(key: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", key.lower()).strip("-")[:60]


def reference_line(entry: dict, exclude=()) -> str:
    """A line the character really says, 50-150 characters, near 120; a line
    already used (`exclude`) is skipped when another exists."""
    lines = [t for _, t in entry.get("samples", []) if 50 <= len(t) <= 150]
    lines = [t for t in lines if t not in exclude] or lines
    if lines:
        return min(lines, key=lambda t: abs(len(t) - 120))
    g = "woman" if entry.get("gender", "").endswith("emale") else "man"
    return FALLBACK_LINES[g]


def instruction(entry: dict) -> str:
    accent = castmod.policy().get("accent", "")
    # "One voice speaking for a group (…):" is a note for people; the designer gets the voice
    text = re.sub(r"^One voice speaking for a group \(.*?\): (\w)", lambda m: m.group(1).upper(), entry["brief"].strip())
    if accent and accent not in text:
        text = text.replace(" Natural, unaffected English, clear diction, no caricature.", "") + " " + accent
    return text[:2000]


def design(key: str, n: int = 4, seed: int | None = None, new_line=False, guidance: float = 2.0,
           line: str | None = None) -> list:
    entries = castmod.load()
    entry = entries[key]
    if entry.get("voice_of"):
        raise SystemExit(f"{key} speaks with {entry['voice_of']}'s voice; design that one")
    out = paths.VOICES / slug(key)
    paths.ensure(out)
    used = {c["reference_text"] for c in candidates(key)}
    line = line or reference_line(entry, exclude=used if new_line else ())  # a line chosen by a person, if given
    prompt = instruction(entry)
    cands = fish.design(prompt, line, n=n, seed=seed, guidance_scale=guidance)
    batch = len(list(out.glob("batch-*.json"))) + len(list(out.glob("superseded-batch-*.json")))  # never reuse a file name
    saved = []
    for c in cands:
        wav = out / f"b{batch}-c{c.get('index', len(saved))}.wav"
        wav.write_bytes(base64.b64decode(c["audio_base64"]))
        saved.append({"file": wav.name, "id": c.get("id"), "signature": c.get("signature"),
                      "duration_ms": c.get("duration_ms"), "sample_rate": c.get("sample_rate"),
                      "heard_as": c.get("features")})  # Fish's reading of the brief: age, tone, accent, pacing
    (out / f"batch-{batch}.json").write_text(json.dumps({
        "key": key, "instruction": prompt, "reference_text": line, "seed": seed, "n": n, "guidance": guidance,
        "candidates": saved,
    }, indent=1, ensure_ascii=False), encoding="utf-8")
    return saved


def candidates(key: str) -> list:
    out = paths.VOICES / slug(key)
    found = []
    for b in sorted(out.glob("batch-*.json")):
        meta = json.loads(b.read_text(encoding="utf-8"))
        for c in meta["candidates"]:
            found.append(dict(c, reference_text=meta["reference_text"], instruction=meta["instruction"]))
    return found


TARGET_LUFS = -18.0


def level(wav):
    """The clip at a standard loudness, for auditions. Voice Design's clips
    range from -38 to -9 LUFS. (Voices are made from the original clip: Fish
    evens its speech out itself, a voice from a -38 LUFS clip spoke at -18.7.)
    Cached beside the clip as <name>.level.wav."""
    import subprocess
    out = wav.with_name(wav.stem + ".level.wav")
    if not out.exists():
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(wav), "-af",
                        f"loudnorm=I={TARGET_LUFS}:TP=-1.5:LRA=11", "-ar", "44100", "-ac", "1", "-c:a", "pcm_s16le",
                        str(out)], check=True)
    return out


def choose(key: str, file: str, save: bool = True) -> dict:
    """Make the private Fish voice from one candidate and record it in the cast."""
    cand = next(c for c in candidates(key) if c["file"] == file)
    # The clip as Voice Design made it: Fish checks the design signature against these exact
    # bytes (a levelled copy is refused, "Invalid voice-design signature"), and the signature
    # records the voice as designed, not cloned. Volume is evened out where it is heard:
    # Fish's normalize_loudness, then render.level_voices.
    wav = (paths.VOICES / slug(key) / file).read_bytes()
    entries = castmod.load()
    entry = entries[key]
    transcript = cand["reference_text"]
    sc = paths.VOICES / slug(key) / "screen.json"
    if sc.exists():  # the clip's transcript must be what it says: if it skipped words, use what was heard
        row = next((r for r in json.loads(sc.read_text(encoding="utf-8")) if r["file"] == file), None)
        if row and row.get("read", 1) < 0.97 and row.get("heard"):
            transcript = row["heard"]
    made = fish.create_voice(f"KJV audio: {key}", entry["brief"][:480], wav, file, transcript,
                             signature=cand.get("signature"), tags=["abible", "kjv"])
    seconds = (len(wav) - 44) / 2 / (cand.get("sample_rate") or 44100)
    record = {"id": made["_id"], "clip": f"{slug(key)}/{file}", "seconds": round(seconds, 2),
              "transcript": transcript, "source": made.get("source"), "state": made.get("state")}
    if save:
        _save_voice(key, record)
    return record


def auto_pick(owner_picks: dict, accept_over: bool = False) -> dict:
    """Choose a candidate for every designed character the owner did not pick.

    In order of how much they speak, each character takes the screened
    candidate (no problems) least like every voice already chosen; within the
    books it speaks in, it must stay under MAX_SIM_SAME_BOOK, and anywhere under
    MAX_SIM_ANYWHERE. Characters with no candidate under both limits are
    returned as "redesign". Returns {key: {"file", "same_book", "anywhere"} or
    {"file": "redesign", ...}}."""
    import numpy as np
    from . import screen

    entries = castmod.load()
    data = np.load(screen.EMB, allow_pickle=False)
    vec = {n: v for n, v in zip(data["names"], data["vecs"])}
    chosen = {}  # key -> embedding
    for key, p in owner_picks.items():
        if p.get("file") and p["file"] != "redesign" and f"{key}|{p['file']}" in vec:
            chosen[key] = vec[f"{key}|{p['file']}"]
    books = {k: set(e.get("books", [])) for k, e in entries.items()}
    out = {}
    order = [k for k, e in sorted(entries.items(), key=lambda kv: -kv[1]["words"])
             if not e.get("voice_of") and k not in chosen and candidates(k)]
    for key in order:
        sc = {}
        f = paths.VOICES / slug(key) / "screen.json"
        if f.exists():
            sc = {r["file"]: r for r in json.loads(f.read_text(encoding="utf-8"))}
        best = None
        clean = [c for c in candidates(key) if not sc.get(c["file"], {}).get("problems")]
        if not clean:  # only skipped-word flags, and most of the line said: usable as a voice sample
            clean = [c for c in candidates(key)
                     if all(p.startswith("said") for p in sc.get(c["file"], {}).get("problems", ["x"]))
                     and sc.get(c["file"], {}).get("read", 0) >= 0.8]
        for c in clean:
            if f"{key}|{c['file']}" not in vec:
                continue
            v = vec[f"{key}|{c['file']}"]
            near_book = max([float(v @ e) for k, e in chosen.items() if books[k] & books[key]] or [0.0])
            near_all = max([float(v @ e) for e in chosen.values()] or [0.0])
            score = (near_book > MAX_SIM_SAME_BOOK or near_all > MAX_SIM_ANYWHERE, near_book + near_all)
            if best is None or score < best[0]:
                best = (score, c["file"], near_book, near_all, v)
        if best is None:
            out[key] = {"file": "redesign", "why": "no candidate passed screening"}
            continue
        (over, _), file, nb, na, v = best
        if over and not accept_over:
            out[key] = {"file": "redesign", "why": f"closest candidate too like another voice ({nb:.2f} in its books, {na:.2f} anywhere)"}
            continue
        out[key] = {"file": file, "same_book": round(nb, 3), "anywhere": round(na, 3)}
        if over:  # after rounds of redesign, the least alike of all its candidates
            out[key]["over_limit"] = True
        chosen[key] = v
    return out


def make_all(choices: dict, workers: int = 4) -> dict:
    """Create the Fish voice for every {key: file}; records are written to the
    cast in one go at the end (parallel writes would clobber each other)."""
    import concurrent.futures as cf
    done, failed = {}, {}
    entries = castmod.load()
    todo = {k: f for k, f in choices.items() if not (entries.get(k, {}).get("voice") or {}).get("id")}

    def one(item):
        k, f = item
        for attempt in range(3):
            try:
                return k, choose(k, f, save=False), None
            except Exception as e:  # noqa: BLE001 - reported below
                err = str(e)
                if " 400:" in err or " 4" in err[:30]:
                    break  # a refusal, not a hiccup: asking again won't help
        return k, None, err

    with cf.ThreadPoolExecutor(workers) as ex:
        for k, rec, err in ex.map(one, todo.items()):
            (done if rec else failed)[k] = rec or err
    data = json.loads(castmod.CAST.read_text(encoding="utf-8"))
    for k, rec in done.items():
        data["cast"][k]["voice"] = rec
    castmod.CAST.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    return {"made": len(done), "already had one": len(choices) - len(todo), "failed": failed}


def _save_voice(key, record):
    data = json.loads(castmod.CAST.read_text(encoding="utf-8"))
    data["cast"][key]["voice"] = record
    castmod.CAST.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")


def audition_page(keys: list) -> str:
    """A local HTML page to listen to every candidate of these characters."""
    rows = []
    for key in keys:
        entry = castmod.load()[key]
        cands = candidates(key)
        players = "".join(
            f'<figure><audio controls preload="none" src="{slug(key)}/{c["file"]}"></audio>'
            f'<figcaption>{c["file"]}</figcaption></figure>' for c in cands) or "<p>No candidates yet.</p>"
        rows.append(f'<section><h2>{key}</h2><p class="brief">{entry.get("brief", "")}</p>'
                    f'<p class="insp">{entry.get("inspiration", "")}</p>'
                    f'<p class="line">Reads: “{reference_line(entry)}”</p>{players}</section>')
    html = ("<!doctype html><meta charset=utf-8><title>Casting room</title><style>"
            "body{font:16px/1.5 system-ui;max-width:900px;margin:2rem auto;padding:0 16px}"
            "section{border-top:1px solid #ccc;padding:1rem 0}.insp{color:#666;font-size:.9em}"
            "figure{display:inline-block;margin:.3rem 1rem .3rem 0}</style>"
            "<h1>Casting room</h1><p>Pick one candidate per character, then "
            "<code>python -m abible voices choose \"KEY\" FILE</code>.</p>" + "".join(rows))
    page = paths.VOICES / "audition.html"
    page.write_text(html, encoding="utf-8")
    return str(page)
