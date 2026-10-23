"""Check every rendered chapter, following the render as it goes (run with the
QA environment):

    python -m abible.qa_run [--follow]

Writes <BOOK>.<CH>.qa.json beside each chapter and a running summary,
.cache/out/kjv/qa-summary.json. With --follow it waits for new chapters until
the render's progress log says it has ended.
"""
import json
import sys
import time

from . import paths, qa, run

OUT = paths.OUT / "kjv"


def pending():
    return [(c, ch) for c, ch in run.chapters()
            if (OUT / f"{c}.{ch}.json").exists() and not (OUT / f"{c}.{ch}.qa.json").exists()]


def summarize():
    s = {"chapters": 0, "listen": [], "voices_flagged": [], "loud": [], "babble": [], "primed_ok": 0, "names": 0}
    for f in sorted(OUT.glob("*.qa.json")):
        r = json.loads(f.read_text(encoding="utf-8"))
        s["chapters"] += 1
        ref = r["chapter"]
        s["listen"] += [dict(x, chapter=ref) for x in r["words"].get("listen", [])]
        s["voices_flagged"] += [dict(x, chapter=ref) for x in r["voices"]["flagged"]]
        s["loud"] += [[ref] + x for x in r["sound"].get("loud", [])]
        s["babble"] += [[ref] + x for x in r["sound"].get("babble", [])]
        s["primed_ok"] += len(r["words"].get("fits_when_primed", []))
        s["names"] += len(r["words"].get("names_spelled_differently", []))
    (OUT / "qa-summary.json").write_text(json.dumps(s, indent=1, ensure_ascii=False), encoding="utf-8")
    return s


def main(follow=False):
    while True:
        todo = pending()
        for code, ch in todo:
            try:
                qa.check_chapter(OUT, code, ch)
            except Exception as e:  # noqa: BLE001 - recorded, the rest go on
                (OUT / f"{code}.{ch}.qa.json").write_text(json.dumps(
                    {"chapter": f"{code} {ch}", "error": str(e), "words": {}, "voices": {"flagged": []}, "sound": {}}),
                    encoding="utf-8")
        s = summarize()
        print(f"{time.strftime('%H:%M')} checked {s['chapters']} chapters: listen {len(s['listen'])}, "
              f"voices flagged {len(s['voices_flagged'])}, loud {len(s['loud'])}, babble {len(s['babble'])}", flush=True)
        log = OUT / "progress.log"
        ended = log.exists() and log.read_text(encoding="utf-8").rstrip().splitlines()[-1].split(" ", 2)[2].startswith("end:")
        if not follow or (ended and not pending()):
            return s
        time.sleep(60)


if __name__ == "__main__":
    main("--follow" in sys.argv)
