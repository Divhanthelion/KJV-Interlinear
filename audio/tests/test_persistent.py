"""The hearing gate's "persistent" rule (render._acceptable) across a halved scene and
across runs: the same words heard the same other way in a take of the whole scene and
in a take of its half don't turn the half's take down, a take kept from an earlier run
is weighed again, a "missing" word always turns a take down, and what one half or one
chapter heard is no evidence for another. Fish and the recognizers are stubbed (each
take's faults are given), so these test the bookkeeping, not the hearing.

    cd audio && python -m pytest tests     (or: python tests/test_persistent.py)
"""
import json
import re
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from abible import fish, paths, redo, render  # noqa: E402

VOICES = {"narrator": ("v-narrator", 10.0, None), "Jabez": ("v-jabez", 10.0, None)}


def line(text, who, v):
    return {"text": text, "who": who, "key": who, "v": v}


# 1 Chronicles 4:10-43 in small: a narrator lead-in, Jabez, then the narrator's names. The
# one cut split_scene allows (a narrator line after a speaker) is before line 2.
SCENE = [line("And he called on the God of Israel, saying,", "narrator", 10),
         line("Bless me indeed, and keep me from evil.", "Jabez", 10),
         line("And God granted him that which he requested.", "narrator", 10),
         line("And the sons of the man were these.", "narrator", 37)]
FIRST, SECOND = SCENE[:2], SCENE[2:]
OTHER = [line("And the men of the city went out.", "narrator", 1),
         line("And they came to the house.", "narrator", 2)]


def changed(v, script, heard, at):
    return {"fault": "changed", "verse": v, "line": at, "script": script, "heard_in_place": heard, "who": "narrator"}


# `at` is the line in the request: the same words are another line in a half, and the
# same fault all the same
def SHIPHI(at):
    return changed(37, "Shiphi,", "shippi", at)


def SOCHO(at):
    return changed(18, "Socho,", "sosho", at)


def ASAIAH(at):
    return changed(36, "Asaiah,", "esaiah", at)


def ADDED(at):
    return {"fault": "added", "verse": 37, "line": at, "words": 2, "heard_extra": "and the", "who": "narrator"}


def MISSING(at):
    return {"fault": "missing", "verse": 37, "line": at, "words": 1, "script": "not", "who": "narrator"}


def ISRAEL(at):  # verse 10 spans both halves: one fault id can come up in either
    return changed(10, "Israel,", "isrel", at)


class Studio:
    """Fish and the hearing gate, stubbed. `plan` gives each scene (its lines' texts) the
    faults of its takes in the order they are heard; `requests` lists the scenes Fish was
    asked to voice, `heard` the scenes the gate heard."""

    def __init__(self, renders: Path, plan: dict):
        self.renders, self.plan, self.requests, self.heard, self.fish_open = renders, plan, [], [], True
        self._n = {}

    def tts(self, body):
        assert self.fish_open, "Fish was asked for a take"
        text = re.sub(r"<\|speaker:\d+\|>", "", body["text"])
        self.requests.append(text)
        words = [{"text": w, "start": i * 0.4, "end": i * 0.4 + 0.3} for i, w in enumerate(text.split())]
        return b"take", words

    def hear(self, lines, mp3, words, voices):
        scene = tuple(l["text"] for l in lines)
        n = self._n.get(scene, 0)
        self._n[scene] = n + 1
        self.heard.append(scene)
        return {"faults": [dict(f) for f in self.plan[scene][n]], "names": [], "listen": [], "voice_listen": [],
                "voices": {"checked": len(lines), "unchecked": 0}}

    def key(self, lines, attempt):
        return render.cache_key(render.body_for(*render.request_text(lines, VOICES)), attempt)

    def files(self, lines, attempt):
        k = self.key(lines, attempt)
        return {"mp3": (self.renders / f"{k}.mp3").exists(), "unheard": (self.renders / f"{k}.unheard.mp3").exists()}

    def verdict(self, lines, attempt):
        k = self.key(lines, attempt)
        return json.loads((self.renders / f"{k}.heard.json").read_text(encoding="utf-8"))


@contextmanager
def studio(plan, attempts=2, halvings=2):
    saved = (paths.RENDERS, fish.tts_with_timestamps, render.HEARING, render.CHECKS_VERSION, render.ATTEMPTS,
             render.MAX_HALVINGS)
    with tempfile.TemporaryDirectory() as tmp:
        s = Studio(Path(tmp), {tuple(l["text"] for l in k): v for k, v in plan})
        paths.RENDERS = s.renders
        fish.tts_with_timestamps = s.tts
        render.HEARING, render.CHECKS_VERSION, render.ATTEMPTS, render.MAX_HALVINGS = s.hear, "test", attempts, halvings
        try:
            yield s
        finally:
            (paths.RENDERS, fish.tts_with_timestamps, render.HEARING, render.CHECKS_VERSION, render.ATTEMPTS,
             render.MAX_HALVINGS) = saved


def keys(done):
    return [k for _, (k, _, _) in done]


def has_a_fault(verdict):
    """redo.state's test for a take in a chapter."""
    return render._acceptable(verdict, set()) is None and not redo._accepted(verdict)


def test_the_cut_is_where_the_test_scene_expects():
    assert render.split_scene(SCENE) == [FIRST, SECOND]


def test_a_half_keeps_the_faults_heard_in_the_whole():
    # The whole scene's takes both turned down (each has a fault the other hasn't); the
    # second half's first take has only faults heard the same way in the whole's takes,
    # on other lines of the request: accepted, the faults listed for an ear
    plan = [(SCENE, [[SOCHO(3), SHIPHI(3), ADDED(3)], [SHIPHI(3), ASAIAH(3)]]),
            (FIRST, [[]]),
            (SECOND, [[SHIPHI(1), ADDED(1)]])]
    with studio(plan) as s:
        done = render.render_scenes(SCENE, VOICES)
        assert keys(done) == [s.key(FIRST, 0), s.key(SECOND, 0)]
        assert len(s.requests) == 4  # two takes of the whole, one of each half
        assert s.files(SECOND, 0) == {"mp3": True, "unheard": False}
        v = s.verdict(SECOND, 0)
        assert v["persistent"] == [SHIPHI(1), ADDED(1)]  # "added" too stays on the listening list
        assert not has_a_fault(v)
        # rendered again (redo renders a chapter up to ROUNDS times): the same takes, from
        # the cache, the whole's turned-down takes giving the half its evidence again
        s.fish_open, heard = False, len(s.heard)
        assert keys(render.render_scenes(SCENE, VOICES)) == keys(done)
        assert len(s.heard) == heard


def test_a_new_fault_in_the_half_still_turns_its_take_down():
    plan = [(SCENE, [[SHIPHI(3)], [SOCHO(3)]]),
            (FIRST, [[]]),
            (SECOND, [[SHIPHI(1), ASAIAH(1)], [SHIPHI(1)]])]
    with studio(plan) as s:
        done = render.render_scenes(SCENE, VOICES)
        assert keys(done) == [s.key(FIRST, 0), s.key(SECOND, 1)]
        assert s.files(SECOND, 0) == {"mp3": False, "unheard": True}
        assert s.verdict(SECOND, 1)["persistent"] == [SHIPHI(1)]


def test_a_missing_word_is_never_persistent():
    assert render._acceptable({"faults": [MISSING(1)]}, {render._fault_id(MISSING(1))}) is None
    assert render._acceptable({"faults": [SHIPHI(1), MISSING(1)]},
                              {render._fault_id(SHIPHI(1)), render._fault_id(MISSING(1))}) is None
    # the same word missing in every take of the whole and of the half: never accepted
    plan = [(SCENE, [[MISSING(3)], [MISSING(3)]]),
            (FIRST, [[]]),
            (SECOND, [[MISSING(1)], [MISSING(1)]])]
    with studio(plan, halvings=1) as s:
        try:
            render.render_scenes(SCENE, VOICES)
        except render.Unsplittable:
            pass
        else:
            raise AssertionError("a take with a missing word was accepted")
        assert s.files(SECOND, 0) == s.files(SECOND, 1) == {"mp3": False, "unheard": True}


def test_a_kept_take_is_weighed_again():
    # 1 Chronicles 4: the half's first take was turned down by a run that had forgotten
    # the whole's takes; it stays on disk as .unheard with a verdict by the current checks
    plan = [(SECOND, [[SHIPHI(1)]])]
    with studio(plan, attempts=1) as s:
        try:
            render.render_scene(SECOND, VOICES)
        except render.Incomplete:
            pass
        else:
            raise AssertionError("the take was accepted on its own evidence")
        assert s.files(SECOND, 0) == {"mp3": False, "unheard": True}
        assert has_a_fault(s.verdict(SECOND, 0))
        s.fish_open = False  # from here no take may be asked for, and none heard again
        heard = len(s.heard)
        # weighed again with nothing seen before it: still turned down, still kept
        try:
            render.render_scene(SECOND, VOICES)
        except render.Incomplete:
            pass
        assert s.files(SECOND, 0) == {"mp3": False, "unheard": True}
        # weighed again with the whole's faults: accepted, and redo.state sees no fault
        key, mp3, words = render.render_scene(SECOND, VOICES, seen={render._fault_id(SHIPHI(3))})
        assert key == s.key(SECOND, 0) and mp3.exists() and words
        assert s.files(SECOND, 0) == {"mp3": True, "unheard": False}
        v = s.verdict(SECOND, 0)
        assert v["persistent"] == [SHIPHI(1)] and v["version"] == "test"
        assert not has_a_fault(v)
        assert len(s.heard) == heard  # a current verdict isn't heard again
        # and the accepted take is reused, weighed by the same evidence
        assert render.render_scene(SECOND, VOICES, seen={render._fault_id(SHIPHI(3))})[0] == key


def test_halves_do_not_share_what_they_heard():
    # verse 10 runs across the cut; the first half heard "Israel" otherwise in its first
    # take, the second half in its first: neither is evidence for the other
    plan = [(SCENE, [[SHIPHI(3)], [SOCHO(3)]]),
            (FIRST, [[ISRAEL(0)], []]),
            (SECOND, [[ISRAEL(0)], []])]
    with studio(plan) as s:
        given = {render._fault_id(ASAIAH(0))}
        done = render.render_scenes(SCENE, VOICES, seen=given)
        assert keys(done) == [s.key(FIRST, 1), s.key(SECOND, 1)]
        assert s.files(SECOND, 0) == {"mp3": False, "unheard": True}
        assert given == {render._fault_id(ASAIAH(0))}  # the caller's set is not written to


def test_chapters_do_not_share_what_they_heard():
    # two scenes rendered one after the other, as render_chapter does: the second's first
    # take has the fault the first scene's turned-down take had, and is turned down
    plan = [(OTHER, [[ISRAEL(0)], []]),
            (SECOND, [[ISRAEL(0)], []])]
    with studio(plan) as s:
        assert keys(render.render_scenes(OTHER, VOICES)) == [s.key(OTHER, 1)]
        assert keys(render.render_scenes(SECOND, VOICES)) == [s.key(SECOND, 1)]
        assert s.files(SECOND, 0) == {"mp3": False, "unheard": True}


def test_a_take_turned_down_for_its_voice_stays_down():
    # a take the chapter's voice check turned down (redo._turn_down) is weighed again with
    # the same fault in `seen`, and a wrong voice is not a persistent kind: it stays down
    plan = [(SECOND, [[]])]
    with studio(plan, attempts=1) as s:
        key = render.render_scene(SECOND, VOICES)[0]
        assert s.files(SECOND, 0) == {"mp3": True, "unheard": False}
        wrong = {"verse": 37, "who": "narrator"}
        redo._turn_down(key, [wrong])
        assert s.files(SECOND, 0) == {"mp3": False, "unheard": True}
        fault = s.verdict(SECOND, 0)["faults"][0]
        assert fault["fault"] == "wrong voice"
        s.fish_open = False  # no new take may be asked for
        heard = len(s.heard)
        try:
            render.render_scene(SECOND, VOICES, seen={render._fault_id(fault)})
        except render.Incomplete:
            pass
        else:
            raise AssertionError("a take with a wrong voice was accepted as persistent")
        assert s.files(SECOND, 0) == {"mp3": False, "unheard": True}
        assert "persistent" not in s.verdict(SECOND, 0)
        assert len(s.heard) == heard


def test_evidence_reaches_the_quarters():
    # the whole scene and its second half both fail; a quarter's first take has only a
    # fault the whole's takes heard, two halvings up: accepted as persistent
    Q1, Q2 = SECOND[:1], SECOND[1:]
    assert render.split_scene(SECOND) == [Q1, Q2]
    plan = [(SCENE, [[SHIPHI(3)], [SOCHO(3)]]),
            (FIRST, [[]]),
            (SECOND, [[ASAIAH(1)], [ISRAEL(1)]]),
            (Q1, [[]]),
            (Q2, [[SHIPHI(0)]])]
    with studio(plan) as s:
        done = render.render_scenes(SCENE, VOICES)
        assert keys(done) == [s.key(FIRST, 0), s.key(Q1, 0), s.key(Q2, 0)]
        assert s.files(SECOND, 1) == {"mp3": False, "unheard": True}
        assert s.files(Q2, 0) == {"mp3": True, "unheard": False}
        assert s.verdict(Q2, 0)["persistent"] == [SHIPHI(0)]


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_"):
            fn()
            print("ok", name)
