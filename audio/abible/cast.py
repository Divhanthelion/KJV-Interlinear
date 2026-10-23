"""The cast: one voice for every speaker in the script.

audio/cast/cast.json is generated from the script and Glyssen's character
details; it keeps whatever was decided before (briefs written by hand in
principals.json, Fish voice ids, the chosen design candidate). Casting keys
are what a line is voiced as: line["as"] if set, else line["who"].

Policies (audio/cast/policy.json):
  - "(old)" variants share their character's voice, with the delivery saying
    the age, so a voice ages instead of being recast ("(child)" and
    "(young)" get voices of their own age);
  - "A/B" keys spoken together are led by A (or by FCBH's choice, "as");
  - the narrator lines of chosen books can be read by the book's author
    ("author_voices"), as the Word of Promise recording did for the letters.
"""
import hashlib
import json
import re
from collections import Counter, defaultdict

from . import books, glyssen, paths, script
from .glyssen import NARRATOR

CAST = paths.CAST / "cast.json"
PRINCIPALS = paths.CAST / "principals.json"
POLICY = paths.CAST / "policy.json"

# Only the old share a voice: a child or a girl needs a voice of her own age.
AGE_VARIANT = re.compile(r"^(.*) \((old)\)$")


def policy() -> dict:
    return json.loads(POLICY.read_text(encoding="utf-8")) if POLICY.exists() else {}


def voice_key(line: dict, code: str | None = None) -> str:
    """Which cast entry voices this line."""
    key = line.get("as") or line["who"]
    if key in ("scripture", "Needs Review"):
        key = NARRATOR  # an unattributed quotation; "Needs Review" lines are listed for a person to settle
    if key == NARRATOR and code and line.get("kind") is None:
        author = policy().get("author_voices", {}).get(code)
        if author:
            return author
    return key


def base_of(key: str) -> tuple[str, str | None]:
    """'David (old)' -> ('David', 'old'); others unchanged."""
    m = AGE_VARIANT.match(key)
    if m and m.group(1) in glyssen.characters():
        return m.group(1), m.group(2)
    return key, None


def usage() -> dict:
    """{key: {"words", "lines", "books", "first", "sample"}} over the whole script."""
    use = defaultdict(lambda: {"words": 0, "lines": 0, "books": [], "first": None, "samples": []})
    for code in books.CODES:
        book = script.load(code)
        for ch in book["chapters"]:
            for line in ch["lines"]:
                key = voice_key(line, code)
                u = use[key]
                u["words"] += len(line["text"].split())
                u["lines"] += 1
                if code not in u["books"]:
                    u["books"].append(code)
                ref = f"{code} {ch['chapter']}:{line.get('v', 0)}"
                u["first"] = u["first"] or ref
                if 40 <= len(line["text"]) <= 150 and len(u["samples"]) < 12:
                    u["samples"].append([ref, line["text"]])
    return dict(use)


# Variation axes for briefs nobody wrote by hand. Each character gets one value
# per axis from a hash of its id, then neighbours in the same books are pushed
# apart (see _spread).
PITCH = ["low", "low-middle", "middle", "middle-high", "high"]
TEXTURE = ["smooth", "slightly husky", "gravelly", "bright and clear", "breathy", "reedy", "rich and resonant", "lightly raspy"]
PACE = ["slow and deliberate", "measured", "brisk", "quick and urgent"]
TEMPER = ["warm", "stern", "anxious", "proud", "earnest", "weary", "cheerful", "cold", "gentle", "forceful"]


def _h(s: str, n: int, salt: str) -> int:
    return int(hashlib.sha256((salt + s).encode()).hexdigest(), 16) % n


ROLE_WORDS = [
    (r"\bking\b|\bqueen\b|pharaoh|emperor|caesar", "royal bearing, used to being obeyed"),
    (r"prophet|seer|man of god", "a prophet's conviction"),
    (r"priest|levite", "a priest's formality"),
    (r"angel|seraph|cherub|living creature|elder", "otherworldly calm and clarity"),
    (r"servant|slave|maid", "deferential, careful with words"),
    (r"soldier|commander|captain|officer|centurion|warrior|army|guard", "a soldier's clipped directness"),
    (r"messenger|herald|watchman", "a messenger's urgency"),
    (r"demon|spirit, evil|unclean", "harsh and unsettling, with an inhuman edge"),
    (r"pharisee|scribe|teacher|lawyer|sadducee", "learned and exacting"),
    (r"woman|wife|mother|daughter|widow", ""),
    (r"shepherd|farmer|fisherman|harvester|reaper", "plain, outdoor speech"),
    (r"governor|official|ruler|noble|prince|chief", "official, measured authority"),
]


def auto_brief(key: str, d, use: dict) -> str:
    """A voice-design prompt from what Glyssen says about the character."""
    base, variant = base_of(key)
    gender = (d.gender if d else "") or ""
    age = (d.age if d else "") or ""
    group = d is not None and d.max_speakers != 1
    g = {"Male": "man", "PreferMale": "man", "Female": "woman", "PreferFemale": "woman"}.get(gender, "person")
    years = {"Child": "a child of about ten", "YoungAdult": f"a young {g} of about twenty",
             "Elder": f"an elderly {g} of about seventy", "Adult": f"a {g} of about {30 + _h(key, 25, 'age')}"}.get(
        age, f"a {g} of about {25 + _h(key, 40, 'age')}")
    # Only the head of the name, or its parenthesis, says who speaks: "slave girls"
    # and "servant (young girl)" are young; "owners of fortune telling slave girl" are not.
    # Glyssen also writes names inside out ("Gilead, elders of"), and collectives
    # name their members after "of" ("council of elders").
    parts = key.split(" (")[0].split(" of ")
    head = parts[0]
    if head.strip().lower() in ("council", "company", "band", "group", "assembly", "crowd") and len(parts) > 1:
        head += " " + parts[1]
    head += " " + " ".join(re.findall(r"\(([^)]*)\)", key))
    if re.search(r"\b(girls?|lads?|boys?|child(ren)?)\b", head, re.I) and age != "Elder":
        years = f"a young {g} of about {12 + _h(key, 6, 'age')}"
    elif re.search(r"\b(young|youths?|maids?|maidens?|damsels?)\b", head, re.I) and age != "Elder":
        years = f"a young {g} of about {18 + _h(key, 8, 'age')}"
    elif re.search(r"\b(old|elders?|aged|grey|gray)\b", head, re.I) and age not in ("Child", "YoungAdult"):
        years = f"an elderly {g} of about {65 + _h(key, 16, 'age')}"
    if group:
        years = f"one voice speaking for a group ({key}): {years}"
    role = next((r for pat, r in ROLE_WORDS if re.search(pat, key, re.I)), "")
    parts = [
        f"{years[0].upper()}{years[1:]}.",
        f"{PITCH[_h(key, len(PITCH), 'p')].capitalize()} pitch, {TEXTURE[_h(key, len(TEXTURE), 't')]} voice,"
        f" {PACE[_h(key, len(PACE), 's')]}, {TEMPER[_h(key, len(TEMPER), 'm')]} in manner.",
    ]
    if role:
        parts.append(role[0].upper() + role[1:] + ".")
    parts.append("Natural, unaffected English, clear diction, no caricature.")
    return " ".join(parts)


def build() -> dict:
    """Regenerate cast.json from the script, keeping earlier decisions."""
    old = json.loads(CAST.read_text(encoding="utf-8"))["cast"] if CAST.exists() else {}
    principals = json.loads(PRINCIPALS.read_text(encoding="utf-8")) if PRINCIPALS.exists() else {}
    details = glyssen.characters()
    use = usage()
    cast = {}
    for key, u in sorted(use.items(), key=lambda kv: -kv[1]["words"]):
        base, variant = base_of(key)
        d = details.get(key)
        entry = {
            "words": u["words"], "lines": u["lines"], "books": u["books"], "first": u["first"],
            "gender": d.gender if d else "", "age": d.age if d else "",
            "group": bool(d and d.max_speakers != 1),
        }
        if variant:
            entry["voice_of"] = base
            entry["aged"] = variant
        elif "/" in key and not d:
            members = [m for m in key.split("/") if m]
            entry["members"] = members
            entry["voice_of"] = members[0] if members[0] in details else None
        p = principals.get(key)
        if p:
            entry.update({k: v for k, v in p.items()})
            entry["brief_by"] = "hand"
        elif "voice_of" not in entry or not entry["voice_of"]:
            entry["brief"] = auto_brief(key, d, u)
            entry["brief_by"] = "auto"
        entry["samples"] = u["samples"]
        prev = old.get(key, {})
        for keep in ("voice", "design", "status", "notes"):
            if keep in prev:
                entry[keep] = prev[keep]
        cast[key] = entry
    # characters who only ever speak in company ("Deborah" of "Deborah/Barak")
    # still need an entry of their own to hold the voice
    for key in [e["voice_of"] for e in cast.values() if e.get("voice_of")]:
        if key not in cast:
            d = details.get(key)
            p = principals.get(key)
            cast[key] = {"words": 0, "lines": 0, "books": [], "first": None,
                         "gender": d.gender if d else "", "age": d.age if d else "",
                         "group": bool(d and d.max_speakers != 1),
                         **({**p, "brief_by": "hand"} if p else {"brief": auto_brief(key, d, {}), "brief_by": "auto"}),
                         "samples": []}
            prev = old.get(key, {})
            for keep in ("voice", "design", "status", "notes"):
                if keep in prev:
                    cast[key][keep] = prev[keep]
    out = {"about": "Generated by `python -m abible cast`; edit principals.json and policy.json, not this file.",
           "cast": cast}
    CAST.write_text(json.dumps(out, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    return cast


def load() -> dict:
    return json.loads(CAST.read_text(encoding="utf-8"))["cast"]


def resolve(key: str, cast: dict | None = None) -> tuple[str, str | None]:
    """The cast entry whose voice speaks for `key`, and the age to play."""
    cast = cast or load()
    seen, aged = set(), None
    while key in cast and cast[key].get("voice_of") and key not in seen:
        seen.add(key)
        aged = aged or cast[key].get("aged")
        key = cast[key]["voice_of"]
    return key, aged


def summary(cast: dict) -> Counter:
    c = Counter()
    for key, e in cast.items():
        c["entries"] += 1
        c["voices to design" if not e.get("voice_of") else "share another voice"] += 1
        c[f"briefs by {e.get('brief_by', 'none')}"] += 1
        if e.get("voice"):
            c["voices made"] += 1
    return c
