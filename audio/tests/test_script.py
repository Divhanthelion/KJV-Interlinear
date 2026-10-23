"""The script never changes a word: every verse's lines rejoin to the verse.

    cd audio && python -m pytest tests     (or: python tests/test_script.py)
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from abible import books, cast, kjv, lexicon, project, script  # noqa: E402
from abible.glyssen import NARRATOR, Piece  # noqa: E402


def test_every_verse_rejoins_exactly():
    total = 0
    for code in books.CODES:
        book = script.load(code)
        assert script.check_book(book) == [], code
        total += len(kjv.verses(code))
    assert total == 31_102 + 116  # verses + Psalm titles (verse 0)


def test_every_chapter_is_announced_first():
    for code in books.CODES:
        for ch in script.load(code)["chapters"]:
            first = ch["lines"][0]
            assert first.get("kind") == "announce" and first["who"] == NARRATOR, (code, ch["chapter"])
            assert first["text"] == books.announcement(code, ch["chapter"])


def test_overrides_rejoin_too():
    if not script.OVERRIDES.exists():
        return
    for ref, fix in json.loads(script.OVERRIDES.read_text(encoding="utf-8"))["verses"].items():
        code, cv = ref.split(" ")
        c, v = map(int, cv.split(":"))
        assert " ".join(line[1] for line in fix["lines"]) == kjv.verses(code)[(c, v)], ref
        assert fix.get("reason"), f"{ref}: every override says why"


def test_kjv_speech_starts_with_a_capital():
    pieces = [Piece(NARRATOR, None, None, "God said,"), Piece("God", None, None, "«Let there be light,»"),
              Piece(NARRATOR, None, None, "and there was light.")]
    segs, notes = project.project_verse("And God said, Let there be light: and there was light.", pieces)
    assert [(p.who, t) for p, t in segs] == [
        (NARRATOR, "And God said,"), ("God", "Let there be light:"), (NARRATOR, "and there was light.")]
    assert notes == []


def test_indirect_speech_stays_with_the_narrator():
    pieces = [Piece(NARRATOR, None, None, "He sternly warned them, saying:"),
              Piece("Jesus", None, None, "«Do not make my presence known.»")]
    segs, notes = project.project_verse("And he straitly charged them that they should not make him known.", pieces)
    assert [(p.who, t) for p, t in segs] == [(NARRATOR, "And he straitly charged them that they should not make him known.")]
    assert notes and notes[0].startswith("merged")


def test_saith_the_lord_interrupts_but_does_not_end_the_speech():
    pieces = [Piece("God", None, None, "«In that day,»"), Piece(NARRATOR, None, None, "says Yahweh,"),
              Piece("God", None, None, "«there will be the noise of a cry from the fish gate»")]
    verse = "And it shall come to pass in that day, saith the LORD, that there shall be the noise of a cry from the fish gate"
    segs, _ = project.project_verse(verse, pieces)
    assert [p.who for p, _ in segs] == ["God", NARRATOR, "God"]
    assert segs[1][1] == "saith the LORD,"


def test_red_letter_outranks_the_projection_but_not_a_quoter():
    verse = "Saying, What wilt thou that I shall do unto thee? And he said, Lord, that I may receive my sight."
    segs = [(Piece("Jesus", None, None, ""), "Saying, What wilt thou that I shall do unto thee?"),
            (Piece(NARRATOR, None, None, ""), "And he said,"),
            (Piece("Bartimaeus", None, None, ""), "Lord, that I may receive my sight.")]
    out, _ = project.apply_red_letter(verse, segs, ["What wilt thou that I shall do unto thee?"])
    assert [(p.who, t) for p, t in out] == [(NARRATOR, "Saying,"), ("Jesus", "What wilt thou that I shall do unto thee?"),
                                            (NARRATOR, "And he said,"), ("Bartimaeus", "Lord, that I may receive my sight.")]
    quoting = [(Piece("Paul", None, None, ""), "And he said unto me, I am Jesus of Nazareth.")]
    out, notes = project.apply_red_letter("And he said unto me, I am Jesus of Nazareth.", quoting, ["I am Jesus of Nazareth."])
    assert [p.who for p, _ in out] == ["Paul"] and "kept with them" in notes[0]


def test_lexicon_tags_only_the_metal():
    assert "phoneme_start" in lexicon.apply("they sank as lead in the mighty waters.")
    assert "phoneme_start" not in lexicon.apply("The LORD shall lead thee.")


def test_old_age_shares_the_voice_and_children_do_not():
    entries = cast.load()
    assert cast.resolve("David (old)", entries) == ("David", "old")
    assert cast.resolve("Jesus (child)", entries)[0] == "Jesus (child)"


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_"):
            fn()
            print("ok", name)
