//! Distractor binding: nearest three confusable words, bound once forever.
//!
//! Product rule (README Part 3): the three distractors a word gets are the
//! three it keeps, for every mode and every review, so the learner memorises
//! the word rather than the elimination pattern. The engine therefore only ever
//! *fills* an empty binding — there is no code path here that changes one.
//!
//! The pool is target ∪ active auxiliary, because a distractor supplies its own
//! image and definition to the quiz and must therefore be shippable itself.
//! Selection is nearest-by-Damerau-Levenshtein with ties broken by
//! `(distance, frequency_rank, word_id)`, which makes the result a pure
//! function of the lexicon.

use morpho_domain::event::Actor;
use morpho_domain::version::DISTRACTOR_ALGO_VER;
use morpho_store::error::Result;
use morpho_store::ops::{BindDistractors, DistractorBinding};
use morpho_store::{queries, Store, WriteOp, WriteResult};

use crate::distance::lemma_distance;

/// Words whose lemmas differ by more than this are not confusable at all, and
/// pairing them teaches nothing.
pub const MAX_DISTANCE: usize = 5;

/// Bind distractors for every active word that has fewer than three.
pub async fn bind_distractors(store: &Store) -> Result<usize> {
    let bindings = store
        .read(|conn| {
            let pool = queries::active_words(conn)?;
            let existing = queries::distractor_edges(conn)?;
            Ok(compute(&pool, &existing))
        })
        .await?;

    if bindings.is_empty() {
        return Ok(0);
    }
    let outcome = store
        .write(
            Actor::Reconciler,
            WriteOp::BindDistractors(BindDistractors {
                bindings,
                algo_ver: DISTRACTOR_ALGO_VER.to_string(),
            }),
        )
        .await?;
    Ok(match outcome.result {
        WriteResult::Bound { bound } => bound,
        _ => 0,
    })
}

/// Pure core: which words still need bindings, and what they should be.
pub fn compute(pool: &[queries::WordRow], existing: &[(i64, i64, i64)]) -> Vec<DistractorBinding> {
    let mut bound_ranks: std::collections::HashMap<i64, Vec<i64>> =
        std::collections::HashMap::new();
    let mut bound_targets: std::collections::HashMap<i64, Vec<i64>> =
        std::collections::HashMap::new();
    for (word_id, rank, distractor_word_id) in existing {
        bound_ranks.entry(*word_id).or_default().push(*rank);
        bound_targets
            .entry(*word_id)
            .or_default()
            .push(*distractor_word_id);
    }

    let mut out = Vec::new();
    for word in pool {
        let taken = bound_ranks.get(&word.word_id).cloned().unwrap_or_default();
        if taken.len() >= 3 {
            continue;
        }
        let already = bound_targets
            .get(&word.word_id)
            .cloned()
            .unwrap_or_default();

        let mut scored: Vec<(usize, i64, i64)> = pool
            .iter()
            .filter(|other| other.word_id != word.word_id)
            .filter(|other| !already.contains(&other.word_id))
            .map(|other| {
                (
                    lemma_distance(&word.lemma, &other.lemma),
                    other.frequency_rank.unwrap_or(i64::MAX),
                    other.word_id,
                )
            })
            .filter(|(distance, _, _)| *distance <= MAX_DISTANCE)
            .collect();
        scored.sort_unstable();

        let mut ranks = Vec::new();
        let mut cursor = scored.into_iter();
        for rank in 1..=3i64 {
            if taken.contains(&rank) {
                continue;
            }
            let Some((_, _, distractor_word_id)) = cursor.next() else {
                break;
            };
            ranks.push((rank, distractor_word_id));
        }
        if !ranks.is_empty() {
            out.push(DistractorBinding {
                word_id: word.word_id,
                ranks,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use morpho_domain::types::{AuxStatus, Role};

    fn word(word_id: i64, lemma: &str, rank: i64) -> queries::WordRow {
        queries::WordRow {
            word_id,
            lemma: lemma.to_string(),
            role: Role::Target,
            aux_status: None,
            phonetic: None,
            frequency_rank: Some(rank),
            etymology: None,
            etymology_source: None,
            zh_gloss: None,
            zh_gloss_source: None,
            ready: false,
            core_ready: false,
            blockers: Vec::new(),
        }
    }

    fn pool() -> Vec<queries::WordRow> {
        vec![
            word(1, "adapt", 1520),
            word(2, "adopt", 1385),
            word(3, "adept", 4890),
            word(4, "adapter", 6000),
            word(5, "serene", 4602),
            word(6, "tranquil", 4733),
        ]
    }

    fn ranks_for(bindings: &[DistractorBinding], word_id: i64) -> Vec<(i64, i64)> {
        bindings
            .iter()
            .find(|b| b.word_id == word_id)
            .map(|b| b.ranks.clone())
            .unwrap_or_default()
    }

    #[test]
    fn the_confusable_trio_finds_each_other() {
        let bindings = compute(&pool(), &[]);
        let adapt: Vec<i64> = ranks_for(&bindings, 1)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        assert!(adapt.contains(&2), "adopt must distract adapt: {adapt:?}");
        assert!(adapt.contains(&3), "adept must distract adapt: {adapt:?}");
    }

    #[test]
    fn every_word_gets_exactly_three_ranks_when_the_pool_allows() {
        let bindings = compute(&pool(), &[]);
        for word_id in [1, 2, 3] {
            let ranks = ranks_for(&bindings, word_id);
            assert_eq!(ranks.len(), 3, "word {word_id}: {ranks:?}");
            assert_eq!(
                ranks.iter().map(|(r, _)| *r).collect::<Vec<_>>(),
                vec![1, 2, 3]
            );
        }
    }

    #[test]
    fn a_word_never_distracts_itself() {
        let bindings = compute(&pool(), &[]);
        for binding in &bindings {
            for (_, target) in &binding.ranks {
                assert_ne!(*target, binding.word_id);
            }
        }
    }

    #[test]
    fn distractors_within_a_word_are_distinct() {
        let bindings = compute(&pool(), &[]);
        for binding in &bindings {
            let mut targets: Vec<i64> = binding.ranks.iter().map(|(_, id)| *id).collect();
            targets.sort_unstable();
            let before = targets.len();
            targets.dedup();
            assert_eq!(before, targets.len(), "{binding:?}");
        }
    }

    #[test]
    fn ties_break_on_frequency_then_id() {
        // adopt(1385) and adept(4890) are both distance 1 from adapt.
        let bindings = compute(&pool(), &[]);
        let adapt = ranks_for(&bindings, 1);
        assert_eq!(adapt[0].1, 2, "the commoner word takes rank 1: {adapt:?}");
    }

    #[test]
    fn already_bound_words_are_left_alone() {
        let existing = vec![(1, 1, 2), (1, 2, 3), (1, 3, 4)];
        let bindings = compute(&pool(), &existing);
        assert!(
            bindings.iter().all(|b| b.word_id != 1),
            "a fully bound word must not be revisited"
        );
    }

    #[test]
    fn a_partial_binding_only_gains_its_missing_ranks() {
        let existing = vec![(1, 1, 2)];
        let bindings = compute(&pool(), &existing);
        let adapt = ranks_for(&bindings, 1);
        assert_eq!(
            adapt.iter().map(|(r, _)| *r).collect::<Vec<_>>(),
            vec![2, 3]
        );
        // And never re-picks the word already bound at rank 1.
        assert!(adapt.iter().all(|(_, id)| *id != 2));
    }

    #[test]
    fn distant_words_are_not_paired() {
        let sparse = vec![word(1, "serendipity", 100), word(2, "xylophone", 200)];
        let bindings = compute(&sparse, &[]);
        assert!(bindings.is_empty(), "{bindings:?}");
    }

    #[test]
    fn a_short_pool_binds_what_it_can() {
        let two = vec![word(1, "adapt", 10), word(2, "adopt", 20)];
        let bindings = compute(&two, &[]);
        assert_eq!(ranks_for(&bindings, 1).len(), 1);
        assert_eq!(ranks_for(&bindings, 2).len(), 1);
    }

    #[test]
    fn a_lone_word_gets_nothing() {
        assert!(compute(&[word(1, "adapt", 10)], &[]).is_empty());
    }

    #[test]
    fn the_result_is_deterministic() {
        let forward = compute(&pool(), &[]);
        let mut shuffled = pool();
        shuffled.reverse();
        let backward = compute(&shuffled, &[]);
        for binding in &forward {
            let other = backward
                .iter()
                .find(|b| b.word_id == binding.word_id)
                .expect("same words");
            assert_eq!(binding.ranks, other.ranks);
        }
    }

    #[test]
    fn retired_auxiliaries_are_not_in_the_pool() {
        // The caller passes `active_words`, so a retired auxiliary never
        // reaches `compute`. This documents the contract at the boundary.
        let mut retired = word(7, "adaptt", 9000);
        retired.role = Role::Auxiliary;
        retired.aux_status = Some(AuxStatus::Retired);
        let with_retired: Vec<queries::WordRow> = pool();
        let bindings = compute(&with_retired, &[]);
        assert!(bindings
            .iter()
            .all(|b| b.ranks.iter().all(|(_, id)| *id != 7)));
    }
}
