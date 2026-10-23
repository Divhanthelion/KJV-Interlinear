"""Fish Audio, as documented (docs.fish.audio, read 2026-10-07) and as measured
(audio/docs/fish-audio.md). Standard library only.

The API key comes from FISH_API_KEY, else the FISH_API_KEY= line of
audio/.env, else of C:\\Users\\ryanj\\code\\picture_ingest\\.env (where the
owner's other projects read it). It is never printed or written anywhere.
"""
import base64
import http.client
import json
import os
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path

from . import paths

API = "https://api.fish.audio"
MODEL = os.environ.get("FISH_MODEL", "s2.1-pro-free")  # free through 2026-11-30, then "s2.1-pro"
ENV_FILES = [paths.AUDIO / ".env", Path(r"C:\Users\ryanj\code\picture_ingest\.env")]
# How many voices one request can carry depends on the reference audio Fish
# holds for them (2026-10-07): voices made from one 9.5 s clip each worked 20
# at a time (189 s) on a 544-word scene. Library voices hold longer references
# than their listed samples show: four of them (about 115 s listed) were
# refused as "Reference audio too long", or worse, the stream stopped after
# its first chunk with no error. So our voices get one clip of about 10 s,
# scenes are packed to this budget, and render.py halves a refused scene and
# rejects incomplete audio.
MAX_REFERENCE_SECONDS = 180.0


class FishError(RuntimeError):
    pass


class PaymentRequired(FishError):
    pass


def _key() -> str:
    k = os.environ.get("FISH_API_KEY", "").strip()
    if k:
        return k
    for f in ENV_FILES:
        if f.exists():
            for line in f.read_text(encoding="utf-8").splitlines():
                if line.startswith("FISH_API_KEY="):
                    k = line.split("=", 1)[1].strip().strip("'\"")
                    if k:
                        return k
    raise FishError("no Fish Audio key: set FISH_API_KEY or put it in audio/.env (never commit it)")


def _request(method, path, body=None, headers=None, timeout=600, raw=False):
    h = {"Authorization": f"Bearer {_key()}", "User-Agent": "abible/1.0"}
    data = None
    if isinstance(body, (bytes, bytearray)):
        data = bytes(body)
    elif body is not None:
        data = json.dumps(body).encode()
        h["Content-Type"] = "application/json"
    h.update(headers or {})
    req = urllib.request.Request(API + path, data=data, method=method, headers=h)
    try:
        resp = urllib.request.urlopen(req, timeout=timeout)
    except urllib.error.HTTPError as e:
        detail = e.read()[:400].decode("utf-8", "replace")
        if e.code == 402:
            raise PaymentRequired(f"Fish Audio wants API credit (402): {detail}") from None
        if e.code in (401, 403):
            raise FishError(f"Fish Audio refused the key ({e.code})") from None
        if e.code in (408, 429) or e.code >= 500:
            raise _Retry(f"HTTP {e.code}: {detail}") from None
        raise FishError(f"Fish Audio error {e.code}: {detail}") from None
    except (urllib.error.URLError, TimeoutError, ConnectionError) as e:
        raise _Retry(str(e)) from None
    return resp if raw else resp.read()


class _Retry(Exception):
    pass


OUTAGE = 1800  # seconds a request keeps being retried while Fish is down or cuts off


def _with_retries(fn, patience: float = OUTAGE):
    """fn(), asked again while it fails in a way that passes (busy, down, cut off), for up to
    `patience` seconds: a long run waits out an outage instead of failing every chapter in
    it (2026-10-13, 05:48: "503 service is drained" through six tries over 4.5 minutes)."""
    delay, last, waited = 5, None, 0.0
    while True:
        try:
            return fn()
        except _Retry as e:
            last = e
            if waited >= patience:
                raise FishError(f"Fish Audio kept failing for {waited / 60:.0f} min ({last})") from None
            time.sleep(delay)
            waited += delay
            delay = min(delay * 2, 120)


def get(path):
    return json.loads(_with_retries(lambda: _request("GET", path)))


def credit() -> dict:
    return get("/wallet/self/api-credit?check_free_credit=true")


# ------------------------------------------------------------------ speech

def tts_with_timestamps(body: dict, model: str = MODEL, timeout: int = 3600):
    """POST /v1/tts/stream/with-timestamp. Returns (mp3 bytes, words), where
    words are [{"text", "start", "end"}] in seconds on the whole audio.
    Alignment snapshots are kept per chunk_seq (latest wins), as documented."""
    def once():
        resp = _request("POST", "/v1/tts/stream/with-timestamp", body, {"model": model}, timeout=timeout, raw=True)
        audio, aligns = bytearray(), {}
        try:
            for raw in resp:
                line = raw.decode("utf-8").strip()
                if not line.startswith("data:"):
                    continue
                ev = json.loads(line[5:].strip())
                if ev.get("audio_base64"):
                    audio += base64.b64decode(ev["audio_base64"])
                if ev.get("alignment"):
                    aligns[ev["chunk_seq"]] = (ev.get("chunk_audio_offset_sec") or 0.0, ev["alignment"])
        except (ValueError, http.client.HTTPException, OSError) as e:
            # the stream cut off mid-event (Leviticus 11, 2026-10-13: "Unterminated string"):
            # what came is no take, ask again
            raise _Retry(f"stream cut short: {type(e).__name__}: {e}") from None
        if not audio:
            raise _Retry("no audio in the response")
        words = [{"text": s["text"], "start": round(s["start"] + off, 3), "end": round(s["end"] + off, 3)}
                 for seq in sorted(aligns) for off, al in [aligns[seq]] for s in al.get("segments", [])]
        return bytes(audio), words
    return _with_retries(once)


# ------------------------------------------------------------------ voices

def design(instruction: str, reference_text: str | None, n: int = 4, seed: int | None = None,
           guidance_scale: float = 2.0):
    """POST /v1/voice-design ($0.01 a request, whatever n). Returns the candidates
    (each with audio_base64 WAV, sample_rate, duration_ms and, when present, a
    signature for /model)."""
    body = {"instruction": instruction[:2000], "language": "en", "n": n, "guidance_scale": guidance_scale}
    if reference_text:
        body["reference_text"] = reference_text[:150]
    if seed is not None:
        body["seed"] = seed
    out = _with_retries(lambda: _request("POST", "/v1/voice-design", body, {"model": "voice-design-1"}, timeout=300))
    return json.loads(out)["candidates"]


def create_voice(title: str, description: str, audio: bytes, filename: str, text: str,
                 signature: str | None = None, tags=()) -> dict:
    """POST /model: a private voice from one reference clip and its transcript."""
    boundary = "----abible" + uuid.uuid4().hex
    fields = [("type", "tts"), ("title", title[:80]), ("description", description[:500]), ("visibility", "private"),
              ("train_mode", "fast"), ("texts", text), ("enhance_audio_quality", "false")]
    fields += [("tags", t) for t in tags]
    if signature:
        fields.append(("voice_design_signatures", signature))
    body = bytearray()
    for name, value in fields:
        body += f"--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n".encode()
    body += (f"--{boundary}\r\nContent-Disposition: form-data; name=\"voices\"; filename=\"{filename}\"\r\n"
             f"Content-Type: audio/wav\r\n\r\n").encode() + audio + b"\r\n"
    body += f"--{boundary}--\r\n".encode()
    out = _with_retries(lambda: _request("POST", "/model", bytes(body),
                                         {"Content-Type": f"multipart/form-data; boundary={boundary}"}))
    return json.loads(out)


def my_voices() -> list:
    out, page = [], 1
    while True:
        r = get(f"/model?self=true&page_size=100&page_number={page}")
        out += r["items"]
        if len(out) >= r["total"] or not r["items"]:
            return out
        page += 1
