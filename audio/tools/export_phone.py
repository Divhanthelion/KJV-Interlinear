"""Tag the rendered chapters as MP3s (one folder per book) under .cache/export/.

Run from audio/:  python tools/export_phone.py
The folder can then be copied to a phone or Google Drive. Needs ffmpeg on PATH.
"""
import sys, subprocess, concurrent.futures as cf
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
sys.stdout.reconfigure(encoding="utf-8")
from abible import books, paths, run
src = paths.OUT / "kjv"
dst = paths.CACHE / "export" / "KJV Dramatized Bible"
jobs = []
for n, (code, _, title, short) in enumerate(books.BOOKS, 1):
    chapters = [ch for c, ch in run.chapters([code])]
    folder = dst / f"{n:02d} {short}"
    folder.mkdir(parents=True, exist_ok=True)
    width = 3 if len(chapters) >= 100 else 2
    for ch in chapters:
        label = ("Psalm " if code == "PSA" else f"{short} ") + str(ch)
        out = folder / f"{short} {ch:0{width}d}.mp3"
        jobs.append((src / f"{code}.{ch}.mp3", out, label, n, short, ch, len(chapters)))
def tag(j):
    s, out, label, n, short, ch, total = j
    if out.exists() and out.stat().st_mtime >= s.stat().st_mtime:
        return  # already exported from this render
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(s), "-c", "copy", "-id3v2_version", "3",
                    "-metadata", f"title={label}", "-metadata", f"album=KJV Dramatized Bible: {short}",
                    "-metadata", "artist=KJV Dramatized Bible", "-metadata", "album_artist=KJV Dramatized Bible",
                    "-metadata", f"track={ch}/{total}", "-metadata", f"disc={n}/66", "-metadata", "genre=Audiobook",
                    "-metadata", "comment=King James Version (1769). Voices generated with Fish Audio; speakers adapted from Glyssen (SIL, FCBH).",
                    str(out)], check=True)
with cf.ThreadPoolExecutor(8) as ex:
    list(ex.map(tag, jobs))
print(len(jobs), "files in", dst)
