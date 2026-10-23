"""Pronunciations the voice gets wrong, fixed with Fish's phoneme tags
(CMU Arpabet, one word per tag: <|phoneme_start|>EH1 N ...<|phoneme_end|>).

audio/lexicon/kjv.tsv, one rule per line:
    pattern <TAB> arpabet <TAB> source / note
The pattern is a regular expression matched case-sensitively on whole words;
it may carry context so a heteronym is fixed only where it means one thing
("lead(?= sank)" is the metal). Only the word itself is replaced: put context
in lookarounds. Every entry needs a source (dictionary, or the reading
checked by ear) in its note.

A second column that starts with "=" is a respelling Fish reads as plain text
("=showbread" for "shewbread"), for a word whose modern spelling it says right:
no phoneme tag, so Fish's timestamps for it stay whole. A capital at the start of
the matched word is kept. Only what Fish is sent changes; the script keeps the
KJV's spelling.
"""
import re
from functools import lru_cache

from . import paths

FILE = paths.LEXICON / "kjv.tsv"
TAG = "<|phoneme_start|>{}<|phoneme_end|>"


@lru_cache(maxsize=None)
def rules():
    out = []
    if not FILE.exists():
        return out
    for row in FILE.read_text(encoding="utf-8").splitlines():
        if not row.strip() or row.startswith("#"):
            continue
        pattern, arpabet, *_ = row.split("\t")
        out.append((re.compile(r"(?<![\w’'-])" + pattern + r"(?![\w’'-])"), arpabet.strip()))
    return out


def _respell(word: str, spelling: str) -> str:
    return spelling[:1].upper() + spelling[1:] if word[:1].isupper() else spelling


def apply(text: str) -> str:
    for rx, arpabet in rules():
        if arpabet.startswith("=") and "\\" in arpabet:  # a respelling built from the match ("=\1'st")
            text = rx.sub(lambda m, s=arpabet[1:]: m.expand(s), text)
        elif arpabet.startswith("="):
            text = rx.sub(lambda m, s=arpabet[1:]: _respell(m.group(0), s), text)
        else:
            text = rx.sub(lambda m: TAG.format(arpabet), text)
    return text
