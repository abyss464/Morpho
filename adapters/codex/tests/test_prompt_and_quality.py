"""The prompt's shape, and the gate that catches a picture that is not one."""

from __future__ import annotations

import pytest

from conftest import write_image
from morpho_codex import prompt, quality


def build(**overrides):
    params = {
        "lemma": "abandon",
        "out_path": "/tmp/staging/render.png",
        "width": 768,
        "height": 576,
        "pos": "verb",
        "primary_definition": "to give up completely",
        "slot1_sentence": "She had to abandon the car in the flood.",
    }
    params.update(overrides)
    return prompt.build(**params)


def test_the_sentence_leads():
    """The card shows a sentence and four pictures, so the picture has to be of
    the moment the sentence describes — and a generator weights the front of a
    prompt hardest. It is also the query CLIP will score the result against, so
    conditioning on anything else handicaps the answer against its own test."""
    text = build()
    assert text.index("She had to abandon") < text.index("Word: abandon")
    assert text.index("Word: abandon") < text.index("Meaning: to give up")


def test_the_instructions_carry_the_wave_one_wording():
    """These are the instructions the 129 images in release 1.5 were made under,
    every one of which beat the incumbent it replaced. The wording is evidence,
    not prose."""
    text = build()
    for phrase in [
        "photorealistic scene image",
        "NOT text",
        "NOT a white background",
        "nothing legible",
        "concrete instantly-readable metaphor",
        "described by the example sentence above",
    ]:
        assert phrase in text, phrase


def test_the_target_size_and_path_are_stated():
    text = build()
    assert "/tmp/staging/render.png" in text
    assert "768x576" in text


def test_absent_disambiguation_is_left_out_rather_than_left_empty():
    """An empty `Meaning:` line is a hole the generator will try to fill. The
    sentence is never absent — the engine defers a word that has none."""
    text = build(pos=None, primary_definition=None)
    assert "Sentence:" in text
    assert "Word: abandon" in text
    assert "Meaning:" not in text
    assert "()" not in text


def test_the_version_is_a_single_named_template():
    assert prompt.CURRENT_VERSION == "codex/1"


# -- the blank gate ---------------------------------------------------------


def test_a_real_picture_passes(tmp_path):
    path = tmp_path / "photo.png"
    write_image(path)
    assert not quality.is_blank(path)


def test_a_flat_canvas_is_blank(tmp_path):
    path = tmp_path / "blank.png"
    write_image(path, noisy=False)
    assert quality.is_blank(path)


def test_an_unreadable_file_counts_as_blank(tmp_path):
    """Either way there is nothing here worth showing a learner, and the
    caller's answer to both is the same."""
    path = tmp_path / "garbage.png"
    path.write_bytes(b"this is not an image")
    assert quality.is_blank(path)
    assert quality.is_blank(tmp_path / "not-there-at-all.png")


def test_the_threshold_is_the_ported_one(monkeypatch):
    assert quality.DEFAULT_BLANK_STDDEV == 0.5
    assert quality.blank_threshold() == 0.5
    monkeypatch.setenv("MORPHO_CODEX_BLANK_STDDEV", "2.5")
    assert quality.blank_threshold() == 2.5
    monkeypatch.setenv("MORPHO_CODEX_BLANK_STDDEV", "not a number")
    assert quality.blank_threshold() == 0.5


@pytest.mark.parametrize("shade", [(0, 0, 0), (128, 128, 128), (255, 255, 255)])
def test_every_flat_shade_is_blank(tmp_path, shade):
    from PIL import Image

    path = tmp_path / "flat.png"
    Image.new("RGB", (64, 64), shade).save(path)
    assert quality.is_blank(path)
