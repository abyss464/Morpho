//! The learning-plan graph: Tarjan SCC → condensation → topological order →
//! grouping (README Part 3 §"派生 · 学习计划").
//!
//! Everything here is pure and total. The whole point of the plan is that the
//! same working-database state produces byte-identical output, so every choice
//! that could be arbitrary is pinned:
//!
//! * nodes and adjacency lists are sorted by `(frequency_rank, word_id)` before
//!   anything runs, so Tarjan visits them in a fixed order;
//! * the condensation is linearised with Kahn's algorithm driven by a min-heap
//!   on the component's sort key, which makes the topological order canonical
//!   rather than an artifact of component discovery order;
//! * ties inside a component break on the same key.
//!
//! An edge `a → b` means "a's selected definition uses b", so b must be learned
//! first. The emitted order therefore places every b before its a, except
//! inside a cycle — which is exactly what an SCC pack is for.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use serde::{Deserialize, Serialize};

/// One word in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphNode {
    pub word_id: i64,
    pub frequency_rank: Option<i64>,
}

impl GraphNode {
    /// Canonical sort key: commoner words first, ties by id.
    fn key(&self) -> (i64, i64) {
        (self.frequency_rank.unwrap_or(i64::MAX), self.word_id)
    }
}

/// Grouping knobs (README: 15–20 words per group).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlanParams {
    pub group_min: usize,
    pub group_max: usize,
}

impl Default for PlanParams {
    fn default() -> Self {
        Self {
            group_min: 15,
            group_max: 20,
        }
    }
}

impl PlanParams {
    fn sane(self) -> Self {
        let group_max = self.group_max.max(1);
        Self {
            group_min: self.group_min.clamp(1, group_max),
            group_max,
        }
    }
}

/// Why a group exists (`plan_groups.group_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupType {
    /// A cycle of mutually-dependent words, learned together.
    Scc,
    /// Words sharing a morphological root.
    Root,
    /// Words in the same WordNet semantic cluster.
    Semantic,
    /// Plain sequential fill.
    Fill,
}

impl GroupType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scc => "scc",
            Self::Root => "root",
            Self::Semantic => "semantic",
            Self::Fill => "fill",
        }
    }
}

/// Optional clustering signals for one word.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Signals {
    /// Morphological root, from the etymology segmentation.
    pub root: Option<String>,
    /// WordNet semantic cluster id. `None` when `wordnet_dir` is unset — the
    /// semantic stage is then skipped entirely rather than faked.
    pub semantic: Option<String>,
}

/// Everything the builder needs.
#[derive(Debug, Clone, Default)]
pub struct PlanInput {
    pub nodes: Vec<GraphNode>,
    /// `(dependent, dependency)` — the dependency must come first.
    pub edges: Vec<(i64, i64)>,
    pub signals: HashMap<i64, Signals>,
    pub params: PlanParams,
}

/// One word's placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedWord {
    pub word_id: i64,
    pub learning_order: i64,
    pub group_seq: i64,
}

/// One group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedGroup {
    pub group_seq: i64,
    pub group_type: GroupType,
    pub word_ids: Vec<i64>,
}

/// `plan_artifacts.stats_json`, and the `PlanStats` the console renders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanStats {
    pub word_count: usize,
    pub group_count: usize,
    pub edge_count: usize,
    pub scc_group_count: usize,
    pub largest_group: usize,
    pub avg_group_size: f64,
}

/// A complete plan.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltPlan {
    pub words: Vec<PlannedWord>,
    pub groups: Vec<PlannedGroup>,
    pub stats: PlanStats,
}

/// Strongly connected components, in Tarjan discovery order.
///
/// Iterative, so a 6 000-node chain cannot blow the stack.
pub fn tarjan_scc(nodes: &[GraphNode], adjacency: &HashMap<i64, Vec<i64>>) -> Vec<Vec<i64>> {
    #[derive(Default)]
    struct State {
        index: usize,
        indices: HashMap<i64, usize>,
        low: HashMap<i64, usize>,
        on_stack: HashMap<i64, bool>,
        stack: Vec<i64>,
        components: Vec<Vec<i64>>,
    }

    let mut state = State::default();
    let empty: Vec<i64> = Vec::new();

    for root in nodes {
        let root = root.word_id;
        if state.indices.contains_key(&root) {
            continue;
        }
        // (node, position in its adjacency list)
        let mut call_stack: Vec<(i64, usize)> = vec![(root, 0)];
        state.indices.insert(root, state.index);
        state.low.insert(root, state.index);
        state.index += 1;
        state.stack.push(root);
        state.on_stack.insert(root, true);

        while let Some((node, cursor)) = call_stack.pop() {
            let neighbors = adjacency.get(&node).unwrap_or(&empty);
            if cursor < neighbors.len() {
                let next = neighbors[cursor];
                call_stack.push((node, cursor + 1));
                if !state.indices.contains_key(&next) {
                    state.indices.insert(next, state.index);
                    state.low.insert(next, state.index);
                    state.index += 1;
                    state.stack.push(next);
                    state.on_stack.insert(next, true);
                    call_stack.push((next, 0));
                } else if *state.on_stack.get(&next).unwrap_or(&false) {
                    let candidate = state.indices[&next];
                    let current = state.low[&node];
                    state.low.insert(node, current.min(candidate));
                }
                continue;
            }

            // Finished `node`: fold its low-link into its parent, then close a
            // component if it is a root.
            if let Some((parent, _)) = call_stack.last().copied() {
                let child_low = state.low[&node];
                let parent_low = state.low[&parent];
                state.low.insert(parent, parent_low.min(child_low));
            }
            if state.low[&node] == state.indices[&node] {
                let mut component = Vec::new();
                while let Some(top) = state.stack.pop() {
                    state.on_stack.insert(top, false);
                    component.push(top);
                    if top == node {
                        break;
                    }
                }
                state.components.push(component);
            }
        }
    }

    state.components
}

/// Build a complete plan from the graph.
pub fn build_plan(input: &PlanInput) -> BuiltPlan {
    let params = input.params.sane();

    let mut nodes = input.nodes.clone();
    nodes.sort_by_key(GraphNode::key);
    nodes.dedup_by_key(|n| n.word_id);
    let node_key: HashMap<i64, (i64, i64)> = nodes.iter().map(|n| (n.word_id, n.key())).collect();

    // Keep only edges whose both ends are in the node set, drop self-loops, and
    // sort so adjacency iteration order is fixed.
    let mut edges: Vec<(i64, i64)> = input
        .edges
        .iter()
        .copied()
        .filter(|(from, to)| from != to && node_key.contains_key(from) && node_key.contains_key(to))
        .collect();
    edges.sort_by_key(|(from, to)| (node_key[from], node_key[to]));
    edges.dedup();

    let mut adjacency: HashMap<i64, Vec<i64>> = HashMap::new();
    for (from, to) in &edges {
        adjacency.entry(*from).or_default().push(*to);
    }

    let components = tarjan_scc(&nodes, &adjacency);

    // Canonicalize every component: members sorted, components keyed by their
    // strongest member.
    let mut component_of: HashMap<i64, usize> = HashMap::new();
    let mut members: Vec<Vec<i64>> = Vec::with_capacity(components.len());
    for component in components {
        let mut sorted = component;
        sorted.sort_by_key(|id| node_key[id]);
        let index = members.len();
        for id in &sorted {
            component_of.insert(*id, index);
        }
        members.push(sorted);
    }
    let component_key = |index: usize| -> (i64, i64) { node_key[&members[index][0]] };

    // Kahn over the condensation. An edge a → b (a depends on b) becomes
    // "b must be emitted before a", so b's component gets an outgoing edge.
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); members.len()];
    let mut indegree: Vec<usize> = vec![0; members.len()];
    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    for (from, to) in &edges {
        let (a, b) = (component_of[from], component_of[to]);
        if a == b || !seen.insert((b, a)) {
            continue;
        }
        successors[b].push(a);
        indegree[a] += 1;
    }

    let mut ready: BinaryHeap<Reverse<((i64, i64), usize)>> = BinaryHeap::new();
    for (index, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            ready.push(Reverse((component_key(index), index)));
        }
    }
    let mut order: Vec<usize> = Vec::with_capacity(members.len());
    while let Some(Reverse((_, index))) = ready.pop() {
        order.push(index);
        let mut next = successors[index].clone();
        next.sort_by_key(|c| component_key(*c));
        for successor in next {
            indegree[successor] -= 1;
            if indegree[successor] == 0 {
                ready.push(Reverse((component_key(successor), successor)));
            }
        }
    }
    // A well-formed condensation is acyclic, so this cannot fire; if a future
    // change breaks that, emit the stragglers rather than silently losing them.
    if order.len() < members.len() {
        let mut missing: Vec<usize> = (0..members.len()).filter(|i| !order.contains(i)).collect();
        missing.sort_by_key(|c| component_key(*c));
        tracing::error!(
            missing = missing.len(),
            "condensation was not acyclic; appending the remainder"
        );
        order.extend(missing);
    }

    // Group the linear sequence of components. Components are atomic: a cut can
    // only ever fall between them, which is what keeps a cycle intact.
    let default_signals = Signals::default();
    let signals = |word_id: &i64| input.signals.get(word_id).unwrap_or(&default_signals);

    let mut groups: Vec<PlannedGroup> = Vec::new();
    let mut current: Vec<i64> = Vec::new();

    let flush = |current: &mut Vec<i64>, groups: &mut Vec<PlannedGroup>| {
        if current.is_empty() {
            return;
        }
        let word_ids = std::mem::take(current);
        let group_type = classify(&word_ids, &signals);
        groups.push(PlannedGroup {
            group_seq: groups.len() as i64 + 1,
            group_type,
            word_ids,
        });
    };

    for index in order {
        let component = &members[index];
        if component.len() > 1 {
            // Stage 1: an SCC pack is its own group, always.
            flush(&mut current, &mut groups);
            groups.push(PlannedGroup {
                group_seq: groups.len() as i64 + 1,
                group_type: GroupType::Scc,
                word_ids: component.clone(),
            });
            continue;
        }

        let word_id = component[0];
        if current.len() + 1 > params.group_max {
            flush(&mut current, &mut groups);
        } else if current.len() >= params.group_min && cluster_breaks(&current, word_id, &signals) {
            // Stages 2 and 3: past the minimum size, cut where the shared root
            // or the semantic cluster the group is built around changes. A
            // group with no cluster of its own just fills to the maximum.
            flush(&mut current, &mut groups);
        }
        current.push(word_id);
    }
    flush(&mut current, &mut groups);

    let mut words = Vec::with_capacity(nodes.len());
    let mut learning_order = 0i64;
    for group in &groups {
        for word_id in &group.word_ids {
            learning_order += 1;
            words.push(PlannedWord {
                word_id: *word_id,
                learning_order,
                group_seq: group.group_seq,
            });
        }
    }

    let stats = PlanStats {
        word_count: words.len(),
        group_count: groups.len(),
        edge_count: edges.len(),
        scc_group_count: groups
            .iter()
            .filter(|g| g.group_type == GroupType::Scc)
            .count(),
        largest_group: groups.iter().map(|g| g.word_ids.len()).max().unwrap_or(0),
        avg_group_size: if groups.is_empty() {
            0.0
        } else {
            words.len() as f64 / groups.len() as f64
        },
    };

    BuiltPlan {
        words,
        groups,
        stats,
    }
}

/// The cluster the current group is built around, if it has one.
///
/// Roots win over semantics: a shared morphological root is a stronger reason
/// to learn words together than a shared WordNet ancestor, and README lists the
/// stages in that order.
fn cluster_of<'a>(current: &[i64], signals: &impl Fn(&i64) -> &'a Signals) -> Option<ClusterKey> {
    let first = signals(&current[0]);
    if let Some(root) = &first.root {
        if current
            .iter()
            .all(|id| signals(id).root.as_ref() == Some(root))
        {
            return Some(ClusterKey::Root(root.clone()));
        }
    }
    if let Some(semantic) = &first.semantic {
        if current
            .iter()
            .all(|id| signals(id).semantic.as_ref() == Some(semantic))
        {
            return Some(ClusterKey::Semantic(semantic.clone()));
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClusterKey {
    Root(String),
    Semantic(String),
}

/// Should the group be cut before `word_id`?
///
/// Only a group that *has* a cluster can be broken by one. An unclustered group
/// is plain sequential fill and runs to `group_max`.
fn cluster_breaks<'a>(
    current: &[i64],
    word_id: i64,
    signals: &impl Fn(&i64) -> &'a Signals,
) -> bool {
    let Some(key) = cluster_of(current, signals) else {
        return false;
    };
    let incoming = signals(&word_id);
    match key {
        ClusterKey::Root(root) => incoming.root.as_deref() != Some(root.as_str()),
        ClusterKey::Semantic(semantic) => incoming.semantic.as_deref() != Some(semantic.as_str()),
    }
}

fn classify<'a>(word_ids: &[i64], signals: &impl Fn(&i64) -> &'a Signals) -> GroupType {
    match cluster_of(word_ids, signals) {
        Some(ClusterKey::Root(_)) => GroupType::Root,
        Some(ClusterKey::Semantic(_)) => GroupType::Semantic,
        None => GroupType::Fill,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(word_id: i64, rank: i64) -> GraphNode {
        GraphNode {
            word_id,
            frequency_rank: Some(rank),
        }
    }

    fn plan(nodes: Vec<GraphNode>, edges: Vec<(i64, i64)>) -> BuiltPlan {
        build_plan(&PlanInput {
            nodes,
            edges,
            signals: HashMap::new(),
            params: PlanParams::default(),
        })
    }

    fn order_of(plan: &BuiltPlan, word_id: i64) -> i64 {
        plan.words
            .iter()
            .find(|w| w.word_id == word_id)
            .map(|w| w.learning_order)
            .unwrap_or_else(|| panic!("word {word_id} missing from the plan"))
    }

    #[test]
    fn empty_input_yields_an_empty_plan() {
        let plan = plan(Vec::new(), Vec::new());
        assert!(plan.words.is_empty());
        assert!(plan.groups.is_empty());
        assert_eq!(plan.stats.avg_group_size, 0.0);
    }

    #[test]
    fn dependencies_are_learned_first() {
        // benevolent(1) needs generous(2); generous needs kind(3).
        let plan = plan(
            vec![node(1, 4312), node(2, 2180), node(3, 900)],
            vec![(1, 2), (2, 3)],
        );
        assert!(order_of(&plan, 3) < order_of(&plan, 2));
        assert!(order_of(&plan, 2) < order_of(&plan, 1));
    }

    #[test]
    fn learning_order_is_dense_and_starts_at_one() {
        let plan = plan((1..=40).map(|i| node(i, i * 10)).collect(), Vec::new());
        let mut orders: Vec<i64> = plan.words.iter().map(|w| w.learning_order).collect();
        orders.sort_unstable();
        assert_eq!(orders, (1..=40).collect::<Vec<_>>());
    }

    #[test]
    fn a_cycle_becomes_one_scc_group() {
        // lucid ↔ coherent, plus an unrelated word.
        let plan = plan(
            vec![node(1, 4471), node(2, 3688), node(3, 100)],
            vec![(1, 2), (2, 1)],
        );
        let scc: Vec<&PlannedGroup> = plan
            .groups
            .iter()
            .filter(|g| g.group_type == GroupType::Scc)
            .collect();
        assert_eq!(scc.len(), 1);
        assert_eq!(scc[0].word_ids, vec![2, 1], "sorted by frequency rank");
        assert_eq!(plan.stats.scc_group_count, 1);
    }

    #[test]
    fn a_three_cycle_stays_together() {
        let plan = plan(
            vec![node(1, 10), node(2, 20), node(3, 30)],
            vec![(1, 2), (2, 3), (3, 1)],
        );
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].group_type, GroupType::Scc);
        assert_eq!(plan.groups[0].word_ids, vec![1, 2, 3]);
    }

    #[test]
    fn every_dependency_edge_respects_the_order_or_shares_a_group() {
        let nodes: Vec<GraphNode> = (1..=12).map(|i| node(i, (13 - i) * 100)).collect();
        let edges = vec![
            (1, 2),
            (2, 3),
            (3, 1),
            (4, 1),
            (5, 4),
            (6, 5),
            (7, 2),
            (8, 9),
        ];
        let plan = plan(nodes, edges.clone());
        let group_of: HashMap<i64, i64> = plan
            .words
            .iter()
            .map(|w| (w.word_id, w.group_seq))
            .collect();
        for (from, to) in edges {
            let ok =
                order_of(&plan, to) < order_of(&plan, from) || group_of[&to] == group_of[&from];
            assert!(ok, "edge {from} -> {to} breaks the invariant");
        }
    }

    #[test]
    fn output_is_independent_of_input_ordering() {
        let nodes: Vec<GraphNode> = (1..=30).map(|i| node(i, i * 7 % 31)).collect();
        let edges = vec![(3, 1), (4, 3), (5, 4), (9, 8), (8, 9), (12, 5), (20, 12)];

        let forward = build_plan(&PlanInput {
            nodes: nodes.clone(),
            edges: edges.clone(),
            signals: HashMap::new(),
            params: PlanParams::default(),
        });
        let reversed = build_plan(&PlanInput {
            nodes: nodes.into_iter().rev().collect(),
            edges: edges.into_iter().rev().collect(),
            signals: HashMap::new(),
            params: PlanParams::default(),
        });
        assert_eq!(forward, reversed);
    }

    #[test]
    fn duplicate_nodes_and_edges_collapse() {
        let plan = plan(
            vec![node(1, 10), node(1, 10), node(2, 20)],
            vec![(1, 2), (1, 2), (1, 1)],
        );
        assert_eq!(plan.words.len(), 2);
        assert_eq!(plan.stats.edge_count, 1, "self-loop and duplicate dropped");
    }

    #[test]
    fn edges_to_unknown_words_are_ignored() {
        let plan = plan(vec![node(1, 10)], vec![(1, 999), (999, 1)]);
        assert_eq!(plan.words.len(), 1);
        assert_eq!(plan.stats.edge_count, 0);
    }

    #[test]
    fn fill_groups_respect_the_size_window() {
        let plan = plan((1..=57).map(|i| node(i, i)).collect(), Vec::new());
        assert_eq!(plan.words.len(), 57);
        // 57 = 20 + 20 + 17 with the default 15..20 window.
        let sizes: Vec<usize> = plan.groups.iter().map(|g| g.word_ids.len()).collect();
        assert_eq!(sizes, vec![20, 20, 17]);
        assert!(plan.groups.iter().all(|g| g.group_type == GroupType::Fill));
    }

    #[test]
    fn shared_roots_cut_the_group_where_the_root_changes() {
        let mut signals = HashMap::new();
        for i in 1..=16 {
            signals.insert(
                i,
                Signals {
                    root: Some("bene".to_string()),
                    semantic: None,
                },
            );
        }
        for i in 17..=30 {
            signals.insert(
                i,
                Signals {
                    root: Some("mal".to_string()),
                    semantic: None,
                },
            );
        }
        let built = build_plan(&PlanInput {
            nodes: (1..=30).map(|i| node(i, i)).collect(),
            edges: Vec::new(),
            signals,
            params: PlanParams::default(),
        });
        // The cut falls exactly where the root changes, not at the size window.
        assert_eq!(built.groups[0].word_ids.len(), 16);
        assert_eq!(built.groups[0].group_type, GroupType::Root);
        assert_eq!(built.groups[1].word_ids.len(), 14);
        assert_eq!(built.groups[1].group_type, GroupType::Root);
    }

    #[test]
    fn semantic_clusters_are_used_when_no_root_matches() {
        let mut signals = HashMap::new();
        for i in 1..=18 {
            signals.insert(
                i,
                Signals {
                    root: None,
                    semantic: Some("calm".to_string()),
                },
            );
        }
        for i in 19..=32 {
            signals.insert(
                i,
                Signals {
                    root: None,
                    semantic: Some("bright".to_string()),
                },
            );
        }
        let built = build_plan(&PlanInput {
            nodes: (1..=32).map(|i| node(i, i)).collect(),
            edges: Vec::new(),
            signals,
            params: PlanParams::default(),
        });
        assert_eq!(built.groups[0].group_type, GroupType::Semantic);
        assert_eq!(built.groups[0].word_ids.len(), 18);
        assert_eq!(built.groups[1].group_type, GroupType::Semantic);
    }

    #[test]
    fn an_scc_never_gets_split_by_the_size_window() {
        let big: Vec<(i64, i64)> = (1..=25)
            .map(|i| (i, if i == 25 { 1 } else { i + 1 }))
            .collect();
        let built = plan((1..=25).map(|i| node(i, i)).collect(), big);
        assert_eq!(built.groups.len(), 1);
        assert_eq!(built.groups[0].word_ids.len(), 25);
        assert_eq!(built.groups[0].group_type, GroupType::Scc);
    }

    #[test]
    fn group_seq_is_one_based_and_contiguous() {
        let built = plan((1..=60).map(|i| node(i, i)).collect(), Vec::new());
        let seqs: Vec<i64> = built.groups.iter().map(|g| g.group_seq).collect();
        assert_eq!(seqs, (1..=built.groups.len() as i64).collect::<Vec<_>>());
        for word in &built.words {
            assert!(seqs.contains(&word.group_seq));
        }
    }

    #[test]
    fn unranked_words_sort_after_ranked_ones() {
        let built = plan(
            vec![
                GraphNode {
                    word_id: 1,
                    frequency_rank: None,
                },
                node(2, 5000),
            ],
            Vec::new(),
        );
        assert!(order_of(&built, 2) < order_of(&built, 1));
    }

    #[test]
    fn tarjan_handles_a_long_chain_without_recursing() {
        let nodes: Vec<GraphNode> = (1..=20_000).map(|i| node(i, i)).collect();
        let mut adjacency: HashMap<i64, Vec<i64>> = HashMap::new();
        for i in 1..20_000 {
            adjacency.insert(i, vec![i + 1]);
        }
        let components = tarjan_scc(&nodes, &adjacency);
        assert_eq!(components.len(), 20_000);
    }

    #[test]
    fn stats_describe_the_result() {
        let built = plan((1..=45).map(|i| node(i, i)).collect(), vec![(2, 1), (3, 1)]);
        assert_eq!(built.stats.word_count, 45);
        assert_eq!(built.stats.edge_count, 2);
        assert_eq!(built.stats.group_count, built.groups.len());
        assert_eq!(
            built.stats.largest_group,
            built.groups.iter().map(|g| g.word_ids.len()).max().unwrap()
        );
        assert!((built.stats.avg_group_size - 45.0 / built.groups.len() as f64).abs() < 1e-9);
    }

    #[test]
    fn degenerate_params_are_clamped_rather_than_dividing_by_zero() {
        let built = build_plan(&PlanInput {
            nodes: (1..=5).map(|i| node(i, i)).collect(),
            edges: Vec::new(),
            signals: HashMap::new(),
            params: PlanParams {
                group_min: 40,
                group_max: 0,
            },
        });
        assert_eq!(built.words.len(), 5);
        assert!(built.groups.iter().all(|g| g.word_ids.len() == 1));
    }
}
