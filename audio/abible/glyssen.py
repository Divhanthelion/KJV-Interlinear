"""SIL Glyssen's character data (MIT; Copyright 2014-2020 SIL LSDev and Faith
Comes By Hearing): who speaks in every verse, what each character is like,
and the English reference script (the World English Bible split into blocks
by speaker), which is what FCBH records its dramatized Bibles from."""
import re
import xml.etree.ElementTree as ET
from collections import defaultdict
from dataclasses import dataclass
from functools import lru_cache

from . import sources

NARRATOR = "narrator"


@dataclass(frozen=True)
class Piece:
    who: str  # Glyssen character id, or "narrator"
    delivery: str | None
    script_as: str | None  # FCBH's choice when `who` names several ("Orpah/Ruth" -> "Orpah")
    text: str  # WEB text of this piece


@dataclass(frozen=True)
class Character:
    id: str
    max_speakers: int  # -1: a group of unknown size
    gender: str
    age: str
    comment: str
    reference: str
    fcbh: str


@lru_cache(maxsize=None)
def characters() -> dict:
    out = {}
    for row in sources.path("glyssen/CharacterDetail.txt").read_text(encoding="utf-8").splitlines():
        if not row or row.startswith("#"):
            continue
        f = (row.split("\t") + [""] * 8)[:8]
        out[f[0]] = Character(f[0], int(f[1] or 1), f[2], f[3], f[5], f[6], f[7])
    return out


@lru_cache(maxsize=None)
def verse_entries() -> dict:
    """{(code, chapter, verse): [dict of CharacterVerse.txt columns]}."""
    cols = ["code", "c", "v", "who", "delivery", "alias", "quote_type", "default", "parallel", "position"]
    out = defaultdict(list)
    for row in sources.path("glyssen/CharacterVerse.txt").read_text(encoding="utf-8").splitlines():
        if not row or row.startswith("#") or row.startswith("Control"):
            continue
        f = dict(zip(cols, row.split("\t")))
        try:
            key = (f["code"], int(f["c"]), int(f["v"]))
        except (KeyError, ValueError):
            continue
        out[key].append(f)
    return out


def _who(character_id: str) -> str:
    return NARRATOR if character_id.startswith(("narrator-", "interruption-", "extra-")) else character_id


@lru_cache(maxsize=None)
def reference(code: str) -> dict:
    """{(chapter, verse): [Piece]} from the English reference script, in order.
    Adjacent pieces by the same speaker with the same delivery are joined."""
    root = ET.parse(sources.path(f"glyssen/English/{code}.xml")).getroot()
    out = defaultdict(list)
    for b in root.iter("block"):
        chapter = int(b.get("chapter"))
        cid = b.get("characterId") or ""
        if chapter == 0 or cid.startswith("BC-") or b.get("style") in ("c", "mt", "mt1", "mt2", "cl", "ms", "s", "s1"):
            continue  # book and chapter titles and section heads: the KJV has its own
        verse = int(b.get("initialStartVerse"))
        who, delivery, script_as = _who(cid), b.get("delivery"), b.get("characterIdOverrideForScript")
        for el in b:
            if el.tag == "verse":
                verse = int(re.match(r"\d+", el.get("num")).group())
            elif el.tag == "text" and (el.text or "").strip():
                pieces = out[(chapter, verse)]
                text = el.text.strip()
                if pieces and pieces[-1].who == who and pieces[-1].delivery == delivery:
                    last = pieces[-1]
                    pieces[-1] = Piece(who, delivery, last.script_as or script_as, last.text + " " + text)
                else:
                    pieces.append(Piece(who, delivery, script_as, text))
    return dict(out)
