"""Where things live. Everything generated but not committed goes under audio/.cache."""
from pathlib import Path

AUDIO = Path(__file__).resolve().parent.parent
REPO = AUDIO.parent
CACHE = AUDIO / ".cache"
SOURCES = CACHE / "sources"
RENDERS = CACHE / "renders"
VOICES = CACHE / "voices"
OUT = CACHE / "out"

SCRIPT = AUDIO / "script"
CAST = AUDIO / "cast"
LEXICON = AUDIO / "lexicon"


def ensure(*dirs: Path) -> None:
    for d in dirs:
        d.mkdir(parents=True, exist_ok=True)
