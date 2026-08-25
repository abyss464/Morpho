//! The dependency-closed cut and the holdback report (README Part 5).
//!
//! Shipping the set of factory-passing words is not enough: a word whose
//! definition depends on a held-back word breaks the readability invariant, and
//! a word whose distractor is held back has no options to show. The exporter
//! therefore takes the **maximal dependency-closed subset**:
//!
//! ```text
//! R ← { every shippable word }
//! repeat: remove any W ∈ R that has an edge W → X with X ∉ R
//! until nothing can be removed
//! ```
//!
//! Two edge families feed it: the dependency words of W's selected definitions,
//! and W's three distractors. The properties are all deliberate — an SCC sinks
//! or floats as a unit, mutual distractors likewise, and the result is the
//! unique maximal closed set regardless of removal order.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

/// One word as the cut sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutNode {
    pub word_id: i64,
    /// Passes every per-word factory gate.
    pub shippable: bool,
    /// Why not, when it does not. Canonical blocker order, first one wins.
    pub blockers: Vec<String>,
    /// Position in the current plan; ties in reporting break on it.
    pub learning_order: i64,
}

/// Why one word is not in the release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holdback {
    pub word_id: i64,
    /// A blocker code, or `dependency_holdback` for a closure removal.
    pub root_cause: String,
    pub root_cause_detail: String,
    pub blocking_word_id: Option<i64>,
    /// How many otherwise-shippable words this one keeps off the boat.
    pub impact_count: usize,
}

/// `dependency_holdback` — the root cause of a closure removal.
pub const DEPENDENCY_HOLDBACK: &str = "dependency_holdback";
/// Reported when a word fails a gate but carries no blocker code.
pub const UNKNOWN_CAUSE: &str = "not_shippable";

/// Result of the fixpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutResult {
    /// The maximal dependency-closed subset, in ascending word id.
    pub exportable: BTreeSet<i64>,
    /// Every excluded word, sorted by impact descending then word id.
    pub excluded: Vec<Holdback>,
    pub shippable_count: usize,
}

/// Compute the cut and the holdback report.
///
/// `edges` are `(from, to)`: `from` needs `to` on the boat.
pub fn compute(nodes: &[CutNode], edges: &[(i64, i64)]) -> CutResult {
    let known: HashSet<i64> = nodes.iter().map(|n| n.word_id).collect();
    let by_id: HashMap<i64, &CutNode> = nodes.iter().map(|n| (n.word_id, n)).collect();

    // Deduplicated, self-loop-free, deterministic edge set restricted to known
    // words. An edge to a word that is not in the lexicon at all cannot be
    // satisfied, so it is kept as a removal reason via `missing`.
    let mut forward: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    let mut reverse: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    let mut dangling: BTreeMap<i64, i64> = BTreeMap::new();
    for (from, to) in edges {
        if from == to || !known.contains(from) {
            continue;
        }
        if !known.contains(to) {
            dangling.entry(*from).or_insert(*to);
            continue;
        }
        forward.entry(*from).or_default().insert(*to);
        reverse.entry(*to).or_default().insert(*from);
    }

    let shippable: BTreeSet<i64> = nodes
        .iter()
        .filter(|n| n.shippable && !dangling.contains_key(&n.word_id))
        .map(|n| n.word_id)
        .collect();
    let shippable_count = shippable.len();

    // Worklist fixpoint over reverse edges: O(V + E).
    let mut retained = shippable.clone();
    let mut removed_because: HashMap<i64, i64> = HashMap::new();
    let mut queue: VecDeque<i64> = nodes
        .iter()
        .map(|n| n.word_id)
        .filter(|id| !retained.contains(id))
        .collect();

    while let Some(gone) = queue.pop_front() {
        let Some(dependents) = reverse.get(&gone) else {
            continue;
        };
        for dependent in dependents {
            if retained.remove(dependent) {
                removed_because.insert(*dependent, gone);
                queue.push_back(*dependent);
            }
        }
    }

    // Impact: how many shippable words each excluded word drags out with it.
    // Walking forward from the (usually few) shippable-but-cut words is far
    // cheaper than a reverse closure from every excluded word.
    let mut impact: HashMap<i64, usize> = HashMap::new();
    for word_id in shippable.iter().filter(|id| !retained.contains(id)) {
        let mut seen = HashSet::new();
        let mut stack = vec![*word_id];
        seen.insert(*word_id);
        while let Some(current) = stack.pop() {
            let Some(targets) = forward.get(&current) else {
                continue;
            };
            for target in targets {
                if !seen.insert(*target) {
                    continue;
                }
                if !retained.contains(target) {
                    *impact.entry(*target).or_default() += 1;
                    stack.push(*target);
                }
            }
        }
    }

    let mut excluded: Vec<Holdback> = nodes
        .iter()
        .filter(|node| !retained.contains(&node.word_id))
        .map(|node| {
            let impact_count = impact.get(&node.word_id).copied().unwrap_or(0);
            if let Some(missing) = dangling.get(&node.word_id) {
                return Holdback {
                    word_id: node.word_id,
                    root_cause: DEPENDENCY_HOLDBACK.to_string(),
                    root_cause_detail: format!(
                        "depends on word {missing}, which is not in the release lexicon"
                    ),
                    blocking_word_id: Some(*missing),
                    impact_count,
                };
            }
            if node.shippable {
                let blocking = removed_because.get(&node.word_id).copied().or_else(|| {
                    forward
                        .get(&node.word_id)
                        .and_then(|targets| {
                            targets.iter().find(|target| !retained.contains(target))
                        })
                        .copied()
                });
                let detail = blocking
                    .and_then(|id| by_id.get(&id))
                    .map(|blocker| {
                        format!(
                            "passes every gate but depends on held-back word {}",
                            blocker.word_id
                        )
                    })
                    .unwrap_or_else(|| "pulled out by the dependency closure".to_string());
                return Holdback {
                    word_id: node.word_id,
                    root_cause: DEPENDENCY_HOLDBACK.to_string(),
                    root_cause_detail: detail,
                    blocking_word_id: blocking,
                    impact_count,
                };
            }
            let cause = node
                .blockers
                .first()
                .cloned()
                .unwrap_or_else(|| UNKNOWN_CAUSE.to_string());
            Holdback {
                word_id: node.word_id,
                root_cause_detail: if node.blockers.is_empty() {
                    "failed a factory gate".to_string()
                } else {
                    node.blockers.join(", ")
                },
                root_cause: cause,
                blocking_word_id: None,
                impact_count,
            }
        })
        .collect();

    // The edit worklist: fix the most blocking word first.
    excluded.sort_by(|a, b| {
        b.impact_count
            .cmp(&a.impact_count)
            .then_with(|| a.word_id.cmp(&b.word_id))
    });

    CutResult {
        exportable: retained,
        excluded,
        shippable_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(word_id: i64, shippable: bool) -> CutNode {
        CutNode {
            word_id,
            shippable,
            blockers: if shippable {
                Vec::new()
            } else {
                vec!["missing_image".to_string()]
            },
            learning_order: word_id,
        }
    }

    fn ids(result: &CutResult) -> Vec<i64> {
        result.exportable.iter().copied().collect()
    }

    fn holdback(result: &CutResult, word_id: i64) -> &Holdback {
        result
            .excluded
            .iter()
            .find(|h| h.word_id == word_id)
            .unwrap_or_else(|| panic!("word {word_id} should be excluded"))
    }

    #[test]
    fn everything_shippable_and_closed_ships() {
        let nodes = vec![node(1, true), node(2, true)];
        let result = compute(&nodes, &[(1, 2)]);
        assert_eq!(ids(&result), vec![1, 2]);
        assert!(result.excluded.is_empty());
        assert_eq!(result.shippable_count, 2);
    }

    #[test]
    fn a_broken_dependency_pulls_its_dependent_out() {
        // 1 needs 2; 2 has no image.
        let nodes = vec![node(1, true), node(2, false)];
        let result = compute(&nodes, &[(1, 2)]);
        assert!(ids(&result).is_empty());
        assert_eq!(holdback(&result, 1).root_cause, DEPENDENCY_HOLDBACK);
        assert_eq!(holdback(&result, 1).blocking_word_id, Some(2));
        assert_eq!(holdback(&result, 2).root_cause, "missing_image");
    }

    #[test]
    fn removal_propagates_transitively() {
        // 1 → 2 → 3, and 3 is broken.
        let nodes = vec![node(1, true), node(2, true), node(3, false)];
        let result = compute(&nodes, &[(1, 2), (2, 3)]);
        assert!(ids(&result).is_empty());
        assert_eq!(
            holdback(&result, 3).impact_count,
            2,
            "3 blocks both 1 and 2"
        );
    }

    #[test]
    fn an_unrelated_component_still_ships() {
        let nodes = vec![node(1, true), node(2, false), node(3, true), node(4, true)];
        let result = compute(&nodes, &[(1, 2), (3, 4)]);
        assert_eq!(ids(&result), vec![3, 4]);
    }

    #[test]
    fn an_scc_sinks_together() {
        // 1 ↔ 2 mutually depend; 2 is broken, so both go.
        let nodes = vec![node(1, true), node(2, false), node(3, true)];
        let result = compute(&nodes, &[(1, 2), (2, 1)]);
        assert_eq!(ids(&result), vec![3]);
    }

    #[test]
    fn an_scc_floats_together() {
        let nodes = vec![node(1, true), node(2, true)];
        let result = compute(&nodes, &[(1, 2), (2, 1)]);
        assert_eq!(ids(&result), vec![1, 2]);
    }

    #[test]
    fn mutual_distractors_behave_like_any_other_cycle() {
        // adapt ↔ adopt as distractors, adopt broken.
        let nodes = vec![node(1, true), node(2, false)];
        let result = compute(&nodes, &[(1, 2), (2, 1)]);
        assert!(ids(&result).is_empty());
    }

    #[test]
    fn the_result_is_independent_of_edge_order() {
        let nodes = vec![node(1, true), node(2, true), node(3, false), node(4, true)];
        let edges = vec![(1, 2), (2, 3), (4, 1)];
        let forward = compute(&nodes, &edges);
        let mut reversed = edges;
        reversed.reverse();
        let backward = compute(&nodes, &reversed);
        assert_eq!(forward.exportable, backward.exportable);
    }

    #[test]
    fn the_result_is_independent_of_node_order() {
        let nodes = vec![node(1, true), node(2, true), node(3, false)];
        let forward = compute(&nodes, &[(1, 2), (2, 3)]);
        let backward = compute(
            &nodes.iter().rev().cloned().collect::<Vec<_>>(),
            &[(1, 2), (2, 3)],
        );
        assert_eq!(forward.exportable, backward.exportable);
    }

    #[test]
    fn the_cut_is_maximal() {
        // Removing anything else would be wrong: 4 and 5 are independent.
        let nodes = vec![
            node(1, true),
            node(2, false),
            node(3, true),
            node(4, true),
            node(5, true),
        ];
        let result = compute(&nodes, &[(1, 2), (3, 1), (4, 5)]);
        assert_eq!(ids(&result), vec![4, 5]);
    }

    #[test]
    fn self_loops_are_ignored() {
        let nodes = vec![node(1, true)];
        let result = compute(&nodes, &[(1, 1)]);
        assert_eq!(ids(&result), vec![1]);
    }

    #[test]
    fn an_edge_to_an_unknown_word_holds_the_dependent_back() {
        let nodes = vec![node(1, true)];
        let result = compute(&nodes, &[(1, 999)]);
        assert!(ids(&result).is_empty());
        let entry = holdback(&result, 1);
        assert_eq!(entry.root_cause, DEPENDENCY_HOLDBACK);
        assert_eq!(entry.blocking_word_id, Some(999));
    }

    #[test]
    fn impact_counts_only_otherwise_shippable_words() {
        // 1 and 2 are shippable and both need 3; 4 is broken on its own and
        // therefore was never going to ship regardless of 3.
        let nodes = vec![node(1, true), node(2, true), node(3, false), node(4, false)];
        let result = compute(&nodes, &[(1, 3), (2, 3), (4, 3)]);
        assert_eq!(holdback(&result, 3).impact_count, 2);
    }

    #[test]
    fn the_report_is_sorted_by_impact_then_id() {
        let nodes = vec![
            node(1, true),
            node(2, true),
            node(3, false),
            node(10, false),
            node(11, false),
        ];
        let result = compute(&nodes, &[(1, 3), (2, 3)]);
        assert_eq!(result.excluded[0].word_id, 3);
        let tail: Vec<i64> = result.excluded[1..].iter().map(|h| h.word_id).collect();
        let mut sorted = tail.clone();
        sorted.sort_unstable();
        assert_eq!(tail, sorted, "equal impact breaks on word id");
    }

    #[test]
    fn an_empty_lexicon_produces_an_empty_release() {
        let result = compute(&[], &[]);
        assert!(result.exportable.is_empty());
        assert!(result.excluded.is_empty());
        assert_eq!(result.shippable_count, 0);
    }

    #[test]
    fn nothing_shippable_is_a_valid_empty_release_with_a_full_report() {
        let nodes = vec![node(1, false), node(2, false)];
        let result = compute(&nodes, &[]);
        assert!(result.exportable.is_empty());
        assert_eq!(result.excluded.len(), 2);
        assert!(result
            .excluded
            .iter()
            .all(|h| h.root_cause == "missing_image"));
        assert!(result
            .excluded
            .iter()
            .all(|h| !h.root_cause_detail.is_empty()));
    }

    #[test]
    fn every_excluded_word_is_explained() {
        let nodes = vec![node(1, true), node(2, false), node(3, true), node(4, false)];
        let result = compute(&nodes, &[(1, 2), (3, 4)]);
        let explained: HashSet<i64> = result.excluded.iter().map(|h| h.word_id).collect();
        for node in &nodes {
            if !result.exportable.contains(&node.word_id) {
                assert!(
                    explained.contains(&node.word_id),
                    "word {} unexplained",
                    node.word_id
                );
            }
        }
    }
}
