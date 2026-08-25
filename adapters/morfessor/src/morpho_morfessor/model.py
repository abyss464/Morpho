"""Model loading, the wave-1 ad-hoc training stopgap, and batch segmentation.

Real Morfessor quality comes from a model trained on a large corpus. That
training job belongs to a later wave; until it exists, this adapter trains a
throwaway Baseline model on the batch it was handed. Segmentations from such a
model are usable-but-crude — treat them as the `morfessor` etymology fallback
the README describes, never as ground truth.

Three properties keep the stopgap honest:

- **Determinism.** `random.seed` is fixed and the training set is sorted and
  deduplicated, so the same set of words always yields the same segmentation
  regardless of the order morphod happened to batch them in.
- **Honest versioning.** The reported `model_ver` embeds a digest of the
  training words and hyperparameters. morphod mixes it into the derived row's
  input hash, so two batches trained on different vocabularies can never be
  mistaken for the same model — and dropping in a real pretrained model
  invalidates every ad-hoc result.
- **A refusal to guess.** A model trained on a handful of words has no
  morphological signal; asking for one anyway shatters words into single
  letters. Below a vocabulary floor, and whenever the resulting morphs come out
  implausibly short, the adapter returns words unsegmented instead of writing
  nonsense into the etymology column. Both guards apply to ad-hoc models only —
  a pretrained model's output is passed through as-is.
"""

from __future__ import annotations

import hashlib
import logging
import os
import random
import time
from dataclasses import dataclass
from importlib import metadata
from pathlib import Path
from typing import Any, Final

import morfessor
import morfessor.utils
from morpho_adapter_common import PermanentError, TransientError

log = logging.getLogger("morpho.adapter.morfessor.model")

# Morfessor's progress bar `print`s dots straight to stdout, which is reserved
# for the response JSON. The envelope would fold them into stderr anyway; the
# cleaner fix is to never emit them. Its own logger is chatty at INFO, so it is
# raised to WARNING unless the operator explicitly asked for DEBUG.
morfessor.utils.show_progress_bar = False
logging.getLogger("morfessor").setLevel(
    logging.DEBUG
    if os.environ.get("MORPHO_ADAPTER_LOG_LEVEL", "").upper() == "DEBUG"
    else logging.WARNING
)

MODEL_PATH_ENV: Final[str] = "MORPHO_MORFESSOR_MODEL"
MAX_EPOCHS_ENV: Final[str] = "MORPHO_MORFESSOR_MAX_EPOCHS"
MORPH_LENGTH_ENV: Final[str] = "MORPHO_MORFESSOR_MORPH_LENGTH"
MIN_VOCAB_ENV: Final[str] = "MORPHO_MORFESSOR_MIN_VOCAB"

#: `adapters/morfessor/model/` — package root is three levels up from this file.
MODEL_DIR: Final[Path] = Path(__file__).resolve().parents[2] / "model"
#: Optional one-line file naming the corpus vintage, e.g. `2026-08`.
VERSION_SIDECAR: Final[str] = "VERSION"
MODEL_GLOBS: Final[tuple[str, ...]] = ("*.bin", "*.model", "*.gz")

#: Fixed so `random.shuffle` inside `train_batch` replays identically.
TRAINING_SEED: Final[int] = 20260826
#: Bounds ad-hoc training well inside morphod's 120 s/batch budget. Measured at
#: roughly 3 s for a 1 900-word batch on one laptop core.
DEFAULT_MAX_EPOCHS: Final[int] = 8
#: Morfessor weights the corpus coding cost by alpha. With type-based training —
#: every word counted once, which is all a word batch gives us — the fixed 1.0
#: default makes "keep the whole word" the cheapest description and nothing ever
#: splits. A hand-picked lower alpha only works at one batch size: over a
#: synthetic affixed vocabulary, alpha=0.5 split cleanly at 2 900 words and not
#: at all at 600. `MorphLengthCorpusWeight` instead retunes alpha each epoch
#: until the average morph length hits a target, which held across batches from
#: 60 to 1 900 words. 5 characters is about the length of an English root.
DEFAULT_MORPH_LENGTH: Final[float] = 5.0
#: Fewer distinct words than this and the auto-tuner has nothing to learn from;
#: it drives alpha down until words shatter into letters.
DEFAULT_MIN_VOCAB: Final[int] = 100
#: Mean characters per morph across the batch, below which the ad-hoc model is
#: declared degenerate and its output discarded.
MIN_MEAN_MORPH_CHARS: Final[float] = 2.5

PRETRAINED_TAG_PREFIX: Final[str] = "model"
ADHOC_TAG_PREFIX: Final[str] = "adhoc"
#: Identity segmentation. Deliberately carries no batch digest: the output does
#: not depend on the batch, so neither should the version morphod hashes.
NO_MODEL_VER_SUFFIX: Final[str] = "nomodel"

SOURCE_PRETRAINED: Final[str] = "pretrained"
SOURCE_ADHOC: Final[str] = "adhoc"
SOURCE_UNSEGMENTED: Final[str] = "unsegmented"


@dataclass(frozen=True)
class LoadedModel:
    model: Any
    model_ver: str


@dataclass(frozen=True)
class Segmentation:
    segments: dict[str, list[str]]
    model_ver: str
    source: str


def library_version() -> str:
    try:
        return metadata.version("morfessor")
    except metadata.PackageNotFoundError:  # pragma: no cover - dependency is declared
        return "unknown"


def model_dir() -> Path:
    return MODEL_DIR


def find_model_file() -> Path | None:
    """Explicit env override wins; otherwise pick the lowest-named model file."""
    override = os.environ.get(MODEL_PATH_ENV)
    if override:
        path = Path(override).expanduser()
        if not path.is_file():
            raise PermanentError(f"{MODEL_PATH_ENV} points at a missing file: {path}")
        return path
    directory = model_dir()
    if not directory.is_dir():
        return None
    candidates = sorted(
        {match for pattern in MODEL_GLOBS for match in directory.glob(pattern) if match.is_file()}
    )
    return candidates[0] if candidates else None


def segment_batch(words: list[str]) -> Segmentation:
    """Segment a deduplicated, order-preserving batch of words."""
    path = find_model_file()
    if path is not None:
        loaded = _load_pretrained(path)
        return Segmentation(
            segments=_apply(loaded.model, words),
            model_ver=loaded.model_ver,
            source=SOURCE_PRETRAINED,
        )

    training = sorted(set(words))
    floor = _min_vocab()
    if len(training) < floor:
        log.warning(
            "no model under %s and only %d distinct words (floor %d); returning words "
            "unsegmented rather than inventing morphology",
            model_dir(),
            len(training),
            floor,
        )
        return _unsegmented(words)

    log.warning(
        "no model under %s; training an ad-hoc model on this batch of %d distinct words "
        "(dev stopgap — segmentations are approximate)",
        model_dir(),
        len(training),
    )
    loaded = _train_adhoc(training)
    segments = _apply(loaded.model, words)
    mean_chars = mean_morph_chars(segments)
    if mean_chars < MIN_MEAN_MORPH_CHARS:
        log.warning(
            "ad-hoc model degenerated to %.2f characters per morph (floor %.2f); discarding "
            "its output and returning words unsegmented",
            mean_chars,
            MIN_MEAN_MORPH_CHARS,
        )
        return _unsegmented(words)
    log.info("ad-hoc segmentation averaged %.2f characters per morph", mean_chars)
    return Segmentation(segments=segments, model_ver=loaded.model_ver, source=SOURCE_ADHOC)


def mean_morph_chars(segments: dict[str, list[str]]) -> float:
    parts = sum(len(value) for value in segments.values())
    if parts == 0:
        return 0.0
    characters = sum(len(part) for value in segments.values() for part in value)
    return characters / parts


def segment_word(model: Any, word: str) -> list[str]:
    """Trained analysis when the word is in the lexicon, Viterbi otherwise.

    `BaselineModel.segment` returns the analysis training actually settled on;
    `viterbi_segment` re-derives one with additive smoothing and is the only
    option for words a pretrained model has never seen. Preferring the stored
    analysis keeps in-vocabulary output consistent with what the model learned.
    """
    try:
        constructions = model.segment(word)
    except KeyError:
        try:
            constructions, _logprob = model.viterbi_segment(word)
        except Exception as exc:
            raise TransientError(f"segmentation of {word!r} failed: {exc}") from exc
    except Exception as exc:
        raise TransientError(f"segmentation of {word!r} failed: {exc}") from exc

    parts = [str(part) for part in constructions if str(part)]
    joined = "".join(parts)
    if joined != word:
        # Morfessor partitions the compound, so this cannot happen with a sane
        # model — but a corrupt pretrained model would silently corrupt the
        # etymology column, and that is worth a loud failure.
        raise PermanentError(
            f"segmentation of {word!r} does not reassemble to the input (got {joined!r})"
        )
    return parts


def _apply(model: Any, words: list[str]) -> dict[str, list[str]]:
    started = time.monotonic()
    segments = {word: segment_word(model, word) for word in words}
    log.debug("segmented %d words in %d ms", len(words), int((time.monotonic() - started) * 1000))
    return segments


def _unsegmented(words: list[str]) -> Segmentation:
    return Segmentation(
        segments={word: [word] for word in words},
        model_ver=f"morfessor/{library_version()}+{NO_MODEL_VER_SUFFIX}",
        source=SOURCE_UNSEGMENTED,
    )


def _load_pretrained(path: Path) -> LoadedModel:
    started = time.monotonic()
    io = morfessor.MorfessorIO()
    try:
        model = io.read_any_model(str(path))
    except Exception as exc:
        raise PermanentError(f"cannot load Morfessor model {path}: {exc}") from exc
    tag = _pretrained_tag(path)
    log.info("loaded model %s in %d ms", path, int((time.monotonic() - started) * 1000))
    return LoadedModel(
        model=model,
        model_ver=f"morfessor/{library_version()}+{PRETRAINED_TAG_PREFIX}-{tag}",
    )


def _pretrained_tag(path: Path) -> str:
    sidecar = path.parent / VERSION_SIDECAR
    if sidecar.is_file():
        label = sidecar.read_text(encoding="utf-8").strip().splitlines()
        if label and label[0].strip():
            return label[0].strip()
    return _digest(path.read_bytes())


def _train_adhoc(training: list[str]) -> LoadedModel:
    if not training:
        raise PermanentError("cannot train an ad-hoc model from an empty batch")
    morph_length = _morph_length()
    max_epochs = _max_epochs()
    started = time.monotonic()
    random.seed(TRAINING_SEED)
    model = morfessor.BaselineModel(
        use_skips=False,
        corpusweight=morfessor.MorphLengthCorpusWeight(morph_length),
    )
    # Morfessor's `load_data` wants (count, compound) pairs; a compound is just
    # the string, which it treats as a sequence of single-character atoms.
    data = [(1, word) for word in training]
    try:
        model.load_data(data, freqthreshold=1, count_modifier=None, init_rand_split=None)
        epochs, cost = model.train_batch(algorithm="recursive", max_epochs=max_epochs)
    except Exception as exc:
        raise TransientError(f"ad-hoc Morfessor training failed: {exc}") from exc
    log.info(
        "trained ad-hoc model on %d words in %d ms (%s epochs, cost %.3f)",
        len(training),
        int((time.monotonic() - started) * 1000),
        epochs,
        cost,
    )
    return LoadedModel(
        model=model,
        model_ver=f"morfessor/{library_version()}+{ADHOC_TAG_PREFIX}-"
        f"{adhoc_tag(training, morph_length, max_epochs)}",
    )


def adhoc_tag(training: list[str], morph_length: float, max_epochs: int) -> str:
    """Digest of everything that shapes an ad-hoc model.

    The training words *and* the hyperparameters go in, so morphod's derived
    rows invalidate correctly when either changes — and two batches trained on
    different vocabularies never share a `model_ver`.
    """
    payload = "\n".join(
        [
            f"seed={TRAINING_SEED}",
            f"morph_length={morph_length!r}",
            f"max_epochs={max_epochs}",
            f"min_mean_morph_chars={MIN_MEAN_MORPH_CHARS!r}",
            f"lib={library_version()}",
            *training,
        ]
    )
    return _digest(payload.encode("utf-8"))


def _max_epochs() -> int:
    return _int_env(MAX_EPOCHS_ENV, DEFAULT_MAX_EPOCHS)


def _min_vocab() -> int:
    return _int_env(MIN_VOCAB_ENV, DEFAULT_MIN_VOCAB, allow_zero=True)


def _int_env(name: str, default: int, *, allow_zero: bool = False) -> int:
    raw = os.environ.get(name)
    if not raw:
        return default
    try:
        value = int(raw)
    except ValueError:
        log.warning("ignoring non-integer %s=%r", name, raw)
        return default
    if value > 0 or (allow_zero and value == 0):
        return value
    return default


def _morph_length() -> float:
    raw = os.environ.get(MORPH_LENGTH_ENV)
    if not raw:
        return DEFAULT_MORPH_LENGTH
    try:
        value = float(raw)
    except ValueError:
        log.warning("ignoring non-numeric %s=%r", MORPH_LENGTH_ENV, raw)
        return DEFAULT_MORPH_LENGTH
    return value if value > 0 else DEFAULT_MORPH_LENGTH


def _digest(data: bytes, length: int = 12) -> str:
    return hashlib.sha256(data).hexdigest()[:length]
