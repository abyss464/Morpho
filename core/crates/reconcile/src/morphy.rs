//! WordNet-style morphological analysis, validated against the lexicon.
//!
//! This is `morphy` (WNdb §morph(7)) with one deliberate change and one
//! deliberate omission.
//!
//! **The change**: every candidate lemma must exist in [`Lexicon`]. WordNet
//! validates against its own index; Morpho validates against the `words` table,
//! because the only question the tokenizer is asking is "can I point the reader
//! at a word they will actually meet?". A detachment that yields a non-word is
//! discarded and the surface form survives, so the token stays honestly out of
//! scope instead of inventing a dependency on something that does not exist.
//!
//! **The omission**: no part of speech. Morphy is called as
//! `morphstr(surface, pos)` and only applies that part of speech's rules;
//! a definition token arrives with no such tag. Every rule is therefore tried,
//! ordered longest suffix first, and lexicon validation is what stops the
//! nonsense — with a compiled-in list of protected surfaces for the handful of
//! high-frequency words where a wrong detachment would still land on a real
//! word (`number` → `numb`, `her` → `he`).
//!
//! Order of resolution, first hit wins:
//!
//!   1. the surface itself, if it is already a lemma — `living` is not analysed
//!      when `living` is a word;
//!   2. the exception list, which is also where a self-mapping stops the search;
//!   3. detachment rules, longest suffix first;
//!   4. consonant-doubling reversal for `-ing` / `-ed` (`running` → `run`);
//!   5. the surface, unchanged.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use morpho_domain::canon::fold_lemma;

use crate::lexicon::{Lexicon, LexiconCache};
use crate::text::Lemmatizer;

/// Version written to `def_extractions.lemmatizer_ver` with the compiled-in
/// exception table.
pub const MORPHY_LEMMATIZER_VER: &str = "morphy-lemmatizer/1";

/// Version used when the WNdb exception files were loaded. It is a distinct
/// string because the two configurations genuinely lemmatize differently, and
/// turning `wordnet_dir` on has to invalidate the extractions it changes.
pub const MORPHY_LEMMATIZER_WNDB_VER: &str = "morphy-lemmatizer-wndb/1";

/// The compiled-in fallback table, in WNdb exception-file format.
const BUILTIN_EXCEPTIONS: &str = include_str!("../data/irregular.exc");

/// WNdb exception files, in the order their entries are merged.
const EXC_FILES: [&str; 4] = ["noun.exc", "verb.exc", "adj.exc", "adv.exc"];

/// A detachment rule: strip the first string, append the second.
type Detach = (&'static str, &'static str);

/// Morphy's three rule tables, merged and re-ordered longest suffix first.
///
/// Merging is forced by the missing part of speech; the re-ordering is what
/// makes the merge safe. Morphy tries `s` → `` before `xes` → `x`, which is
/// harmless when only nouns are in play but would let `boxes` stop at a
/// hypothetical `boxe`. Longest first asks the most specific question first.
/// Within one length the original table order is preserved: noun, then verb,
/// then adjective.
///
/// Three rules differ from the WNdb tables, all because a part of speech is not
/// available to narrow them:
///
/// * `iest` → `y` and `ier` → `y` are added. Morphy reaches `easiest` through
///   `adj.exc`, which the compiled-in fallback cannot enumerate; under lexicon
///   validation the rule is safe;
/// * morphy's verb rule `es` → `` is replaced by `oes` → `o`. English attaches
///   `-es` only after `s`, `x`, `z`, `ch`, `sh`, `o` or a `y` that became `i`,
///   and every one of those has its own rule here — so the bare form adds no
///   real inflection and only lets a noun plural whose singular is missing fall
///   through to a shorter word (`nodes` → `nod`, `tones` → `ton`);
/// * `er` / `est` restore a silent `e` before trying without one, matching what
///   `ing` and `ed` already do. Morphy lists the bare variants first, but that
///   is the layout of two part-of-speech blocks rather than a preference, and
///   pooled it makes `rider` reduce to `rid` rather than `ride`.
#[rustfmt::skip]
const DETACH_RULES: &[Detach] = &[
    //  suffix    lemma ending      part of speech it comes from
    ("ches",  "ch"),              // noun
    ("shes",  "sh"),              // noun
    ("iest",  "y"),               // adjective — see below
    ("ses",   "s"),               // noun
    ("xes",   "x"),               // noun
    ("zes",   "z"),               // noun
    ("men",   "man"),             // noun
    ("ies",   "y"),               // noun
    ("oes",   "o"),               // noun — see below
    ("ing",   "e"),               // verb
    ("ing",   ""),                // verb
    ("est",   "e"),               // adjective
    ("est",   ""),                // adjective
    ("ier",   "y"),               // adjective — see below
    ("es",    "e"),               // verb
    ("ed",    "e"),               // verb
    ("ed",    ""),                // verb
    ("er",    "e"),               // adjective
    ("er",    ""),                // adjective
    ("s",     ""),                // noun
];

/// Suffixes before which a final consonant doubles.
///
/// Only the verbal ones. `-er` / `-est` doubling (`bigger`, `hottest`) is left
/// to the exception tables: undoubling a comparative is what turns `matter`
/// into `mat` and `butter` into `but`, and the handful of real doubled
/// comparatives is short enough to enumerate.
const DOUBLING_SUFFIXES: [&str; 2] = ["ing", "ed"];

/// Consonants that double before an inflectional ending.
///
/// `s`, `f` and `l`-final stems like `pass`, `stuff` and `fill` reach their
/// lemma through the plain rules, so `s` and `f` are excluded to keep
/// `guessed` from becoming `gues`. `w`, `x` and `y` never double.
const DOUBLING_CONSONANTS: [char; 11] = ['b', 'd', 'g', 'l', 'm', 'n', 'p', 'r', 't', 'v', 'z'];

/// Shortest lemma a *detachment rule* may produce.
///
/// Two-letter English lemmas are a closed set — `be`, `do`, `go`, `he`, `we`,
/// `it` — and every inflection of one is irregular, so the exception table
/// reaches them all (`was`, `goes`, `does`, `its`) and no rule needs to. What
/// the rules do produce at that length is wreckage: `ass` → `as`, `per` → `pe`,
/// `her` → `he`, `bed` → `be`. The floor applies to detachment and undoubling
/// only; exception entries are exact and bypass it.
const MIN_CANDIDATE_CHARS: usize = 3;

/// Folded surface → the lemmas it may reduce to, in preference order.
///
/// A surface that maps to itself is morphy's "already a lemma" marker: it stops
/// the search before any detachment rule runs.
#[derive(Debug, Default)]
pub struct ExceptionTable {
    entries: HashMap<String, Vec<String>>,
    /// True once at least one WNdb file contributed.
    from_wndb: bool,
}

impl ExceptionTable {
    /// The compiled-in table alone.
    pub fn builtin() -> Self {
        let mut table = Self::default();
        table.absorb(BUILTIN_EXCEPTIONS);
        table
    }

    /// The compiled-in table with WNdb's exception files layered over it.
    ///
    /// Layering is per key: where WNdb has an entry it supersedes the built-in
    /// one entirely, and every key WNdb does not mention keeps the fallback.
    /// That is what makes a partial or truncated `dict` directory a
    /// degradation rather than a regression.
    pub fn layered(dir: &Path) -> Self {
        let mut wndb = Self::default();
        for name in EXC_FILES {
            let path = dir.join(name);
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    wndb.absorb(&text);
                    wndb.from_wndb = true;
                }
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "skipping WordNet exception file");
                }
            }
        }
        if !wndb.from_wndb {
            tracing::warn!(dir = %dir.display(), "no WordNet exception files; using the built-in irregular table");
            return Self::builtin();
        }

        let mut table = Self::builtin();
        table.from_wndb = true;
        for (surface, lemmas) in wndb.entries {
            table.entries.insert(surface, lemmas);
        }
        tracing::info!(
            dir = %dir.display(),
            entries = table.entries.len(),
            "loaded WordNet exception lists"
        );
        table
    }

    /// The version string a lemmatizer built on this table reports.
    pub fn version(&self) -> &'static str {
        if self.from_wndb {
            MORPHY_LEMMATIZER_WNDB_VER
        } else {
            MORPHY_LEMMATIZER_VER
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn get(&self, folded: &str) -> &[String] {
        self.entries.get(folded).map_or(&[], Vec::as_slice)
    }

    /// Parse one exception file. Later lines merge into earlier ones, so the
    /// four parts of speech accumulate rather than overwrite: `better` offers
    /// `good` from `adj.exc` and `well` from `adv.exc`.
    fn absorb(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let Some(surface) = fields.next() else {
                continue;
            };
            // Multi-word entries (`had_left have_left`) and hyphenated ones
            // never reach us: the tokenizer splits on both.
            if surface.contains('_') || surface.contains('-') {
                continue;
            }
            let surface = fold_lemma(surface);
            if surface.is_empty() {
                continue;
            }
            let slot = self.entries.entry(surface).or_default();
            for lemma in fields {
                if lemma.contains('_') || lemma.contains('-') {
                    continue;
                }
                let lemma = fold_lemma(lemma);
                if !lemma.is_empty() && !slot.contains(&lemma) {
                    slot.push(lemma);
                }
            }
        }
    }
}

/// Morphy over the live lexicon.
pub struct MorphyLemmatizer {
    exceptions: Arc<ExceptionTable>,
    lexicon: Arc<LexiconCache>,
}

impl MorphyLemmatizer {
    pub fn new(exceptions: Arc<ExceptionTable>, lexicon: Arc<LexiconCache>) -> Self {
        Self {
            exceptions,
            lexicon,
        }
    }

    /// The compiled-in exception table over a fresh, empty cache.
    pub fn builtin() -> Self {
        Self::new(
            Arc::new(ExceptionTable::builtin()),
            Arc::new(LexiconCache::new()),
        )
    }

    pub fn lexicon(&self) -> Arc<LexiconCache> {
        self.lexicon.clone()
    }

    /// The lemma, resolved against one snapshot.
    fn resolve(&self, folded: &str, lexicon: &Lexicon) -> Option<String> {
        // 1. An exact hit outranks every rule. `living` is a word before it is
        //    a participle.
        if lexicon.contains(folded) {
            return Some(folded.to_string());
        }

        // 2. Exceptions, in file order. A self-mapping is a full stop.
        for candidate in self.exceptions.get(folded) {
            if candidate == folded {
                return Some(folded.to_string());
            }
            if lexicon.contains(candidate) {
                return Some(candidate.clone());
            }
        }

        // 3. Detachment, longest suffix first.
        for rule in DETACH_RULES {
            if let Some(candidate) = detach(folded, rule) {
                if lexicon.contains(&candidate) {
                    return Some(candidate);
                }
            }
        }

        // 4. Undo a doubled consonant, which only ever helps once the plain
        //    rules have failed: `filling` is `fill` long before `fil`.
        undouble(folded)
            .into_iter()
            .find(|candidate| lexicon.contains(candidate))
    }
}

impl Lemmatizer for MorphyLemmatizer {
    fn version(&self) -> &str {
        self.exceptions.version()
    }

    fn lemmatize(&self, surface: &str) -> String {
        let folded = fold_lemma(surface);
        if folded.is_empty() {
            return folded;
        }
        let lexicon = self.lexicon.snapshot();
        self.resolve(&folded, &lexicon).unwrap_or(folded)
    }
}

/// Apply one rule, or `None` when it does not fit.
fn detach(folded: &str, (suffix, replacement): &Detach) -> Option<String> {
    let stem = folded.strip_suffix(suffix)?;
    if stem.is_empty() {
        return None;
    }
    let candidate = format!("{stem}{replacement}");
    (candidate.chars().count() >= MIN_CANDIDATE_CHARS).then_some(candidate)
}

/// `running` → `run`, `stopped` → `stop`.
fn undouble(folded: &str) -> Vec<String> {
    let mut out = Vec::new();
    for suffix in DOUBLING_SUFFIXES {
        let Some(stem) = folded.strip_suffix(suffix) else {
            continue;
        };
        let mut chars = stem.chars().rev();
        let (Some(last), Some(previous)) = (chars.next(), chars.next()) else {
            continue;
        };
        if last != previous || !DOUBLING_CONSONANTS.contains(&last) {
            continue;
        }
        let candidate: String = stem[..stem.len() - last.len_utf8()].to_string();
        if candidate.chars().count() >= MIN_CANDIDATE_CHARS && !out.contains(&candidate) {
            out.push(candidate);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lemmatizer over a fixed lexicon.
    fn lemmatizer(words: &[&str]) -> MorphyLemmatizer {
        MorphyLemmatizer::new(
            Arc::new(ExceptionTable::builtin()),
            Arc::new(LexiconCache::seeded(words.iter().copied())),
        )
    }

    fn lemma(words: &[&str], surface: &str) -> String {
        lemmatizer(words).lemmatize(surface)
    }

    #[test]
    fn the_builtin_table_parses() {
        let table = ExceptionTable::builtin();
        assert!(table.len() > 200, "{} entries", table.len());
        assert_eq!(table.get("made").to_vec(), vec!["make"]);
        assert_eq!(table.get("children").to_vec(), vec!["child"]);
        assert_eq!(
            table.get("number").to_vec(),
            vec!["number"],
            "a protected surface"
        );
        assert_eq!(table.version(), MORPHY_LEMMATIZER_VER);
    }

    #[test]
    fn an_exact_lemma_is_never_analysed() {
        assert_eq!(lemma(&["living", "live"], "living"), "living");
        assert_eq!(lemma(&["meeting", "meet"], "Meeting"), "meeting");
        // Without the exact hit the participle rule takes over.
        assert_eq!(lemma(&["live"], "living"), "live");
    }

    #[test]
    fn irregulars_come_from_the_builtin_table() {
        let words = [
            "be", "have", "make", "give", "know", "take", "child", "foot",
        ];
        assert_eq!(lemma(&words, "was"), "be");
        assert_eq!(lemma(&words, "were"), "be");
        assert_eq!(lemma(&words, "been"), "be");
        assert_eq!(lemma(&words, "had"), "have");
        assert_eq!(lemma(&words, "made"), "make");
        assert_eq!(lemma(&words, "given"), "give");
        assert_eq!(lemma(&words, "known"), "know");
        assert_eq!(lemma(&words, "taken"), "take");
        assert_eq!(lemma(&words, "children"), "child");
        assert_eq!(lemma(&words, "feet"), "foot");
    }

    #[test]
    fn e_restoration_recovers_the_silent_e() {
        assert_eq!(lemma(&["have"], "having"), "have");
        assert_eq!(lemma(&["make"], "making"), "make");
        assert_eq!(lemma(&["relate"], "relating"), "relate");
        assert_eq!(lemma(&["use"], "used"), "use");
        assert_eq!(lemma(&["large"], "larger"), "large");
    }

    #[test]
    fn doubling_is_reversed_only_after_the_plain_rules_fail() {
        assert_eq!(lemma(&["run"], "running"), "run");
        assert_eq!(lemma(&["stop"], "stopped"), "stop");
        assert_eq!(lemma(&["plan"], "planned"), "plan");
        assert_eq!(lemma(&["refer"], "referring"), "refer");
        // Reached by the rule alone: neither is in the exception table.
        assert_eq!(lemma(&["jog"], "jogging"), "jog");
        assert_eq!(lemma(&["trim"], "trimmed"), "trim");
        // `fill` is reachable without undoubling, and must win over `fil`.
        assert_eq!(lemma(&["fill", "fil"], "filling"), "fill");
        assert_eq!(lemma(&["hope", "hop"], "hoping"), "hope");
        assert_eq!(lemma(&["hope", "hop"], "hopping"), "hop");
    }

    #[test]
    fn plural_rules_follow_morphy() {
        assert_eq!(lemma(&["study"], "studies"), "study");
        assert_eq!(lemma(&["box"], "boxes"), "box");
        assert_eq!(lemma(&["bus"], "buses"), "bus");
        assert_eq!(lemma(&["church"], "churches"), "church");
        assert_eq!(lemma(&["dish"], "dishes"), "dish");
        assert_eq!(lemma(&["dog"], "dogs"), "dog");
        assert_eq!(lemma(&["glass"], "glasses"), "glass");
        assert_eq!(lemma(&["hero"], "heroes"), "hero");
        assert_eq!(lemma(&["house"], "houses"), "house");
        assert_eq!(lemma(&["size"], "sizes"), "size");
    }

    /// `-es` only attaches after a sibilant, `o`, or a `y` turned `i`, and each
    /// of those has its own rule. A bare `es` → `` would strip the `e` off a
    /// plural whose singular is missing and land on a shorter word instead.
    #[test]
    fn a_plural_whose_singular_is_missing_stays_out_of_scope() {
        assert_eq!(lemma(&["nod"], "nodes"), "nodes");
        assert_eq!(lemma(&["ton"], "tones"), "tones");
        assert_eq!(lemma(&["nod", "node"], "nodes"), "node");
    }

    /// Restoring the silent `e` first, as `-ing` and `-ed` already do.
    #[test]
    fn comparative_and_agent_suffixes_restore_the_silent_e_first() {
        assert_eq!(lemma(&["ride", "rid"], "rider"), "ride");
        assert_eq!(lemma(&["write", "writ"], "writer"), "write");
        assert_eq!(
            lemma(&["rid"], "rider"),
            "rid",
            "and fall back when it must"
        );
        assert_eq!(lemma(&["large", "larg"], "largest"), "large");
        assert_eq!(lemma(&["near"], "nearest"), "near");
    }

    #[test]
    fn comparatives_and_superlatives_detach() {
        assert_eq!(lemma(&["kind"], "kindest"), "kind");
        assert_eq!(lemma(&["high"], "higher"), "high");
        assert_eq!(lemma(&["easy"], "easier"), "easy");
        assert_eq!(lemma(&["easy"], "easiest"), "easy");
        assert_eq!(lemma(&["big"], "bigger"), "big");
        assert_eq!(lemma(&["good"], "better"), "good");
    }

    #[test]
    fn an_unvalidated_detachment_keeps_the_surface() {
        // `covfefe` strips to nothing real, so it stays out of scope.
        assert_eq!(lemma(&["have", "make"], "sprocketing"), "sprocketing");
        assert_eq!(lemma(&["have"], "abstruse"), "abstruse");
        // The stem exists as a string but not as a word.
        assert_eq!(lemma(&["run"], "singing"), "singing");
    }

    #[test]
    fn an_empty_lexicon_analyses_nothing() {
        assert_eq!(lemma(&[], "having"), "having");
        assert_eq!(lemma(&[], "made"), "made");
        assert_eq!(lemma(&[], "Kindly"), "kindly");
    }

    #[test]
    fn protected_surfaces_survive_a_tempting_detachment() {
        assert_eq!(lemma(&["numb", "number"], "number"), "number");
        assert_eq!(lemma(&["numb"], "number"), "number");
        assert_eq!(lemma(&["he"], "her"), "her");
        assert_eq!(lemma(&["be"], "bed"), "bed");
        assert_eq!(lemma(&["she"], "shed"), "shed");
        assert_eq!(lemma(&["the"], "thing"), "thing");
        assert_eq!(lemma(&["we"], "wing"), "wing");
        assert_eq!(lemma(&["corn"], "corner"), "corner");
        assert_eq!(lemma(&["even"], "evening"), "evening");
        assert_eq!(lemma(&["for"], "forest"), "forest");
        assert_eq!(lemma(&["new"], "news"), "news");
    }

    #[test]
    fn very_short_candidates_are_refused() {
        assert_eq!(lemma(&["a"], "as"), "as");
        assert_eq!(lemma(&["i"], "is"), "is");
        assert_eq!(lemma(&["as"], "ass"), "ass");
        assert_eq!(lemma(&["pe"], "per"), "per");
        assert_eq!(lemma(&["he"], "her"), "her");
        assert_eq!(lemma(&["be"], "bed"), "bed");
    }

    /// The two-letter lemmas the floor would otherwise hide are all irregular,
    /// and exceptions are exact rather than generated, so they still resolve.
    #[test]
    fn the_length_floor_does_not_block_the_exception_table() {
        let words = ["be", "do", "go", "it", "ox"];
        assert_eq!(lemma(&words, "was"), "be");
        assert_eq!(lemma(&words, "been"), "be");
        assert_eq!(lemma(&words, "does"), "do");
        assert_eq!(lemma(&words, "goes"), "go");
        assert_eq!(lemma(&words, "its"), "it");
        assert_eq!(lemma(&words, "oxen"), "ox");
    }

    #[test]
    fn case_is_folded_before_anything_else() {
        assert_eq!(lemma(&["have"], "HAVING"), "have");
        assert_eq!(lemma(&["have"], "  Having  "), "have");
        assert_eq!(lemma(&["kindly"], ""), "");
    }

    #[test]
    fn a_refreshed_cache_changes_the_answer() {
        let cache = Arc::new(LexiconCache::new());
        let lemmatizer = MorphyLemmatizer::new(Arc::new(ExceptionTable::builtin()), cache.clone());
        assert_eq!(lemmatizer.lemmatize("having"), "having");
        cache.replace(["have"].into_iter().collect());
        assert_eq!(lemmatizer.lemmatize("having"), "have");
    }

    #[test]
    fn undoubling_needs_a_doubled_consonant() {
        assert_eq!(undouble("running"), ["run"]);
        assert_eq!(undouble("stopped"), ["stop"]);
        assert!(undouble("meaning").is_empty());
        assert!(undouble("guessed").is_empty(), "s never doubles here");
        assert!(undouble("ing").is_empty());
    }

    #[test]
    fn detachment_refuses_to_consume_the_whole_word() {
        let participle: Detach = ("ing", "e");
        let plural: Detach = ("s", "");
        assert_eq!(detach("ing", &participle), None);
        assert_eq!(detach("s", &plural), None);
        assert_eq!(detach("having", &participle).as_deref(), Some("have"));
    }

    /// Longest suffix first is the whole reason the merged table is safe.
    #[test]
    fn the_rule_table_is_ordered_longest_suffix_first() {
        let lengths: Vec<usize> = DETACH_RULES
            .iter()
            .map(|(suffix, _)| suffix.len())
            .collect();
        assert!(
            lengths.windows(2).all(|pair| pair[0] >= pair[1]),
            "{lengths:?}"
        );
    }
}
