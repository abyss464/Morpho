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
    // README: exam_corpus > llm.
    match source {
        ExampleSource::Manual => 1.0,
        ExampleSource::ExamCorpus => 0.85,
        ExampleSource::Llm => 0.6,
    }
}

const fn image_prior(source: ImageSource) -> f64 {
    // README: manual > stock library > sdxl.
    match source {
        ImageSource::Manual => 1.0,
        ImageSource::Unsplash | ImageSource::Pexels | ImageSource::Pixabay => 0.8,
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

    let total = 0.45 * prior + 0.35 * resolution + 0.20 * pos_match;
    Scored {
        score: clamp(total),
        detail: ScoreDetail {
            total: clamp(total),
            source_prior: Some(prior),
            resolution: Some(resolution),
            pos_match: Some(pos_match),
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

    #[test]
    fn exam_corpus_outranks_generated_examples() {
        let facts = |source| ExampleFacts {
            source,
            coverage: coverage(10, 2, 0),
            highlight_valid: true,
        };
        assert!(
            score_example(&facts(ExampleSource::ExamCorpus)).score
                > score_example(&facts(ExampleSource::Llm)).score
        );
    }

    #[test]
    fn stock_photos_outrank_generated_ones() {
        let facts = |source| ImageFacts {
            source,
            width: Some(1600),
            height: Some(1200),
            pos_matches_primary: true,
        };
        assert!(
            score_image(&facts(ImageSource::Unsplash)).score
                > score_image(&facts(ImageSource::Sdxl)).score
        );
        assert!(
            score_image(&facts(ImageSource::Manual)).score
                > score_image(&facts(ImageSource::Unsplash)).score
        );
    }

    #[test]
    fn resolution_below_the_target_box_costs_marks() {
        let big = score_image(&ImageFacts {
            source: ImageSource::Pexels,
            width: Some(1600),
            height: Some(1200),
            pos_matches_primary: true,
        });
        let small = score_image(&ImageFacts {
            source: ImageSource::Pexels,
            width: Some(320),
            height: Some(240),
            pos_matches_primary: true,
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
