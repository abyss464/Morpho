"""`codex.generate`, end to end against a stub generator."""

from __future__ import annotations

import pytest
from morpho_adapter_common import PermanentError
from PIL import Image

from conftest import write_image
from morpho_codex import prompt
from morpho_codex.ops import generate


def request(staging, **overrides):
    params = {
        "word_id": 7,
        "lemma": "abandon",
        "pos": "verb",
        "primary_definition": "to give up completely",
        "slot1_sentence": "She had to abandon the car in the flood.",
        "prompt_ver": prompt.CURRENT_VERSION,
        "width": 768,
        "height": 576,
        "out_path": str(staging),
    }
    params.update(overrides)
    return params


def test_a_generation_lands_at_the_path_the_engine_owns(fake_codex, staging):
    result = generate(request(staging))
    assert staging.is_file()
    with Image.open(staging) as image:
        assert image.format == "WEBP"
        assert image.size == (768, 576)
    assert result["model"] == "stub-generator"
    assert "She had to abandon the car" in result["prompt"]


def test_the_render_is_cleaned_up(fake_codex, staging):
    """Only the engine's own path may be left behind: morphod hashes whatever is
    in its staging directory and the janitor sweeps the rest, but leaving a
    second copy of every picture there is untidy at 6000 words."""
    generate(request(staging))
    assert list(staging.parent.iterdir()) == [staging]


def test_a_silent_refusal_is_permanent(fake_codex, staging):
    """Exit zero with nothing written is the ordinary shape of a content-policy
    refusal, and the same prompt will be refused again."""
    fake_codex("import sys; sys.stdin.read()")
    with pytest.raises(PermanentError, match="no image"):
        generate(request(staging))


def test_a_blank_canvas_never_reaches_the_library(fake_codex, staging):
    """`ops/verify_genimg.py`'s gate: a quota failure sometimes writes a solid
    white rectangle rather than failing, and one of those in a slot is worse
    than no picture at all."""
    fake_codex(
        "import re, sys\n"
        "from pathlib import Path\n"
        f"sys.path.insert(0, {str(__file__).rsplit('/', 1)[0]!r})\n"
        "from conftest import write_image\n"
        "target = re.search(r'exact path:\\n(\\S+)', sys.stdin.read()).group(1)\n"
        "write_image(Path(target), noisy=False)\n"
    )
    with pytest.raises(PermanentError, match="blank canvas"):
        generate(request(staging))
    assert not staging.exists(), "a rejected render must not land at the owned path"


def test_a_failing_generator_is_permanent(fake_codex, staging):
    fake_codex("import sys; sys.stdin.read(); sys.stderr.write('quota\\n'); sys.exit(3)")
    with pytest.raises(PermanentError, match="exited 3"):
        generate(request(staging))


def test_an_absent_generator_is_the_contract_message(staging, monkeypatch):
    """Mirrors the sdxl adapter's "sdxl backend not configured": the operator
    sees a word in dead letters with something they can act on."""
    monkeypatch.setenv("MORPHO_CODEX_BIN", "/nonexistent/codex")
    with pytest.raises(PermanentError, match="codex backend not configured"):
        generate(request(staging))


def test_a_word_with_no_sentence_is_never_drawn(fake_codex, staging):
    """Owner ruling, wave 9: this source draws the scene a sentence describes,
    and nothing else. The engine defers a word that has no slot-1 sentence, so a
    request without one is a caller bug — and answering it would produce a
    picture judged against a question it was never asked."""
    for value in (None, "", "   "):
        with pytest.raises(PermanentError, match="slot1_sentence"):
            generate(request(staging, slot1_sentence=value))
    assert not staging.exists()


def test_the_lemma_and_definition_are_disambiguation_not_subject(fake_codex, staging):
    """A word with a sentence and nothing else still generates: the headword and
    the meaning only say which sense the sentence is using."""
    result = generate(request(staging, primary_definition=None, pos=None))
    assert staging.is_file()
    text = result["prompt"]
    assert text.index("She had to abandon") < text.index("Word: abandon")
    assert "Meaning:" not in text


def test_an_unknown_prompt_version_is_refused(fake_codex, staging):
    """A candidate's source_ref records the version that drew it. Drawing under
    a different one would make that record a lie."""
    with pytest.raises(PermanentError, match="prompt"):
        generate(request(staging, prompt_ver="codex/99"))


@pytest.mark.parametrize(
    "overrides",
    [
        {"lemma": ""},
        {"lemma": 7},
        {"width": 8},
        {"height": 99_999},
        {"out_path": "relative/path.webp"},
        {"pos": 7},
        {"slot1_sentence": "x" * 5_000},
        {"slot1_sentence": 7},
    ],
)
def test_a_bad_request_is_permanent(fake_codex, staging, overrides):
    with pytest.raises(PermanentError):
        generate(request(staging, **overrides))


def test_an_odd_aspect_ratio_is_letterboxed_not_squashed(fake_codex, staging, tmp_path):
    """A hosted generator returns whatever shape it likes. A squashed photograph
    is a worse card than one with a border."""
    fake_codex(
        "import re, sys\n"
        "from pathlib import Path\n"
        f"sys.path.insert(0, {str(__file__).rsplit('/', 1)[0]!r})\n"
        "from conftest import write_image\n"
        "target = re.search(r'exact path:\\n(\\S+)', sys.stdin.read()).group(1)\n"
        "write_image(Path(target), size=(1024, 256))\n"
    )
    generate(request(staging))
    with Image.open(staging) as image:
        assert image.size == (768, 576)
        # A 4:1 source in a 4:3 box is width-limited, so the top and bottom are
        # the letterbox colour.
        assert image.convert("RGB").getpixel((384, 4)) == (0, 0, 0)


def test_the_encoding_is_deterministic(fake_codex, staging, tmp_path):
    """Media is content addressed, so two identical renders must produce two
    identical files — otherwise the library grows a copy per generation."""
    source = tmp_path / "source.png"
    write_image(source)
    from morpho_codex import image

    first = tmp_path / "a.webp"
    second = tmp_path / "b.webp"
    image.to_webp(source, first, 768, 576)
    image.to_webp(source, second, 768, 576)
    assert first.read_bytes() == second.read_bytes()
