"""Split a verse of a translation without quotation marks (the KJV) into
speakers, by projecting Glyssen's reference script (the WEB, split by speaker).

WEB and KJV words are aligned, each boundary between WEB pieces is mapped into
the KJV, and snapped to a gap after punctuation. Two facts about the KJV make
this reliable: direct speech starts with a capital after a comma or colon
("And God said, Let there be light"), and it follows a verb of speaking.

Where the KJV has no place for a boundary, the words stay with whoever said
them in the KJV's own wording: into a quote, the KJV has indirect speech
("he charged them that they should tell no man", which the WEB script turns
into "Tell no one"), so they stay with the narrator; out of a quote, the KJV
speech runs on, so they stay with the speaker. Such verses are reported.
"""
import difflib
import re

from .glyssen import NARRATOR, Piece

# Words that differ only by era or by translation habit, mapped to one form.
EQUIV = {
    "yahweh": "lord", "jehovah": "lord", "you": "ye", "thee": "ye", "thou": "ye", "your": "thy", "yours": "thine",
    "thine": "thy", "has": "hath", "does": "doth", "says": "saith", "to": "unto", "are": "art", "were": "wast",
    "sky": "heaven", "heavens": "heaven", "shows": "sheweth", "show": "shew",
}

SPEECH = re.compile(
    r"\b(said|saith|saying|say|spake|speak|spoken|answered|answering|cried|crying|called|calling|asked|"
    r"commanded|sware|prayed|praying|wrote|written|sent|shouted|sang|sung|told|tell|thus|lamented|"
    r"blessed|cursed|vowed|charged|proclaimed|replied|besought|beseeching|declared)\b[^,:;.?!]{0,30}[,:;]\s*$",
    re.I)


def toks(text: str):
    """[(normalized word, start, end)] over the text."""
    out = []
    for m in re.finditer(r"[A-Za-z0-9’']+(?:-[A-Za-z0-9’']+)*", text):
        w = m.group().lower().replace("’", "'")
        w = re.sub(r"(eth|est|edst|st)$", "", w) if len(w) > 5 else w
        out.append((EQUIV.get(w, w), m.start(), m.end()))
    return out


def gaps(text: str):
    """Places a verse can be split: (offset after the space, punctuation, next word capitalized)."""
    return [(m.end(), m.group(1), m.group(2).isupper()) for m in re.finditer(r"([,;:.?!)])\s+(?=(\S))", text)]


def _is_char(p: Piece) -> bool:
    return p.who != NARRATOR


def project_verse(kjv: str, pieces: list):
    """Returns ([(Piece, KJV text)], notes) or (None, notes) when it can't."""
    if len(pieces) == 1 or len({p.who for p in pieces}) == 1:
        p = pieces[0]
        return [(p, kjv)], []
    web = " ".join(p.text for p in pieces)
    wt, kt = toks(web), toks(kjv)
    sm = difflib.SequenceMatcher(None, [w for w, _, _ in wt], [w for w, _, _ in kt], autojunk=False)
    pairs = [(a + i, b + i) for a, b, n in sm.get_matching_blocks() for i in range(n)]
    bounds, pos = [], 0
    for p in pieces[:-1]:
        pos += len(toks(p.text))
        bounds.append(pos)
    cands = gaps(kjv)
    notes, cuts = [], []
    for i, bw in enumerate(bounds):
        before = [kb for wb, kb in pairs if wb < bw]
        after = [kb for wb, kb in pairs if wb >= bw]
        lo = (before[-1] + 1) if before else 0
        hi = after[0] if after else len(kt)
        lo_c = kt[lo - 1][2] if lo > 0 else 0
        hi_c = kt[hi][1] if hi < len(kt) else len(kjv)
        into_quote = pieces[i + 1].who != pieces[i].who and _is_char(pieces[i + 1])
        # "In that day, saith the LORD, that there shall be": the narrator only
        # interrupts, and the same speaker carries on in lower case.
        resumes = into_quote and i > 0 and pieces[i - 1].who == pieces[i + 1].who and not _is_char(pieces[i])
        floor = cuts[-1] if cuts else 0
        ok = [c for c in cands if lo_c <= c[0] <= hi_c + 1 and c[0] > floor]
        if into_quote and not resumes:
            # KJV direct speech opens with a capital after a verb of speaking.
            # The WEB may word the introduction differently ("David said to
            # Solomon his son," / "And David said to Solomon, My son,"), so look
            # a few words either side; with no such place at all, the KJV has
            # indirect speech here and the merge below keeps it with the narrator.
            wide_lo = kt[max(lo - 4, 0)][1]
            wide_hi = kt[min(hi + 3, len(kt) - 1)][2] if kt else len(kjv)
            near = [c for c in cands if wide_lo <= c[0] <= wide_hi + 1 and c[0] > floor and c[2]]
            spoken = [c for c in near if SPEECH.search(kjv[max(0, c[0] - 40):c[0]])]
            ok = [c for c in ok if c[2] and c in spoken] or [c for c in ok if c[2]] or spoken
        if len(ok) == 1:
            cuts.append(ok[0][0])
        elif ok:
            mid = (lo_c + hi_c) / 2
            cuts.append(min(ok, key=lambda c: abs(c[0] - mid))[0])
            notes.append(f"ambiguous: {len(ok)} places for the boundary before {pieces[i + 1].who}")
        else:
            a, b = pieces[i], pieces[i + 1]
            if into_quote and _is_char(a):
                return None, notes + [f"no place in the KJV for the change from {a.who} to {b.who}"]
            keep = a if into_quote or _is_char(a) else b
            merged = Piece(keep.who, keep.delivery, keep.script_as, a.text + " " + b.text)
            segs, more = project_verse(kjv, pieces[:i] + [merged] + pieces[i + 2:])
            why = "indirect speech in the KJV" if into_quote else "the KJV speech runs on"
            gone = b.who if into_quote else a.who if keep is b else b.who
            return segs, notes + [f"merged: {gone} kept with {keep.who} ({why})"] + more
    segs, start = [], 0
    for p, c in zip(pieces, cuts + [len(kjv)]):
        segs.append((p, kjv[start:c].strip()))
        start = c
    merged = []
    for p, t in segs:
        if merged and merged[-1][0].who == p.who:
            merged[-1] = (merged[-1][0], merged[-1][1] + " " + t)
        else:
            merged.append((p, t))
    if any(not t for _, t in merged):
        return None, notes + ["a speaker would get no words"]
    return merged, notes


def is_christ(who: str) -> bool:
    return who == "Jesus" or who.startswith("Jesus (")


def no_red_letter(segs: list):
    """A verse the KJV prints without red letter: whatever the projection gave
    Christ is the narrator's (the KJV has it as narration or indirect speech)."""
    if not any(is_christ(p.who) for p, _ in segs):
        return segs, []
    narrator = Piece(NARRATOR, None, None, "")
    out = []
    for p, t in segs:
        p = narrator if is_christ(p.who) else p
        if out and out[-1][0].who == p.who:
            out[-1] = (out[-1][0], out[-1][1] + " " + t)
        else:
            out.append((p, t))
    return out, ["no red letter in the KJV here: the projection's lines for Christ given to the narrator"]


def apply_red_letter(kjv: str, segs: list, spans: list):
    """Make Christ's words exactly the KJV's red letter (its own markup, so it
    outranks the projection). Character by character: red text in narrator
    lines becomes Christ's, non-red text in Christ's lines becomes the
    narrator's, and red text inside another speaker's line stays with that
    speaker (they are quoting him; nested quotations stay with whoever quotes
    them, as in FCBH's scripts). Returns (segs, notes)."""
    red = [False] * len(kjv)
    at = 0
    for span in spans:
        i = kjv.find(span, at)
        if i < 0:
            return segs, [f"red-letter span not found in the verse: {span[:40]}"]
        for k in range(i, i + len(span)):
            red[k] = True
        at = i + len(span)
    # the speaker of every character, from the projected lines
    owner, at = [None] * len(kjv), 0
    for p, t in segs:
        i = kjv.find(t, at)
        for k in range(i, i + len(t)):
            owner[k] = p
        at = i + len(t)
    christ = next((p for p, _ in segs if is_christ(p.who)), Piece("Jesus", None, None, ""))
    narrator = Piece(NARRATOR, None, None, "")
    nested = set()
    final = []
    for k in range(len(kjv)):
        p = owner[k] or narrator
        if red[k] and p.who == NARRATOR:
            p = christ
        elif not red[k] and is_christ(p.who):
            p = narrator
        elif red[k] and not is_christ(p.who):
            nested.add(p.who)
        final.append(p)
    out, start = [], 0
    for k in range(1, len(kjv) + 1):
        if k == len(kjv) or final[k] is not final[start]:
            piece = kjv[start:k].strip()
            if piece:
                if out and out[-1][0].who == final[start].who:
                    out[-1] = (out[-1][0], out[-1][1] + " " + piece)
                else:
                    out.append((final[start], piece))
            start = k
    changed = [(p.who, t) for p, t in out] != [(p.who, t) for p, t in segs]
    notes = []
    if changed:
        had = sum(is_christ(p.who) for p, _ in segs)
        notes.append(f"red letter applied (the projection had {had} line(s) for Christ)")
    if nested:
        notes.append(f"red letter inside {', '.join(sorted(nested))}'s words: kept with them (quoting Christ)")
    if any(not t[0].isalnum() and t[0] not in "(‘'" for _, t in out):
        notes.append("a red-letter boundary splits a word or punctuation: check by hand")
    return out, notes
