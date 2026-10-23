"""The dramatized script: every verse as lines, each with one speaker.

audio/script/kjv/<BOOK>.json holds, per chapter, the narrator's announcement,
then each verse's lines in order. Joining a verse's lines with single spaces
gives back the verse exactly (checked here and by tests/test_script.py).
Corrections a person makes go in audio/script/kjv-overrides.json, never in
the generated files; review.json lists every verse that needs a look.
"""
import json
from collections import Counter

from . import books, glyssen, kjv, paths, project
from .glyssen import NARRATOR, Piece

OUT = paths.SCRIPT / "kjv"
OVERRIDES = paths.SCRIPT / "kjv-overrides.json"
REVIEWED = paths.SCRIPT / "kjv-reviewed.json"  # checked by a person and right as made: not listed for review


def _overrides() -> dict:
    if not OVERRIDES.exists():
        return {}
    return json.loads(OVERRIDES.read_text(encoding="utf-8"))["verses"]


def _scripture_as(code, c, v):
    """For Glyssen's 'scripture' quotations, the prophet being quoted, if named."""
    for e in glyssen.verse_entries().get((code, c, v), []):
        if e["who"] == "scripture" and e.get("default"):
            return e["default"]
    return None


def _line(v, piece: Piece, text, code, c):
    line = {"v": v, "who": piece.who, "text": text}
    script_as = piece.script_as or (_scripture_as(code, c, v) if piece.who == "scripture" else None)
    if script_as:
        line["as"] = script_as
    if piece.delivery:
        line["delivery"] = piece.delivery
    return line


def build_book(code: str):
    """Returns (book dict, review entries)."""
    text = kjv.verses(code)
    ref = glyssen.reference(code)
    red = kjv.red_letter()
    extras = kjv.extras()
    fixes = _overrides()
    reviewed = json.loads(REVIEWED.read_text(encoding="utf-8"))["verses"] if REVIEWED.exists() else {}
    chapters, review = {}, []
    for (c, v) in sorted(text):
        lines = chapters.setdefault(c, [{"kind": "announce", "who": NARRATOR, "text": books.announcement(code, c)}])
        if code == "PSA" and c == 119:
            for h in extras["psalm119_headings"]:
                if h["before_verse"] == v:
                    lines.append({"kind": "heading", "who": NARRATOR, "text": h["text"]})
        key = f"{code} {c}:{v}"
        verse = text[(c, v)]
        if key in fixes:
            fix = fixes[key]
            for item in fix["lines"]:
                line = {"v": v, "who": item[0], "text": item[1]}
                if len(item) > 2 and item[2]:
                    line["delivery"] = item[2]
                lines.append(line)
            continue
        pieces = ref.get((c, v))
        notes = []
        if not pieces:
            segs = [(Piece(NARRATOR, None, None, ""), verse)]
        else:
            segs, notes = project.project_verse(verse, pieces)
            if segs is None:
                segs = [(Piece(NARRATOR, None, None, ""), verse)]
                notes = notes + ["left with the narrator until reviewed"]
        if (code, c, v) in red:
            segs, more = project.apply_red_letter(verse, segs, red[(code, c, v)])
            notes += more
        elif code in books.RED_LETTER_NAME:
            segs, more = project.no_red_letter(segs)
            notes += more
        if any(p.who == "Needs Review" for p, _ in segs):
            notes.append("Glyssen itself leaves this speaker undecided ('Needs Review'): narrator until settled")
        for p, t in segs:
            lines.append(_line(v, p, t, code, c))
        if notes and key not in reviewed:
            review.append({"ref": key, "notes": notes, "kjv": verse,
                           "lines": [[p.who, t] for p, t in segs],
                           "reference": [[p.who, p.text] for p in (pieces or [])]})
    colophon = extras["colophons"].get(code)
    if colophon:
        last = max(chapters)
        chapters[last].append({"kind": "colophon", "who": NARRATOR, "text": colophon})
    book = {"book": code, "translation": "KJV", "chapters": [{"chapter": c, "lines": chapters[c]} for c in sorted(chapters)]}
    return book, review


def check_book(book: dict) -> list:
    """Every verse's lines, joined with spaces, must be the verse exactly."""
    text = kjv.verses(book["book"])
    got = {}
    for ch in book["chapters"]:
        for line in ch["lines"]:
            if "v" in line:
                got.setdefault((ch["chapter"], line["v"]), []).append(line["text"])
                if not line["text"] or line["text"] != line["text"].strip():
                    return [f"{book['book']} {ch['chapter']}:{line['v']}: empty or padded line"]
    errors = []
    for key, verse in text.items():
        joined = " ".join(got.get(key, []))
        if joined != verse:
            errors.append(f"{book['book']} {key[0]}:{key[1]}: lines give {joined!r}, the verse is {verse!r}")
    for key in got:
        if key not in text:
            errors.append(f"{book['book']} {key[0]}:{key[1]}: not a verse")
    return errors


def build_all(codes=None) -> Counter:
    paths.ensure(OUT)
    stats, review = Counter(), []
    for code in codes or books.CODES:
        book, rev = build_book(code)
        errors = check_book(book)
        if errors:
            raise SystemExit("\n".join(errors[:20]))
        (OUT / f"{code}.json").write_text(_dump(book), encoding="utf-8")
        review += rev
        for ch in book["chapters"]:
            for line in ch["lines"]:
                stats["lines"] += 1
                stats["narrator lines" if line["who"] == NARRATOR else "character lines"] += 1
        stats["verses needing review"] += len(rev)
    if not codes:
        (OUT / "review.json").write_text(json.dumps(review, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    return stats


def _dump(book: dict) -> str:
    """One line per script line, so diffs stay readable."""
    out = ['{"book": %s, "translation": "KJV", "chapters": [' % json.dumps(book["book"])]
    for i, ch in enumerate(book["chapters"]):
        out.append(' {"chapter": %d, "lines": [' % ch["chapter"])
        out.append(",\n".join("  " + json.dumps(line, ensure_ascii=False) for line in ch["lines"]))
        out.append(" ]}" + ("," if i < len(book["chapters"]) - 1 else ""))
    out.append("]}")
    return "\n".join(out) + "\n"


def load(code: str) -> dict:
    return json.loads((OUT / f"{code}.json").read_text(encoding="utf-8"))
