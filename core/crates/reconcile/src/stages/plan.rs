//! Plan rebuild.
//!
//! `input_hash = blake3(algo_ver ‖ params ‖ ordered active ids ‖ ordered edge
//! set ‖ grouping features)` (README Part 3 §"派生 · 学习计划"). The stage
//! recomputes that hash from live state and compares it with the current
//! artifact's — no flag, no propagation, just a comparison. Equal means the
//! plan is current and nothing happens.
//!
//! The grouping features are part of the hash because the semantic and
//! root-based stages change where the cuts fall: installing WordNet must
//! produce a new plan, not silently keep the old one.

use morpho_domain::event::Actor;
use morpho_domain::hash::HashInput;
use morpho_domain::version::{PLAN_ALGO_VER, PLAN_INPUT_ALGO_VER};
use morpho_store::error::Result;
use morpho_store::ops::{PlanGroupRow, PlanWordRow, WritePlan};
use morpho_store::{queries, Store, WriteOp, WriteResult};

use crate::engine::EngineContext;
use crate::graph::{self, GraphNode, PlanInput, Signals};

/// Rebuild the plan if its inputs moved. Returns whether a new artifact landed.
pub async fn build_plan(store: &Store, context: &EngineContext) -> Result<bool> {
    let input = collect_input(store, context).await?;
    let hash = plan_input_hash(&input);

    let current = store.read(queries::current_plan).await?;
    if current.as_ref().is_some_and(|plan| plan.input_hash == hash) {
        return Ok(false);
    }

    let built = graph::build_plan(&input);
    let params_json = serde_json::to_string(&input.params).unwrap_or_else(|_| "{}".to_string());
    let stats_json = serde_json::to_string(&built.stats).unwrap_or_else(|_| "{}".to_string());

    let outcome = store
        .write(
            Actor::Reconciler,
            WriteOp::WritePlan(WritePlan {
                input_hash: hash,
                algo_ver: PLAN_ALGO_VER.to_string(),
                params_json,
                stats_json,
                groups: built
                    .groups
                    .iter()
                    .map(|group| PlanGroupRow {
                        group_seq: group.group_seq,
                        group_type: group.group_type.as_str().to_string(),
                    })
                    .collect(),
                words: built
                    .words
                    .iter()
                    .map(|word| PlanWordRow {
                        word_id: word.word_id,
                        learning_order: word.learning_order,
                        group_seq: word.group_seq,
                    })
                    .collect(),
            }),
        )
        .await?;

    Ok(matches!(
        outcome.result,
        WriteResult::Plan { applied: true, .. }
    ))
}

/// Gather the plan's inputs from one read snapshot.
pub async fn collect_input(store: &Store, context: &EngineContext) -> Result<PlanInput> {
    let params = context.plan;
    let wordnet = context.sources.wordnet.clone();

    store
        .read(move |conn| {
            let words = queries::active_words(conn)?;
            let edges = queries::dependency_edges(conn)?;

            let mut signals = std::collections::HashMap::new();
            for word in &words {
                let root = word
                    .etymology
                    .as_deref()
                    .and_then(root_of)
                    .map(|root| root.to_string());
                // Skipped entirely when WordNet is not installed, rather than
                // approximated by something that is not semantics.
                let semantic = wordnet.as_ref().and_then(|db| db.cluster_key(&word.lemma));
                if root.is_some() || semantic.is_some() {
                    signals.insert(word.word_id, Signals { root, semantic });
                }
            }

            Ok(PlanInput {
                nodes: words
                    .iter()
                    .map(|word| GraphNode {
                        word_id: word.word_id,
                        frequency_rank: word.frequency_rank,
                    })
                    .collect(),
                edges,
                signals,
                params,
            })
        })
        .await
}

/// The first morph of a Morfessor segmentation, used as a shared-root key.
///
/// Only segmented etymologies carry a usable root: Wiktionary prose is a
/// sentence, not a decomposition, so it contributes no grouping signal.
fn root_of(etymology: &str) -> Option<&str> {
    if !etymology.contains(" + ") {
        return None;
    }
    let first = etymology.split(" + ").next()?.trim();
    // A single letter is a segmentation artifact, not a morpheme; real
    // prefixes ("un-", "re-", "in-") start at two.
    (first.chars().count() >= 2).then_some(first)
}

/// `plan_artifacts.input_hash`.
pub fn plan_input_hash(input: &PlanInput) -> String {
    let mut hasher = HashInput::new(PLAN_INPUT_ALGO_VER)
        .field(PLAN_ALGO_VER)
        .field(input.params.group_min.to_string())
        .field(input.params.group_max.to_string());

    // Ordered node set. Sorting here rather than trusting the caller makes the
    // hash a function of the state, not of the query plan.
    let mut nodes: Vec<(i64, i64)> = input
        .nodes
        .iter()
        .map(|node| (node.frequency_rank.unwrap_or(i64::MAX), node.word_id))
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    hasher = hasher.field(nodes.len().to_string());
    for (rank, word_id) in &nodes {
        hasher = hasher.field(format!("{word_id}:{rank}"));
    }

    // Ordered edge set.
    let mut edges: Vec<(i64, i64)> = input.edges.clone();
    edges.sort_unstable();
    edges.dedup();
    hasher = hasher.field(edges.len().to_string());
    for (from, to) in &edges {
        hasher = hasher.field(format!("{from}>{to}"));
    }

    // Grouping features, ordered by word id.
    let mut signals: Vec<(i64, String, String)> = input
        .signals
        .iter()
        .map(|(word_id, signal)| {
            (
                *word_id,
                signal.root.clone().unwrap_or_default(),
                signal.semantic.clone().unwrap_or_default(),
            )
        })
        .filter(|(_, root, semantic)| !(root.is_empty() && semantic.is_empty()))
        .collect();
    signals.sort();
    hasher = hasher.field(signals.len().to_string());
    for (word_id, root, semantic) in &signals {
        hasher = hasher.field(format!("{word_id}|{root}|{semantic}"));
    }

    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::PlanParams;
    use std::collections::HashMap;

    fn input() -> PlanInput {
        PlanInput {
            nodes: vec![
                GraphNode {
                    word_id: 1,
                    frequency_rank: Some(10),
                },
                GraphNode {
                    word_id: 2,
                    frequency_rank: Some(20),
                },
            ],
            edges: vec![(1, 2)],
            signals: HashMap::new(),
            params: PlanParams::default(),
        }
    }

    #[test]
    fn the_hash_is_stable_for_identical_input() {
        assert_eq!(plan_input_hash(&input()), plan_input_hash(&input()));
    }

    #[test]
    fn the_hash_ignores_input_ordering() {
        let mut shuffled = input();
        shuffled.nodes.reverse();
        assert_eq!(plan_input_hash(&input()), plan_input_hash(&shuffled));
    }

    #[test]
    fn adding_a_word_changes_the_hash() {
        let mut grown = input();
        grown.nodes.push(GraphNode {
            word_id: 3,
            frequency_rank: Some(30),
        });
        assert_ne!(plan_input_hash(&input()), plan_input_hash(&grown));
    }

    #[test]
    fn adding_an_edge_changes_the_hash() {
        let mut linked = input();
        linked.edges.push((2, 1));
        assert_ne!(plan_input_hash(&input()), plan_input_hash(&linked));
    }

    #[test]
    fn changing_a_frequency_rank_changes_the_hash() {
        let mut reranked = input();
        reranked.nodes[0].frequency_rank = Some(99);
        assert_ne!(plan_input_hash(&input()), plan_input_hash(&reranked));
    }

    #[test]
    fn changing_the_group_window_changes_the_hash() {
        let mut retuned = input();
        retuned.params.group_max = 25;
        assert_ne!(plan_input_hash(&input()), plan_input_hash(&retuned));
    }

    #[test]
    fn gaining_a_semantic_signal_changes_the_hash() {
        // Installing WordNet must produce a new plan, not keep the old one.
        let mut enriched = input();
        enriched.signals.insert(
            1,
            Signals {
                root: None,
                semantic: Some("adj:00002098".to_string()),
            },
        );
        assert_ne!(plan_input_hash(&input()), plan_input_hash(&enriched));
    }

    #[test]
    fn empty_signals_do_not_disturb_the_hash() {
        let mut padded = input();
        padded.signals.insert(1, Signals::default());
        assert_eq!(plan_input_hash(&input()), plan_input_hash(&padded));
    }

    #[test]
    fn duplicate_nodes_and_edges_do_not_disturb_the_hash() {
        let mut doubled = input();
        doubled.nodes.push(doubled.nodes[0]);
        doubled.edges.push(doubled.edges[0]);
        assert_eq!(plan_input_hash(&input()), plan_input_hash(&doubled));
    }

    #[test]
    fn only_segmented_etymologies_yield_a_root() {
        assert_eq!(root_of("bene + volent"), Some("bene"));
        assert_eq!(
            root_of("From Old French benevolent, borrowed from Latin"),
            None
        );
        assert_eq!(root_of("un + happi + ness"), Some("un"));
        // Too short to mean anything.
        assert_eq!(root_of("a + dapt"), None);
    }
}
