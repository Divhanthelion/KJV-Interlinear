"""The hearing gate: a take from Fish is accepted only when a recognizer hears every
word of its script in its place and every line is in its own voice (run with the QA
environment; `abible.redo` sets it on render.HEARING).

- Words: omissions.audit (the take transcribed, aligned word for word, every difference
  heard again in place). Its "missing", "changed" and "added" turn the take down;
  "names" and "listen" go with the take's verdict for a person's ear.
- Voices: abible.voicecheck (every line compared with the reference clip of every voice
  in the request, short lines pooled per voice).

Each verdict carries CHECKS_VERSION, a fingerprint of the checking code and its
settings: a verdict made by other checks is no verdict, and the take is heard again
(no new request to Fish). Takes accepted by the first, weaker checks were reused that
way before (Matthew 5 and Psalm 89 came out of a "fix" run with the same audio).
"""
import hashlib
import threading
from pathlib import Path

from . import omissions, qa, render, voicecheck

_gpu = threading.Lock()


def _fingerprint() -> str:
    here = Path(__file__).parent
    h = hashlib.sha256()
    for name in ("gate.py", "omissions.py", "qa.py", "voicecheck.py"):
        h.update((here / name).read_bytes())
    return h.hexdigest()[:16]


CHECKS_VERSION = _fingerprint()
FAULTS = ("missing", "changed", "added")


def faults(lines: list, mp3, words: list, voices: dict) -> dict:
    """The take's verdict: {"faults": [...], "names": [...], "listen": [...] (words),
    "voice_listen": [...], "voices": {"checked", "unchecked"}}. Every item carries "line",
    its index in `lines`, so a take shared by two chapters can be placed in each."""
    spans = render.line_times(lines, words)
    with _gpu:  # the recognizers; the voice check runs on the CPU, outside the lock
        heard = omissions.take_transcript(mp3)
        report = omissions.audit(lines, mp3, heard, spans)
    found = [{"fault": kind} | item for kind in FAULTS for item in report[kind]]
    timeline = [{"line": n, "v": line.get("v"), "who": line.get("who"), "key": line["key"],
                 "voice": voices[line["key"]][0], "start": start, "end": end}
                for n, (line, (start, end)) in enumerate(zip(lines, spans))]
    vc = voicecheck.check_lines(qa.pcm16k(mp3), timeline)
    found += [{"fault": "wrong voice"} | item for item in vc["wrong"]]
    return {"faults": found, "names": report["names"], "listen": report["listen"],
            "voice_listen": vc["listen"], "voices": {"checked": vc["checked"], "unchecked": vc["unchecked"]}}
