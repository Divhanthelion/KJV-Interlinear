# Decisions

## Made (2026-10-07), all reversible

| # | Decision | Why |
|---|---|---|
| D1 | Our own narration with Fish Audio, generated from our exact text | only way to guarantee every word matches (the earlier search for a recording ruled out Scourby and McLean, and found no free recording that matches word for word) |
| D2 | Speakers from SIL Glyssen (MIT), the data Faith Comes By Hearing casts its dramatized Bibles with | 1,281 characters, verse by verse, with age, gender and delivery; FCBH's own script of the WEB |
| D3 | For the KJV, Glyssen's WEB script is projected onto the KJV wording, with the KJV's capitals and red letter as checks; KJV indirect speech stays with the narrator | the KJV has no quotation marks; we never put the narrator's words in a character's mouth |
| D4 | Nested quotations stay with whoever quotes them | FCBH's convention; a character retelling a conversation is still that character |
| D5 | One voice per character for the whole Bible and every translation | consistency; a new translation designs only the characters it adds |
| D6 | Every voice designed new (Fish Voice Design) from a written description; no voice cloned from a real person; no brief sent to Fish names a person (the `inspiration` notes in `cast/principals.json` are for us and never sent) | Fish's rules and ours; castings inspire, they are not copied |
| D7 | Uniqueness measured with speaker embeddings (limits in voices.py) | "unique" has to be checked, not hoped |
| D8 | "(old)" variants share the character's voice; children get their own | a voice should age, not be recast; an adult can't voice a child |
| D9 | Groups are voiced by one spokesperson voice for now | ensembles need a mixing step; later (SOP 3E) |
| D10 | Narrator reads book titles, chapter announcements, Psalm titles, Psalm 119's letters and the epistles' colophons | they are part of the 1769 text as printed; titles taken from eBible's USFM |
| D11 | One Fish request per chapter where the voices fit; else scenes cut at narrator lines | Fish keeps voices steady within a request; the owner's lecture renders found joins and cold starts audible |
| D12 | Delivery cues only at the start of a line, from a short list | `[emphasis]` and `[break]` misbehaved in the lecture renders |
| D13 | Pronunciation by phoneme tags from our own lexicon (sources: CMU dictionary, ISBE 1915 respellings, the owner's ear) | Fish's dictionaries match substrings ("Ai" in "said") |
| D14 | Words checked by three transcription passes; a person listens to what survives all three | the recognizers expect modern English and mishear KJV grammar |
| D15 | Output: MP3, mono, 96 kbit/s, −18 LUFS; timings in the format the apps already read, plus each line's speaker | drama needs a little more than speech-only 64 kbit/s; the format is Scriptorium's |
| D16 | Speed 1.0 (about 185 words a minute) | the test voices; revisit with the real cast (D-open 9) |
| D18 | Every verified fault is fixed by rendering the chapter again (missing or garbled words, a line in the wrong voice, anything the owner hears), until a recognizer hears every word and every line is in its own voice; the owner's ear is the last check | owner, 2026-10-11: "I want this totally and completely perfect" (replacing 2026-10-09's "I wouldn't bother trying to correct what we've already done", which was about not holding up the first full render) |
| D19 | Text is sent to Fish without parentheses (`render.speakable`); the script keeps them | Fish read the KJV's parenthetical asides as directions and left them out (2026-10-10) |
| D21 | A take is accepted only through the hearing gate (`abible.gate`): every word of its text heard in its place, in order beside its own neighbours (a single word not heard, heard as another even when the recognizer is primed, or added, turns it down), and every line compared with its character's reference voice and the other voice of its request (at most 2 a request, D22; a line nearer the other voice turns it down); Fish is asked for up to 4 takes (`redo.ATTEMPTS`), then the scene is halved, at most twice (`render.MAX_HALVINGS`); the same words heard the same other way, or the same words added, in two takes are the recognizer's or the voice's habit, not a slip: they don't turn a take down and are listed for a person's ear; voices too alike to tell apart (`.cache/voices/close-pairs.json`) never share a request | Fish's timestamps include words it never spoke, so only a recognizer can tell (2026-10-10); a word heard anywhere in the clip passed a dropped "not" (SOP 5, the hearing gate) |
| D22 | At most 2 voices in one Fish request (`render.MAX_VOICES`; first 3, then 2 after the whole-Bible measurement: wrong-voice lines 0.4% at 2 voices, 3.8% at 3); a chapter with more is cut into scenes at narrator lines, and inside a passage where a third voice, or a voice too alike to tell from one already in the request, would join | with more, Fish loses track of who is who: 1 Samuel 17, one request of nine voices, gave 15 of 81 lines another character's voice (David as Jesse throughout); the same chapter at 3 voices a request: none (2026-10-12). Revises D11's "one request per chapter" |
| D23 | Pace is left as each voice reads it (Peter, David and John read about 25% faster than the narrator) | owner, 2026-10-12: "leave as is" |
| D24 | Delivery cues only for clear acted moments (weeping, shouting, whispering, pleading, trembling, laughing, mocking, amazed, angry, weak); laments and songs are read as heartfelt speech, not sobbed or sung; notes are matched as whole words | owner, 2026-10-12. The first cue table matched inside words ("sing" in "blessing" gave God's blessing in Genesis 1:28 "[singing softly]") |
| D25 | Names read inconsistently or wrongly are pinned to dictionary pronunciations (CMU, ISBE 1915) and re-rendered; the owner approves a short reel of the hardest | owner, 2026-10-12 |
| D20 | In Scriptorium the dramatized KJV is the recording `kjv-drama`: Ogg Opus 16 kbps mono like the app's other recordings, reader "a full cast", licence "Generated with Fish Audio" | the app's Listen feature (Scriptorium branch `audio/kjv-drama`, from `audio/bibles`); a store build waits on open question 2 |
| D17 | The pipeline lives in `audio/` on branch `audio/dramatized-kjv` of this repository, self-contained (Python, standard library for everything but the checks) | it moves to Scriptorium or its own repository unchanged (open 12) |

## Open: for the owner

1. **API credit.** Voice Design needs it ($0.01 a request; the cast is
   ~$10–$30). Add it at <https://fish.audio/app/developers>. $100 or more
   also raises the account from 5 to 15 requests at a time (not needed: the
   whole KJV renders in a night at 5). Note: the balance went from $0.00 to
   −$0.36 during the 2026-10-07 session. Every kind of request made in it
   (free-model speech with library or own voices, up to 20 voices, varied
   settings, voice creation) was re-tested and cost nothing, and the key is
   shared with contracts, rustfinal and picture_ingest, so the charge could
   not be traced from here: the usage page on fish.audio shows what it was.
2. **Licence.** Fish's terms limit the free plan to personal, non-commercial
   use; its free-model page says small businesses are fine. Topping up makes
   the account a paying one. Recommended: email support@fish.audio and get it
   in writing that `s2.1-pro-free` audio may ship in a free Bible app.
3. **Accent.** Proposed: one natural, timeless English with a light British
   colour for everyone (the KJV recording tradition). Alternatives: neutral
   American; or subtle distinct flavours for foreign courts (Egypt, Babylon,
   Persia, Rome), which risks caricature.
4. **The letters.** Proposed: read by their writers (Paul, Peter, James,
   John, Jude; Revelation by John; Lamentations by Jeremiah; Ecclesiastes by
   the Preacher). Alternative: the narrator, as FCBH does.
5. **The Psalms.** Now the narrator. Alternative: each psalm by the author
   its title names (David, Asaph, the sons of Korah, Moses, Solomon, Heman,
   Ethan), the rest by the narrator.
5b. **The Song of Solomon.** Glyssen leaves most of its speakers open, so
   the script reads it with the narrator except a few lines Glyssen assigns.
   Alternative: a full three-voice reading (the bride, the groom, the
   friends), following the speaker headings most modern Bibles print.
6. **The angel of the LORD.** Now its own voice. FCBH's tradition casts it
   with the actor who plays Jesus.
7. **God's voice.** Proposed: one voice, deep and still, with no processing.
   Some productions add a little reverb to divine speech; we can try it on
   the pilot.
8. **The casting session.** The 125 principal voices: about 4 candidates
   each on one page. Then the name reel.
9. **Pace.** 1.0 gives about 70 hours; Thomas Nelson's dramatized KJV runs
   77½. 0.9 would be closer to it.
10. **Music and sound effects.** The productions that inspired this have a
    score and effects. Not in scope yet; it would be a separate layer
    (licensed or public-domain music, CC0 effects) mixed under the voices.
11. **Where it ships.** ~70 hours at 96 kbit/s is about 3 GB: too big to
    bundle in a store package. Options: an optional in-app download (changes
    the privacy policy's "no network requests"), or a separate download.
12. **Where the pipeline lives.** Here (KJV Interlinear), in Scriptorium, or
    its own repository. It serves both apps.
13. **Test voices in the Fish account.** 22 private voices titled "abible
    probe (delete me)" and tagged `abible-test`, made to measure the
    request limits (a clone of a library test clip). Delete them when
    convenient (README has the command), or tell me to.
