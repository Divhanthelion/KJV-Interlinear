"""python -m abible <command>   (run from audio/; see audio/README.md)

  sources pin | fetch          pin or fetch the third-party sources (sources/sources.json)
  extras                       Psalm 119's letters and the colophons, from the pinned USFM
  script [BOOK...]             build the KJV script (script/kjv/) and its review list
  cast                         rebuild cast/cast.json from the script and the briefs
  credit                       the Fish account's API credit
  voices design KEY... [--n N] Voice Design candidates for these characters (needs API credit)
  voices top N                 design candidates for the N characters who speak most and have no voice
  voices audition KEY...       write the casting page (.cache/voices/audition.html)
  voices choose KEY FILE       make the chosen candidate the character's voice
  render BOOK [CH...]          voice chapters into .cache/out/kjv/ (all chapters if none given)
  run [BOOK...] [--workers 5]  render everything (or these books), 5 chapters at a time, resumable
  qa BOOK [CH...]              check rendered chapters (run with the QA environment)
  app SCRIPTORIUM [--kbps 16] [--partial]  hand the rendered KJV to Scriptorium's Listen feature (catalogue + Ogg Opus)
"""
import json
import sys


def main(argv):
    if not argv:
        print(__doc__)
        return 1
    cmd, args = argv[0], argv[1:]
    if cmd == "sources":
        from . import sources
        {"pin": sources.pin, "fetch": sources.fetch}[args[0]]()
    elif cmd == "extras":
        from . import kjv
        kjv.build_extras()
    elif cmd == "script":
        from . import script
        for k, v in sorted(script.build_all(args or None).items()):
            print(f"{v:8d}  {k}")
    elif cmd == "cast":
        from . import cast
        for k, v in sorted(cast.summary(cast.build()).items()):
            print(f"{v:8d}  {k}")
    elif cmd == "credit":
        from . import fish
        print(json.dumps(fish.credit(), indent=1))
    elif cmd == "voices":
        from . import cast, voices
        sub = args[0]
        if sub == "design":
            n = int(args[args.index("--n") + 1]) if "--n" in args else 4
            keys = [a for i, a in enumerate(args) if i > 0 and a != "--n" and args[i - 1] != "--n"]
            for key in keys:
                print(key, [c["file"] for c in voices.design(key, n=n)])
        elif sub == "top":
            entries = cast.load()
            todo = [k for k, e in sorted(entries.items(), key=lambda kv: -kv[1]["words"])
                    if not e.get("voice_of") and not e.get("voice") and not voices.candidates(k)][:int(args[1])]
            for key in todo:
                print(key, [c["file"] for c in voices.design(key)])
        elif sub == "audition":
            print(voices.audition_page(args[1:]))
        elif sub == "choose":
            print(json.dumps(voices.choose(args[1], args[2]), indent=1))
    elif cmd == "render":
        from . import render, script
        code = args[0]
        chapters = [int(a) for a in args[1:]] or [c["chapter"] for c in script.load(code)["chapters"]]
        for ch in chapters:
            t = render.render_chapter(code, ch)
            print(f"{code} {ch}: {t['duration']:.0f} s, {len(t['renders'])} request(s)")
    elif cmd == "run":
        from . import run
        workers = int(args[args.index("--workers") + 1]) if "--workers" in args else 5
        codes = [a for i, a in enumerate(args) if not a.startswith("--") and (i == 0 or args[i - 1] != "--workers")]
        done, failures = run.run(codes or None, workers=workers, force="--force" in args)
        print(f"{done} chapters rendered, {len(failures)} failed (see .cache/out/kjv/progress.log)")
    elif cmd == "qa":
        from . import paths, qa, script
        code = args[0]
        chapters = [int(a) for a in args[1:]] or [c["chapter"] for c in script.load(code)["chapters"]]
        for ch in chapters:
            r = qa.check_chapter(paths.OUT / "kjv", code, ch)
            w = r["words"]
            print(f"{code} {ch}: listen {len(w['listen'])}, primed-ok {len(w['fits_when_primed'])}, "
                  f"names {len(w['names_spelled_differently'])}, voices flagged {len(r['voices']['flagged'])}, "
                  f"loud {len(r['sound'].get('loud', []))}, babble {len(r['sound'].get('babble', []))}")
    elif cmd == "app":
        from pathlib import Path
        from . import app_export
        kbps = int(args[args.index("--kbps") + 1]) if "--kbps" in args else 16
        # the folder: the one argument that is neither a switch nor --kbps's value
        folders = [a for i, a in enumerate(args) if not a.startswith("--") and (i == 0 or args[i - 1] != "--kbps")]
        if len(folders) != 1:
            print(__doc__)
            return 1
        app_export.export(Path(folders[0]), kbps=kbps, partial="--partial" in args)
    else:
        print(__doc__)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
