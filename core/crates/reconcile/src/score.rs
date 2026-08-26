//! Candidate scoring (README Part 3 §"选择语义", rule 1).
//!
//! Scoring inputs are: readability against the live lexicon with a heavy
//! out-of-scope penalty, a length window, source priors, part-of-speech /
//! primary-sense match, and resolution for images. Everything is a pure
//! function of already-materialized inputs, so a `scorer_ver` bump is the only
//! thing that ever invalidates a score.
//!
//! Scores live in `[0, 1]`. The breakdown is stored verbatim in `score_detail`
//! so the console can explain a choice without re-deriving it.

use serde::Serialize;

use morpho_domain::types::{DefinitionSource, ExampleSource, ImageSource};
use morpho_domain::version::SCORER_ALGO_VER;

/// Hysteresis margin: an alternative must beat the incumbent by this much
/// before automatic selection switches (README rule 3). Without it, two
/// candidates a thousandth apart would trade the slot on every pass.
pub const HYSTERESIS_DELTA: f64 = 0.05;

/// Marker written into `image_candidates.source_ref` by the relaxed-licence
/// second pass over Openverse.
pub const RELAXED_LICENSE: &str = "relaxed-license";
/// Marker written into `image_candidates.source_ref` by the gloss-widened
/// second pass over a keyless provider.
pub const WIDENED_QUERY: &str = "widened-query";

/// How much a second-pass candidate gives up against a first-pass one.
///
/// It has to exceed [`HYSTERESIS_DELTA`], or a second-pass picture sitting in
/// the image slot would be immune to the strict hit that arrives later: the
/// selector only switches past the margin, so a penalty of a hundredth would
/// merely reorder the ranking without ever moving the slot.
///
/// It is subtracted rather than folded into the weighted sum on purpose. Every
/// candidate already in the library came from a strict pass, so a subtraction
/// that is zero for `Strict` leaves all of their scores bit-for-bit identical —
/// which is what makes this change safe without a `scorer_ver` bump and the
/// full rescore that comes with one.
pub const STRATEGY_PENALTY: f64 = 0.10;

/// The penalty is useless at or below the switching margin: see above.
const _: () = assert!(STRATEGY_PENALTY > HYSTERESIS_DELTA);

/// How much a picture gives up for being the one another word already shows.
///
/// Media is content-addressed, so two words that search for the same idea come
/// back with the same `file_hash` — and a question renders the word beside its
/// three fixed distractors, which makes two identical option images an
/// unanswerable card. This is the pressure that pulls the second-best picture
/// into the slot when the best one is spoken for.
///
/// It clears [`HYSTERESIS_DELTA`] for the same reason [`STRATEGY_PENALTY`] does,
/// and here the reason is sharper: two candidates of equal merit, one duplicated
/// and one not, must actually *move* the slot rather than merely reorder behind
/// it. A penalty inside the margin would rank the unique picture first and leave
/// the duplicate sitting in the selection forever.
pub const DUPLICATE_IMAGE_PENALTY: f64 = 0.10;

/// Same reasoning as the strategy penalty, enforced the same way.
const _: () = assert!(DUPLICATE_IMAGE_PENALTY > HYSTERESIS_DELTA);

/// Which search strategy produced an image candidate.
///
/// A word with no candidate after the first pass is searched again on looser
/// terms — Openverse without its licence filter, the keyless providers with a
/// query widened by the gloss's content words. Those hits are real pictures and
/// worth having, and they are still weaker evidence than a hit on the word
/// itself under a licence the bundle may ship: the licence is looser, or the
/// query drifted away from the word. So they are ranked below.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImageStrategy {
    /// The first pass: exact terms, publishable licence only.
    #[default]
    Strict,
    /// Openverse again, without `license_type` — every CC licence, NC and ND
    /// included. The licence the result actually carries is recorded verbatim.
    RelaxedLicense,
    /// A keyless provider again, asked for the word plus the content words of
    /// its primary gloss.
    WidenedQuery,
}

impl ImageStrategy {
    /// The note this strategy leaves in `source_ref`, or `None` for the first
    /// pass, which annotates nothing.
    pub const fn note(self) -> Option<&'static str> {
        match self {
            Self::Strict => None,
            Self::RelaxedLicense => Some(RELAXED_LICENSE),
            Self::WidenedQuery => Some(WIDENED_QUERY),
        }
    }

    /// Read the strategy back off a stored `source_ref`.
    ///
    /// Only the trailing parenthesised note is consulted, never the whole
    /// string: a Commons file may legitimately be *called*
    /// `File:Relaxed-license terms.jpg`, and that is a title, not a provenance
    /// claim.
    pub fn from_source_ref(source_ref: Option<&str>) -> Self {
        let Some(notes) = source_ref.and_then(trailing_note) else {
            return Self::Strict;
        };
        for note in notes.split(',').map(str::trim) {
            if note == RELAXED_LICENSE {
                return Self::RelaxedLicense;
            }
            if note == WIDENED_QUERY {
                return Self::WidenedQuery;
            }
        }
        Self::Strict
    }

    /// Marks deducted from the weighted total.
    pub const fn penalty(self) -> f64 {
        match self {
            Self::Strict => 0.0,
            Self::RelaxedLicense | Self::WidenedQuery => STRATEGY_PENALTY,
        }
    }
}

/// The contents of a trailing `(…)` group, if the string ends in one.
fn trailing_note(source_ref: &str) -> Option<&str> {
    let inner = source_ref.trim_end().strip_suffix(')')?;
    let open = inner.rfind('(')?;
    Some(&inner[open + 1..])
}

/// How a definition's tokens land against the live lexicon.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenCoverage {
    /// Tokens resolving to a base word.
    pub base: usize,
    /// Tokens resolving to a target or auxiliary word.
    pub in_scope: usize,
    /// Tokens resolving to nothing in the lexicon.
    pub out_of_scope: usize,
}

impl TokenCoverage {
    pub fn total(&self) -> usize {
        self.base + self.in_scope + self.out_of_scope
    }

    /// `1.0` when every token is known, falling fast as out-of-scope tokens
    /// appear. A definition the learner cannot read is worthless however
    /// elegant it is, so the penalty is three times the token's weight and the
    /// result is clamped at zero.
    pub fn readability(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        let penalty = 3.0 * self.out_of_scope as f64 / total as f64;
        (1.0 - penalty).clamp(0.0, 1.0)
    }
}

/// Triangular preference for a token count inside `[low, high]`.
fn length_window(count: usize, low: usize, high: usize) -> f64 {
    if count == 0 {
        return 0.0;
    }
    if count >= low && count <= high {
        return 1.0;
    }
    if count < low {
        return count as f64 / low as f64;
    }
    // Beyond the window, decay towards zero over another window's width.
    let overshoot = (count - high) as f64;
    (1.0 - overshoot / (high.max(1) as f64)).clamp(0.0, 1.0)
}

const fn definition_prior(source: DefinitionSource) -> f64 {
    // README: manual > llm_rewrite > freedict > wordnet.
    match source {
        DefinitionSource::Manual => 1.0,
        DefinitionSource::LlmRewrite => 0.85,
        DefinitionSource::Freedict => 0.70,
        DefinitionSource::Wordnet => 0.55,
    }
}

const fn example_prior(source: ExampleSource) -> f64 {
    // Ruling #18: manual > exam_corpus > freedict > tatoeba > llm.
    //
    // An exam sentence was written for exactly this purpose; a dictionary's own
    // usage line was written to illustrate the sense it sits under; a Tatoeba
    // sentence merely contains the word; a generated one merely looks like it
    // does. The ordering follows how much the sentence was chosen *for the
    // word*, which is what the mode-1 card needs.
    match source {
        ExampleSource::Manual => 1.0,
        ExampleSource::ExamCorpus => 0.85,
        ExampleSource::Freedict => 0.72,
        ExampleSource::Tatoeba => 0.64,
        ExampleSource::Llm => 0.55,
    }
}

const fn image_prior(source: ImageSource) -> f64 {
    // Ruling #18: manual > keyed stock > wikimedia/openverse > sdxl.
    //
    // The stock libraries are curated and shot to illustrate a concept; the
    // open collections are indexed, not curated, so a hit is more often merely
    // topical. Both beat a picture that depicts nothing that ever existed.
    match source {
        ImageSource::Manual => 1.0,
        ImageSource::Unsplash | ImageSource::Pexels | ImageSource::Pixabay => 0.8,
        ImageSource::Wikimedia | ImageSource::Openverse => 0.7,
        ImageSource::Sdxl => 0.5,
    }
}

/// A score plus the breakdown that justifies it.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub score: f64,
    pub detail: ScoreDetail,
}

impl Scored {
    pub fn detail_json(&self) -> String {
        serde_json::to_string(&self.detail).unwrap_or_else(|_| "{}".to_string())
    }

    pub const fn scorer_ver() -> &'static str {
        SCORER_ALGO_VER
    }
}

/// `score_detail` payload. Every field is a `[0, 1]` component except `total`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ScoreDetail {
    pub total: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_prior: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_reference: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_match: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_of_scope_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<usize>,
    /// Marks a second-pass image candidate gave up. Absent — not zero — on a
    /// first-pass candidate, so the breakdown of everything the strict passes
    /// produced is unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_penalty: Option<f64>,
}

/// What a definition candidate looks like to the scorer.
#[derive(Debug, Clone)]
pub struct DefinitionFacts {
    pub source: DefinitionSource,
    pub coverage: TokenCoverage,
    /// True when the definition uses the very word it defines.
    pub self_referential: bool,
}

/// Score one definition candidate.
///
/// A definition exists to be *read*, so readability dominates; the length
/// window and the source prior break ties between two readable options.
pub fn score_definition(facts: &DefinitionFacts) -> Scored {
    let readability = facts.coverage.readability();
    // 4–14 tokens: long enough to disambiguate, short enough to hold.
    let length = length_window(facts.coverage.total(), 4, 14);
    let prior = definition_prior(facts.source);
    let self_reference = if facts.self_referential { 0.0 } else { 1.0 };

    let total = 0.45 * readability + 0.18 * length + 0.25 * prior + 0.12 * self_reference;
    Scored {
        score: clamp(total),
        detail: ScoreDetail {
            total: clamp(total),
            readability: Some(readability),
            length: Some(length),
            source_prior: Some(prior),
            self_reference: Some(self_reference),
            out_of_scope_tokens: Some(facts.coverage.out_of_scope),
            token_count: Some(facts.coverage.total()),
            ..ScoreDetail::default()
        },
    }
}

/// What an example candidate looks like to the scorer.
#[derive(Debug, Clone)]
pub struct ExampleFacts {
    pub source: ExampleSource,
    pub coverage: TokenCoverage,
    /// The highlight range resolves to the target word inside the sentence.
    pub highlight_valid: bool,
}

/// Score one example candidate.
///
/// Sentences run longer than definitions, and a broken highlight makes the
/// mode-1 card unusable, so it is scored as a hard component rather than a
/// nudge.
pub fn score_example(facts: &ExampleFacts) -> Scored {
    let readability = facts.coverage.readability();
    let length = length_window(facts.coverage.total(), 6, 22);
    let prior = example_prior(facts.source);
    let highlight = if facts.highlight_valid { 1.0 } else { 0.0 };

    let total = 0.35 * readability + 0.20 * length + 0.20 * prior + 0.25 * highlight;
    Scored {
        score: clamp(total),
        detail: ScoreDetail {
            total: clamp(total),
            readability: Some(readability),
            length: Some(length),
            source_prior: Some(prior),
            highlight: Some(highlight),
            out_of_scope_tokens: Some(facts.coverage.out_of_scope),
            token_count: Some(facts.coverage.total()),
            ..ScoreDetail::default()
        },
    }
}

/// What an image candidate looks like to the scorer.
#[derive(Debug, Clone)]
pub struct ImageFacts {
    pub source: ImageSource,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// The candidate's `pos` hint matches the word's primary sense (or it
    /// carries no hint at all, which is neutral).
    pub pos_matches_primary: bool,
    /// Which pass found it. Read back from `source_ref`.
    pub strategy: ImageStrategy,
}

/// Target render size (README Part 5: WebP 768×576).
pub const TARGET_WIDTH: i64 = 768;
pub const TARGET_HEIGHT: i64 = 576;

/// Score one image candidate.
pub fn score_image(facts: &ImageFacts) -> Scored {
    let prior = image_prior(facts.source);
    let resolution = match (facts.width, facts.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => {
            // Full marks at or above the target box; a downscale is free, an
            // upscale is visible.
            let ratio = (w as f64 / TARGET_WIDTH as f64).min(h as f64 / TARGET_HEIGHT as f64);
            ratio.clamp(0.0, 1.0)
        }
        // Unknown dimensions are treated as adequate, not as a fault: the
        // encoder already fitted the bytes to the target box.
        _ => 0.8,
    };
    let pos_match = if facts.pos_matches_primary { 1.0 } else { 0.7 };
    let penalty = facts.strategy.penalty();

    let total = 0.45 * prior + 0.35 * resolution + 0.20 * pos_match - penalty;
    Scored {
        score: clamp(total),
        detail: ScoreDetail {
            total: clamp(total),
            source_prior: Some(prior),
            resolution: Some(resolution),
            pos_match: Some(pos_match),
            strategy_penalty: (penalty > 0.0).then_some(penalty),
            ..ScoreDetail::default()
        },
    }
}

fn clamp(value: f64) -> f64 {
    if value.is_nan() {
        return 0.0;
    }
    value.clamp(0.0, 1.0)
}

/// The score automatic selection ranks an image candidate by.
///
/// [`score_image`] is a pure function of the candidate and is cached in
/// `auto_score` under a `scorer_ver`; whether a picture is *also* somebody
/// else's is a property of the selection table, which changes every time a slot
/// moves. Folding it into the stored score would make every selection invalidate
/// scores across the whole lexicon and rescore a live database in circles. So it
/// is subtracted here, at ranking time, from a value nothing persists.
///
/// The result is deliberately not clamped into `[0, 1]`: it is a comparison key,
/// and flooring it at zero would let two weak candidates tie where the
/// preference is real.
pub fn image_selection_score(auto_score: f64, duplicate: bool) -> f64 {
    if duplicate {
        auto_score - DUPLICATE_IMAGE_PENALTY
    } else {
        auto_score
    }
}

/// Should automatic selection move a slot from `current` to `challenger`?
///
/// README rule 3: an `auto`, unpinned slot only switches when the challenger
/// beats the incumbent by more than the hysteresis margin. An empty slot takes
/// the best candidate outright (rule 2).
pub fn should_switch(current: Option<f64>, challenger: f64) -> bool {
    match current {
        None => true,
        Some(incumbent) => challenger > incumbent + HYSTERESIS_DELTA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(base: usize, in_scope: usize, oos: usize) -> TokenCoverage {
        TokenCoverage {
            base,
            in_scope,
            out_of_scope: oos,
        }
    }

    #[test]
    fn readability_is_perfect_when_every_token_is_known() {
        assert_eq!(coverage(4, 2, 0).readability(), 1.0);
    }

    #[test]
    fn out_of_scope_tokens_are_punished_hard() {
        let clean = coverage(9, 1, 0).readability();
        let one_bad = coverage(9, 0, 1).readability();
        assert!(one_bad < clean);
        // One bad token in ten already costs 30% of the readability score.
        assert!((one_bad - 0.7).abs() < 1e-9);
        // Four in ten wipes it out entirely rather than going negative.
        assert_eq!(coverage(6, 0, 4).readability(), 0.0);
    }

    #[test]
    fn empty_coverage_scores_zero_rather_than_dividing_by_zero() {
        assert_eq!(coverage(0, 0, 0).readability(), 0.0);
        assert!(coverage(0, 0, 0).total() == 0);
    }

    #[test]
    fn length_window_peaks_inside_the_band() {
        assert_eq!(length_window(8, 4, 14), 1.0);
        assert_eq!(length_window(4, 4, 14), 1.0);
        assert_eq!(length_window(14, 4, 14), 1.0);
        assert!(length_window(2, 4, 14) < 1.0);
        assert!(length_window(20, 4, 14) < 1.0);
        assert_eq!(length_window(0, 4, 14), 0.0);
        assert_eq!(length_window(100, 4, 14), 0.0);
    }

    #[test]
    fn definition_source_priors_follow_the_whitepaper() {
        let facts = |source| DefinitionFacts {
            source,
            coverage: coverage(6, 2, 0),
            self_referential: false,
        };
        let manual = score_definition(&facts(DefinitionSource::Manual)).score;
        let rewrite = score_definition(&facts(DefinitionSource::LlmRewrite)).score;
        let freedict = score_definition(&facts(DefinitionSource::Freedict)).score;
        let wordnet = score_definition(&facts(DefinitionSource::Wordnet)).score;
        assert!(manual > rewrite && rewrite > freedict && freedict > wordnet);
    }

    #[test]
    fn readability_outweighs_the_source_prior() {
        // A WordNet gloss everyone can read beats an unreadable manual one.
        let readable_wordnet = score_definition(&DefinitionFacts {
            source: DefinitionSource::Wordnet,
            coverage: coverage(8, 2, 0),
            self_referential: false,
        });
        let unreadable_manual = score_definition(&DefinitionFacts {
            source: DefinitionSource::Manual,
            coverage: coverage(5, 2, 3),
            self_referential: false,
        });
        assert!(readable_wordnet.score > unreadable_manual.score);
    }

    #[test]
    fn a_self_referential_definition_is_penalized() {
        let base = DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(6, 2, 0),
            self_referential: false,
        };
        let mut circular = base.clone();
        circular.self_referential = true;
        assert!(score_definition(&base).score > score_definition(&circular).score);
    }

    #[test]
    fn a_broken_highlight_sinks_an_example() {
        let good = score_example(&ExampleFacts {
            source: ExampleSource::ExamCorpus,
            coverage: coverage(10, 2, 0),
            highlight_valid: true,
        });
        let broken = score_example(&ExampleFacts {
            source: ExampleSource::ExamCorpus,
            coverage: coverage(10, 2, 0),
            highlight_valid: false,
        });
        assert!(good.score - broken.score >= 0.24);
    }

    /// Ruling #18's example ordering, end to end.
    #[test]
    fn example_source_priors_follow_the_ruling() {
        let facts = |source| ExampleFacts {
            source,
            coverage: coverage(10, 2, 0),
            highlight_valid: true,
        };
        let ranked: Vec<f64> = [
            ExampleSource::Manual,
            ExampleSource::ExamCorpus,
            ExampleSource::Freedict,
            ExampleSource::Tatoeba,
            ExampleSource::Llm,
        ]
        .into_iter()
        .map(|source| score_example(&facts(source)).score)
        .collect();
        assert!(
            ranked.windows(2).all(|pair| pair[0] > pair[1]),
            "manual > exam_corpus > freedict > tatoeba > llm, got {ranked:?}"
        );
    }

    /// Ruling #18's image ordering, end to end.
    #[test]
    fn image_source_priors_follow_the_ruling() {
        let facts = |source| ImageFacts {
            source,
            width: Some(1600),
            height: Some(1200),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        };
        let ranked: Vec<f64> = [
            ImageSource::Manual,
            ImageSource::Unsplash,
            ImageSource::Wikimedia,
            ImageSource::Sdxl,
        ]
        .into_iter()
        .map(|source| score_image(&facts(source)).score)
        .collect();
        assert!(
            ranked.windows(2).all(|pair| pair[0] > pair[1]),
            "manual > stock > keyless > sdxl, got {ranked:?}"
        );
        // The three stock libraries tie with each other, as do the two open
        // collections: the source is evidence about curation, not about which
        // brand it came from.
        for pair in [
            [ImageSource::Unsplash, ImageSource::Pexels],
            [ImageSource::Pexels, ImageSource::Pixabay],
            [ImageSource::Wikimedia, ImageSource::Openverse],
        ] {
            assert_eq!(
                score_image(&facts(pair[0])).score,
                score_image(&facts(pair[1])).score,
                "{} and {} should tie",
                pair[0],
                pair[1]
            );
        }
    }

    /// A keyless picture that is actually there beats a generated one, and a
    /// readable sentence beats a prior — the prior is a tiebreak, not a veto.
    #[test]
    fn a_real_photo_outranks_a_generated_one_even_at_lower_resolution() {
        let commons = score_image(&ImageFacts {
            source: ImageSource::Wikimedia,
            width: Some(960),
            height: Some(720),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        });
        let generated = score_image(&ImageFacts {
            source: ImageSource::Sdxl,
            width: Some(768),
            height: Some(576),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        });
        assert!(commons.score > generated.score);
    }

    #[test]
    fn resolution_below_the_target_box_costs_marks() {
        let big = score_image(&ImageFacts {
            source: ImageSource::Pexels,
            width: Some(1600),
            height: Some(1200),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        });
        let small = score_image(&ImageFacts {
            source: ImageSource::Pexels,
            width: Some(320),
            height: Some(240),
            pos_matches_primary: true,
            strategy: ImageStrategy::Strict,
        });
        assert!(big.score > small.score);
        assert_eq!(big.detail.resolution, Some(1.0));
    }

    #[test]
    fn every_score_stays_inside_the_unit_interval() {
        for oos in 0..8 {
            let scored = score_definition(&DefinitionFacts {
                source: DefinitionSource::Manual,
                coverage: coverage(8, 0, oos),
                self_referential: oos % 2 == 0,
            });
            assert!((0.0..=1.0).contains(&scored.score), "{scored:?}");
        }
        let weird = score_image(&ImageFacts {
            source: ImageSource::Sdxl,
            width: Some(-5),
            height: Some(0),
            pos_matches_primary: false,
            strategy: ImageStrategy::Strict,
        });
        assert!((0.0..=1.0).contains(&weird.score));
    }

    #[test]
    fn detail_json_round_trips_and_carries_the_total() {
        let scored = score_definition(&DefinitionFacts {
            source: DefinitionSource::Freedict,
            coverage: coverage(6, 2, 1),
            self_referential: false,
        });
        let parsed: serde_json::Value = serde_json::from_str(&scored.detail_json()).unwrap();
        assert!((parsed["total"].as_f64().unwrap() - scored.score).abs() < 1e-9);
        assert_eq!(parsed["out_of_scope_tokens"], 1);
        assert!(
            parsed.get("resolution").is_none(),
            "unused fields are omitted"
        );
    }

    // -- second-pass strategies --------------------------------------------

    fn keyless(strategy: ImageStrategy) -> ImageFacts {
        ImageFacts {
            source: ImageSource::Openverse,
            width: Some(1600),
            height: Some(1200),
            pos_matches_primary: true,
            strategy,
        }
    }

    #[test]
    fn a_second_pass_candidate_ranks_below_the_strict_pass() {
        let strict = score_image(&keyless(ImageStrategy::Strict)).score;
        for strategy in [ImageStrategy::RelaxedLicense, ImageStrategy::WidenedQuery] {
            let relaxed = score_image(&keyless(strategy)).score;
            assert!(
                relaxed < strict,
                "{strategy:?} scored {relaxed} vs {strict}"
            );
        }
    }

    /// The point of the penalty: a strict hit arriving later must be able to
    /// take the slot off a second-pass incumbent, which means clearing the
    /// hysteresis margin rather than merely outranking it.
    #[test]
    fn a_later_strict_hit_displaces_a_second_pass_incumbent() {
        let incumbent = score_image(&keyless(ImageStrategy::WidenedQuery)).score;
        let challenger = score_image(&keyless(ImageStrategy::Strict)).score;
        assert!(should_switch(Some(incumbent), challenger));
    }

    /// Every candidate in the library predates the second passes, so a strict
    /// score has to come out exactly as it did before — no rescore, no
    /// `scorer_ver` bump, no churn on a live database.
    #[test]
    fn a_strict_candidate_scores_exactly_what_it_always_did() {
        let facts = keyless(ImageStrategy::Strict);
        // 0.45 * 0.7 + 0.35 * 1.0 + 0.20 * 1.0
        let scored = score_image(&facts);
        assert!((scored.score - 0.865).abs() < 1e-9, "{scored:?}");
        assert_eq!(scored.detail.strategy_penalty, None);
        let json: serde_json::Value = serde_json::from_str(&scored.detail_json()).unwrap();
        assert!(json.get("strategy_penalty").is_none());
    }

    #[test]
    fn a_second_pass_candidate_says_so_in_its_breakdown() {
        let scored = score_image(&keyless(ImageStrategy::RelaxedLicense));
        assert_eq!(scored.detail.strategy_penalty, Some(STRATEGY_PENALTY));
        let json: serde_json::Value = serde_json::from_str(&scored.detail_json()).unwrap();
        assert!((json["strategy_penalty"].as_f64().unwrap() - STRATEGY_PENALTY).abs() < 1e-9);
        assert!((json["total"].as_f64().unwrap() - scored.score).abs() < 1e-9);
    }

    #[test]
    fn the_strategy_is_read_back_off_the_source_ref() {
        assert_eq!(
            ImageStrategy::from_source_ref(Some("openverse:abc (relaxed-license)")),
            ImageStrategy::RelaxedLicense
        );
        assert_eq!(
            ImageStrategy::from_source_ref(Some("wikimedia:File:Lake.jpg (widened-query)")),
            ImageStrategy::WidenedQuery
        );
        // The first pass annotates nothing, and the article-lead note is a
        // first-pass strategy that must not be penalised.
        assert_eq!(
            ImageStrategy::from_source_ref(Some("wikimedia:File:Lake.jpg")),
            ImageStrategy::Strict
        );
        assert_eq!(
            ImageStrategy::from_source_ref(Some("wikimedia:File:Lake.jpg (article-lead)")),
            ImageStrategy::Strict
        );
        assert_eq!(ImageStrategy::from_source_ref(None), ImageStrategy::Strict);
        // Composed notes still resolve.
        assert_eq!(
            ImageStrategy::from_source_ref(Some(
                "wikimedia:File:A.jpg (article-lead, widened-query)"
            )),
            ImageStrategy::WidenedQuery
        );
    }

    /// A file whose *title* contains a strategy word is a title, not a claim.
    #[test]
    fn only_the_trailing_note_is_read_as_provenance() {
        assert_eq!(
            ImageStrategy::from_source_ref(Some("wikimedia:File:Relaxed-license terms.jpg")),
            ImageStrategy::Strict
        );
        assert_eq!(
            ImageStrategy::from_source_ref(Some("wikimedia:File:Widened-query (diagram).png")),
            ImageStrategy::Strict
        );
    }

    // -- global image uniqueness -------------------------------------------

    #[test]
    fn a_picture_another_word_already_shows_ranks_below_a_fresh_one() {
        let taken = image_selection_score(0.865, true);
        let free = image_selection_score(0.865, false);
        assert!(taken < free);
        assert!((free - taken - DUPLICATE_IMAGE_PENALTY).abs() < 1e-9);
    }

    /// The ordering the whole gate rests on: penalty > margin, so a candidate of
    /// equal merit whose hash is free actually takes the slot instead of merely
    /// ranking above the duplicate that sits in it.
    #[test]
    fn the_duplicate_penalty_clears_the_switching_margin() {
        const { assert!(DUPLICATE_IMAGE_PENALTY > HYSTERESIS_DELTA) };
        let incumbent = image_selection_score(0.865, true);
        let challenger = image_selection_score(0.865, false);
        assert!(should_switch(Some(incumbent), challenger));
        // And one hundredth of a mark would not have: the compile-time
        // assertion above is what keeps that from being tuned into the code.
        assert!(!should_switch(Some(0.865 - 0.01), 0.865));
    }

    /// Nothing moves when no candidate is spoken for — the pool scores exactly
    /// as it did before the penalty existed, bit for bit.
    #[test]
    fn a_pool_with_no_shared_hash_is_scored_exactly_as_before() {
        for raw in [0.0, 0.3, 0.865, 1.0] {
            assert_eq!(image_selection_score(raw, false), raw);
        }
        let incumbent = image_selection_score(0.90, false);
        let challenger = image_selection_score(0.88, false);
        assert!(!should_switch(Some(incumbent), challenger));
    }

    /// Two duplicates are still ranked against each other on merit, which is why
    /// the effective score is a comparison key rather than a clamped `[0, 1]`
    /// score.
    #[test]
    fn two_duplicates_keep_their_relative_order_even_at_the_bottom() {
        let better = image_selection_score(0.06, true);
        let worse = image_selection_score(0.03, true);
        assert!(better > worse);
        assert!(worse < 0.0, "a comparison key may go negative");
    }

    #[test]
    fn a_duplicate_still_loses_to_a_far_better_picture_of_its_own_kind() {
        // Merit is not overruled: a strict stock hit that happens to be shared
        // still beats a widened keyless one that is not.
        let stock = image_selection_score(
            score_image(&ImageFacts {
                source: ImageSource::Unsplash,
                width: Some(1600),
                height: Some(1200),
                pos_matches_primary: true,
                strategy: ImageStrategy::Strict,
            })
            .score,
            true,
        );
        let widened = image_selection_score(
            score_image(&keyless(ImageStrategy::WidenedQuery)).score,
            false,
        );
        assert!(stock > widened);
    }

    #[test]
    fn hysteresis_keeps_a_near_tie_from_oscillating() {
        assert!(should_switch(None, 0.1), "an empty slot takes anything");
        assert!(!should_switch(Some(0.80), 0.82));
        assert!(
            !should_switch(Some(0.80), 0.85),
            "exactly delta is not enough"
        );
        assert!(should_switch(Some(0.80), 0.86));
        assert!(!should_switch(Some(0.80), 0.50));
    }
}
