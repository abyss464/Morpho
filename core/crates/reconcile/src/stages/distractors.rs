//! Distractor binding: nearest three confusable words, bound once forever.
//!
//! Product rule (README Part 3): the three distractors a word gets are the
//! three it keeps, for every mode and every review, so the learner memorises
//! the word rather than the elimination pattern. The engine therefore only ever
//! *fills* an empty binding — there is no code path here that changes one.
//!
//! The pool is target ∪ active auxiliary, because a distractor supplies its own
//! image and definition to the quiz and must therefore be shippable itself.
//!
//! Selection prefers same-POS candidates, then nearest-by-Damerau-Levenshtein,
//! with ties broken by `(pos_mismatch, distance, frequency_rank, word_id)`.
//! Morphological relatives (words sharing a stem) are excluded so that
//! "adapt"/"adapter" or "invest"/"investor" never pair.

use std::collections::{HashMap, HashSet};

use morpho_domain::event::Actor;
use morpho_domain::version::DISTRACTOR_ALGO_VER;
use morpho_store::error::Result;
use morpho_store::ops::{BindDistractors, DistractorBinding};
use morpho_store::{queries, Store, WriteOp, WriteResult};

use crate::distance::lemma_distance;

/// Words whose lemmas differ by more than this are not confusable at all, and
/// pairing them teaches nothing.
pub const MAX_DISTANCE: usize = 5;

/// Minimum length of the common prefix for `shares_stem` to consider two
/// lemmas morphological relatives.
const MIN_STEM_PREFIX: usize = 4;

/// Minimum ratio of common-prefix length to the shorter lemma's length for
/// `shares_stem` to fire.
const STEM_RATIO: f64 = 0.6;

/// True if two lowercased lemmas likely share a morphological root.
///
/// The heuristic: if the longest common prefix is at least `MIN_STEM_PREFIX`
/// characters *and* covers at least `STEM_RATIO` of the shorter word, the pair
/// is a likely derivation (adapt/adapter, invest/investor, nation/national).
/// Short overlaps (car/card, ban/banana) and coincidental prefixes (for/force)
/// are rejected.
pub fn shares_stem(a: &str, b: &str) -> bool {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let prefix_len = a
        .chars()
        .zip(b.chars())
        .take_while(|(ca, cb)| ca == cb)
        .count();
    let min_len = a.chars().count().min(b.chars().count());
    if min_len == 0 {
        return false;
    }
    prefix_len >= MIN_STEM_PREFIX && (prefix_len as f64) >= (min_len as f64) * STEM_RATIO
}

/// POS context for distractor selection.
///
/// `primary` maps `word_id → primary POS` (the `is_primary = 1` selection).
/// `all` maps `word_id → set of all enabled POS values`. A candidate whose POS
/// set intersects the target's primary POS is preferred.
#[derive(Debug, Default)]
pub struct PosContext {
    pub primary: HashMap<i64, String>,
    pub all: HashMap<i64, HashSet<String>>,
}

/// Bind distractors for every active word that has fewer than three.
pub async fn bind_distractors(store: &Store) -> Result<usize> {
    let bindings = store
        .read(|conn| {
            let pool = queries::active_words(conn)?;
            let existing = queries::distractor_edges(conn)?;

            let mut pos_ctx = PosContext::default();
            for (word_id, pos) in queries::primary_pos_map(conn)? {
                pos_ctx.primary.insert(word_id, pos);
            }
            for (word_id, pos) in queries::word_pos_set(conn)? {
                pos_ctx.all.entry(word_id).or_default().insert(pos);
            }

            Ok(compute(&pool, &existing, &pos_ctx))
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

/// Which slice of the pool a caller will accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateFilter {
    /// Everything active, shippable or not — what first-time binding uses,
    /// because a word with no distractors at all is worse than a word with an
    /// unfinished one.
    AnyReadiness,
    /// Only `core_ready` words. A repair has an existing binding to fall back
    /// on, so it can afford to insist.
    CoreReadyOnly,
}

/// Every acceptable distractor for one word, best first.
///
/// The single place the selection semantics live: same-POS first, then nearest
/// by Damerau-Levenshtein, ties broken by `(frequency_rank, word_id)`, with
/// morphological relatives and anything in `excluded` dropped. Both the
/// first-time binding and the repair path read from here, so a rule can never
/// hold on one and not the other.
fn ranked_candidates(
    word_id: i64,
    lemma: &str,
    pool: &[queries::WordRow],
    pos_ctx: &PosContext,
    excluded: &HashSet<i64>,
    filter: CandidateFilter,
) -> Vec<i64> {
    let target_primary_pos = pos_ctx.primary.get(&word_id);

    // Sort key: (pos_mismatch, distance, frequency_rank, word_id).
    // pos_mismatch = 0 when the candidate has any sense matching the target's
    // primary POS, 1 otherwise (or when POS is unknown).
    let mut scored: Vec<(u8, usize, i64, i64)> = pool
        .iter()
        .filter(|other| other.word_id != word_id)
        .filter(|other| !excluded.contains(&other.word_id))
        .filter(|other| filter == CandidateFilter::AnyReadiness || other.core_ready)
        .filter(|other| !shares_stem(lemma, &other.lemma))
        .map(|other| {
            let distance = lemma_distance(lemma, &other.lemma);
            let pos_mismatch = match target_primary_pos {
                Some(target_pos) => {
                    let other_poses = pos_ctx.all.get(&other.word_id);
                    if other_poses.is_some_and(|s| s.contains(target_pos)) {
                        0u8
                    } else {
                        1u8
                    }
                }
                None => 1u8,
            };
            (
                pos_mismatch,
                distance,
                other.frequency_rank.unwrap_or(i64::MAX),
                other.word_id,
            )
        })
        .filter(|(_, distance, _, _)| *distance <= MAX_DISTANCE)
        .collect();
    scored.sort_unstable();
    scored.into_iter().map(|(_, _, _, id)| id).collect()
}

/// Pure core: which words still need bindings, and what they should be.
pub fn compute(
    pool: &[queries::WordRow],
    existing: &[(i64, i64, i64)],
    pos_ctx: &PosContext,
) -> Vec<DistractorBinding> {
    let mut bound_ranks: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut bound_targets: HashMap<i64, Vec<i64>> = HashMap::new();
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

        let excluded: HashSet<i64> = already.iter().copied().chain([word.word_id]).collect();
        let scored = ranked_candidates(
            word.word_id,
            &word.lemma,
            pool,
            pos_ctx,
            &excluded,
            CandidateFilter::AnyReadiness,
        );

        let mut ranks = Vec::new();
        let mut cursor = scored.into_iter();
        for rank in 1..=3i64 {
            if taken.contains(&rank) {
                continue;
            }
            let Some(distractor_word_id) = cursor.next() else {
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

// ---------------------------------------------------------------------------
// Repair: bindings that predate the stem-exclusion rule
// ---------------------------------------------------------------------------

/// The reason string recorded on every event this repair writes.
pub const STEM_VIOLATION_REASON: &str = "stem_violation_rebind";

/// One binding that breaks the stem rule, with the replacement chosen for it.
///
/// `new_word_id` is `None` when the pool offered nothing acceptable. That is a
/// deliberate outcome, not a failure: the existing binding stays, because a
/// morphological relative the learner can still answer beats a distractor that
/// drags the word out of the release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebindPlan {
    pub word_id: i64,
    pub lemma: String,
    pub rank: i64,
    pub old_word_id: i64,
    pub old_lemma: String,
    pub new_word_id: Option<i64>,
    pub new_lemma: Option<String>,
    /// `core_ready` of the replacement; `false` when there is none.
    pub core_ready_new: bool,
}

impl RebindPlan {
    /// True when a replacement was found and the row will move.
    pub fn is_resolved(&self) -> bool {
        self.new_word_id.is_some()
    }
}

/// True when a bound pair are morphological relatives — `adapt`/`adapter`.
///
/// Bindings written before the exclusion rule existed still hold pairs that
/// [`compute`] would refuse to create today; this is the same predicate, read
/// backwards over what is already in the table.
pub fn is_stem_violation(pair: &queries::DistractorPair) -> bool {
    shares_stem(&pair.lemma, &pair.distractor_lemma)
}

/// Plan a replacement for every binding that breaks the stem rule.
///
/// `pairs` is the whole `distractors` table with both lemmas resolved; `pool`
/// is the same active-word pool [`compute`] draws from. Pure: the caller
/// decides whether to write the result, so a dry run and a real run compute
/// exactly the same plan.
///
/// Replacements are drawn from the `core_ready` slice of the pool only. A
/// distractor that is not itself shippable flips the quizzed word's `ready` off
/// and shrinks the export, so trading a merely-related distractor for one that
/// costs a word its place in the release is a bad trade at any distance.
pub fn plan_stem_rebinds(
    pairs: &[queries::DistractorPair],
    pool: &[queries::WordRow],
    pos_ctx: &PosContext,
) -> Vec<RebindPlan> {
    let by_id: HashMap<i64, &queries::WordRow> =
        pool.iter().map(|word| (word.word_id, word)).collect();

    // Everything a word already holds is off the table for its replacements,
    // and stays off: `UNIQUE (word_id, distractor_word_id)` means a word with
    // two violating ranks must be given two different replacements.
    let mut held: HashMap<i64, HashSet<i64>> = HashMap::new();
    for pair in pairs {
        held.entry(pair.word_id)
            .or_default()
            .insert(pair.distractor_word_id);
    }

    let mut violations: Vec<&queries::DistractorPair> = pairs
        .iter()
        .filter(|pair| is_stem_violation(pair))
        .collect();
    // The store already orders by `(word_id, rank)`; sorting again makes the
    // plan independent of how the caller obtained the rows.
    violations.sort_by_key(|pair| (pair.word_id, pair.rank));

    let mut out = Vec::with_capacity(violations.len());
    for pair in violations {
        let mut excluded = held.get(&pair.word_id).cloned().unwrap_or_default();
        excluded.insert(pair.word_id);

        let chosen = ranked_candidates(
            pair.word_id,
            &pair.lemma,
            pool,
            pos_ctx,
            &excluded,
            CandidateFilter::CoreReadyOnly,
        )
        .into_iter()
        .next()
        .and_then(|word_id| by_id.get(&word_id).copied());

        if let Some(replacement) = chosen {
            held.entry(pair.word_id)
                .or_default()
                .insert(replacement.word_id);
        }

        out.push(RebindPlan {
            word_id: pair.word_id,
            lemma: pair.lemma.clone(),
            rank: pair.rank,
            old_word_id: pair.distractor_word_id,
            old_lemma: pair.distractor_lemma.clone(),
            new_word_id: chosen.map(|word| word.word_id),
            new_lemma: chosen.map(|word| word.lemma.clone()),
            core_ready_new: chosen.is_some_and(|word| word.core_ready),
        });
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
            word(4, "adage", 6000),
            word(5, "serene", 4602),
            word(6, "tranquil", 4733),
        ]
    }

    fn empty_pos() -> PosContext {
        PosContext::default()
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
        let bindings = compute(&pool(), &[], &empty_pos());
        let adapt: Vec<i64> = ranks_for(&bindings, 1)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        assert!(adapt.contains(&2), "adopt must distract adapt: {adapt:?}");
        assert!(adapt.contains(&3), "adept must distract adapt: {adapt:?}");
    }

    #[test]
    fn morphological_relatives_are_excluded() {
        let p = vec![
            word(1, "adapt", 1520),
            word(2, "adopt", 1385),
            word(3, "adept", 4890),
            word(4, "adapter", 6000),
        ];
        let bindings = compute(&p, &[], &empty_pos());
        let adapt: Vec<i64> = ranks_for(&bindings, 1)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        assert!(
            !adapt.contains(&4),
            "adapter must NOT distract adapt: {adapt:?}"
        );
    }

    #[test]
    fn shares_stem_catches_derivations() {
        assert!(shares_stem("adapt", "adapter"));
        assert!(shares_stem("invest", "investor"));
        assert!(shares_stem("nation", "national"));
        assert!(shares_stem("employ", "employer"));
        assert!(shares_stem("compete", "competition"));
    }

    #[test]
    fn shares_stem_rejects_coincidences() {
        assert!(!shares_stem("car", "card"));
        assert!(!shares_stem("for", "force"));
        assert!(!shares_stem("ban", "banana"));
        assert!(!shares_stem("adapt", "adopt")); // prefix "ad" < 4
        assert!(!shares_stem("", "anything"));
    }

    #[test]
    fn same_pos_candidates_are_preferred() {
        // Create a pool where word 10 is a noun, word 11 is a verb (same
        // distance), and the target's primary POS is noun.
        let p = vec![
            word(1, "adapt", 1000),
            word(10, "abact", 2000), // distance 2 from "adapt"
            word(11, "exact", 2000), // distance 2 from "adapt"
        ];
        let mut ctx = PosContext::default();
        ctx.primary.insert(1, "noun".into());
        ctx.all.entry(1).or_default().insert("noun".into());
        // word 10 is a noun
        ctx.all.entry(10).or_default().insert("noun".into());
        // word 11 is a verb only
        ctx.all.entry(11).or_default().insert("verb".into());

        let bindings = compute(&p, &[], &ctx);
        let adapt = ranks_for(&bindings, 1);
        assert_eq!(adapt[0].1, 10, "the same-POS word takes rank 1: {adapt:?}");
    }

    #[test]
    fn every_word_gets_exactly_three_ranks_when_the_pool_allows() {
        let bindings = compute(&pool(), &[], &empty_pos());
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
        let bindings = compute(&pool(), &[], &empty_pos());
        for binding in &bindings {
            for (_, target) in &binding.ranks {
                assert_ne!(*target, binding.word_id);
            }
        }
    }

    #[test]
    fn distractors_within_a_word_are_distinct() {
        let bindings = compute(&pool(), &[], &empty_pos());
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
        let bindings = compute(&pool(), &[], &empty_pos());
        let adapt = ranks_for(&bindings, 1);
        assert_eq!(adapt[0].1, 2, "the commoner word takes rank 1: {adapt:?}");
    }

    #[test]
    fn already_bound_words_are_left_alone() {
        let existing = vec![(1, 1, 2), (1, 2, 3), (1, 3, 4)];
        let bindings = compute(&pool(), &existing, &empty_pos());
        assert!(
            bindings.iter().all(|b| b.word_id != 1),
            "a fully bound word must not be revisited"
        );
    }

    #[test]
    fn a_partial_binding_only_gains_its_missing_ranks() {
        let existing = vec![(1, 1, 2)];
        let bindings = compute(&pool(), &existing, &empty_pos());
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
        let bindings = compute(&sparse, &[], &empty_pos());
        assert!(bindings.is_empty(), "{bindings:?}");
    }

    #[test]
    fn a_short_pool_binds_what_it_can() {
        let two = vec![word(1, "adapt", 10), word(2, "adopt", 20)];
        let bindings = compute(&two, &[], &empty_pos());
        assert_eq!(ranks_for(&bindings, 1).len(), 1);
        assert_eq!(ranks_for(&bindings, 2).len(), 1);
    }

    #[test]
    fn a_lone_word_gets_nothing() {
        assert!(compute(&[word(1, "adapt", 10)], &[], &empty_pos()).is_empty());
    }

    #[test]
    fn the_result_is_deterministic() {
        let pos = empty_pos();
        let forward = compute(&pool(), &[], &pos);
        let mut shuffled = pool();
        shuffled.reverse();
        let backward = compute(&shuffled, &[], &pos);
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
        let bindings = compute(&with_retired, &[], &empty_pos());
        assert!(bindings
            .iter()
            .all(|b| b.ranks.iter().all(|(_, id)| *id != 7)));
    }

    // -----------------------------------------------------------------------
    // Repairing bindings that predate the stem rule
    // -----------------------------------------------------------------------

    /// A word that is shippable, and therefore eligible as a replacement.
    fn core(word_id: i64, lemma: &str, rank: i64) -> queries::WordRow {
        let mut row = word(word_id, lemma, rank);
        row.core_ready = true;
        row
    }

    fn pair(
        word_id: i64,
        lemma: &str,
        rank: i64,
        distractor_word_id: i64,
        distractor_lemma: &str,
    ) -> queries::DistractorPair {
        queries::DistractorPair {
            word_id,
            lemma: lemma.to_string(),
            rank,
            distractor_word_id,
            distractor_lemma: distractor_lemma.to_string(),
        }
    }

    /// adapt / adopt / adept / adapter / adaptive, all shippable.
    fn repair_pool() -> Vec<queries::WordRow> {
        vec![
            core(1, "adapt", 1520),
            core(2, "adopt", 1385),
            core(3, "adept", 4890),
            core(4, "adapter", 6000),
            core(5, "adaptive", 7000),
        ]
    }

    #[test]
    fn violation_detection_agrees_with_shares_stem() {
        let pairs = vec![
            pair(1, "adapt", 1, 4, "adapter"),
            pair(1, "adapt", 2, 2, "adopt"),
            pair(1, "adapt", 3, 3, "adept"),
            pair(5, "invest", 1, 6, "investor"),
        ];
        for p in &pairs {
            assert_eq!(
                is_stem_violation(p),
                shares_stem(&p.lemma, &p.distractor_lemma),
                "{p:?}"
            );
        }
        let plans = plan_stem_rebinds(&pairs, &repair_pool(), &empty_pos());
        let touched: Vec<(i64, i64)> = plans.iter().map(|p| (p.word_id, p.rank)).collect();
        assert_eq!(touched, vec![(1, 1), (5, 1)], "{plans:?}");
    }

    #[test]
    fn a_table_without_relatives_plans_nothing() {
        let pairs = vec![
            pair(1, "adapt", 1, 2, "adopt"),
            pair(1, "adapt", 2, 3, "adept"),
        ];
        assert!(plan_stem_rebinds(&pairs, &repair_pool(), &empty_pos()).is_empty());
    }

    #[test]
    fn only_core_ready_words_replace_a_violation() {
        // adopt is the commoner of the two words one edit from adapt, so it
        // would win outright — but it cannot ship, and binding it would cost
        // adapt its own place in the release.
        let pool = vec![
            core(1, "adapt", 1520),
            word(2, "adopt", 1385),
            core(3, "adept", 4890),
            core(4, "adapter", 6000),
        ];
        let plans = plan_stem_rebinds(&[pair(1, "adapt", 1, 4, "adapter")], &pool, &empty_pos());
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].new_word_id, Some(3), "{plans:?}");
        assert_eq!(plans[0].new_lemma.as_deref(), Some("adept"));
        assert!(plans[0].core_ready_new);
    }

    #[test]
    fn an_unresolvable_violation_keeps_its_binding() {
        // Nothing shippable is near enough that is not also a relative.
        let pool = vec![
            core(1, "adapt", 1520),
            word(2, "adopt", 1385),
            core(4, "adapter", 6000),
        ];
        let plans = plan_stem_rebinds(&[pair(1, "adapt", 1, 4, "adapter")], &pool, &empty_pos());
        assert_eq!(plans.len(), 1);
        assert!(!plans[0].is_resolved());
        assert_eq!(plans[0].new_word_id, None);
        assert_eq!(plans[0].new_lemma, None);
        assert!(!plans[0].core_ready_new);
        assert_eq!(plans[0].old_word_id, 4, "the row it keeps");
    }

    #[test]
    fn a_distant_pool_leaves_the_violation_in_place() {
        let pool = vec![
            core(1, "adapt", 1520),
            core(4, "adapter", 6000),
            core(9, "xylophone", 100),
        ];
        let plans = plan_stem_rebinds(&[pair(1, "adapt", 1, 4, "adapter")], &pool, &empty_pos());
        assert!(!plans[0].is_resolved(), "{plans:?}");
    }

    #[test]
    fn two_violations_on_one_word_get_different_replacements() {
        let pairs = vec![
            pair(1, "adapt", 1, 4, "adapter"),
            pair(1, "adapt", 2, 5, "adaptive"),
        ];
        let plans = plan_stem_rebinds(&pairs, &repair_pool(), &empty_pos());
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].new_word_id, Some(2), "{plans:?}");
        assert_eq!(plans[1].new_word_id, Some(3), "{plans:?}");
    }

    #[test]
    fn a_replacement_never_repeats_a_binding_the_word_already_holds() {
        let pairs = vec![
            pair(1, "adapt", 1, 2, "adopt"),
            pair(1, "adapt", 2, 4, "adapter"),
        ];
        let plans = plan_stem_rebinds(&pairs, &repair_pool(), &empty_pos());
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].new_word_id,
            Some(3),
            "adopt is bound at rank 1 already: {plans:?}"
        );
    }

    #[test]
    fn no_plan_pairs_a_word_with_itself_or_a_relative() {
        let pool = repair_pool();
        let pairs = vec![
            pair(1, "adapt", 1, 4, "adapter"),
            pair(1, "adapt", 2, 5, "adaptive"),
            pair(4, "adapter", 1, 5, "adaptive"),
            pair(5, "adaptive", 1, 1, "adapt"),
        ];
        for plan in plan_stem_rebinds(&pairs, &pool, &empty_pos()) {
            let Some(new_word_id) = plan.new_word_id else {
                continue;
            };
            assert_ne!(new_word_id, plan.word_id, "{plan:?}");
            assert_ne!(new_word_id, plan.old_word_id, "{plan:?}");
            let replacement = pool
                .iter()
                .find(|w| w.word_id == new_word_id)
                .expect("replacements come from the pool");
            assert!(
                !shares_stem(&plan.lemma, &replacement.lemma),
                "{plan:?} is still a relative"
            );
            assert!(
                lemma_distance(&plan.lemma, &replacement.lemma) <= MAX_DISTANCE,
                "{plan:?}"
            );
            assert!(replacement.core_ready, "{plan:?}");
        }
    }

    #[test]
    fn the_repair_path_prefers_the_same_pos() {
        // adept is the commoner word, but adopt shares adapt's primary POS.
        let pool = vec![
            core(1, "adapt", 1520),
            core(2, "adopt", 5000),
            core(3, "adept", 1000),
            core(4, "adapter", 6000),
        ];
        let mut ctx = PosContext::default();
        ctx.primary.insert(1, "noun".into());
        ctx.all.entry(1).or_default().insert("noun".into());
        ctx.all.entry(2).or_default().insert("noun".into());
        ctx.all.entry(3).or_default().insert("verb".into());

        let plans = plan_stem_rebinds(&[pair(1, "adapt", 1, 4, "adapter")], &pool, &ctx);
        assert_eq!(plans[0].new_word_id, Some(2), "{plans:?}");
    }

    #[test]
    fn the_rebind_plan_is_deterministic() {
        let pairs = vec![
            pair(1, "adapt", 1, 4, "adapter"),
            pair(1, "adapt", 2, 5, "adaptive"),
            pair(4, "adapter", 1, 5, "adaptive"),
        ];
        let pool = repair_pool();
        let forward = plan_stem_rebinds(&pairs, &pool, &empty_pos());

        let mut shuffled_pairs = pairs;
        shuffled_pairs.reverse();
        let mut shuffled_pool = pool;
        shuffled_pool.reverse();
        let backward = plan_stem_rebinds(&shuffled_pairs, &shuffled_pool, &empty_pos());

        assert_eq!(forward, backward);
    }

    #[test]
    fn a_word_outside_the_active_pool_is_still_repaired() {
        // The quizzed word may have left the pool (retired auxiliary), but the
        // violating row is still in the table and still ships if it comes back.
        let pool = vec![core(2, "adopt", 1385), core(3, "adept", 4890)];
        let plans = plan_stem_rebinds(&[pair(1, "adapt", 1, 4, "adapter")], &pool, &empty_pos());
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].new_word_id, Some(2), "{plans:?}");
    }
}
