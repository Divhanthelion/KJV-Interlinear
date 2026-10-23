# Fish Audio: what it does, as documented and as measured

Studied 2026-10-07 from Fish's own documentation (`docs.fish.audio`, its
`llms-full.txt` and `openapi.json`), the owner's two working renderers
(`rustfinal/rust_basics/app/lectures`, `contracts/xtask`), and experiments
run against the API with the owner's key. "Measured" means we tried it and
saw the result; everything else is the documentation's word.

## The account (2026-10-07)

| | |
|---|---|
| API credit | **$0.00** at the start, never topped up (`GET /wallet/self/api-credit`); **−$0.36** by the end of the session, from a charge we could not trace (decisions.md, open 1) |
| Web plan | free, 8,000 web credits (these do not pay for the API) |
| Concurrency | 5 requests at a time ("Starter", under $100 ever paid; $100 unlocks 15, $1,000 unlocks 50) |
| Free model | `s2.1-pro-free`: $0, "fair use", **through 2026-11-30**; afterwards `s2.1-pro` at $15 per million UTF-8 bytes |

What costs money, and what that means here:

| Thing | Price | With $0 credit | For the KJV |
|---|---|---|---|
| Speech, `s2.1-pro-free` | $0 | works | ~4.3 M bytes: free now, ~$65 at paid rates |
| Speech, `drama-3-preview` | not listed | **402** | untested |
| Voice Design (`voice-design-1`) | $0.01 per request, 1–4 candidates | **402** | ~900 voices, 1–3 requests each: **$10–$30** |
| Creating a voice (`POST /model`) | free | **works** (measured) | — |
| Speech to text (`transcribe-1-pro`) | $0.36 per audio hour | 402 | we use Whisper on the GPU instead, free |

**Licence (open question for the owner).** Fish's Terms (effective
2024-08-18) say free-plan users may use the service only for "internal,
personal, non-commercial use", while paid users may use it commercially. The
S2.1 free-API announcement says the free model is for "testing, prototyping,
development, and smaller businesses" and asks only products over $1M a year
to get in touch. Our app is free, but it is published in stores. A small
top-up (which Voice Design needs anyway) makes the account a paying one; it is
still worth asking Fish (support@fish.audio) in writing that audio made with
`s2.1-pro-free` may ship in a free Bible app.

Fish also "encourages" disclosing that distributed audio is AI-generated; the
app's credits should say so plainly.

## Models

| Model | Notes |
|---|---|
| `s2.1-pro` | production; S2 with better quality and speed; 83 languages |
| `s2.1-pro-free` | the same model at $0 until 2026-11-30, no latency or DPA guarantees |
| `s2-pro` | previous S2; open weights (Qwen3-4B backbone) |
| `s1` | old; `(parenthesis)` emotions, no multi-speaker |
| `drama-3-preview` | new 2026-09-23: "direct in plain language: tone, pacing, character"; multi-speaker; preview, paid |

Select with the `model` HTTP header. A missing or unknown header silently
falls back to `s2.1-pro` (paid), so always send it.

## Speech with word timings

`POST /v1/tts/stream/with-timestamp` (Server-Sent Events). Each event has
`audio_base64` (append them all, in order: one MP3), `chunk_seq`,
`chunk_audio_offset_sec`, `content`, and `alignment` = `{audio_duration,
segments: [{text, start, end}]}`. Keep the latest alignment per `chunk_seq`
and add the chunk's offset. We use this endpoint for everything: the timings
give verse and line times and let us check every render.

Request body (what `abible/render.py` sends):

| Field | Value | Why |
|---|---|---|
| `text` | the scene, with `<\|speaker:N\|>` tags | multi-speaker (below) |
| `reference_id` | list of voice ids, index = speaker number | |
| `format`, `mp3_bitrate` | `mp3`, 128 | decoded and re-encoded after |
| `latency` | `normal` | the docs' "most stable" |
| `normalize` | `true` | keeps capitals and numbers sane (measured: "LORD", "I AM THAT I AM", "JEHOVAH" all read as words) |
| `chunk_length` | 300 (maximum) | fewest internal joins |
| `temperature`, `top_p` | 0.7, 0.7 | Fish's defaults, stated so a change shows in the cache key |
| `prosody.speed` | 1.0 | measured: about 185 words a minute (Genesis 24, 1,816 words in 587 s); see decisions |
| `condition_on_previous_chunks` | default `true` | the voice stays steady within a request |

| `prosody.normalize_loudness` | `true` | evens out each request's loudness (S2 family; the default, sent so it can't change under us) |
| `features` | `["quality-guard"]` | Fish's own opt-in check on a synthesis; measured 2026-10-09: accepted by the free model, no charge, no slower |

Other fields exist: `repetition_penalty` (1.2), `max_new_tokens` (1024 per
chunk), `min_chunk_length`, `early_stop_threshold`, `prosody.volume` (dB,
−20 to 20), and `pronunciation_dictionary` (below).

### Measured

- **Speed of rendering**: Genesis 24, 9,870 characters, one request, four
  voices: 275 s for 587 s of audio, about 36 characters a second per request.
  With 5 requests at a time the whole KJV (~4.3 M characters) is about
  **6–7 hours** of rendering.
- **Accuracy**: Genesis 24's 1,816 words came back with no real error (the
  19 differences the first transcription found were spellings or the
  recognizer's own mistakes; see QA in the SOP).
- **Silent truncation**: once a stream simply ended after its first chunk,
  with no error, and the client counted it a success. Every render is now
  checked against the script (`render.coverage`, 97% of words or it is
  asked again).

## Many voices in one request

S2 reads dialogue with several voices in one request: `reference_id` is a
list and the text switches voice with `<|speaker:0|>`, `<|speaker:1|>`, …
(indices into the list). Measured on Genesis 3:1–5 (narrator, serpent,
woman): every line came out in the right voice (speaker embeddings: the
narrator's lines 0.71–0.81 like each other, the woman's 0.25 or less like
anyone else), and the timings carry straight across. So a whole chapter, all
its characters, is **one request**, conditioned throughout: no cold starts
and no joins between speakers.

The limit is the reference audio Fish holds for the voices:

- Voices made from **one 9.5-second clip** each: **20 in one request** (189 s)
  read a 544-word scene completely.
- **Library voices** hold more reference audio than their listed samples
  show. Four of them (115 s by their samples) were refused ("Reference audio
  too long"), or worse, the stream stopped after its first chunk with no
  error. Two of the same four were fine.

Hence: our voices are made from **one clip of about 10 s**, scenes are packed
to 180 s of reference audio, a refused or incomplete scene is halved at a
narrator line and tried again.

## Delivery cues

S2 treats `[bracketed words]` as free-form direction, not a fixed list
(`[whispering]`, `[weeping]`, `[angry]`, `[laughing nervously]`).

Measured: `[weeping]` before David's "O my son Absalom" made the line 16%
slower and a little softer with the same voice (0.82 like itself untagged,
the same as two plain renders of one voice); `[shouting]` before "Lazarus,
come forth" was barely louder (`normalize` levels it). Neither cue was
spoken or left a stray sound (the recognizer heard only the words).

The owner's lecture renders found `[emphasis]` came out as "hm" with a
changed voice, and `[break]` scrambled the word timings. So: cues only at the
start of a line, only from the short list in `render.DELIVERY` (weeping,
calling out, whispering, praying, angry, mocking, trembling, pleading,
laughing, quietly to oneself, singing softly, amazed, weak), never mid-line,
never `[emphasis]` or `[break]`. The QA catches any stray sound as an extra
word.

## Pronunciation

- **Phoneme tags**, one English word in CMU Arpabet:
  `<|phoneme_start|>M EY2 HH ER0 SH AE2 L AE0 L HH AE1 SH B AE2 Z<|phoneme_end|>`.
  Measured: honoured, and not read aloud. The timings list the tag's
  contents as words ("phoneme start M EY2 …"); `render.collapse_phonemes`
  folds them back into one.
- **Pronunciation dictionaries**: up to 3 per request, 5,000 rules each,
  inline or published in the web app. Keys are plain substrings matched
  leftmost-longest, case-insensitive unless marked, so "Ai" would also match
  inside "said". We use our own tags from `lexicon/kjv.tsv` instead, where a
  rule is a whole word with optional context.
- Measured misreadings without help: "the **lead** sank" read as "leed";
  Zaphnathpaaneah, Chedorlaomer and "MENE, MENE, TEKEL, UPHARSIN" mangled.
  The archaic words in the tests were fine (thee, thou, hath, doth, shew,
  wilt, subtil); the full word list is checked as the lexicon is built.

## Making voices

**Voice Design**, `POST /v1/voice-design`, header `model: voice-design-1`:
`instruction` (1–2,000 characters describing the voice), `reference_text`
(the line the candidates read, up to 150 characters per the schema),
`language`, `n` (1–4 candidates, one price), `seed`, `guidance_scale`
(2.0; higher follows the prompt more), `num_step` (32). Returns WAV
candidates (base64) with an id. $0.01 per successful request; needs API
credit (402 at $0).

Each candidate also carries `signature` and Fish's own description of the
voice (`features`: gender, age, tone, accent, pacing). The description
follows the prompt, not the sound: a "male" candidate measured 219 Hz, so
pitch is checked separately (SOP 3C). Measured on our cast: about 5 s per
request of 4 candidates; 3,700 candidates for $9.

**Creating a voice**, `POST /model` (multipart): `type=tts`, `title`,
`description`, `visibility=private`, `train_mode=fast`, `voices` (the clip),
`texts` (its transcript, else Fish runs speech recognition),
`enhance_audio_quality` (default on; off for clean designed audio),
`voice_design_signatures` (from Voice Design candidates: stamps the voice
`source=voice_design`), `tags`. Measured: works with $0 credit, private,
`state: trained` at once. **The signature is checked against the clip's exact
bytes**: a levelled copy is refused ("Invalid voice-design signature"), so
voices are made from the clip as designed. All 909 voices of the KJV cast
were made this way (2026-10-09, 20 minutes, none failed).

`GET /model?self=true` lists your voices; `licensed=true` lists only voices
Fish has cleared with the speaker (24 in English, mostly calm middle-aged men,
so not a cast). Most other library voices are uploads of unknown origin.

Fish's own rule, which we keep: **never clone a real person's voice without
written permission**, celebrities included. Castings inspire our briefs. The
brief sent to Voice Design describes a voice and never names anyone; the
separate inspiration notes in `cast/principals.json` cite productions for our
own reference, are never sent to Fish, and no voice is cloned or imitated from
them.

## The official SDK

`fish-audio-python` (reviewed 2026-10-09, last updated 2026-10-02) lags the
API: no word timings, no multi-speaker lists, no Voice Design or design
signatures, no `normalize_loudness`, no `quality-guard`, no pronunciation
dictionaries. Our standard-library client calls the API directly
(`abible/fish.py`).

## Changes since this was written

- 2026-10-08: S1 is retired on 2026-12-31 (requests for `s1` will be served
  and billed as `s2.1-pro`). We never used S1.

## Things that are not what they seem

- WAV responses carry a placeholder header length; count bytes instead.
- A missing `model` header bills the paid model.
- A 200 response can hold only part of the text (above).
- `reference_text` for Voice Design is 300 characters in the guide but 150
  in the schema: use 150.

## Sources

- API reference: <https://docs.fish.audio/api-reference/openapi.json>, <https://docs.fish.audio/llms-full.txt>
- Pricing and limits: <https://docs.fish.audio/developer-guide/models-pricing/pricing-and-rate-limits>
- Free model: <https://fish.audio/blog/s2-1-pro-free-api/>
- Terms: <https://fish.audio/terms/>
- The owner's renderers: `C:\Users\ryanj\code\rustfinal\rust_basics\docs\lectures.md` (Part 4),
  `C:\Users\ryanj\code\contracts\docs\lectures.md` (section 5)
