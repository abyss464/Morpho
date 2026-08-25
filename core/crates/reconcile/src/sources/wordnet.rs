//! WordNet, parsed in-process from the WNdb data files.
//!
//! README Part 4: "WordNet 3.1 **进程内原生**" — no subprocess, no service. The
//! `index.{noun,verb,adj,adv}` and `data.{noun,verb,adj,adv}` files are read
//! once at startup into memory, and serve two jobs:
//!
//! * **definition fallback** — a synset's gloss becomes a `wordnet` definition
//!   candidate when the Free Dictionary has nothing;
//! * **semantic clustering** — hypernym chains give the plan builder a cheap,
//!   deterministic cluster key for its third grouping stage.
//!
//! When `wordnet_dir` is unset the whole thing is absent: no fallback
//! candidates, and the semantic grouping stage is skipped rather than
//! approximated.
//!
//! ## File format (WNdb §wndb(5))
//!
//! `index.pos` lines: `lemma pos synset_cnt p_cnt [ptr_symbol…] sense_cnt
//! tagsense_cnt synset_offset…`
//!
//! `data.pos` lines: `offset lex_filenum ss_type w_cnt word lex_id… p_cnt
//! [ptr_symbol offset pos source/target]… | gloss`
//!
//! Both files start with copyright headers whose lines begin with two spaces.

use std::collections::HashMap;
use std::path::Path;

use morpho_domain::canon::{canonicalize, fold_lemma};
use morpho_domain::types::{FetchedDefinition, Pos};

/// A WordNet part of speech and its file suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WnPos {
    Noun,
    Verb,
    Adj,
    Adv,
}

impl WnPos {
    pub const ALL: [WnPos; 4] = [WnPos::Noun, WnPos::Verb, WnPos::Adj, WnPos::Adv];

    const fn suffix(self) -> &'static str {
        match self {
            Self::Noun => "noun",
            Self::Verb => "verb",
            Self::Adj => "adj",
            Self::Adv => "adv",
        }
    }

    const fn contract_pos(self) -> Pos {
        match self {
            Self::Noun => Pos::Noun,
            Self::Verb => Pos::Verb,
            Self::Adj => Pos::Adj,
            Self::Adv => Pos::Adv,
        }
    }

    fn from_char(c: char) -> Option<Self> {
        match c {
            'n' => Some(Self::Noun),
            'v' => Some(Self::Verb),
            // 'a' is an adjective head, 's' an adjective satellite; both live
            // in data.adj and both read as adjectives.
            'a' | 's' => Some(Self::Adj),
            'r' => Some(Self::Adv),
            _ => None,
        }
    }
}

/// One synset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synset {
    pub offset: u64,
    pub pos: WnPos,
    /// Definition text, with the illustrative quotations stripped off.
    pub gloss: String,
    /// Hypernym targets as `(pos, offset)`.
    pub hypernyms: Vec<(WnPos, u64)>,
}

/// An in-memory WordNet database.
#[derive(Debug, Default)]
pub struct WordNet {
    /// Folded lemma → the synsets it belongs to, in sense order.
    index: HashMap<String, Vec<(WnPos, u64)>>,
    synsets: HashMap<(WnPos, u64), Synset>,
}

/// How deep to climb the hypernym chain when deriving a cluster key.
///
/// Three hops lands on something like "trait" or "change of state": broad
/// enough that a group has more than one member, narrow enough that the group
/// still means something.
const CLUSTER_DEPTH: usize = 3;

impl WordNet {
    /// Load every WNdb file in `dir`.
    ///
    /// A missing or unreadable part of speech is logged and skipped: three
    /// working files are better than refusing to start.
    pub fn load(dir: &Path) -> std::io::Result<Self> {
        let mut db = WordNet::default();
        for pos in WnPos::ALL {
            let data = dir.join(format!("data.{}", pos.suffix()));
            let index = dir.join(format!("index.{}", pos.suffix()));
            match std::fs::read_to_string(&data) {
                Ok(text) => db.parse_data(pos, &text),
                Err(err) => {
                    tracing::warn!(path = %data.display(), error = %err, "skipping WordNet data file");
                    continue;
                }
            }
            match std::fs::read_to_string(&index) {
                Ok(text) => db.parse_index(&text),
                Err(err) => {
                    tracing::warn!(path = %index.display(), error = %err, "skipping WordNet index file")
                }
            }
        }
        if db.synsets.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no WordNet data files under {}", dir.display()),
            ));
        }
        tracing::info!(
            synsets = db.synsets.len(),
            lemmas = db.index.len(),
            "loaded WordNet"
        );
        Ok(db)
    }

    pub fn is_empty(&self) -> bool {
        self.synsets.is_empty()
    }

    pub fn synset_count(&self) -> usize {
        self.synsets.len()
    }

    pub fn lemma_count(&self) -> usize {
        self.index.len()
    }

    fn parse_index(&mut self, text: &str) {
        for line in text.lines() {
            if line.starts_with("  ") || line.trim().is_empty() {
                continue;
            }
            let Some(entry) = parse_index_line(line) else {
                continue;
            };
            let (lemma, pos, offsets) = entry;
            let bucket = self.index.entry(lemma).or_default();
            for offset in offsets {
                if !bucket.contains(&(pos, offset)) {
                    bucket.push((pos, offset));
                }
            }
        }
    }

    fn parse_data(&mut self, pos: WnPos, text: &str) {
        for line in text.lines() {
            if line.starts_with("  ") || line.trim().is_empty() {
                continue;
            }
            if let Some(synset) = parse_data_line(pos, line) {
                self.synsets.insert((synset.pos, synset.offset), synset);
            }
        }
    }

    /// Synsets of one lemma, in WordNet's sense order (most frequent first).
    pub fn senses(&self, lemma: &str) -> Vec<&Synset> {
        let key = fold_lemma(lemma).replace(' ', "_");
        self.index
            .get(&key)
            .map(|offsets| {
                offsets
                    .iter()
                    .filter_map(|key| self.synsets.get(key))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Definition candidates for one lemma, capped and deduplicated.
    ///
    /// WordNet orders senses by corpus frequency, so taking the first few is
    /// taking the ones a learner is most likely to meet.
    pub fn definitions(&self, lemma: &str, limit: usize) -> Vec<FetchedDefinition> {
        let mut out = Vec::new();
        if limit == 0 {
            return out;
        }
        let mut seen = std::collections::HashSet::new();
        for synset in self.senses(lemma) {
            let text = canonicalize(&synset.gloss);
            if text.is_empty() || !seen.insert(text.to_lowercase()) {
                continue;
            }
            out.push(FetchedDefinition {
                pos: synset.pos.contract_pos(),
                text,
                source_ref: Some(format!(
                    "wordnet:{}:{:08}",
                    synset.pos.suffix(),
                    synset.offset
                )),
            });
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    /// A deterministic semantic cluster key for one lemma.
    ///
    /// Takes the lemma's first (most frequent) synset and climbs up to
    /// [`CLUSTER_DEPTH`] hypernyms. Words that end up under the same ancestor
    /// share a key, which is what the plan's semantic grouping stage needs.
    pub fn cluster_key(&self, lemma: &str) -> Option<String> {
        let first = *self.senses(lemma).first()?;
        let mut current = (first.pos, first.offset);
        let mut seen = std::collections::HashSet::new();
        seen.insert(current);
        for _ in 0..CLUSTER_DEPTH {
            let Some(synset) = self.synsets.get(&current) else {
                break;
            };
            // Hypernyms are already in file order; take the first for
            // determinism rather than trying to be clever about which parent
            // is more "central".
            let Some(next) = synset.hypernyms.first().copied() else {
                break;
            };
            if !seen.insert(next) {
                break;
            }
            current = next;
        }
        Some(format!("{}:{:08}", current.0.suffix(), current.1))
    }

    /// Wu-Palmer similarity in `(0, 1]`.
    ///
    /// `2·(depth(LCS)+1) / ((depth(a)+1) + (depth(b)+1))`, where depth counts
    /// hypernym hops from the top of the chain. Plain path length is not enough
    /// here: "serene vs tranquil" (siblings under *calm*) and "serene vs
    /// quality" (a word against its own distant ancestor) are the same number
    /// of hops apart, and only the depth of the shared ancestor tells them
    /// apart. Returns `None` when either lemma is unknown.
    pub fn similarity(&self, a: &str, b: &str) -> Option<f64> {
        let a_first = *self.senses(a).first()?;
        let b_first = *self.senses(b).first()?;
        let path_a = self.ancestors((a_first.pos, a_first.offset));
        let path_b = self.ancestors((b_first.pos, b_first.offset));
        let index_b: HashMap<(WnPos, u64), usize> =
            path_b.iter().enumerate().map(|(d, k)| (*k, d)).collect();

        let mut best: Option<usize> = None;
        for (index, key) in path_a.iter().enumerate() {
            if let Some(other) = index_b.get(key) {
                let hops = index + other;
                if best.is_none_or(|current| hops < current) {
                    // Depth of the shared ancestor, counted from the root.
                    best = Some(path_a.len() - index - 1);
                }
            }
        }
        let lcs_depth = best?;
        let depth_a = path_a.len() - 1;
        let depth_b = path_b.len() - 1;
        Some(2.0 * (lcs_depth + 1) as f64 / ((depth_a + 1) + (depth_b + 1)) as f64)
    }

    fn ancestors(&self, start: (WnPos, u64)) -> Vec<(WnPos, u64)> {
        let mut path = vec![start];
        let mut seen = std::collections::HashSet::new();
        seen.insert(start);
        let mut current = start;
        // WordNet's noun hierarchy is about 16 deep; 32 is a safety stop for
        // a corrupt file, not a semantic choice.
        for _ in 0..32 {
            let Some(synset) = self.synsets.get(&current) else {
                break;
            };
            let Some(next) = synset.hypernyms.first().copied() else {
                break;
            };
            if !seen.insert(next) {
                break;
            }
            path.push(next);
            current = next;
        }
        path
    }
}

fn parse_index_line(line: &str) -> Option<(String, WnPos, Vec<u64>)> {
    let mut fields = line.split_whitespace();
    let lemma = fields.next()?.to_string();
    let pos = WnPos::from_char(fields.next()?.chars().next()?)?;
    let synset_cnt: usize = fields.next()?.parse().ok()?;
    let ptr_cnt: usize = fields.next()?.parse().ok()?;
    for _ in 0..ptr_cnt {
        fields.next()?;
    }
    // sense_cnt (repeats synset_cnt) and tagsense_cnt.
    fields.next()?;
    fields.next()?;
    let mut offsets = Vec::with_capacity(synset_cnt);
    for _ in 0..synset_cnt {
        offsets.push(fields.next()?.parse().ok()?);
    }
    Some((lemma.to_lowercase(), pos, offsets))
}

fn parse_data_line(file_pos: WnPos, line: &str) -> Option<Synset> {
    let (head, gloss) = match line.split_once('|') {
        Some((head, gloss)) => (head, gloss),
        None => (line, ""),
    };
    let mut fields = head.split_whitespace();
    let offset: u64 = fields.next()?.parse().ok()?;
    let _lex_filenum = fields.next()?;
    let ss_type = fields.next()?.chars().next()?;
    let pos = WnPos::from_char(ss_type).unwrap_or(file_pos);

    let w_cnt = usize::from_str_radix(fields.next()?, 16).ok()?;
    for _ in 0..w_cnt {
        fields.next()?; // word
        fields.next()?; // lex_id
    }
    let p_cnt: usize = fields.next()?.parse().ok()?;
    let mut hypernyms = Vec::new();
    for _ in 0..p_cnt {
        let symbol = fields.next()?;
        let target: u64 = fields.next()?.parse().ok()?;
        let target_pos = WnPos::from_char(fields.next()?.chars().next()?);
        fields.next()?; // source/target byte field
                        // '@' hypernym, '@i' instance hypernym.
        if symbol == "@" || symbol == "@i" {
            if let Some(target_pos) = target_pos {
                hypernyms.push((target_pos, target));
            }
        }
    }

    Some(Synset {
        offset,
        pos,
        gloss: clean_gloss(gloss),
        hypernyms,
    })
}

/// Strip the illustrative quotations WordNet appends to a gloss.
///
/// A gloss reads `definition; "example one"; "example two"`. Only the
/// definition belongs in a candidate — the quotations are someone else's
/// sentences, and examples come from the exam corpus.
fn clean_gloss(raw: &str) -> String {
    let mut out = String::new();
    for part in raw.split(';') {
        let trimmed = part.trim();
        if trimmed.starts_with('"') {
            break;
        }
        if !out.is_empty() {
            out.push_str("; ");
        }
        out.push_str(trimmed);
    }
    canonicalize(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-built miniature WNdb tree:
    ///   serene → calm → quality (adj chain via hypernyms)
    ///   tranquil → calm
    fn fixture() -> WordNet {
        let data = "\
  1 This software and database is being provided to you.\n\
00001740 00 a 01 serene 0 001 @ 00002098 a 0000 | free from disturbance; \"a serene expression\"\n\
00002098 00 a 01 calm 0 001 @ 00003553 a 0000 | not agitated\n\
00003553 00 a 01 quality 0 000 | an essential attribute\n\
00004000 00 a 01 tranquil 0 001 @ 00002098 a 0000 | free from disturbance by heavy waves\n";
        let index = "\
  1 header line\n\
serene a 1 1 @ 1 0 00001740\n\
calm a 1 1 @ 1 0 00002098\n\
tranquil a 1 1 @ 1 0 00004000\n\
quality a 1 0 1 0 00003553\n";
        let mut db = WordNet::default();
        db.parse_data(WnPos::Adj, data);
        db.parse_index(index);
        db
    }

    #[test]
    fn parses_glosses_and_hypernyms() {
        let db = fixture();
        assert_eq!(db.synset_count(), 4);
        assert_eq!(db.lemma_count(), 4);
        let serene = db.senses("serene");
        assert_eq!(serene.len(), 1);
        assert_eq!(serene[0].gloss, "free from disturbance");
        assert_eq!(serene[0].hypernyms, vec![(WnPos::Adj, 2098)]);
    }

    #[test]
    fn strips_illustrative_quotations_from_glosses() {
        assert_eq!(
            clean_gloss(" free from disturbance; \"a serene expression\"; \"calm seas\" "),
            "free from disturbance"
        );
        assert_eq!(
            clean_gloss("marked by care; showing thought"),
            "marked by care; showing thought"
        );
        assert_eq!(clean_gloss(""), "");
    }

    #[test]
    fn definitions_carry_the_contract_pos_and_provenance() {
        let db = fixture();
        let definitions = db.definitions("serene", 5);
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].pos, Pos::Adj);
        assert_eq!(definitions[0].text, "free from disturbance");
        assert_eq!(
            definitions[0].source_ref.as_deref(),
            Some("wordnet:adj:00001740")
        );
    }

    #[test]
    fn definition_lookup_is_case_insensitive() {
        let db = fixture();
        assert_eq!(db.definitions("SERENE", 5).len(), 1);
        assert_eq!(db.definitions("Serene", 5).len(), 1);
    }

    #[test]
    fn an_unknown_lemma_yields_nothing_rather_than_a_guess() {
        let db = fixture();
        assert!(db.definitions("benevolent", 5).is_empty());
        assert!(db.cluster_key("benevolent").is_none());
        assert!(db.similarity("benevolent", "serene").is_none());
    }

    #[test]
    fn definition_limit_is_honored() {
        let db = fixture();
        assert!(db.definitions("serene", 0).is_empty());
    }

    #[test]
    fn near_synonyms_share_a_cluster_key() {
        let db = fixture();
        let serene = db.cluster_key("serene").unwrap();
        let tranquil = db.cluster_key("tranquil").unwrap();
        assert_eq!(serene, tranquil, "both climb to the same ancestor");
    }

    #[test]
    fn cluster_keys_are_stable_across_calls() {
        let db = fixture();
        assert_eq!(db.cluster_key("serene"), db.cluster_key("serene"));
    }

    #[test]
    fn similarity_is_one_for_a_lemma_against_itself() {
        let db = fixture();
        assert_eq!(db.similarity("serene", "serene"), Some(1.0));
    }

    #[test]
    fn similarity_falls_off_with_distance() {
        let db = fixture();
        let siblings = db.similarity("serene", "tranquil").unwrap();
        let distant = db.similarity("serene", "quality").unwrap();
        assert!(siblings > distant, "{siblings} vs {distant}");
    }

    #[test]
    fn a_hypernym_cycle_cannot_hang_the_walk() {
        let data = "\
00000001 00 n 01 a 0 001 @ 00000002 n 0000 | first\n\
00000002 00 n 01 b 0 001 @ 00000001 n 0000 | second\n";
        let index = "a n 1 1 @ 1 0 00000001\nb n 1 1 @ 1 0 00000002\n";
        let mut db = WordNet::default();
        db.parse_data(WnPos::Noun, data);
        db.parse_index(index);
        assert!(db.cluster_key("a").is_some());
        assert!(db.similarity("a", "b").is_some());
    }

    #[test]
    fn header_lines_are_skipped() {
        let db = fixture();
        // The two-space copyright lines must not become synsets.
        assert!(db.senses("this").is_empty());
        assert!(db.senses("1").is_empty());
    }

    #[test]
    fn malformed_lines_are_dropped_not_fatal() {
        let mut db = WordNet::default();
        db.parse_data(WnPos::Noun, "garbage\n00000001 00 n zz\n");
        db.parse_index("also garbage\n");
        assert_eq!(db.synset_count(), 0);
        assert!(db.is_empty());
    }

    #[test]
    fn loading_an_empty_directory_is_an_error_not_a_silent_empty_database() {
        let dir = tempfile::tempdir().unwrap();
        assert!(WordNet::load(dir.path()).is_err());
    }

    #[test]
    fn adjective_satellites_read_as_adjectives() {
        assert_eq!(WnPos::from_char('s'), Some(WnPos::Adj));
        assert_eq!(WnPos::from_char('a'), Some(WnPos::Adj));
        assert_eq!(WnPos::from_char('r'), Some(WnPos::Adv));
        assert_eq!(WnPos::from_char('x'), None);
    }

    #[test]
    fn multiword_lemmas_use_underscores() {
        let data = "00000001 00 n 01 ad_hoc 0 000 | for this purpose\n";
        let index = "ad_hoc n 1 0 1 0 00000001\n";
        let mut db = WordNet::default();
        db.parse_data(WnPos::Noun, data);
        db.parse_index(index);
        assert_eq!(db.definitions("ad hoc", 3).len(), 1);
    }
}
