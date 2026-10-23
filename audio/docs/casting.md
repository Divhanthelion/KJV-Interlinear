# Casting: how the voices are chosen

The owner's brief (2026-10-07): every voice unique throughout the whole Bible,
consistent, and informed by pop culture and the ways biblical characters have
been cast, taking inspiration from the dramatized KJV audio Bibles.

## What we learned from the dramatized Bibles

| Production | Text | How it was cast |
|---|---|---|
| *Dramatized Audio Bible – KJV* (Thomas Nelson, 2023) | KJV | full cast, orchestral score and sound effects, 77½ hours |
| *KJV Audio Drama New Testament* (Faith Comes By Hearing) | KJV | about 180 characters, sound effects; cast with Glyssen, the data we use |
| *The Word of Promise* (Thomas Nelson) | NKJV | 600+ actors, 90+ hours, score and effects. Michael York narrates; Martin Jarvis is God; Jim Caviezel Jesus; Richard Dreyfuss Moses; Gary Sinise David; Jon Voight Abraham; Max von Sydow Noah; Malcolm McDowell Solomon; Joan Allen Deborah; Marcia Gay Harden Esther; Ernie Hudson Peter; Marisa Tomei Mary Magdalene; Luke Perry Judas. **The cast includes the writers** (John Heard Matthew, Lou Diamond Phillips Mark, Chris McDonald Luke, Louis Gossett Jr. John, John Schneider James, Stacy Keach Paul); Mark and Luke have no lines of their own in the text, so the books are evidently voiced by their writers. |
| *Truth & Life* (Zondervan, 2010) | RSV-CE NT | John Rhys-Davies narrates; Brian Cox is God; Neal McDonough Jesus; Julia Ormond Mary; Kristen Bell Mary Magdalene; Malcolm McDowell Caiaphas |
| *The Bible Experience* (Zondervan, 2006–07) | TNIV | 200+ African-American actors: Samuel L. Jackson God, Blair Underwood Jesus, Forest Whitaker Moses, Angela Bassett Esther, Eartha Kitt the serpent, Cuba Gooding Jr. Jonah, Denzel Washington the Song of Songs |

Patterns worth keeping:

1. **The narrator is a warm, mature, cultured voice that never upstages the
   story** (York, Rhys-Davies; Orson Welles narrating *King of Kings*, 1961).
   He carries half the Bible's words.
2. **God is deep and calm, not loud.** The productions that worked chose
   gravity and stillness (Jarvis, Cox); the films that are remembered chose
   intimacy (the burning bush in *The Prince of Egypt*). We want both:
   immense, still, warm.
3. **Jesus is human, warm and present.** The portrayals people love most
   (Jonathan Roumie in *The Chosen*, Robert Powell in *Jesus of Nazareth*)
   speak *to* people, never at them, and carry authority quietly.
4. **Contrast inside a scene matters more than star power.** Moses against
   Aaron, Jacob against Esau, Job's four friends, Saul against David against
   Jonathan, Pilate against Caiaphas against Herod.
5. **Villains through calm, not growls**: Donald Pleasence's tempter in *The
   Greatest Story Ever Told*, the androgynous Satan of *The Passion of the
   Christ*.

## What the text says about voices

Where scripture describes a voice or a manner of speech, the brief follows it:

| Character | The text | So the voice is |
|---|---|---|
| Moses | "I am slow of speech, and of a slow tongue" (Exodus 4:10); "very meek" (Numbers 12:3); 80 at the Exodus | old, weathered, deliberate, a little heavy-tongued; immovable |
| Aaron | "I know that he can speak well" (Exodus 4:14) | fluent and smoother than Moses |
| Jacob and Esau | "The voice is Jacob's voice, but the hands are the hands of Esau" (Genesis 27:22) | unmistakably different: Jacob soft and clever, Esau loud and rough |
| Peter | "thy speech bewrayeth thee" (Matthew 26:73), a Galilean | a rough, rustic working man's voice |
| Paul | "his bodily presence is weak, and his speech contemptible" (2 Corinthians 10:10) | intense, quick, reedy: a writer, not an orator |
| Elihu | "I am young, and ye are very old" (Job 32:6) | the youngest voice among Job's friends |
| Samuel | "I am old and grayheaded" (1 Samuel 12:2) | grave and upright |
| Goliath | a giant | the biggest voice in the Bible |
| Wisdom | a woman crying in the streets (Proverbs 8) | radiant, clear, commanding |
| God | "a still small voice" (1 Kings 19:12); "as a man speaketh unto his friend" (Exodus 33:11) | deep and quiet, intimate as well as immense |

## The rules

- **Every voice is designed new** with Fish's Voice Design from a written
  description. No voice is cloned from a recording of a real person, and no
  description names a person or asks for an imitation. Fish forbids cloning
  without permission; we wouldn't anyway. (The `inspiration` notes in
  `cast/principals.json` cite productions for our own reference; they are
  never sent to Fish.)
- **One character, one voice, everywhere**, in every translation.
- **Unique is measured**: speaker embeddings keep any two voices apart (SOP
  3C), and the strictest limit applies inside a book, where listeners hear
  them side by side.
- **Accent**: natural, timeless English with a light British colour (the
  tradition of the KJV recordings), the same for everyone. Voices differ by
  age, pitch, texture, pace and temperament, not by caricatured accents.
  (Open: docs/decisions.md.)
- **Age**: a character keeps one voice as they age ("David (old)" is David,
  older); children get a child's voice.
- **The letters**: read by their writers, unless the owner prefers the
  narrator (decisions.md). The Word of Promise cast its writers, which
  suggests it did the same.

## The cast in numbers (KJV)

| | |
|---|---|
| Speaking characters and groups | 999 casting keys |
| Voices to design | 909 |
| Sharing a voice (old age, joint lines) | 90 |
| Briefs written by hand | 125, the speakers of about 93% of all words |
| Briefs from attributes | 783 |
| Narrator's share of the words | ~45% with author voices for the letters (~53% without) |
| God's share | ~18% (Glyssen gives the prophets' "thus saith the LORD" oracles to God) |

The hand briefs are in `cast/principals.json`, each with its inspiration and
its scripture; the full cast, with every brief and the lines each character
reads, is `cast/cast.json`.
