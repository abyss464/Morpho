"""What the generator is asked for, and the version that says which asking.

The wording is `ops/genimg_cron.sh`'s, carried over rather than rewritten: those
are the instructions the 129 images in release 1.5 were made under, every one of
which beat the incumbent it replaced on CLIP score. Rewording it would throw away
the only evidence anybody has about what works.

Two things changed in the move. The batch script asked for a whole list at once
and let the model name its own files; an op asks for one picture at one path,
because morphod owns the staging directory and hands over exactly one output
path per job (adapter-protocol.md ruling #1). And the version tag is explicit —
it rides in the candidate's `source_ref`, so a picture always says which
instructions drew it.
"""

from __future__ import annotations

from typing import Final

#: The template this module implements. `codex.generate` refuses a request that
#: asks for a version it does not have, rather than quietly drawing under a
#: different one — a `source_ref` that names the wrong template is worse than a
#: failed job, because nothing downstream can tell.
CURRENT_VERSION: Final[str] = "codex/1"

_INSTRUCTIONS: Final[str] = (
    "Generate one vocabulary-learning illustration image using your image "
    "generation capability, and save it to this exact path:\n"
    # On a line of its own, with nothing after it. A trailing full stop would be
    # part of the path as far as anything reading this is concerned.
    "{out_path}\n"
    "\n"
    "{subject}\n"
    "\n"
    "Requirements: generate a photorealistic scene image, NOT text, NOT words, "
    "NOT letters, NOT a white background. The image must depict a VISUAL SCENE "
    "described by the example sentence above, showing the MEANING of the word "
    "as used there — a learner must be able to pick it out of 4 images as the "
    "one matching the sentence. Photographic or "
    "realistic illustration, clear single subject, landscape {width}x{height} "
    "(4:3). NEVER generate an image that contains any written text, typography, "
    "or letters — no captions, no labels, no signage, no numbers, nothing "
    "legible, anywhere in the frame. For abstract words use a concrete "
    "instantly-readable metaphor, still rendered as a real photographic scene, "
    "never as a text/diagram/icon.\n"
    "\n"
    "Write the file and nothing else. If the generation fails, retry once; on a "
    "second failure stop without writing anything."
)


def build(
    *,
    lemma: str,
    slot1_sentence: str,
    out_path: str,
    width: int,
    height: int,
    pos: str | None = None,
    primary_definition: str | None = None,
) -> str:
    """The prompt for one word, under [`CURRENT_VERSION`].

    `slot1_sentence` is required: the picture is of the scene that sentence
    describes, and the lemma and definition only say which sense of the word it
    is using. Nothing here draws from a headword alone.
    """
    return _INSTRUCTIONS.format(
        out_path=out_path,
        width=width,
        height=height,
        subject=_subject(lemma, pos, primary_definition, slot1_sentence),
    )


def _subject(
    lemma: str,
    pos: str | None,
    primary_definition: str | None,
    slot1_sentence: str,
) -> str:
    """What the picture is of.

    The sentence leads. That ordering is the whole reason this source exists
    rather than another bare-concept generator: the card shows the learner a
    sentence and four pictures, so the picture has to be of the moment that
    sentence describes, not of the dictionary entry — and the CLIP score that
    decides whether the result wins its slot queries with that same sentence.

    The headword and the meaning follow as disambiguation, for a lemma whose
    sentence could be read several ways. They are never the subject.
    """
    headword = f"{lemma} ({pos})" if pos else lemma
    lines = [f'Sentence: "{slot1_sentence}"', f"Word: {headword}"]
    if primary_definition:
        lines.append(f"Meaning: {primary_definition}")
    return "\n".join(lines)
