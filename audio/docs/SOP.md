# Standard operating procedure: a dramatized audio Bible, every voice its own

How we turn a Bible translation into a full-cast audio Bible with Fish Audio,
so that every word of the text is heard exactly as printed and every speaking
character has a voice of their own, the same voice wherever they speak.
Written while producing the KJV (October 2026); every other translation we
package follows the same steps. What the KJV taught us is marked
**(KJV lesson)**.

The quality bar is the app's: every word checked, nothing sampled where it can
be counted. Audio adds one thing text never needed, a person's ear, and the
procedure says exactly where a person listens.

```
 text ──► 1 sources ──► 2 script ──► 3 cast ──► 4 lexicon ──► 5 render ──► 6 check ──► 7 package
 (pinned)              (who says    (a voice    (names,        (Fish,        (words,      (MP3 +
                        each word)   for each)   heteronyms)    by chapter)   voices,      timings,
                                                                              sound)       credits)
```

---

## 0. Before starting a translation

Answer these and write the answers in `docs/decisions.md`:

1. **May we record it?** The text's licence must allow an audio rendering and
   distribution (public domain, CC BY, CC BY-SA, or written permission).
   CC BY-ND forbids derivatives: an audio recording is arguably one. Ask
   before recording any ND or NC text.
2. **May we distribute the audio?** See Fish's terms (docs/fish-audio.md,
   "Licence").
3. **Which quotation style?** This decides step 2:
   - **A. Quotation marks** (WEB, BSB, ASV with marks, most modern texts):
     the translation marks its own direct speech.
   - **B. No quotation marks** (KJV, and other older texts): speech has to
     be located.
4. **What is outside the verses?** Book titles, chapter numbers, Psalm titles,
   section headings, colophons, acrostic letters. Decide which are read
   (KJV: titles, chapters, Psalm titles as verse 0, Psalm 119's letters and
   the epistles' colophons; no editorial section headings, the KJV has none).

## 1. Sources

Every input is pinned: URL, and SHA-256 of the exact bytes
(`sources/sources.json`, `python -m abible sources pin|fetch`). A changed file
stops the build until someone looks at what changed.

| Source | What for | Licence |
|---|---|---|
| The translation's text, as the app ships it | the words | per translation |
| SIL **Glyssen** (`CharacterVerse.txt`, `CharacterDetail.txt`, English reference script) at a pinned commit | who speaks in each verse; what each character is like; FCBH's dramatized script of the WEB | MIT (SIL and Faith Comes By Hearing) |
| eBible.org USFM of the translation | titles and other non-verse text | per translation |
| Clear Bible **speaker-quotations** (optional) | word-level quotations for 8 modern translations, mapped to the Hebrew and Greek | CC BY 4.0 |

Glyssen is what Faith Comes By Hearing uses to cast its dramatized Bibles in
hundreds of languages. Its English reference script is the World English
Bible already split into blocks, one speaker each, with a delivery note where
it matters ("weeping", "calling out from the sky"). It identifies 1,281
characters and groups, distinguishes people who share a name ("Abimelech 1,
king of the Philistines (in Gerar)" versus "Abimelech, son of Gideon"), and
marks age ("David (old)", "Jesus (child)").

## 2. Script: who says each word

The script lists, for every chapter: the announcement, then every verse as
one or more lines, each with one speaker (`script/<translation>/<BOOK>.json`).

**The rule that is never broken:** the lines of a verse, joined with single
spaces, are the verse exactly, character for character. `tests/test_script.py`
checks all of them, every build.

### 2A. Translations with quotation marks

1. Split each verse at the translation's own opening and closing marks. Text
   at the first level is the narrator's; each quotation is a speech.
2. Give each speech its speaker from Glyssen's `CharacterVerse.txt` (by verse
   and order; where Glyssen lists alternatives, take its default).
3. **Nested quotations stay with whoever is quoting** (the servant retelling
   Abraham's words in Genesis 24 speaks them all). This is FCBH's convention
   and ours.
4. Cross-check with the reference script (2B's projection): where the two
   disagree, the verse goes to review.

### 2B. Translations without quotation marks (the KJV)

`abible/project.py` projects the reference script onto the translation, verse
by verse:

1. Align the WEB piece words with the verse's words (word-level diff, with
   era spellings mapped: you/ye/thee, has/hath, Yahweh/LORD).
2. Map each boundary between pieces into the verse, then snap it to a gap
   after punctuation.
3. **(KJV lesson)** The KJV marks direct speech with a capital after a verb of
   speaking: "And God said, Let there be light". A boundary into a speech
   must land before a capital, after a verb of speaking within a few words.
4. **(KJV lesson)** Where the KJV has no such place, the KJV has *indirect*
   speech ("he charged them that they should tell no man"; Glyssen's script
   turns it into "Tell no one"). Those words stay with the narrator: we never
   put words in a character's mouth that the translation gives to the
   narrator. Out of a speech, the reverse: the speech runs on, so the words
   stay with the speaker.
5. **(KJV lesson)** "Saith the LORD" interrupts a speech that then carries on
   in lower case ("In that day, saith the LORD, that there shall be…"): when
   the same speaker resumes after a narrator interjection, no capital is
   required.
6. **Red letter** (KJV, and any translation that marks Christ's words): the
   translation's own markup is a second, independent source for one speaker.
   It outranks the projection, character by character, except that red text
   inside another speaker's words stays with that speaker (Paul quoting
   Christ in Acts 22). Verses the red letter changes go to review, because the
   markup itself can be wrong **(KJV lesson: eBible marks "And the common
   people heard him gladly", Mark 12:37, as Christ's words)**.

Measured on the KJV (31,218 verses with the Psalm titles; 38,780 lines):
the projection placed every speaker change it could find a place for, and the
KJV's own red letter agreed with it in about 98% of the 2,028 verses it marks.
293 verses were left for review. Every P1 and P2 verse is settled: 50
corrected (`script/kjv-overrides.json`, two of them from a P3 sample), 37
confirmed right as made (`script/kjv-reviewed.json`); 206 P3 verses remain.

### 2C. Review

`script/<translation>/review.json` lists every verse a rule could not settle,
with the projection, the reference pieces and the reason. Work it in order:

| Priority | Kind | Who settles it |
|---|---|---|
| P1 | projection failed; Glyssen itself undecided ("Needs Review") | a person |
| P2 | ambiguous boundary; red letter changed the projection | a person, or an LLM with a person checking every answer |
| P3 | merged as indirect speech or run-on; nested quotes of Christ; no red letter | a person, every verse: most are right by rule, but a random 15 of the KJV's had 2 wrong |

A correction goes in `script/<translation>-overrides.json` with the verse's
lines and a one-line reason; never in the generated files. The overrides are
checked by the same rule (lines rejoin to the verse). A verse checked and
found right goes in `script/<translation>-reviewed.json`, which takes it off
the list.

### 2D. Non-verse text

Spoken by the narrator, never mixed into verse lines: the chapter
announcement ("The First Book of Moses, called Genesis. Chapter one." then
"Genesis, chapter two."; "Psalm twenty-three."), Psalm 119's letters, the
colophons. Titles are taken from the edition's USFM, not typed.

## 3. Cast: a voice for every speaker

### 3A. Who needs a voice

`python -m abible cast` lists every casting key in the script with how much
they say, in which books, their Glyssen attributes (gender, age, group size)
and lines they really say (`cast/cast.json`). The rules in
`cast/policy.json`:

- **One voice per character for the whole Bible**, and for every translation:
  Moses sounds the same in Exodus and Deuteronomy, in the KJV and the WEB.
  A new translation reuses the cast and designs only characters it adds.
- **Age:** "(old)" shares the character's voice and plays it older;
  "(child)" and "(young)" get voices of their own age.
- **Joint speakers** ("Deborah/Barak"): led by FCBH's choice or the first.
- **Groups** (Israelites, the crowd before Pilate): one voice speaks for the
  group for now; ensemble mixing (3E) comes later.
- **Scripture quotations** in the narration ("that it might be fulfilled
  which was spoken by the prophet, saying, …") are voiced by the prophet
  Glyssen names.
- **Author voices** (an owner decision, see decisions.md): the letters read
  by their writers (Paul, Peter, James, John, Jude), Revelation by John,
  Lamentations by Jeremiah, Ecclesiastes by the Preacher (The Word of
  Promise cast the writers of the books). Titles and colophons stay with the
  narrator.

KJV: 999 keys, 909 voices to make (the rest share).

### 3B. Briefs

A brief is the description Voice Design turns into a voice: age, pitch,
texture, pace, temperament, character. Two kinds:

- **By hand** (`cast/principals.json`) for everyone who speaks a lot or
  matters: the KJV's 125 hand briefs cover the narrator, God, Jesus and the
  speakers of about 93% of all words. Each has the brief, the castings that
  inspired it, and the scripture behind it ("I am slow of speech", Exodus
  4:10, for Moses; "the voice is Jacob's voice, but the hands are the hands
  of Esau", Genesis 27:22, so the twins must not sound alike).
- **From attributes** for the rest: gender, age, group, role words in the
  name ("king", "prophet", "soldier"), and pitch/texture/pace/temperament
  spread so that neighbours differ.

**Never in a brief:** a real person's name, "sounds like…", an imitation of
an actor, a regional or ethnic caricature. Castings inspire; they are not
copied. The accent policy is one line, appended to every brief.

### 3C. Designing and choosing

1. `voices design KEY` asks Voice Design for 4 candidates; each reads a line
   the character really says (150 characters at most), so the audition is in
   character. $0.01 a request.
2. **Principals** (hand briefs): the owner listens on the casting page
   (`voices audition`) and picks: `voices choose KEY FILE`.
3. **Everyone else**: chosen automatically, the candidate that said its line
   correctly (transcribed) and is least like every voice already cast.
4. **Screening, before anyone listens** (`screen.py`): the candidate must read
   its line (transcribed), sound the right age by Fish's description, and
   have the right pitch for its gender (men about 85–155 Hz, women 165–255).
   **(KJV lesson)** Fish's description follows the prompt, not the sound: a
   "male" Jonathan candidate measured 219 Hz. 6 of 496 principal candidates
   failed on pitch, 9 on reading their line.
5. **Uniqueness, measured not assumed:** a speaker embedding of every chosen
   voice; no two voices in one book above 0.60 similarity, none anywhere above
   0.72. Measured on the 496 principal candidates: one voice rendered twice,
   0.78–0.85; four candidates of one brief, median 0.51; candidates of
   different characters, median 0.26, 99th percentile 0.65. A candidate over
   the line is rejected and designed again.
6. The chosen clip (about 10 s, with its transcript and Fish's design
   signature) becomes a private Fish voice. Keep the clip: it can rebuild the
   voice anywhere. **The transcript must be what the clip says.** **(KJV
   lesson)** The screening once left capitalised words out of the reading
   check, so a candidate for "the altar" that skipped "Lord God Almighty" in
   its line passed, became a voice with a transcript claiming those words, and
   then skipped them in every take of Revelation 16:7 (its own line). The
   check now counts every word (`screen.read_share`); a voice whose clip
   differs from its line gets the transcript it was heard to say.

### 3D. What the owner hears before anything is rendered at scale

The casting page for the principals, and a "cast reel": every voice reading
its line, in order of appearance, book by book.

### 3E. Later: ensembles

Crowds and groups as several voices at once (3–4 voices from a pool of extras,
mixed with small offsets) for short shouts ("Crucify him", "Hosanna"); long
group speeches keep one spokesperson.

## 4. Lexicon: names and words the voice gets wrong

`lexicon/<translation>.tsv`: a whole-word pattern (with context where a word
has two readings), its CMU Arpabet, and the source. The renderer turns each
match into a Fish phoneme tag.

1. **Every proper name** in the translation, listed from the text. For each:
   CMU dictionary if it has it, else the International Standard Bible
   Encyclopedia's (1915, public domain) respelling ("Ab-ed'-ne-go") turned
   into Arpabet. Names the voice already reads well need no entry.
2. **Heteronyms** with the reading decided by context: lead (the metal),
   wind, tear, bow, read, live, close, wound, minute, Job/job (KJV: the
   metal "lead" in Exodus 15:10, Numbers 31:22, Job 19:24, Jeremiah 6:29,
   Ezekiel 22 and 27, Zechariah 5).
3. **Words outside modern English** that the voice may modernize or misread:
   -est/-eth/-edst forms, shew, wist, wot, holpen, ensample.
4. **The name reel:** every name once, in a short sentence, for the owner to
   hear. Fix, re-render the reel, until it's right.

**(KJV lesson)** Unaided, the voice read "lead" (metal) as "leed" and mangled
Zaphnathpaaneah, Chedorlaomer and MENE, MENE, TEKEL, UPHARSIN; it read
capitals ("I AM THAT I AM", "JEHOVAH", "HOLINESS TO THE LORD") correctly as
words.

**Respellings.** Where the modern spelling of a word is read right, the entry
is that spelling after "=" ("[Ss]hewbread = showbread", "[Hh]oughed = hocked",
"[Cc]arcases = carcasses"): plain text, so Fish's timestamps stay whole, which
phoneme tags break. Only what Fish is sent changes; the script keeps the KJV.

**(KJV lesson)** The whole-Bible word check (step 6) is also a pronunciation
survey: a word the recognizer hears as another word in place after place is
usually the voice's reading, not the recognizer's ("shewbread" as "shoe
bread", "emerods" as "emirates", "ass" as "arse" in 10 of 150 places,
"houghed" as "huffed"). Those became lexicon entries before the chapters were
rendered again.

## 5. Render

`python -m abible render BOOK CHAPTER` (or a whole book): one chapter at a time.

- **One request per scene of at most 2 voices** (`render.MAX_VOICES`, decision
  D22), up to ~14,000 characters; scenes are cut at narrator lines, and a beat
  with more voices than that is cut where the next voice would be one too many.
  Within a request Fish conditions every line on what came before. **(KJV
  lesson)** The first render put a whole chapter in one request when its
  voices fit (up to 20): with many voices Fish loses track of who is who.
  Measured over the whole KJV: lines in another character's voice were 0.4% at
  2 voices a request, 3.8% at 3, 7-16% at 6 to 10; 1 Samuel 17 (nine voices)
  gave David's lines Jesse's voice and David's voice to Goliath.
- Between requests each take's own silence is cut to 0.12 s at either end and
  0.35 s put between (`render.EDGE`, `GAP`): with many more joins, an untrimmed
  join was a 0.65 s pause against 0.30 s for a change of speaker in a request.
  Chapter files are written beside their place and moved in only when whole
  (a run killed when the machine slept left Exodus 29 cut at 5:28 of 7:58).
- Settings: docs/fish-audio.md. Model `s2.1-pro-free` until 2026-11-30.
- Delivery cues only at the start of a line, only for clear acted moments
  (decision D24: weeping, calling out, whispering, pleading, fear, mockery,
  anger, amazement, laughter, thinking aloud, prayer), from the explicit table
  `render.DELIVERY`, each note looked up whole. **(KJV lesson)** The first
  table searched inside words: "sing" in "blessing" sent God's blessing in
  Genesis 1:28 as "[singing softly]". Laments and songs are read as speech.
- Everything Fish returns is cached by a hash of the request: change one
  line and only its scene is voiced again.
- Up to 5 chapters at a time (the free account's limit).

**(KJV lesson)** A stream can end early with no error. Every render must
account for 97% of the scene's words (counted on the text as sent, after
respellings and phoneme tags) or it is asked for again.

**(KJV lesson)** That 97% is measured on Fish's own timestamps, and Fish
returns timestamps even for words it never spoke (several stamped at the same
instant). It shows the stream is whole, not that every word was said. Found in
the KJV render: asides in parentheses left out (Fish reads "(...)" as a
direction; `render.speakable` now drops the brackets from what it is sent),
short narrator lead-ins between two characters skipped, stretches missing
(Deuteronomy 17:1-2, 73 words), single words dropped ("And Jesus [said] unto
him", John 9:37), phrases garbled (John 7:22-23), and a voice that never said
"Lord God Almighty" because its design clip had skipped those words
(Revelation 16:7; see step 3C).

### The hearing gate

Every take passes `abible.gate` before it is used (`python -m abible.redo`, in
the QA environment); nothing is accepted on Fish's word alone.

1. **Words** (`omissions.audit`). The take is transcribed (Whisper medium.en,
   batched) and aligned word for word with the script. Words are compared
   exactly after the spelling table (`qa.SAME`, which must hold only true
   equivalents: "shew"/"show", not "throughly"/"thoroughly") and true
   homophones (`qa.HOMOPHONES`: here/hear, not ye/you); numbers by value (the
   KJV's "four thousand and five hundred" and "an hundred and seven and
   twenty" are the recognizer's "4,500" and "127"; "Second Kings" is "2
   Kings"); only proper names (a capitalised word the KJV never writes in lower
   case, so not "And", "LORD" or "O") may match by their sound outline, and a
   stretch of names counts as said otherwise only if as many words were heard
   as names written. Every other difference is heard again by large-v3 from a
   clip around it and settled only when the script's words are heard **in
   their place**: the clip must match more of the script it covers with those
   words than without them (`omissions.in_place`). A stretch of ten words or
   more (batched Whisper drops whole stretches) passes at nine in ten, and the
   words the clip doesn't match are then each settled like any other
   difference, as if the first pass had heard the rest: until 2026-10-13 they
   passed, and a word Fish dropped inside such a stretch with them. What still
   isn't is heard
   once more primed with the text before it; what fits then is a "listen" item
   for a person, what doesn't is a fault ("changed"), and words with nothing
   heard in their place are never primed ("missing"). Words heard that the
   script doesn't have, at the start or end of a take too and inside a stretch
   heard otherwise, are a fault ("added") when the clip matches more of the
   script with them put in than without.
   **(KJV lessons)** Each looser rule let real faults through: accepting a word
   heard anywhere in the clip passed a dropped "not" on the next verse's "Thou
   shalt not" (about half of all single-word drops); requiring it to be joined
   to a neighbour still passed one of a repeated pair ("Verily, verily" said
   once, the first "not" of Matthew 25:45) and a stutter ("drink drink");
   matching every word by consonant outline made ye/you, thee/the and man/men
   "the same"; filing every capitalised word as a name hid "And", "But" and
   "LORD". Each case is a test in `tests/test_word_check.py`. Whisper is also an
   unreliable witness: over a long window it skips text, batched it can drop a
   stretch and misplace what's around it, and on a short clip it can stop at a
   pause. So the clips are placed by Fish's line timings as well as the
   transcript's, heard in overlapping 15-second pieces, and heard again from
   each line's own start when they fall short.
2. **Voices** (`abible.voicecheck`, CPU). Every line, short ones too, is
   compared (WeSpeaker embedding) with the reference clip of every voice in its
   request. It is in the wrong voice when another voice beats its own by more
   than 0.10 (0.05 for a line under 1.2 s: clean short tags score the other
   voice 0.14 or more below their own, swapped ones 0.10 or more above); a
   voice's short lines are also pooled as an extra check; a line below 0.40
   like its own voice with nothing better, a line with no timing, or one too
   short to tell (under 0.3 s) is a "listen" item. Calibrated on 1,112 clean
   lines and the 17 swapped lines of the first 1 Samuel 17 (all caught), and on
   narrator tags given God's voice in Genesis 1 (all caught, none in 327 clean
   lines). Voices whose reference clips are 0.6 alike or more (the narrator
   and Elisha, Daniel, Stephen, Hezekiah, the Holy Spirit, Cain) can't be told
   apart, so `render.scenes` never puts two such voices in one request
   (`.cache/voices/close-pairs.json`): each speaks in a request of its own,
   where Fish can't swap them. **(KJV lessons)** A check against the take's own
   lines can't see a whole character in another voice (David's lines all
   sound alike, just not like David); and pooling short lines let a narrator's
   "And he said," in the character's voice hide among clean ones.
3. A take with a fault is kept as `<key>.unheard.mp3` with its verdict, and
   Fish is asked again, up to 4 takes, then the scene is halved, twice at
   most. The same words heard the same other way, or the same extra words
   heard, in two takes are no slip a new take would mend (the recognizer's
   blind spot or habit, or the voice's own reading: a lexicon matter): they
   don't turn a take down and are listed as "persistent" for a person to hear.
   A wrong voice found only by the whole chapter's check turns its take down
   too, and the chapter is rendered again, 3 renders at most (`redo.ROUNDS`);
   a chapter with one still left isn't done (`redo.state`), so the next run
   takes it up, and the export refuses it. A person who listens to such a line
   and finds it in its own voice records it in `.cache/out/kjv/heard-ok.json`
   (`{"GEN.1": [{"v": 3, "who": "God", "by": …, "note": …}]}`): the chapter
   check is overruled for that line, as a false alarm it keeps raising would
   otherwise never let the chapter be done.
4. Every verdict carries `gate.CHECKS_VERSION`, a fingerprint of the checking
   code. A verdict made by other checks is no verdict: the take, accepted or
   turned down, is heard again (no new request to Fish). **(KJV lesson)**
   Without this, takes passed by the first, weaker gate were reused, and
   Matthew 5 and Psalm 89 came out of a "fix" run with the same audio.

`python -m abible.redo` brings every chapter to the current plan through the
gate. A chapter is done (`redo.state`) when its audio is the current plan
(`render.plan_id`: requests, text and voices), every take has a current
verdict with no fault left, and the steps after the render (re-timing, the
chapter's voice and sound checks, the reports) have finished for those takes
(`redo.json`, written last), so a run stopped in between picks the chapter up
again. What needs an ear goes to `omissions.json`, `voicecheck.json` and the
listening guide. `python -m abible.app_export` refuses a chapter that isn't
done.

## 6. Check

The gate (above) is the word and voice check. Besides it:

1. **Sound.** Stretches far louder than the chapter's speech (noise,
   blasts), and over 25 s without a pause (babble): the faults the owner's
   lecture renders produced.
2. **Timing and files** (`abible.timingcheck`): every verse start against
   where the recognizer heard the verse begin, and every MP3's length
   against its timing file, before anything is exported.
3. **A person listens** (the casting room's `/guide`): words read the same
   way in two takes, words heard right only when primed, lines unlike their
   own voice, names said otherwise, and a reel of the hardest names; also the
   first chapter of every book and a few dialogue-heavy chapters (many
   joins). Notes go in `docs/listening.md`; a fix is a script, lexicon or cast
   change, never an audio edit.

### Volume, standardized in three places

Voice Design returns clips anywhere from −38 to −9 LUFS (measured on the 496
principal candidates), which makes quiet candidates sound worse than they are.

1. **Auditions:** the casting room plays every candidate levelled to −18 LUFS
   (spread after levelling: about 3 dB).
2. **The voice's sample:** levelled to −18 LUFS before it becomes a Fish
   voice. Fish evens out its speech largely by itself (a voice made from a
   −38 LUFS clip spoke at −18.7 LUFS; from the same clip levelled, −15.5),
   but not entirely.
3. **Each chapter:** every voice gets one gain for the chapter, bringing its
   median speech level to the chapter's (`render.level_voices`, at most
   12 dB, eased over 30 ms), so a character still whispers and shouts within
   their own level; then the chapter is normalized to −18 LUFS. Pilot: the
   spread between characters fell from 8.0 to 1.0 dB (Genesis 3) and 4.7 to
   0.8 dB (Ruth 4).

Verse timings come from Fish's timestamps, and where Fish stamped a stretch
of words at one instant a verse can appear to start after the next one
(Romans 5, Daniel 5's MENE, MENE, TEKEL). `python -m abible.retime BOOK.CH`
re-times only those lines from the recording (the audio is untouched; the
original timings stay in `<BOOK>.<CH>.fish-timing.json`).

## 7. Package

Per chapter: `<BOOK>.<CH>.mp3` (loudness −18 LUFS, peaks −1.5 dB) and
`<BOOK>.<CH>.json` in the timing format the apps already read
(`{"verses": [["1", 4.29], …], "duration": …}`), plus every line's speaker,
voice, start and end, so the app can show who is speaking.

For Scriptorium: `python -m abible app <Scriptorium folder>` writes the
recording's catalogue (`data/audio/sideload/kjv-drama.json`: reader, licence,
credit, each chapter's length and verse starts; a side-loaded recording, packed
only by a build with `SCRIPTORIUM_SIDELOAD_AUDIO=1`) and encodes every chapter
to Ogg Opus, 16 kbps mono, speech-tuned, in `app/audio/kjv-drama/` (0.58 GB for
the KJV), checking each file is as long as its MP3. It writes nothing unless
every chapter passes `app_export.check`: done by `abible.redo` with the current
checks (no fault left, no line in another voice), each MP3 whole and as long as
its timings, its verse labels the chapter's verses once each and in order.
`python -m abible.app_export` runs the checks alone; `--partial` (on either)
lists the chapters not through the gate as warnings and ships them as they
are, for a deliberate test build only. Scriptorium's docs/AUDIO.md says how the
app plays them.

Credits, shown in the app and in the files' tags:

- the translation and its licence;
- "Voices generated with Fish Audio" (AI disclosure; Fish asks for it and
  listeners deserve it);
- "Speaker assignments adapted from Glyssen (SIL and Faith Comes By Hearing,
  MIT)" and, if used, "MACULA Quotation and Speaker Data, © 2023 Clear Bible,
  Inc, CC BY 4.0".

## 8. Applying this to another translation

| Step | Quotation marks (A) | No quotation marks (B) |
|---|---|---|
| 1 | pin the text and its USFM | same |
| 2 | split at its marks, speakers from Glyssen, cross-check | project the reference script; red letter if marked; capital rule only where the language uses it |
| 2C | review disagreements | review the queue |
| 3 | reuse the cast; design only new keys | same |
| 4 | reuse the lexicon; add the translation's spellings ("Elijah" vs "Elias", "Yahweh") | same |
| 5–7 | same | same |

The WEB needs no projection at all: Glyssen's reference script *is* the WEB's
dramatized script, so its step 2 is a direct conversion and a check that the
text matches the app's WEB word for word.

## 9. Timeline and cost (KJV)

| Item | Cost | Time |
|---|---|---|
| Script and review | — | done except review (293 verses) |
| ~909 voices by Voice Design | $10–$30 of API credit | a day, plus the owner's casting session |
| Lexicon and name reel | — | days, mostly listening |
| Rendering | $0 until 2026-11-30 (~$65 after) | 6–7 hours |
| Checking | $0 (local GPU) | a night |
| Listening | — | the owner's time: the listen list, first chapters, 2% sample |
