//! Damerau-Levenshtein distance, used to pick confusable distractors.
//!
//! This is the *unrestricted* variant (with an alphabet table), not the
//! restricted "optimal string alignment" approximation: OSA charges 2 for
//! `adept → adapt → adopt`-style transposition chains where a real
//! Damerau-Levenshtein charges 1, and confusable-word selection is exactly the
//! place where that difference bites.

use std::collections::HashMap;

/// Edit distance allowing insertion, deletion, substitution and transposition
/// of adjacent characters.
///
/// Operates on Unicode scalar values, so a multi-byte character counts once.
pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let source: Vec<char> = a.chars().collect();
    let target: Vec<char> = b.chars().collect();
    let (n, m) = (source.len(), target.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }

    let max_distance = n + m;
    let width = m + 2;
    let mut d = vec![0usize; (n + 2) * width];
    let at = |i: usize, j: usize| i * width + j;

    d[at(0, 0)] = max_distance;
    for i in 0..=n {
        d[at(i + 1, 0)] = max_distance;
        d[at(i + 1, 1)] = i;
    }
    for j in 0..=m {
        d[at(0, j + 1)] = max_distance;
        d[at(1, j + 1)] = j;
    }

    // Last row in which each character of `a` was seen.
    let mut last_row: HashMap<char, usize> = HashMap::new();

    for i in 1..=n {
        // Last column in `b` matching source[i-1].
        let mut last_match_col = 0usize;
        for j in 1..=m {
            let i1 = *last_row.get(&target[j - 1]).unwrap_or(&0);
            let j1 = last_match_col;
            let cost = usize::from(source[i - 1] != target[j - 1]);
            if cost == 0 {
                last_match_col = j;
            }
            d[at(i + 1, j + 1)] = (d[at(i, j)] + cost)
                .min(d[at(i + 1, j)] + 1)
                .min(d[at(i, j + 1)] + 1)
                .min(d[at(i1, j1)] + (i - i1 - 1) + 1 + (j - j1 - 1));
        }
        last_row.insert(source[i - 1], i);
    }

    d[at(n + 1, m + 1)]
}

/// Case-insensitive distance, which is what lemma comparison wants.
pub fn lemma_distance(a: &str, b: &str) -> usize {
    damerau_levenshtein(&a.to_lowercase(), &b.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings_are_zero_apart() {
        assert_eq!(damerau_levenshtein("adapt", "adapt"), 0);
        assert_eq!(damerau_levenshtein("", ""), 0);
    }

    #[test]
    fn empty_string_costs_the_other_length() {
        assert_eq!(damerau_levenshtein("", "serene"), 6);
        assert_eq!(damerau_levenshtein("serene", ""), 6);
    }

    #[test]
    fn single_edits_cost_one() {
        assert_eq!(damerau_levenshtein("adapt", "adopt"), 1); // substitution
        assert_eq!(damerau_levenshtein("adapt", "adap"), 1); // deletion
        assert_eq!(damerau_levenshtein("adap", "adapt"), 1); // insertion
        assert_eq!(damerau_levenshtein("adapt", "adatp"), 1); // transposition
    }

    #[test]
    fn the_confusable_trio_is_one_apart_pairwise() {
        assert_eq!(lemma_distance("adapt", "adopt"), 1);
        assert_eq!(lemma_distance("adapt", "adept"), 1);
        assert_eq!(lemma_distance("adopt", "adept"), 1);
    }

    #[test]
    fn unrestricted_variant_beats_optimal_string_alignment() {
        // OSA reports 3 for this classic case; true Damerau-Levenshtein is 2.
        assert_eq!(damerau_levenshtein("ca", "abc"), 2);
    }

    #[test]
    fn is_symmetric() {
        for (a, b) in [
            ("benevolent", "malevolent"),
            ("serene", "serenity"),
            ("", "x"),
            ("copious", "capacious"),
        ] {
            assert_eq!(
                damerau_levenshtein(a, b),
                damerau_levenshtein(b, a),
                "{a} vs {b}"
            );
        }
    }

    #[test]
    fn respects_the_triangle_inequality_on_samples() {
        let words = ["adapt", "adopt", "adept", "adapter", "opt", "apt"];
        for a in words {
            for b in words {
                for c in words {
                    assert!(
                        damerau_levenshtein(a, c)
                            <= damerau_levenshtein(a, b) + damerau_levenshtein(b, c),
                        "{a} {b} {c}"
                    );
                }
            }
        }
    }

    #[test]
    fn counts_characters_not_bytes() {
        // "é" is two bytes but one character.
        assert_eq!(damerau_levenshtein("café", "cafe"), 1);
        assert_eq!(damerau_levenshtein("café", "café"), 0);
    }

    #[test]
    fn case_folding_is_opt_in() {
        assert_eq!(damerau_levenshtein("Adapt", "adapt"), 1);
        assert_eq!(lemma_distance("Adapt", "adapt"), 0);
    }
}
