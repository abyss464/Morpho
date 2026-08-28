//! The export payload: everything `release.db` needs, loaded from the working
//! database in one read and ordered so the output is reproducible.
//!
//! Nothing here is derived on the fly — the reconciler has already computed
//! readiness, the plan and the TTS coverage. The exporter's job is to *check*
//! and to *cut*, not to recompute.

use std::collections::HashMap;

use rusqlite::Connection;

use morpho_domain::blocker::BlockerCode;
use morpho_domain::tts::TtsConfig;
use morpho_domain::types::{Role, TtsKind};
use morpho_store::error::Result;
use morpho_store::queries;

pub use morpho_store::queries::GlossAnchor;

/// One word, with everything the release needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportWord {
    pub word_id: i64,
    pub lemma: String,
    pub phonetic: Option<String>,
    pub frequency_rank: Option<i64>,
    pub role: Role,
    pub etymology: Option<String>,
    pub learning_order: i64,
    pub group_seq: i64,
    pub image_file_hash: Option<String>,
    pub word_audio_hash: Option<String>,
    pub core_ready: bool,
    pub blockers: Vec<String>,
    pub distractor_count: usize,
    /// The cached extraction of every selected definition matches the current
    /// tokenizer/lemmatizer pair.
    pub extraction_fresh: bool,
    /// Every slot this word ships from — its enabled senses, its example slots
    /// and its picture — points at a candidate that is still
    /// `status = 'available'`.
    pub selections_available: bool,
}

impl ExportWord {
    /// README Part 5's per-word factory gates.
    ///
    /// `core_ready` already carries gates 1–5 and the plan position; the extra
    /// two are the three distractor bindings (gate 6) and extraction freshness
    /// (gate 2's other half). Distractor *readiness* is deliberately not
    /// checked here — the dependency closure covers it, and attributing a
    /// removal to the closure is what makes the holdback report useful.
    pub fn shippable(&self) -> bool {
        self.core_ready
            && self.distractor_count >= 3
            && self.extraction_fresh
            && self.selections_available
    }

    /// The blocker list a holdback report should quote, in canonical order.
    pub fn gate_blockers(&self) -> Vec<String> {
        let mut out = self.blockers.clone();
        if self.distractor_count < 3
            && !out.contains(&BlockerCode::DistractorsUnbound.as_str().to_string())
        {
            out.push(BlockerCode::DistractorsUnbound.as_str().to_string());
        }
        if !self.extraction_fresh {
            out.push(STALE_EXTRACTION.to_string());
        }
        if !self.selections_available {
            out.push(REJECTED_SELECTION.to_string());
        }
        // Distractor readiness is the closure's business, not a per-word gate.
        out.retain(|code| !code.starts_with("distractor_"));
        out
    }
}

/// Root cause used when a selected definition's tokenization is behind the
/// current tool versions. Not in the `BlockerCode` union — it is an
/// export-time condition, and `HoldbackEntry.root_cause` accepts any string.
pub const STALE_EXTRACTION: &str = "stale_extraction";

/// Root cause used when a slot the release would ship from points at a
/// candidate that has left `status = 'available'`.
///
/// The reconciler releases such a slot and re-selects, and both write paths now
/// refuse to create one, so this is the same kind of check as
/// `definition_token_resolves`: a last look before the bytes go out, because a
/// shipped word built on rejected content is a broken product and the whole
/// point of the gate is that it does not depend on the layer above being right.
pub const REJECTED_SELECTION: &str = "rejected_selection";

/// One selected sense.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSense {
    pub word_id: i64,
    pub pos: String,
    pub definition: String,
    pub is_primary: bool,
    pub audio_hash: Option<String>,
}

/// One selected example.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportExample {
    pub word_id: i64,
    pub display_order: i64,
    pub sentence: String,
    pub hl_start: i64,
    pub hl_end: i64,
    pub audio_hash: Option<String>,
}

/// One plan group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportGroup {
    pub group_seq: i64,
    pub group_type: String,
}

/// Everything loaded for one export.
#[derive(Debug, Clone)]
pub struct ExportPayload {
    pub plan_id: i64,
    pub words: Vec<ExportWord>,
    pub senses: Vec<ExportSense>,
    pub examples: Vec<ExportExample>,
    pub groups: Vec<ExportGroup>,
    /// `(word_id, rank, distractor_word_id)`
    pub distractors: Vec<(i64, i64, i64)>,
    /// Dependency edges among active words, `(from, to)`. Edges into a gloss
    /// anchor are already gone: the anchor terminates the chain, so it must not
    /// drag its dependent out of the cut (admin-api.md ruling #18a).
    pub dependency_edges: Vec<(i64, i64)>,
    /// Every glossed word in the lexicon, whether referenced or not.
    pub gloss_anchors: Vec<GlossAnchor>,
    /// `(word_id, anchor_word_id)` — the edges `dependency_edges` dropped, kept
    /// so the release can ship exactly the anchors its words actually mention.
    pub anchor_refs: Vec<(i64, i64)>,
    /// `(word_id, lemma)` — definition tokens that resolve to no word at all.
    /// Readiness already blocks on these; the exporter re-checks because a
    /// shipped word with an unreadable token is a broken product.
    pub unresolved_tokens: Vec<(i64, String)>,
    /// `file_hash` → `(kind, rel_path, bytes)` for every referenced media file.
    pub media: HashMap<String, MediaEntry>,
}

/// A media file the release will carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaEntry {
    pub kind: String,
    pub rel_path: String,
    pub bytes: i64,
}

/// Load everything for one export.
pub fn load(
    conn: &Connection,
    tts: &TtsConfig,
    tokenizer_ver: &str,
    lemmatizer_ver: &str,
) -> Result<Option<ExportPayload>> {
    let Some(plan) = queries::current_plan(conn)? else {
        return Ok(None);
    };
    let placements = queries::plan_placements(conn, plan.plan_id)?;
    let assets = queries::tts_assets(conn)?;
    let ready_audio = |kind: TtsKind, text: &str| -> Option<String> {
        let hash = tts.input_hash(kind, text);
        assets
            .get(&hash)
            .filter(|asset| asset.status == "ready")
            .and_then(|asset| asset.file_hash.clone())
    };

    let stale = stale_extraction_words(conn, tokenizer_ver, lemmatizer_ver)?;
    let unavailable = words_selecting_unavailable_candidates(conn)?;
    let distractor_counts = distractor_counts(conn)?;
    let images = selected_images(conn)?;

    let mut words: Vec<ExportWord> = Vec::new();
    for word in queries::active_words(conn)? {
        let placement = placements.get(&word.word_id);
        words.push(ExportWord {
            word_audio_hash: ready_audio(TtsKind::Word, &word.lemma),
            learning_order: placement.map(|p| p.learning_order).unwrap_or(i64::MAX),
            group_seq: placement.map(|p| p.group_seq).unwrap_or(0),
            image_file_hash: images.get(&word.word_id).cloned(),
            core_ready: word.core_ready,
            blockers: word.blockers.clone(),
            distractor_count: distractor_counts.get(&word.word_id).copied().unwrap_or(0),
            extraction_fresh: !stale.contains(&word.word_id),
            selections_available: !unavailable.contains(&word.word_id),
            word_id: word.word_id,
            lemma: word.lemma,
            phonetic: word.phonetic,
            frequency_rank: word.frequency_rank,
            role: word.role,
            etymology: word.etymology,
        });
    }
    words.sort_by_key(|word| (word.learning_order, word.word_id));

    let mut senses = load_senses(conn)?;
    for sense in &mut senses {
        sense.audio_hash = ready_audio(TtsKind::Definition, &sense.definition);
    }
    let mut examples = load_examples(conn)?;
    for example in &mut examples {
        example.audio_hash = ready_audio(TtsKind::Example, &example.sentence);
    }

    let groups = queries::plan_group_types(conn, plan.plan_id)?
        .into_iter()
        .map(|(group_seq, group_type)| ExportGroup {
            group_seq,
            group_type,
        })
        .collect();

    let media = media_registry(conn)?;

    Ok(Some(ExportPayload {
        plan_id: plan.plan_id,
        words,
        senses,
        examples,
        groups,
        distractors: queries::distractor_edges(conn)?,
        dependency_edges: queries::dependency_edges(conn)?,
        gloss_anchors: queries::gloss_anchors(conn)?,
        anchor_refs: queries::gloss_anchor_refs(conn)?,
        unresolved_tokens: queries::unresolved_definition_tokens(conn)?,
        media,
    }))
}

fn load_senses(conn: &Connection) -> Result<Vec<ExportSense>> {
    let mut stmt = conn.prepare(
        "SELECT ds.word_id, ds.pos, dc.text, ds.is_primary
         FROM definition_selections ds
         JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id
         JOIN active_words w ON w.word_id = ds.word_id
         WHERE ds.enabled = 1
         ORDER BY ds.word_id, ds.is_primary DESC, ds.pos",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ExportSense {
                word_id: row.get(0)?,
                pos: row.get(1)?,
                definition: row.get(2)?,
                is_primary: row.get::<_, i64>(3)? != 0,
                audio_hash: None,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn load_examples(conn: &Connection) -> Result<Vec<ExportExample>> {
    let mut stmt = conn.prepare(
        "SELECT es.word_id, es.slot, ec.text, ec.hl_start, ec.hl_end
         FROM example_selections es
         JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
         JOIN active_words w ON w.word_id = es.word_id
         ORDER BY es.word_id, es.slot",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ExportExample {
                word_id: row.get(0)?,
                display_order: row.get(1)?,
                sentence: row.get(2)?,
                hl_start: row.get(3)?,
                hl_end: row.get(4)?,
                audio_hash: None,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn selected_images(conn: &Connection) -> Result<HashMap<i64, String>> {
    let mut stmt = conn.prepare(
        "SELECT s.word_id, c.file_hash
         FROM image_selections s
         JOIN image_candidates c ON c.img_cand_id = s.img_cand_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

fn distractor_counts(conn: &Connection) -> Result<HashMap<i64, usize>> {
    let mut out: HashMap<i64, usize> = HashMap::new();
    for (word_id, _, _) in queries::distractor_edges(conn)? {
        *out.entry(word_id).or_default() += 1;
    }
    Ok(out)
}

/// Words with a selected definition whose extraction is behind the current
/// tokenizer/lemmatizer pair — README Part 5 gate 2, "提取是新鲜的".
fn stale_extraction_words(
    conn: &Connection,
    tokenizer_ver: &str,
    lemmatizer_ver: &str,
) -> Result<std::collections::HashSet<i64>> {
    let mut stmt = conn.prepare(
        "SELECT ds.word_id, dc.text_hash, e.input_hash
         FROM definition_selections ds
         JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id
         LEFT JOIN def_extractions e ON e.def_cand_id = ds.def_cand_id
         WHERE ds.enabled = 1",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stale = std::collections::HashSet::new();
    for (word_id, text_hash, recorded) in rows {
        let expected = morpho_domain::hash::def_extraction_input_hash(
            &text_hash,
            tokenizer_ver,
            lemmatizer_ver,
        );
        if recorded.as_deref() != Some(expected.as_str()) {
            stale.insert(word_id);
        }
    }
    Ok(stale)
}

/// Words with a slot pointing at a candidate that is no longer available.
///
/// All three kinds, because all three reach the release: the enabled senses,
/// every filled example slot (slot 1 is the mode-1 card and the other two are
/// the app's review mode), and the one picture.
fn words_selecting_unavailable_candidates(
    conn: &Connection,
) -> Result<std::collections::HashSet<i64>> {
    let mut stmt = conn.prepare(
        "SELECT ds.word_id FROM definition_selections ds
           JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id
          WHERE ds.enabled = 1 AND dc.status <> 'available'
         UNION
         SELECT es.word_id FROM example_selections es
           JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id
          WHERE ec.status <> 'available'
         UNION
         SELECT isel.word_id FROM image_selections isel
           JOIN image_candidates ic ON ic.img_cand_id = isel.img_cand_id
          WHERE ic.status <> 'available'",
    )?;
    let rows = stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

fn media_registry(conn: &Connection) -> Result<HashMap<String, MediaEntry>> {
    let mut stmt = conn.prepare("SELECT file_hash, kind, rel_path, bytes FROM media_files")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                MediaEntry {
                    kind: row.get(1)?,
                    rel_path: row.get(2)?,
                    bytes: row.get(3)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(core_ready: bool, distractors: usize, fresh: bool) -> ExportWord {
        available(core_ready, distractors, fresh, true)
    }

    fn available(
        core_ready: bool,
        distractors: usize,
        fresh: bool,
        selections_available: bool,
    ) -> ExportWord {
        ExportWord {
            word_id: 1,
            lemma: "serene".into(),
            phonetic: None,
            frequency_rank: Some(10),
            role: Role::Target,
            etymology: None,
            learning_order: 1,
            group_seq: 1,
            image_file_hash: Some("h".into()),
            word_audio_hash: Some("a".into()),
            core_ready,
            blockers: Vec::new(),
            distractor_count: distractors,
            extraction_fresh: fresh,
            selections_available,
        }
    }

    #[test]
    fn a_complete_word_is_shippable() {
        assert!(word(true, 3, true).shippable());
    }

    #[test]
    fn each_gate_can_block_on_its_own() {
        assert!(!word(false, 3, true).shippable());
        assert!(!word(true, 2, true).shippable());
        assert!(!word(true, 3, false).shippable());
        assert!(!available(true, 3, true, false).shippable());
    }

    /// A word whose slot points at a rejected candidate is held back under its
    /// own name, even when everything the reconciler cached about it still says
    /// the word is ready — which is the state that made this necessary.
    #[test]
    fn a_rejected_selection_blocks_under_its_own_cause() {
        let subject = available(true, 3, true, false);
        assert!(!subject.shippable());
        assert_eq!(subject.gate_blockers(), vec![REJECTED_SELECTION]);
    }

    #[test]
    fn gate_blockers_add_the_export_only_causes() {
        let mut subject = word(true, 1, false);
        subject.blockers = vec!["missing_image".into()];
        let causes = subject.gate_blockers();
        assert_eq!(
            causes,
            vec!["missing_image", "distractors_unbound", STALE_EXTRACTION]
        );
    }

    #[test]
    fn gate_blockers_drop_distractor_readiness() {
        let mut subject = word(true, 3, true);
        subject.blockers = vec![
            "missing_image".into(),
            "distractor_1_not_ready".into(),
            "distractor_3_not_ready".into(),
        ];
        assert_eq!(subject.gate_blockers(), vec!["missing_image"]);
    }

    #[test]
    fn a_word_already_reporting_unbound_is_not_reported_twice() {
        let mut subject = word(true, 0, true);
        subject.blockers = vec!["distractors_unbound".into()];
        assert_eq!(subject.gate_blockers(), vec!["distractors_unbound"]);
    }
}
