"""Render many chapters at once, resumably.

    python -m abible run [BOOK...] [--workers 5] [--force]

Chapters run in parallel (the Fish account allows 5 requests at a time; each
chapter's scenes go one after another). A chapter whose MP3 and timings exist
is skipped unless --force. Progress goes to .cache/out/kjv/progress.log and
failures to .cache/out/kjv/failures.json; run again to retry them (renders
already returned by Fish are cached and cost nothing).
"""
import concurrent.futures as cf
import json
import threading
import time
import traceback

from . import books, paths, render, script

OUT = paths.OUT / "kjv"
_lock = threading.Lock()


def chapters(codes=None):
    for code in codes or books.CODES:
        for ch in script.load(code)["chapters"]:
            yield code, ch["chapter"]


def _log(line):
    """Never lets a logging hiccup stop the run: on 2026-10-09 a failed write
    at chapter 207 ended the logging loop while the workers rendered on."""
    stamp = time.strftime("%Y-%m-%d %H:%M:%S ")
    for attempt in range(5):
        try:
            with _lock:
                with open(OUT / "progress.log", "a", encoding="utf-8") as f:
                    f.write(stamp + line + "\n")
            return
        except OSError:
            time.sleep(0.5 * (attempt + 1))


def run(codes=None, workers=5, force=False):
    paths.ensure(OUT)
    todo = [(c, ch) for c, ch in chapters(codes)
            if force or not ((OUT / f"{c}.{ch}.mp3").exists() and (OUT / f"{c}.{ch}.json").exists())]
    failures, done, t0 = {}, 0, time.time()
    _log(f"start: {len(todo)} chapters, {workers} at a time")

    def one(item):
        code, ch = item
        t = time.time()
        timing = render.render_chapter(code, ch)
        return code, ch, timing["duration"], len(timing["renders"]), time.time() - t

    with cf.ThreadPoolExecutor(workers) as ex:
        futures = {ex.submit(one, item): item for item in todo}
        for fut in cf.as_completed(futures):
            code, ch = futures[fut]
            try:
                _, _, dur, n, secs = fut.result()
                done += 1
                _log(f"{code} {ch}: {dur:.0f} s of audio, {n} request(s), {secs:.0f} s ({done}/{len(todo)})")
            except Exception as e:  # noqa: BLE001 - logged and retried on the next run
                failures[f"{code} {ch}"] = f"{type(e).__name__}: {e}"
                _log(f"{code} {ch}: FAILED {type(e).__name__}: {str(e)[:200]}")
                with open(OUT / "failures.trace", "a", encoding="utf-8") as f:
                    f.write(f"== {code} {ch}\n{traceback.format_exc()}\n")
    (OUT / "failures.json").write_text(json.dumps(failures, indent=1, ensure_ascii=False), encoding="utf-8")
    _log(f"end: {done} done, {len(failures)} failed, {(time.time() - t0) / 3600:.1f} h")
    return done, failures
