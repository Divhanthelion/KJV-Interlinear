"""The hearing gate's word check (omissions.audit) catches each fault a looser rule let
through, and passes what was said right. The recognizers are stubbed (every clip is
heard as `clip`, or as what was said), so these test the rules, not Whisper.

    cd audio && python -m pytest tests     (or: python tests/test_word_check.py)
"""
import sys
import types
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from abible import omissions, qa  # noqa: E402

MT_25_45 = ("Then shall he answer them, saying, Verily I say unto you, Inasmuch as ye did it not to one of the "
            "least of these, ye did it not to me.")
JN_3_3 = ("Jesus answered and said unto him, Verily, verily, I say unto thee, Except a man be born again, he "
          "cannot see the kingdom of God.")
SA2_18_33 = "O my son Absalom, my son, my son Absalom! would God I had died for thee, O Absalom, my son, my son!"
JN_6_53 = "Except ye eat the flesh of the Son of man, and drink his blood, ye have no life in you."
GEN_1_8 = "And God called the firmament Heaven. And the evening and the morning were the second day."
EX_20 = ["Thou shalt not kill.", "Thou shalt not commit adultery."]
CH1_1 = "Adam, Sheth, Enosh, Kenan, Mahalaleel, Jered, Henoch, Methuselah, Lamech,"
EX_14_26 = ("And the LORD said unto Moses, Stretch out thine hand over the sea, that the waters may come again "
            "upon the Egyptians, upon their chariots, and upon their horsemen.")
EX_14_26_FIRST = "And the Lord said unto Moses, and upon their horsemen."  # 17 words in between not heard


def check(script_lines, spoken, clip=None, first=None):
    """The faults audit finds when `spoken` is what the take's first pass heard (or
    `first`, with every clip heard as what was spoken)."""
    real = omissions.clip_text, qa.model, qa.pcm16k
    omissions.clip_text = lambda mp3, start, end: clip or spoken
    qa.model = lambda name: types.SimpleNamespace(
        transcribe=lambda *a, **k: ([types.SimpleNamespace(text=clip or spoken)], None))
    qa.pcm16k = lambda *a, **k: None
    try:
        words = [{"text": w, "start": i * 0.4, "end": i * 0.4 + 0.3} for i, w in enumerate((first or spoken).split())]
        lines = [{"text": t, "v": n + 1, "who": "x"} for n, t in enumerate(script_lines)]
        report = omissions.audit(lines, Path("x.mp3"), words, None)
    finally:
        omissions.clip_text, qa.model, qa.pcm16k = real
    return {k: report[k] for k in ("missing", "changed", "added") if report[k]}


def test_said_right_passes():
    assert check([MT_25_45], MT_25_45) == {}
    assert check(EX_20, " ".join(EX_20)) == {}


def test_a_dropped_word_heard_in_the_next_verse_is_missing():
    # The old rule found "not" anywhere in the clip, here in "Thou shalt not commit adultery".
    assert check(EX_20, "Thou shalt kill. Thou shalt not commit adultery.")


def test_one_of_a_repeated_pair_dropped():
    assert check([MT_25_45], MT_25_45.replace("ye did it not to one", "ye did it to one"))
    assert check([JN_3_3], JN_3_3.replace("Verily, verily,", "Verily,"))
    assert check([SA2_18_33], SA2_18_33.replace("my son, my son!", "my son!"))


def test_a_word_dropped_inside_a_long_stretch_the_first_pass_missed():
    # Batched Whisper drops whole stretches; the clip heard in their place matched nine in ten
    # of a long one, so a word Fish dropped inside it passed until 2026-10-13
    assert check([EX_14_26], EX_14_26.replace("come again upon", "come upon"), first=EX_14_26_FIRST)
    assert check([EX_14_26], EX_14_26, first=EX_14_26_FIRST) == {}


def test_a_stutter_is_added():
    assert check([JN_6_53], JN_6_53.replace("drink his", "drink drink his"))


def test_words_added_at_the_edges_of_a_take():
    assert check([GEN_1_8], GEN_1_8 + " And God said, Let the waters under the heaven")
    assert check([GEN_1_8], "And it was so. " + GEN_1_8)


def test_a_word_added_inside_a_stretch_heard_otherwise():
    assert check(["And the LORD spake unto Moses, saying,"], "And the Lord God spoke unto Moses, saying",
                 clip="And the LORD God spake unto Moses, saying")


def test_names_may_be_spelled_the_recognizers_way():
    assert check([CH1_1], "Adam, Seth, Enosh, Keenan, Mahalalel, Jared, Enoch, Methuselah, Lamech") == {}
    assert check(["the ark of the LORD continued in the house of Obed-edom the Gittite"],
                 "the ark of the Lord continued in the house of Obeidodom the Gittite") == {}
    assert check(["the Philistines gathered together their armies to battle, and pitched in Ephes-dammim."],
                 "the Philistines gathered together their armies to battle and pitched in Ephastamim") == {}


def test_a_name_dropped_from_a_list_of_names_is_missing():
    assert check([CH1_1], "Adam, Sheth, Enosh, Keenan, Mahalalel, Enoch, Methuselah, Lamech")
    assert check([CH1_1], "Adam, Seth, Enosh, Mahalalel, Jared, Enoch, Methuselah, Lamech")


def test_only_names_match_by_sound():
    # "O", "LORD" and "And" are capitalised but no names; he/she share an outline.
    assert check(["Have mercy upon me, O LORD; for I am weak:"], "Have mercy upon me, my Lord, for I am weak")
    assert check(["And he said unto her, Go"], "And she said unto her, Go")


def test_numbers_compare_by_value():
    assert check(["this is Ahasuerus which reigned, from India even unto Ethiopia, over an hundred and seven and "
                  "twenty provinces:"],
                 "this is Ahasuerus which reigned from India even unto Ethiopia over 127 provinces") == {}
    assert check(["the north side four thousand and five hundred, and the south side four thousand and five hundred,"],
                 "the north side 4 ,500 and the south side 4 ,500") == {}
    assert check(["Second Kings, chapter five."], "2 Kings, chapter 5.") == {}


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_"):
            fn()
            print("ok", name)
