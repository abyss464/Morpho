//! Readiness evaluation (README Part 3 §"任务、事件、就绪度").
//!
//! ```text
//! core_ready(W) = primary sense selected and approved
//!               ∧ every enabled sense approved
//!               ∧ no unresolved out-of-scope token in a selected definition
//!               ∧ every dependency covered by base words or an earlier plan slot
//!               ∧ example slot 1 selected and approved ∧ live image approved
//!               ∧ TTS ready for the lemma, every selected sense, every selected example
//!               ∧ W has a position in the current plan
//!
//! ready(W)      = core_ready(W) ∧ three distractors bound ∧ each one core_ready
//! ```
//!
//! Splitting the two caps the recursion at depth 1, which is what stops mutual
//! distractors (adapt ↔ adopt) from deadlocking each other: a distractor only
//! needs its own core assets, never its own distractors.
//!
//! This module is pure. The facts come from one snapshot read; the evaluation
//! is a fold over them, so a full pass is cheap enough to run inline every
//! cycle instead of being a job.

use std::collections::HashMap;

use morpho_domain::blocker::{BlockerCode, BlockerSet};

/// Everything readiness needs to know about one word.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordFacts {
    pub word_id: i64,
    /// At least one `enabled` definition selection exists.
    pub has_enabled_sense: bool,
    /// Some selection carries `is_primary`.
    pub has_primary: bool,
    /// The primary sense is approved.
    pub primary_approved: bool,
    /// Enabled sense slots that are selected but not approved.
    pub unapproved_senses: usize,
    /// A selected definition contains a lemma whose OOV row is open or absent.
    pub oos_pending: bool,
    /// Some dependency is not covered by base words or an earlier plan slot.
    pub uncovered_dependency: bool,
    /// Example slot 1 (the mode-1 sentence) is filled.
    pub has_example_slot1: bool,
    /// Selected example slots that are not approved.
    pub unapproved_examples: usize,
    /// An image selection exists.
    pub has_image: bool,
    /// The selected image is approved.
    pub image_approved: bool,
    /// Desired TTS texts with no asset for the current voice configuration.
    pub tts_missing: usize,
    /// Desired TTS texts whose asset is `failed`.
    pub tts_failed: usize,
    /// The word has a placement in the current plan.
    pub in_plan: bool,
    /// Bound distractors as `(rank, distractor_word_id)`.
    pub distractors: Vec<(i64, i64)>,
}

/// One word's evaluated readiness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readiness {
    pub word_id: i64,
    pub ready: bool,
    pub core_ready: bool,
    pub blockers: BlockerSet,
}

/// Blockers that do not involve the distractor recursion.
pub fn core_blockers(facts: &WordFacts) -> BlockerSet {
    let mut blockers = BlockerSet::new();

    blockers.insert_if(!facts.has_enabled_sense, BlockerCode::MissingDefinition);
    blockers.insert_if(!facts.has_primary, BlockerCode::MissingPrimarySense);
    blockers.insert_if(
        facts.unapproved_senses > 0 || (facts.has_primary && !facts.primary_approved),
        BlockerCode::SenseNotApproved,
    );
    blockers.insert_if(facts.oos_pending, BlockerCode::OosPending);
    blockers.insert_if(facts.uncovered_dependency, BlockerCode::DependencyNotReady);
    blockers.insert_if(!facts.has_example_slot1, BlockerCode::MissingExample);
    blockers.insert_if(
        facts.unapproved_examples > 0,
        BlockerCode::ExampleNotApproved,
    );
    blockers.insert_if(!facts.has_image, BlockerCode::MissingImage);
    blockers.insert_if(
        facts.has_image && !facts.image_approved,
        BlockerCode::ImageNotApproved,
    );
    blockers.insert_if(facts.tts_missing > 0, BlockerCode::TtsMissing);
    blockers.insert_if(facts.tts_failed > 0, BlockerCode::TtsFailed);
    blockers.insert_if(!facts.in_plan, BlockerCode::NotInPlan);

    blockers
}

/// Evaluate every word in one shot.
///
/// Two sweeps, because `ready` reads the `core_ready` of *other* words: the
/// first computes every core verdict, the second layers the distractor
/// blockers on top. No fixpoint is needed — that is the whole point of the
/// depth-1 cap.
pub fn evaluate_all(facts: &[WordFacts]) -> Vec<Readiness> {
    let cores: HashMap<i64, (BlockerSet, bool)> = facts
        .iter()
        .map(|f| {
            let blockers = core_blockers(f);
            let core_ready = blockers.core_ready();
            (f.word_id, (blockers, core_ready))
        })
        .collect();

    facts
        .iter()
        .map(|f| {
            let (mut blockers, core_ready) = cores
                .get(&f.word_id)
                .cloned()
                .unwrap_or_else(|| (core_blockers(f), false));

            if f.distractors.len() < 3 {
                blockers.insert(BlockerCode::DistractorsUnbound);
            }
            for (rank, distractor_word_id) in &f.distractors {
                // A distractor pointing at a word outside the evaluated set is
                // not ready by definition: it cannot supply an image or a
                // definition for the quiz.
                let distractor_core_ready = cores
                    .get(distractor_word_id)
                    .map(|(_, ready)| *ready)
                    .unwrap_or(false);
                if !distractor_core_ready {
                    if let Some(code) = BlockerCode::distractor_not_ready(*rank) {
                        blockers.insert(code);
                    }
                }
            }

            Readiness {
                word_id: f.word_id,
                ready: blockers.ready(),
                core_ready,
                blockers,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A word with every core gate satisfied and no distractors yet.
    fn complete(word_id: i64) -> WordFacts {
        WordFacts {
            word_id,
            has_enabled_sense: true,
            has_primary: true,
            primary_approved: true,
            unapproved_senses: 0,
            oos_pending: false,
            uncovered_dependency: false,
            has_example_slot1: true,
            unapproved_examples: 0,
            has_image: true,
            image_approved: true,
            tts_missing: 0,
            tts_failed: 0,
            in_plan: true,
            distractors: Vec::new(),
        }
    }

    fn with_distractors(mut facts: WordFacts, ids: [i64; 3]) -> WordFacts {
        facts.distractors = vec![(1, ids[0]), (2, ids[1]), (3, ids[2])];
        facts
    }

    #[test]
    fn a_complete_word_is_core_ready_but_not_ready_without_distractors() {
        let results = evaluate_all(&[complete(1)]);
        assert!(results[0].core_ready);
        assert!(!results[0].ready);
        assert!(results[0]
            .blockers
            .contains(BlockerCode::DistractorsUnbound));
    }

    #[test]
    fn a_bare_word_reports_every_missing_gate() {
        let results = evaluate_all(&[WordFacts {
            word_id: 1,
            ..WordFacts::default()
        }]);
        let codes = results[0].blockers.to_strings();
        for expected in [
            "missing_definition",
            "missing_primary_sense",
            "missing_example",
            "missing_image",
            "not_in_plan",
            "distractors_unbound",
        ] {
            assert!(codes.contains(&expected.to_string()), "{codes:?}");
        }
        assert!(!results[0].core_ready);
        assert!(!results[0].ready);
    }

    #[test]
    fn each_gate_moves_exactly_its_own_blocker() {
        type Mutate = fn(&mut WordFacts);
        let cases: Vec<(Mutate, BlockerCode)> = vec![
            (
                |f| f.has_enabled_sense = false,
                BlockerCode::MissingDefinition,
            ),
            (|f| f.has_primary = false, BlockerCode::MissingPrimarySense),
            (
                |f| f.primary_approved = false,
                BlockerCode::SenseNotApproved,
            ),
            (|f| f.unapproved_senses = 1, BlockerCode::SenseNotApproved),
            (|f| f.oos_pending = true, BlockerCode::OosPending),
            (
                |f| f.uncovered_dependency = true,
                BlockerCode::DependencyNotReady,
            ),
            (|f| f.has_example_slot1 = false, BlockerCode::MissingExample),
            (
                |f| f.unapproved_examples = 1,
                BlockerCode::ExampleNotApproved,
            ),
            (|f| f.has_image = false, BlockerCode::MissingImage),
            (|f| f.image_approved = false, BlockerCode::ImageNotApproved),
            (|f| f.tts_missing = 2, BlockerCode::TtsMissing),
            (|f| f.tts_failed = 1, BlockerCode::TtsFailed),
            (|f| f.in_plan = false, BlockerCode::NotInPlan),
        ];
        for (mutate, expected) in cases {
            let mut facts = complete(1);
            mutate(&mut facts);
            let blockers = core_blockers(&facts);
            assert!(
                blockers.contains(expected),
                "{expected} not reported for {facts:?}"
            );
            assert!(!blockers.core_ready());
        }
    }

    #[test]
    fn a_missing_image_never_also_reports_it_as_unapproved() {
        let mut facts = complete(1);
        facts.has_image = false;
        facts.image_approved = false;
        let blockers = core_blockers(&facts);
        assert!(blockers.contains(BlockerCode::MissingImage));
        assert!(!blockers.contains(BlockerCode::ImageNotApproved));
    }

    #[test]
    fn mutual_distractors_do_not_deadlock() {
        // adapt ↔ adopt each name the other; both are otherwise complete.
        let adapt = with_distractors(complete(1), [2, 3, 4]);
        let adopt = with_distractors(complete(2), [1, 3, 4]);
        let results = evaluate_all(&[
            adapt,
            adopt,
            with_distractors(complete(3), [1, 2, 4]),
            with_distractors(complete(4), [1, 2, 3]),
        ]);
        assert!(results.iter().all(|r| r.core_ready));
        assert!(
            results.iter().all(|r| r.ready),
            "the depth-1 cap must let a mutual pair settle"
        );
    }

    #[test]
    fn a_broken_distractor_blocks_only_its_own_rank() {
        let mut broken = complete(2);
        broken.has_image = false;
        let results = evaluate_all(&[
            with_distractors(complete(1), [2, 3, 4]),
            broken,
            complete(3),
            complete(4),
        ]);
        let subject = &results[0];
        assert!(subject.core_ready, "the word's own assets are fine");
        assert!(!subject.ready);
        assert!(subject.blockers.contains(BlockerCode::Distractor1NotReady));
        assert!(!subject.blockers.contains(BlockerCode::Distractor2NotReady));
        assert!(!subject.blockers.contains(BlockerCode::Distractor3NotReady));
    }

    #[test]
    fn a_distractor_outside_the_evaluated_set_is_not_ready() {
        let results = evaluate_all(&[with_distractors(complete(1), [99, 98, 97])]);
        assert!(results[0]
            .blockers
            .contains(BlockerCode::Distractor1NotReady));
        assert!(results[0]
            .blockers
            .contains(BlockerCode::Distractor3NotReady));
        assert!(!results[0].ready);
    }

    #[test]
    fn a_partially_bound_word_reports_unbound() {
        let mut facts = complete(1);
        facts.distractors = vec![(1, 2), (2, 3)];
        let results = evaluate_all(&[facts, complete(2), complete(3)]);
        assert!(results[0]
            .blockers
            .contains(BlockerCode::DistractorsUnbound));
    }

    #[test]
    fn the_distractor_recursion_never_touches_core_ready() {
        let results = evaluate_all(&[with_distractors(complete(1), [99, 98, 97])]);
        assert!(results[0].core_ready);
        assert!(!results[0].ready);
    }

    #[test]
    fn evaluation_is_deterministic_regardless_of_input_order() {
        let words = vec![
            with_distractors(complete(1), [2, 3, 4]),
            with_distractors(complete(2), [1, 3, 4]),
            complete(3),
            complete(4),
        ];
        let forward = evaluate_all(&words);
        let mut reversed: Vec<WordFacts> = words.into_iter().rev().collect();
        let backward = evaluate_all(&reversed);
        reversed.reverse();
        for word in &reversed {
            let a = forward.iter().find(|r| r.word_id == word.word_id).unwrap();
            let b = backward.iter().find(|r| r.word_id == word.word_id).unwrap();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn blocker_json_is_stable_for_a_stable_state() {
        let results = evaluate_all(&[complete(1)]);
        assert_eq!(results[0].blockers.to_json(), r#"["distractors_unbound"]"#);
    }
}
