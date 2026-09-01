---
file: core/crates/reconcile/src/morphy.rs
---

# Morphy lemmatizer

WordNet-style morphological analyzer validated against the Morpho lexicon. Strips inflectional suffixes and checks that the result is a known word; unknown detachments are discarded so out-of-scope tokens stay honestly unresolved. No part-of-speech input -- all rules are tried, longest suffix first, and lexicon validation prevents nonsense.

Resolution order: (1) surface itself if it is a lemma, (2) exception table, (3) detachment rules longest-suffix-first, (4) consonant-doubling reversal for -ing/-ed, (5) surface unchanged.

## MorphyLemmatizer::new(lexicon, exceptions?) -> MorphyLemmatizer

Builds the lemmatizer with compiled-in irregular forms, optionally extended with WNdb exception files from a directory.

## MorphyLemmatizer::lemmatize(surface) -> &str

Returns the best lemma for a surface form.

## Constraints
- Protected surfaces (number, her, etc.) are never detached even when the result is a valid word.
- Version strings (`MORPHY_LEMMATIZER_VER` / `MORPHY_LEMMATIZER_WNDB_VER`) are written to `def_extractions.lemmatizer_ver`; changing rules requires bumping the version.
