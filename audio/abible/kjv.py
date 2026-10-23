"""The KJV text exactly as the app has it (old_testament/, new_testament/; Psalm
titles are verse 0), the red letter (data/words_of_jesus.json), and the 1769
text that sits outside verses: Psalm 119's letter headings and the epistles'
colophons, taken from eBible's USFM (audio/sources/kjv-extras.json)."""
import json
import re
import zipfile
from functools import lru_cache

from . import books, paths, sources

EXTRAS = paths.AUDIO / "sources" / "kjv-extras.json"


@lru_cache(maxsize=None)
def verses(code: str) -> dict:
    """{(chapter, verse): text} for one book, in the repo's text."""
    out = {}
    with open(paths.REPO / books.BY_CODE[code][1], encoding="utf-8") as f:
        for line in f:
            line = line.rstrip("\n")
            if not line:
                continue
            ref, text = line.split(" ", 1)
            c, v = ref.split(":")
            out[(int(c), int(v))] = text
    return out


@lru_cache(maxsize=None)
def red_letter() -> dict:
    """{(code, chapter, verse): [spans spoken by Christ, in order]}."""
    data = json.loads((paths.REPO / "data" / "words_of_jesus.json").read_text(encoding="utf-8"))["verses"]
    by_name = {v: k for k, v in books.RED_LETTER_NAME.items()}
    out = {}
    for key, spans in data.items():
        name, ref = key.rsplit(" ", 1)
        c, v = ref.split(":")
        out[(by_name[name], int(c), int(v))] = spans
    return out


def build_extras() -> None:
    """Extract Psalm 119's headings and the colophons from the pinned USFM."""
    z = zipfile.ZipFile(sources.path("ebible/eng-kjv_usfm.zip"))
    headings, colophons = [], {}
    for name in sorted(z.namelist()):
        if not name.endswith(".usfm"):
            continue
        t = z.read(name).decode("utf-8-sig")
        code = re.search(r"\\id (\w+)", t).group(1)
        if code not in books.BY_CODE:
            continue
        if code == "PSA":
            chapter = None
            for m in re.finditer(r"\\c (\d+)|\\s1 \\tl\s+\S+\s+([A-Z]+)\.\\tl\*[\s\S]*?\\v (\d+) ", t):
                if m.group(1):
                    chapter = int(m.group(1))
                elif chapter == 119:
                    # the letter's name, before the verse it heads
                    headings.append({"chapter": 119, "before_verse": int(m.group(3)), "text": m.group(2).title() + "."})
        for s in re.findall(r"\\s1 ([^\n]*)", t):
            if "\\tl" in s:
                continue
            clean = re.sub(r"\\\+?w ([^|\\]*)\|[^\\]*\\\+?w\*", r"\1", s)
            clean = re.sub(r"\\\+?(add|nd)\*?", "", clean)
            colophons[code] = re.sub(r"\s+", " ", clean).strip()
    EXTRAS.write_text(json.dumps({
        "source": "eBible.org eng-kjv USFM (1769 KJV, public domain); see sources.json for its SHA-256",
        "psalm119_headings": headings,
        "colophons": colophons,
    }, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")


@lru_cache(maxsize=None)
def extras() -> dict:
    return json.loads(EXTRAS.read_text(encoding="utf-8"))
