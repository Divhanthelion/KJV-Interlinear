"""The casting room: a local page to hear every character's candidates and pick
one. Picks are saved to .cache/voices/picks.json as you click.

    python -m abible.casting_room [PORT]      (default 8765)
"""
import html
import json
import sys
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from . import cast as castmod, paths, voices

PICKS = paths.VOICES / "picks.json"


def picks() -> dict:
    return json.loads(PICKS.read_text(encoding="utf-8")) if PICKS.exists() else {}


def _screen(key):
    f = paths.VOICES / voices.slug(key) / "screen.json"
    return {r["file"]: r for r in json.loads(f.read_text(encoding="utf-8"))} if f.exists() else {}


def _nearest():
    try:
        from . import screen
        return screen.nearest_others()
    except Exception:
        return {}


def page(everyone=False) -> str:
    entries = castmod.load()
    chosen = picks()
    near = _nearest()
    keys = [k for k, e in sorted(entries.items(), key=lambda kv: -kv[1]["words"])
            if voices.candidates(k) and not e.get("voice_of") and (everyone or e.get("brief_by") == "hand")]
    done = sum(1 for k in keys if chosen.get(k, {}).get("file"))
    rows = []
    for n, key in enumerate(keys, 1):
        e = entries[key]
        sc = _screen(key)
        cands = []
        for c in voices.candidates(key):
            s = sc.get(c["file"], {})
            f = c.get("heard_as") or {}
            heard = ", ".join(x for x in (f.get("age"), f.get("tone"), f.get("accent")) if x)
            warn = "".join(f'<span class="warn">{html.escape(p)}</span>' for p in s.get("problems", []))
            nn = near.get(f"{key}|{c['file']}")
            close = ""
            if nn and nn[1] >= 0.6:
                close = f'<span class="close">close to {html.escape(nn[0].split("|")[0])} ({nn[1]:.2f})</span>'
            picked = chosen.get(key, {}).get("file") == c["file"]
            latest = c["file"].split("-")[0] == max(x["file"].split("-")[0] for x in voices.candidates(key))
            fresh = '<span class="new">new</span>' if latest and not c["file"].startswith("b0") else ""
            src = f"/v/{voices.slug(key)}/{urllib.parse.quote(c['file'])}"
            cands.append(
                f'<div class="cand{" picked" if picked else ""}">'
                f'<audio controls preload="none" src="{src}"></audio>'
                f'{fresh}<div class="meta"><i>“{html.escape(c["reference_text"][:70])}{"…" if len(c["reference_text"]) > 70 else ""}”</i><br>'
                f'{html.escape(heard)}</div>{warn}{close}'
                f'<button data-key="{html.escape(key)}" data-file="{c["file"]}">{"Chosen" if picked else "Choose"}</button>'
                f'</div>')
        state = chosen.get(key, {}).get("file")
        redo = state == "redesign"
        rows.append(
            f'<section id="c{n}" class="{"done" if state else ""}"><header><h2>{n}. {html.escape(key)}</h2>'
            f'<span class="words">{e["words"]:,} words</span></header>'
            f'<p class="brief">{html.escape(e.get("brief", ""))}</p>'
            f'<p class="insp">{html.escape(e.get("inspiration", ""))}</p>'
            + "".join(f'<p class="hist">Earlier: {html.escape(h)}</p>' for h in chosen.get(key, {}).get("history", [])) +
            f'<p class="line">Candidates read lines the character really says.</p>'
            f'<div class="cands">{"".join(cands)}</div>'
            f'<div class="redo"><button class="redesign{" on" if redo else ""}" data-key="{html.escape(key)}" '
            f'data-file="redesign">{"Marked: design again" if redo else "None of these: design again"}</button>'
            f'<input class="note" data-key="{html.escape(key)}" placeholder="Note for a redesign (optional)" '
            f'value="{html.escape(chosen.get(key, {}).get("note", ""))}"></div></section>')
    return f"""<!doctype html><html lang=en><meta charset=utf-8>
<meta name=viewport content="width=device-width,initial-scale=1"><title>Casting room</title>
<style>
:root{{--bg:#faf8f4;--fg:#1f1d1a;--muted:#6b665e;--card:#fff;--line:#e4dfd6;--accent:#7a4e1d;--ok:#2f6b3a;--warn:#a33a2c}}
@media (prefers-color-scheme:dark){{:root{{--bg:#16140f;--fg:#ece7de;--muted:#a59d90;--card:#201d17;--line:#3a352c;--accent:#d9a35b;--ok:#7fc28b;--warn:#ef8a7c}}}}
body{{margin:0;background:var(--bg);color:var(--fg);font:16px/1.5 Georgia,serif}}
main{{max-width:980px;margin:0 auto;padding:16px}}
h1{{font-size:1.6rem;margin:.5rem 0}} .top{{position:sticky;top:0;background:var(--bg);padding:.5rem 0;border-bottom:1px solid var(--line);z-index:2}}
section{{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:14px 16px;margin:14px 0}}
section.done{{border-color:var(--ok)}}
header{{display:flex;justify-content:space-between;align-items:baseline;gap:8px}} h2{{font-size:1.15rem;margin:0}}
.words,.insp,.meta{{color:var(--muted);font-size:.85rem}} .brief{{margin:.4rem 0}} .line{{font-style:italic;margin:.3rem 0}}
.cands{{display:grid;grid-template-columns:repeat(auto-fill,minmax(210px,1fr));gap:10px;margin-top:8px}}
.cand{{border:1px solid var(--line);border-radius:8px;padding:8px;display:flex;flex-direction:column;gap:6px}}
.cand.picked{{border:2px solid var(--ok)}} audio{{width:100%}}
.new{{font:12px system-ui;background:var(--accent);color:var(--bg);border-radius:4px;padding:1px 6px;width:fit-content}}
.hist{{font:13px system-ui;color:var(--accent);margin:.2rem 0}}
.warn,.close{{font:12px system-ui;border-radius:4px;padding:1px 6px;width:fit-content}}
.warn{{background:var(--warn);color:#fff}} .close{{border:1px solid var(--muted);color:var(--muted)}}
button{{font:14px system-ui;padding:6px 10px;border-radius:6px;border:1px solid var(--accent);background:transparent;color:var(--accent);cursor:pointer}}
.picked button,button.on{{background:var(--ok);border-color:var(--ok);color:#fff}}
.redo{{display:flex;gap:8px;margin-top:10px;flex-wrap:wrap}} .note{{flex:1;min-width:200px;font:14px system-ui;padding:6px;border:1px solid var(--line);border-radius:6px;background:var(--bg);color:var(--fg)}}
label{{font:14px system-ui}}
</style><main>
<div class=top><h1>Casting room</h1><div><b id=done>{done}</b> of {len(keys)} cast &nbsp;
<label><input type=checkbox id=hide> hide characters already cast</label> &nbsp;
<a href="/guide"><b>listening guide</b></a> &nbsp; <a href="/clashes">chosen voices that sound alike</a> &nbsp;
{'<a href="/">principal characters only</a>' if everyone else '<a href="/?all=1">also show the minor characters (cast automatically unless you pick)</a>'}</div></div>
<p>Every candidate plays at the same loudness. Warnings come from the automatic screening;
"close to" means another character's candidate sounds similar (1.0 = the same voice).</p>
{"".join(rows)}</main>
<script>
async function save(key, file, note) {{
  const r = await fetch('/pick', {{method:'POST', headers:{{'Content-Type':'application/json'}}, body: JSON.stringify({{key, file, note}})}});
  if (!r.ok) {{ alert('Could not save: ' + r.status); return; }}
  location.reload();
}}
document.addEventListener('click', e => {{
  const b = e.target.closest('button[data-key]'); if (!b) return;
  const note = document.querySelector(`input.note[data-key="${{CSS.escape(b.dataset.key)}}"]`);
  save(b.dataset.key, b.dataset.file, note ? note.value : '');
}});
document.addEventListener('change', e => {{
  if (!e.target.classList.contains('note')) return;
  fetch('/note', {{method:'POST', headers:{{'Content-Type':'application/json'}}, body: JSON.stringify({{key:e.target.dataset.key, note:e.target.value}})}});
}});
document.addEventListener('play', e => {{ document.querySelectorAll('audio').forEach(a => {{ if (a !== e.target) a.pause(); }}); }}, true);
const hide = document.getElementById('hide');
try {{ hide.checked = localStorage.getItem('hideDone') === '1'; }} catch (_) {{}}
function apply() {{ document.querySelectorAll('section.done').forEach(s => s.style.display = hide.checked ? 'none' : ''); }}
hide.addEventListener('change', () => {{ try {{ localStorage.setItem('hideDone', hide.checked ? '1' : '0'); }} catch (_) {{}} apply(); }});
apply();
</script></html>"""


def clashes(threshold=0.65):
    """Chosen voices that measure alike and speak in the same books."""
    import numpy as np
    from . import screen
    entries, chosen = castmod.load(), picks()
    data = np.load(screen.EMB, allow_pickle=False)
    vec = dict(zip(data["names"], data["vecs"]))
    got = {k: (v["file"], vec[f"{k}|{v['file']}"]) for k, v in chosen.items()
           if v.get("file") and f"{k}|{v['file']}" in vec}
    keys, out = list(got), []
    for i, a in enumerate(keys):
        for b in keys[i + 1:]:
            shared = sorted(set(entries[a]["books"]) & set(entries[b]["books"]))
            sim = float(got[a][1] @ got[b][1])
            if shared and sim >= threshold:
                out.append((sim, a, got[a][0], b, got[b][0], shared))
    return sorted(out, reverse=True)


def clash_page() -> str:
    order = [k for k, e in sorted(castmod.load().items(), key=lambda kv: -kv[1]["words"])
             if voices.candidates(k) and not e.get("voice_of") and e.get("brief_by") == "hand"]
    rows = []
    for sim, a, fa, b, fb, shared in clashes():
        cells = "".join(
            f'<div class="cand"><b>{html.escape(k)}</b><audio controls preload="none" '
            f'src="/v/{voices.slug(k)}/{urllib.parse.quote(f)}"></audio>'
            f'<a href="/#c{order.index(k) + 1 if k in order else 0}">choose another for {html.escape(k)}</a></div>'
            for k, f in ((a, fa), (b, fb)))
        rows.append(f'<section><header><h2>{sim:.2f} alike</h2><span class="words">both speak in '
                    f'{", ".join(shared[:8])}{"…" if len(shared) > 8 else ""}</span></header>'
                    f'<div class="cands">{cells}</div></section>')
    body = "".join(rows) or "<p>No two chosen voices in the same book measure alike.</p>"
    style = page().split("<style>")[1].split("</style>")[0]
    return (f"<!doctype html><html lang=en><meta charset=utf-8><meta name=viewport content=\"width=device-width,"
            f"initial-scale=1\"><title>Similar voices</title><style>{style}</style><main><div class=top>"
            f"<h1>Chosen voices that sound alike</h1><a href=\"/\">back to the casting room</a></div>"
            f"<p>Pairs of your picks that measure 0.65 or more alike (1.0 = the same voice; two different "
            f"characters usually measure about 0.26) and speak in the same books. Listen to each pair: if you "
            f"can tell them apart easily, leave them; if not, choose another candidate for one of them.</p>"
            f"{body}</main></html>")


SPOT_WORDS = ["ass", "asses", "forgat", "ephah", "shittim"]


def guide_page() -> str:
    """The listening guide: what the checks still find, and what only an ear can settle."""
    from collections import defaultdict

    from . import books
    out = paths.OUT / "kjv"
    load = lambda name: json.loads((out / name).read_text(encoding="utf-8")) if (out / name).exists() else {}
    words, voices, redo = load("omissions.json"), load("voicecheck.json"), load("redo.json")
    order = {c: i for i, c in enumerate(books.CODES)}

    def sort_key(k):
        code, ch = k.split(".")
        return order[code], int(ch)

    starts = {}

    def player(k, v):
        if k not in starts:
            t = json.loads((out / f"{k}.json").read_text(encoding="utf-8"))
            starts[k] = dict(t["verses"])
        code, ch = k.split(".")
        start = starts[k].get(str(v), 0.0) if v is not None else 0.0
        return (f'<audio controls preload="none" src="/a/{k}.mp3#t={max(0.0, start - 0.5):.1f}"></audio>'
                f'<span class="meta">{books.BY_CODE[code][3]} {ch}:{v} at {int(start // 60)}:{int(start % 60):02d}</span>')

    def section(title, note, rows):
        body = "".join(rows) or "<p class=insp>None.</p>"
        return (f"<section><header><h2>{title} ({len(rows)})</h2></header>"
                f"{f'<p class=insp>{note}</p>' if note else ''}<div class=cands>{body}</div></section>")

    keys = sorted(words, key=sort_key)
    faults = [f'<div class="cand"><b>{kind}</b> “{html.escape(o.get("script") or o.get("heard_extra", ""))}” '
              f'heard “{html.escape(o.get("heard_in_place", ""))}”{player(k, o.get("verse"))}</div>'
              for k in keys for kind in ("missing", "changed", "added") for o in words[k].get(kind, [])]
    faults += [f'<div class="cand"><b>wrong voice</b> {html.escape(w["who"] or "")}{player(k, w.get("v"))}</div>'
               for k in sorted(voices, key=sort_key) for w in voices[k].get("wrong", [])]
    persistent = [f'<div class="cand"><b>“{html.escape(p["script"])}”</b> heard as “{html.escape(p.get("heard_in_place", ""))}”'
                  f'{player(k, p.get("verse"))}</div>' for k in keys for p in words[k].get("persistent", [])]
    primed = [f'<div class="cand"><b>“{html.escape(x.get("script") or x.get("heard_extra", ""))}”</b> '
              f'{html.escape(x.get("why", ""))}{player(k, x.get("verse"))}</div>'
              for k in keys for x in words[k].get("listen", [])]
    unlike = [f'<div class="cand"><b>{html.escape(w.get("who") or "")}</b> sounds only {w["own"]:.2f} like its own voice'
              f'{player(k, w.get("v"))}</div>' for k in sorted(voices, key=sort_key) for w in voices[k].get("listen", [])]
    by_name = defaultdict(list)
    for k in keys:
        for x in words[k].get("names", []):
            by_name[x["script"].strip(".,;:?!")].append((k, x))
    names = [f'<div class="cand"><b>{html.escape(n)}</b> heard as “{html.escape(xs[0][1].get("heard_in_place", ""))}”'
             f' <span class="meta">({len(xs)} place{"s" if len(xs) > 1 else ""})</span>{player(xs[0][0], xs[0][1].get("verse"))}</div>'
             for n, xs in sorted(by_name.items(), key=lambda kv: -len(kv[1]))]
    reel_data = json.loads((out / "name-reel.json").read_text(encoding="utf-8")) if (out / "name-reel.json").exists() else []
    reel = []
    for x in reel_data:
        k, _, v = x["where"].partition(":")
        if (out / f"{k}.json").exists():
            reel.append(f'<div class="cand"><b>{html.escape(x["name"])}</b> {html.escape(x["proposed"])}{player(k, v or None)}</div>')
    done = sum(1 for r in redo.values() if not r.get("error") and r.get("version"))
    style = page().split("<style>")[1].split("</style>")[0]
    return (f"<!doctype html><html lang=en><meta charset=utf-8><meta name=viewport content=\"width=device-width,"
            f"initial-scale=1\"><title>Listening guide</title><style>{style}</style><main><div class=top>"
            f"<h1>Listening guide</h1><a href=\"/\">casting room</a></div>"
            f"<p>Every take is heard by a recognizer, word by word in its place, and every line is compared with its "
            f"character's own voice; a take with a fault is turned down and Fish asked again. {done} chapters have been "
            f"through the current checks. Below: what is still wrong, then what only an ear can settle. Each player "
            f"starts half a second before the verse.</p>"
            + section("Still wrong", "Words missing, changed or added, and lines in another character's voice. "
                      "The goal is none.", faults)
            + section("Words read the same way in two takes", "A new take didn't change these: the voice's reading, "
                      "or the recognizer's ear. Tell me any that are wrong: a lexicon entry fixes the reading.", persistent)
            + section("Heard right only when the recognizer was primed with the text", "Usually old words the "
                      "recognizer writes the modern way; worth a listen.", primed)
            + section("Lines unlike their own voice", "Nothing else matched them better, but they sound unlike the "
                      "character's voice: a blend or a drift.", unlike)
            + section("Name reel: pronunciations to approve", "Names and words pinned to a dictionary reading "
                      "(CMU, ISBE 1915) where Fish read them several ways or wrongly. Listen to each and tell me any "
                      "you want said otherwise.", reel)
            + section("Names said otherwise", "By name, most places first: one place to hear each.", names)
            + "</main></html>")


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _send(self, code, body, ctype):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _send_ranged(self, data, ctype):
        """Answer byte ranges, so a player can start partway into a chapter."""
        rng = self.headers.get("Range", "")
        if rng.startswith("bytes="):
            a, _, b = rng[6:].partition("-")
            start = int(a) if a else max(0, len(data) - int(b))
            end = int(b) if a and b else len(data) - 1
            end = min(end, len(data) - 1)
            part = data[start:end + 1]
            self.send_response(206)
            self.send_header("Content-Range", f"bytes {start}-{end}/{len(data)}")
        else:
            part = data
            self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(len(part)))
        self.end_headers()
        self.wfile.write(part)

    def do_GET(self):
        path = urllib.parse.unquote(urllib.parse.urlparse(self.path).path)
        if path == "/guide":
            return self._send(200, guide_page().encode("utf-8"), "text/html; charset=utf-8")
        if path.startswith("/a/"):
            f = (paths.OUT / "kjv" / path[3:]).resolve()
            if (paths.OUT / "kjv").resolve() in f.parents and f.suffix == ".mp3" and f.exists():
                return self._send_ranged(f.read_bytes(), "audio/mpeg")
        if path == "/clashes":
            return self._send(200, clash_page().encode("utf-8"), "text/html; charset=utf-8")
        if path == "/":
            everyone = "all=1" in urllib.parse.urlparse(self.path).query
            return self._send(200, page(everyone).encode("utf-8"), "text/html; charset=utf-8")
        if path.startswith("/v/"):
            f = (paths.VOICES / path[3:]).resolve()
            if paths.VOICES.resolve() in f.parents and f.suffix == ".wav" and f.exists():
                # every candidate at the same loudness, so voices are compared, not levels
                return self._send(200, voices.level(f).read_bytes(), "audio/wav")
        self._send(404, b"not found", "text/plain")

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        data = picks()
        key = body.get("key")
        if key not in castmod.load():
            return self._send(400, b"unknown character", "text/plain")
        if self.path == "/pick":
            data[key] = {"file": body.get("file"), "note": body.get("note", "")}
        elif self.path == "/note":
            data.setdefault(key, {})["note"] = body.get("note", "")
        PICKS.write_text(json.dumps(data, indent=1, ensure_ascii=False), encoding="utf-8")
        self._send(200, b"ok", "text/plain")


def main(port=8765):
    paths.ensure(paths.VOICES)
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()


if __name__ == "__main__":
    main(int(sys.argv[1]) if len(sys.argv) > 1 else 8765)
