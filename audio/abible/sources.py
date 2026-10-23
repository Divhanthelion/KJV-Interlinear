"""Third-party sources, pinned by URL and SHA-256 (audio/sources/sources.json).

`pin` downloads every file once and records its hash; `fetch` downloads
whatever is missing from the cache and refuses any file whose hash changed.
"""
import hashlib
import json
import time
import urllib.request

from . import books, paths

MANIFEST = paths.AUDIO / "sources" / "sources.json"
GLYSSEN_COMMIT = "e912d2097dffc625778782c18043e21d8820c414"  # sillsdev/Glyssen master, 2026-08-25
GLYSSEN_RAW = f"https://raw.githubusercontent.com/sillsdev/Glyssen/{GLYSSEN_COMMIT}/"


def wanted():
    """(local path under .cache/sources, URL) for every source file."""
    out = [
        ("glyssen/LICENSE", GLYSSEN_RAW + "LICENSE"),
        ("glyssen/CharacterDetail.txt", GLYSSEN_RAW + "GlyssenCharacters/Resources/CharacterDetail.txt"),
        ("glyssen/CharacterVerse.txt", GLYSSEN_RAW + "GlyssenCharacters/Resources/CharacterVerse.txt"),
    ]
    for code in books.CODES:
        out.append((f"glyssen/English/{code}.xml", GLYSSEN_RAW + f"DistFiles/reference_texts/English/{code}.xml"))
    out.append(("ebible/eng-kjv_usfm.zip", "https://ebible.org/Scriptures/eng-kjv_usfm.zip"))
    return out


def _get(url: str) -> bytes:
    last = None
    for attempt in range(6):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "abible/1.0"})
            return urllib.request.urlopen(req, timeout=120).read()
        except Exception as e:  # flaky connections here reset often; try again
            last = e
            time.sleep(2 + 3 * attempt)
    raise RuntimeError(f"could not download {url}: {last}")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def pin() -> None:
    entries = []
    for rel, url in wanted():
        dest = paths.SOURCES / rel
        data = dest.read_bytes() if dest.exists() else _get(url)
        paths.ensure(dest.parent)
        dest.write_bytes(data)
        entries.append({"path": rel, "url": url, "sha256": sha256(data), "bytes": len(data)})
        print(f"pinned {rel} {len(data)} bytes")
    MANIFEST.write_text(json.dumps({
        "pinned": time.strftime("%Y-%m-%d"),
        "glyssen_commit": GLYSSEN_COMMIT,
        "files": entries,
    }, indent=1) + "\n", encoding="utf-8")


def fetch() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    for e in manifest["files"]:
        dest = paths.SOURCES / e["path"]
        if dest.exists() and sha256(dest.read_bytes()) == e["sha256"]:
            continue
        data = _get(e["url"])
        if sha256(data) != e["sha256"]:
            raise RuntimeError(f"{e['path']}: the file at {e['url']} has changed since it was pinned "
                               f"(expected {e['sha256']}, got {sha256(data)}). Check what changed before re-pinning.")
        paths.ensure(dest.parent)
        dest.write_bytes(data)
        print(f"fetched {e['path']}")


def path(rel: str):
    p = paths.SOURCES / rel
    if not p.exists():
        raise FileNotFoundError(f"{p} is missing: run `python -m abible sources fetch`")
    return p
