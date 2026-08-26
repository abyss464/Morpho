//! Scoring and automatic selection (README Part 3 §"选择语义").
//!
//! 1. candidates whose `scorer_ver` is behind get rescored;
//! 2. an empty slot takes the highest-scoring available candidate;
//! 3. an `auto`, unpinned slot only switches when a challenger clears the
//!    hysteresis margin;
//! 4. a pinned slot is never touched;
//! 5. the first sense a word gets is marked primary, chosen by the strongest
//!    frequency evidence available.
//!
//! A word's three example slots are the one place where a slot is not an
//! independent race: `UNIQUE (word_id, ex_cand_id)` makes them an assignment,
//! so they are decided together by [`assign_example_slots`], and rule 3 applies
//! to a newcomer entering the set rather than to the order within it.
//!
//! Both halves compute from one read snapshot and commit one write, and the
//! write re-checks what the read saw — so a human editing a slot mid-sweep
//! wins, and the discarded decision is simply recomputed next pass.

use std::collections::{BTreeMap, HashMap};

use rusqlite::Connection;

use morpho_domain::canon::fold_lemma;
use morpho_domain::event::Actor;
use morpho_domain::types::{
    CandidateKind, DefinitionSource, ExampleSource, ImageSource, Role, SelectedBy, SlotRef,
};
use morpho_domain::version::SCORER_ALGO_VER;
use morpho_store::error::Result;
use morpho_store::ops::{ApplyAutoSelections, ApplyScores, AutoSelection, ScoreUpdate};
use morpho_store::{Store, WriteOp};

use crate::engine::EngineContext;
use crate::score::{
    self, DefinitionFacts, ExampleFacts, ImageFacts, Scored, TokenCoverage, HYSTERESIS_DELTA,
};
use crate::text::TextPipeline;

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// Rescore every candidate whose `scorer_ver` is not the current one.
pub async fn score_candidates(store: &Store, context: &EngineContext) -> Result<usize> {
    let pipeline = context.pipeline.clone();
    let updates = store
        .read(move |conn| collect_scores(conn, &pipeline))
        .await?;
    if updates.is_empty() {
        return Ok(0);
    }
    let count = updates.len();
    store
        .write(
            Actor::Reconciler,
            WriteOp::ApplyScores(ApplyScores {
                updates,
                scorer_ver: SCORER_ALGO_VER.to_string(),
            }),
        )
        .await?;
    Ok(count)
}

fn collect_scores(conn: &Connection, pipeline: &TextPipeline) -> Result<Vec<ScoreUpdate>> {
    let lexicon = Lexicon::load(conn)?;
    let mut updates = Vec::new();

    // -- definitions: coverage comes from the cached extraction ------------
    let mut stmt = conn.prepare(
        "SELECT dc.def_cand_id, dc.source, w.lemma,
                (SELECT COUNT(*) FROM def_tokens t WHERE t.def_cand_id = dc.def_cand_id),
                (SELECT COUNT(*) FROM def_tokens t JOIN words x ON x.lemma = t.lemma
                  WHERE t.def_cand_id = dc.def_cand_id AND x.role = 'base'),
                (SELECT COUNT(*) FROM def_tokens t JOIN words x ON x.lemma = t.lemma
                  WHERE t.def_cand_id = dc.def_cand_id AND x.role IN ('target','auxiliary')),
                EXISTS (SELECT 1 FROM def_tokens t
                         WHERE t.def_cand_id = dc.def_cand_id AND t.lemma = w.lemma)
         FROM definition_candidates dc
         JOIN words w ON w.word_id = dc.word_id
         JOIN def_extractions e ON e.def_cand_id = dc.def_cand_id
         WHERE dc.status = 'available'
           AND (dc.scorer_ver IS NULL OR dc.scorer_ver <> ?1)",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![SCORER_ALGO_VER], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)? != 0,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (def_cand_id, source, total, base, in_scope, self_ref) in rows {
        let source = source
            .parse::<DefinitionSource>()
            .unwrap_or(DefinitionSource::Freedict);
        let coverage = TokenCoverage {
            base: base as usize,
            in_scope: in_scope as usize,
            out_of_scope: (total - base - in_scope).max(0) as usize,
        };
        let scored = score::score_definition(&DefinitionFacts {
            source,
            coverage,
            self_referential: self_ref,
        });
        updates.push(update(CandidateKind::Definition, def_cand_id, &scored));
    }

    // -- examples: tokenized on the fly, there is no cached extraction -----
    let mut stmt = conn.prepare(
        "SELECT ec.ex_cand_id, ec.source, ec.text, ec.hl_start, ec.hl_end, w.lemma
         FROM example_candidates ec
         JOIN words w ON w.word_id = ec.word_id
         WHERE ec.status = 'available'
           AND (ec.scorer_ver IS NULL OR ec.scorer_ver <> ?1)",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![SCORER_ALGO_VER], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (ex_cand_id, source, text, hl_start, hl_end, lemma) in rows {
        let source = source
            .parse::<ExampleSource>()
            .unwrap_or(ExampleSource::ExamCorpus);
        let coverage = lexicon.coverage(&pipeline.extract(&text));
        let highlight_valid = highlight_is_valid(&text, hl_start, hl_end, &lemma);
        let scored = score::score_example(&ExampleFacts {
            source,
            coverage,
            highlight_valid,
        });
        updates.push(update(CandidateKind::Example, ex_cand_id, &scored));
    }

    // -- images -------------------------------------------------------------
    let mut stmt = conn.prepare(
        "SELECT ic.img_cand_id, ic.source, ic.width, ic.height, ic.pos,
                (SELECT ds.pos FROM definition_selections ds
                  WHERE ds.word_id = ic.word_id AND ds.is_primary = 1)
         FROM image_candidates ic
         WHERE ic.status = 'available'
           AND (ic.scorer_ver IS NULL OR ic.scorer_ver <> ?1)",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![SCORER_ALGO_VER], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (img_cand_id, source, width, height, pos, primary_pos) in rows {
        let source = source.parse::<ImageSource>().unwrap_or(ImageSource::Manual);
        // No hint is neutral, not wrong.
        let pos_matches_primary = match (&pos, &primary_pos) {
            (Some(hint), Some(primary)) => hint == primary,
            _ => true,
        };
        let scored = score::score_image(&ImageFacts {
            source,
            width,
            height,
            pos_matches_primary,
        });
        updates.push(update(CandidateKind::Image, img_cand_id, &scored));
    }

    Ok(updates)
}

fn update(kind: CandidateKind, cand_id: i64, scored: &Scored) -> ScoreUpdate {
    ScoreUpdate {
        kind,
        cand_id,
        auto_score: scored.score,
        detail_json: scored.detail_json(),
    }
}

/// Does the stored highlight actually cover the target word?
fn highlight_is_valid(text: &str, hl_start: i64, hl_end: i64, lemma: &str) -> bool {
    if hl_start < 0 || hl_end <= hl_start || hl_end as usize > text.len() {
        return false;
    }
    let (start, end) = (hl_start as usize, hl_end as usize);
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return false;
    }
    let slice = fold_lemma(&text[start..end]);
    let lemma = fold_lemma(lemma);
    // The highlighted span is the word or an inflection of it.
    slice.starts_with(&lemma) || lemma.starts_with(&slice)
}

/// Lemma → role, for readability scoring of text that has no cached extraction.
struct Lexicon {
    roles: HashMap<String, Role>,
}

impl Lexicon {
    fn load(conn: &Connection) -> Result<Self> {
        let mut stmt = conn.prepare("SELECT lemma, role FROM words")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Self {
            roles: rows
                .into_iter()
                .filter_map(|(lemma, role)| Some((fold_lemma(&lemma), role.parse().ok()?)))
                .collect(),
        })
    }

    fn coverage(&self, tokens: &[morpho_domain::types::ExtractedToken]) -> TokenCoverage {
        let mut coverage = TokenCoverage::default();
        for token in tokens {
            match self.roles.get(&fold_lemma(&token.lemma)) {
                Some(Role::Base) => coverage.base += 1,
                Some(Role::Target) | Some(Role::Auxiliary) => coverage.in_scope += 1,
                None => coverage.out_of_scope += 1,
            }
        }
        coverage
    }
}

// ---------------------------------------------------------------------------
// Automatic selection
// ---------------------------------------------------------------------------

/// One candidate as the selector sees it.
#[derive(Debug, Clone)]
struct Choice {
    cand_id: i64,
    score: f64,
}

/// The current state of one slot.
#[derive(Debug, Clone, Copy)]
struct SlotState {
    cand_id: i64,
    pinned: bool,
    score: Option<f64>,
}

/// Run automatic selection over every slot of every active word.
pub async fn auto_select(store: &Store, _context: &EngineContext) -> Result<usize> {
    let decisions = store.read(collect_selections).await?;
    if decisions.is_empty() {
        return Ok(0);
    }
    let outcome = store
        .write(
            Actor::Reconciler,
            WriteOp::ApplyAutoSelections(ApplyAutoSelections {
                selections: decisions,
            }),
        )
        .await?;
    Ok(match outcome.result {
        morpho_store::WriteResult::Selected { applied, .. } => applied,
        _ => 0,
    })
}

fn collect_selections(conn: &Connection) -> Result<Vec<AutoSelection>> {
    let mut decisions = Vec::new();

    // -- definitions -------------------------------------------------------
    let mut stmt = conn.prepare(
        "SELECT dc.word_id, dc.pos, dc.def_cand_id, COALESCE(dc.auto_score, 0.0), dc.source
         FROM definition_candidates dc
         JOIN active_words w ON w.word_id = dc.word_id
         WHERE dc.status = 'available'
         ORDER BY dc.word_id, dc.pos, dc.def_cand_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut by_word: BTreeMap<i64, BTreeMap<String, Vec<Choice>>> = BTreeMap::new();
    let mut evidence: HashMap<i64, Vec<PosEvidence>> = HashMap::new();
    for (word_id, pos, cand_id, auto_score, source) in rows {
        let source_rank = match source.parse::<DefinitionSource>() {
            Ok(DefinitionSource::Manual) => 0,
            Ok(DefinitionSource::LlmRewrite) => 1,
            Ok(DefinitionSource::Freedict) => 2,
            _ => 3,
        };
        let bucket = evidence.entry(word_id).or_default();
        match bucket.iter_mut().find(|entry| entry.pos == pos) {
            Some(entry) => {
                entry.candidates += 1;
                entry.source_rank = entry.source_rank.min(source_rank);
                entry.first_cand_id = entry.first_cand_id.min(cand_id);
            }
            None => bucket.push(PosEvidence {
                pos: pos.clone(),
                candidates: 1,
                source_rank,
                first_cand_id: cand_id,
            }),
        }
        by_word
            .entry(word_id)
            .or_default()
            .entry(pos)
            .or_default()
            .push(Choice {
                cand_id,
                score: auto_score,
            });
    }

    let def_slots = definition_slots(conn)?;
    let words_with_primary = words_with_primary(conn)?;

    for (word_id, positions) in &by_word {
        let evidence_pos = evidence
            .get(word_id)
            .and_then(|entries| strongest_pos(entries));
        let needs_primary = !words_with_primary.contains(word_id);
        // Apply the evidence slot first so it is the one that becomes primary.
        let mut ordered: Vec<&String> = positions.keys().collect();
        ordered.sort_by_key(|pos| (Some(*pos) != evidence_pos.as_ref(), (*pos).clone()));

        for pos in ordered {
            let choices = &positions[pos];
            let state = def_slots.get(&(*word_id, pos.clone())).copied();
            let Some(choice) = pick(choices, state) else {
                continue;
            };
            decisions.push(AutoSelection {
                slot: SlotRef::Definition {
                    word_id: *word_id,
                    pos: pos.clone(),
                },
                cand_id: choice,
                expected_cand_id: state.map(|s| s.cand_id),
                make_primary: needs_primary && Some(pos) == evidence_pos.as_ref(),
            });
        }
    }

    // -- examples: three ordered slots ------------------------------------
    let mut stmt = conn.prepare(
        "SELECT ec.word_id, ec.ex_cand_id, COALESCE(ec.auto_score, 0.0)
         FROM example_candidates ec
         JOIN active_words w ON w.word_id = ec.word_id
         WHERE ec.status = 'available'
         ORDER BY ec.word_id, ec.ex_cand_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut examples: BTreeMap<i64, Vec<Choice>> = BTreeMap::new();
    for (word_id, cand_id, auto_score) in rows {
        examples.entry(word_id).or_default().push(Choice {
            cand_id,
            score: auto_score,
        });
    }
    let example_slots = example_slot_states(conn)?;

    for (word_id, choices) in &examples {
        let mut ranked = choices.clone();
        ranked.sort_by(rank_choices);

        let states: [Option<SlotState>; 3] =
            [1, 2, 3].map(|slot| example_slots.get(&(*word_id, slot)).copied());
        let wanted = assign_example_slots(&ranked, states);

        for (index, want) in wanted.iter().enumerate() {
            let state = states[index];
            let (Some(cand_id), false) = (*want, state.is_some_and(|s| s.pinned)) else {
                continue;
            };
            if state.map(|s| s.cand_id) == Some(cand_id) {
                continue;
            }
            decisions.push(AutoSelection {
                slot: SlotRef::Example {
                    word_id: *word_id,
                    slot: index as i64 + 1,
                },
                cand_id,
                expected_cand_id: state.map(|s| s.cand_id),
                make_primary: false,
            });
        }
    }

    // -- images: exactly one slot ------------------------------------------
    let mut stmt = conn.prepare(
        "SELECT ic.word_id, ic.img_cand_id, COALESCE(ic.auto_score, 0.0)
         FROM image_candidates ic
         JOIN active_words w ON w.word_id = ic.word_id
         WHERE ic.status = 'available'
         ORDER BY ic.word_id, ic.img_cand_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut by_word: BTreeMap<i64, Vec<Choice>> = BTreeMap::new();
    for (word_id, cand_id, auto_score) in rows {
        by_word.entry(word_id).or_default().push(Choice {
            cand_id,
            score: auto_score,
        });
    }
    let image_slots = image_slot_states(conn)?;
    for (word_id, choices) in &by_word {
        let state = image_slots.get(word_id).copied();
        let Some(cand_id) = pick(choices, state) else {
            continue;
        };
        decisions.push(AutoSelection {
            slot: SlotRef::Image { word_id: *word_id },
            cand_id,
            expected_cand_id: state.map(|s| s.cand_id),
            make_primary: false,
        });
    }

    Ok(decisions)
}

/// What the sources say about one part of speech of one word.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PosEvidence {
    pos: String,
    /// How many distinct senses the sources recorded under this part of speech.
    candidates: usize,
    /// Best (lowest) source rank seen: manual < llm_rewrite < freedict < wordnet.
    source_rank: u8,
    first_cand_id: i64,
}

/// Rule 5: which part of speech gets `is_primary` on a word's first selection.
///
/// "Strongest frequency evidence available" is, concretely, the number of
/// senses a dictionary records: a word used mostly as an adjective accumulates
/// more adjective senses than noun ones. Sense *count* is cross-part-of-speech
/// evidence; sense *order* is not — the Free Dictionary orders senses within a
/// part of speech, and its first block is whichever one it happened to list
/// first, which is how "vivid" ends up defined as a felt-tip pen.
///
/// Ties fall back to source authority and then to the lowest candidate id, so
/// the answer is a pure function of the lexicon.
fn strongest_pos(evidence: &[PosEvidence]) -> Option<String> {
    evidence
        .iter()
        .max_by(|a, b| {
            a.candidates
                .cmp(&b.candidates)
                .then_with(|| b.source_rank.cmp(&a.source_rank))
                .then_with(|| b.first_cand_id.cmp(&a.first_cand_id))
        })
        .map(|entry| entry.pos.clone())
}

/// Decide all three example slots at once.
///
/// The three slots are an *assignment*, not three separate races —
/// `UNIQUE (word_id, ex_cand_id)` forbids one sentence filling two of them, so
/// asking each slot independently produces answers that cannot all be true.
/// Deciding them together also lets the hysteresis margin mean the right thing:
///
/// * a **newcomer** taking a slot from an incumbent is what rule 3 protects
///   against, so it must clear the margin. When it cannot, the whole assignment
///   is held back — a partial one would be a different assignment than the
///   ranking asked for, and the next pass would compute the same partial answer
///   forever;
/// * a **reorder** among sentences the word already shows changes no content at
///   all, only which one is the mode-1 card, and is a pure function of the
///   ranking. There is nothing to oscillate, so the margin does not apply.
///
/// Returns the desired occupant of slots 1, 2 and 3. Pinned slots keep what
/// they hold. A slot may come back empty when the word has fewer sentences than
/// slots; the assignment compacts towards slot 1, which is the one readiness
/// depends on.
fn assign_example_slots(ranked: &[Choice], states: [Option<SlotState>; 3]) -> [Option<i64>; 3] {
    let pinned: Vec<i64> = states
        .iter()
        .flatten()
        .filter(|state| state.pinned)
        .map(|state| state.cand_id)
        .collect();

    // What the ranking alone asks for, ignoring where anything sits now.
    let mut wanted: [Option<i64>; 3] = [None; 3];
    let mut cursor = 0usize;
    for (slot, want) in wanted.iter_mut().enumerate() {
        if let Some(state) = states[slot].filter(|state| state.pinned) {
            *want = Some(state.cand_id);
            continue;
        }
        while cursor < ranked.len() && pinned.contains(&ranked[cursor].cand_id) {
            cursor += 1;
        }
        let Some(choice) = ranked.get(cursor) else {
            break;
        };
        cursor += 1;
        *want = Some(choice.cand_id);
    }

    let held: Vec<i64> = states.iter().flatten().map(|state| state.cand_id).collect();
    let score = |cand_id: i64| {
        ranked
            .iter()
            .find(|choice| choice.cand_id == cand_id)
            .map_or(0.0, |choice| choice.score)
    };
    for (slot, want) in wanted.iter().enumerate() {
        let (Some(want), Some(state)) = (*want, states[slot]) else {
            continue;
        };
        if state.cand_id == want || held.contains(&want) {
            continue;
        }
        // Rule 3, and the only place it applies here.
        if score(want) <= state.score.unwrap_or(0.0) + HYSTERESIS_DELTA {
            return states.map(|state| state.map(|state| state.cand_id));
        }
    }
    wanted
}

/// Rules 2–4: pick a candidate for a slot, or `None` to leave it alone.
fn pick(choices: &[Choice], state: Option<SlotState>) -> Option<i64> {
    if choices.is_empty() {
        return None;
    }
    if state.is_some_and(|s| s.pinned) {
        return None;
    }
    let best = choices.iter().min_by(|a, b| rank_choices(a, b))?;
    match state {
        // Rule 2: an empty slot takes the best candidate outright.
        None => Some(best.cand_id),
        Some(current) => {
            if current.cand_id == best.cand_id {
                return None;
            }
            // Rule 3: switch only past the hysteresis margin.
            let incumbent = current.score.unwrap_or(0.0);
            (best.score > incumbent + HYSTERESIS_DELTA).then_some(best.cand_id)
        }
    }
}

/// Higher score first, then lowest id — a total, deterministic order even when
/// two candidates score identically.
fn rank_choices(a: &Choice, b: &Choice) -> std::cmp::Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.cand_id.cmp(&b.cand_id))
}

fn definition_slots(conn: &Connection) -> Result<HashMap<(i64, String), SlotState>> {
    let mut stmt = conn.prepare(
        "SELECT ds.word_id, ds.pos, ds.def_cand_id, ds.pinned, dc.auto_score
         FROM definition_selections ds
         JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                (row.get::<_, i64>(0)?, row.get::<_, String>(1)?),
                SlotState {
                    cand_id: row.get(2)?,
                    pinned: row.get::<_, i64>(3)? != 0,
                    score: row.get(4)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

fn example_slot_states(conn: &Connection) -> Result<HashMap<(i64, i64), SlotState>> {
    let mut stmt = conn.prepare(
        "SELECT es.word_id, es.slot, es.ex_cand_id, es.pinned, ec.auto_score
         FROM example_selections es
         JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                (row.get::<_, i64>(0)?, row.get::<_, i64>(1)?),
                SlotState {
                    cand_id: row.get(2)?,
                    pinned: row.get::<_, i64>(3)? != 0,
                    score: row.get(4)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

fn image_slot_states(conn: &Connection) -> Result<HashMap<i64, SlotState>> {
    let mut stmt = conn.prepare(
        "SELECT s.word_id, s.img_cand_id, s.pinned, c.auto_score
         FROM image_selections s
         JOIN image_candidates c ON c.img_cand_id = s.img_cand_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                SlotState {
                    cand_id: row.get(1)?,
                    pinned: row.get::<_, i64>(2)? != 0,
                    score: row.get(3)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

fn words_with_primary(conn: &Connection) -> Result<std::collections::HashSet<i64>> {
    let mut stmt =
        conn.prepare("SELECT word_id FROM definition_selections WHERE is_primary = 1")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

/// Marker so the unused-parameter lint stays quiet about `SelectedBy`, which
/// the write op supplies.
const _: Option<SelectedBy> = None;

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(cand_id: i64, score: f64) -> Choice {
        Choice { cand_id, score }
    }

    #[test]
    fn an_empty_slot_takes_the_best_candidate() {
        let choices = vec![choice(1, 0.4), choice(2, 0.9), choice(3, 0.6)];
        assert_eq!(pick(&choices, None), Some(2));
    }

    #[test]
    fn a_tie_breaks_on_the_lowest_id() {
        let choices = vec![choice(7, 0.8), choice(3, 0.8)];
        assert_eq!(pick(&choices, None), Some(3));
    }

    #[test]
    fn a_pinned_slot_is_never_touched() {
        let choices = vec![choice(1, 0.99)];
        let pinned = SlotState {
            cand_id: 5,
            pinned: true,
            score: Some(0.1),
        };
        assert_eq!(pick(&choices, Some(pinned)), None);
    }

    #[test]
    fn a_near_tie_does_not_move_the_slot() {
        let choices = vec![choice(1, 0.82)];
        let current = SlotState {
            cand_id: 5,
            pinned: false,
            score: Some(0.80),
        };
        assert_eq!(pick(&choices, Some(current)), None);
    }

    #[test]
    fn a_clear_winner_moves_the_slot() {
        let choices = vec![choice(1, 0.95)];
        let current = SlotState {
            cand_id: 5,
            pinned: false,
            score: Some(0.80),
        };
        assert_eq!(pick(&choices, Some(current)), Some(1));
    }

    #[test]
    fn the_incumbent_is_left_alone_when_it_is_already_the_best() {
        let choices = vec![choice(5, 0.9), choice(6, 0.5)];
        let current = SlotState {
            cand_id: 5,
            pinned: false,
            score: Some(0.9),
        };
        assert_eq!(pick(&choices, Some(current)), None);
    }

    #[test]
    fn an_unscored_incumbent_still_gets_hysteresis() {
        let choices = vec![choice(1, 0.04)];
        let current = SlotState {
            cand_id: 5,
            pinned: false,
            score: None,
        };
        assert_eq!(pick(&choices, Some(current)), None, "0.04 < 0 + delta");
        let choices = vec![choice(1, 0.2)];
        assert_eq!(pick(&choices, Some(current)), Some(1));
    }

    #[test]
    fn no_candidates_means_no_decision() {
        assert_eq!(pick(&[], None), None);
    }

    // -- example slot assignment ------------------------------------------

    fn filled(cand_id: i64, score: f64) -> Option<SlotState> {
        Some(SlotState {
            cand_id,
            pinned: false,
            score: Some(score),
        })
    }

    fn held_pin(cand_id: i64, score: f64) -> Option<SlotState> {
        Some(SlotState {
            cand_id,
            pinned: true,
            score: Some(score),
        })
    }

    #[test]
    fn empty_slots_take_the_top_three_in_order() {
        let ranked = vec![
            choice(1, 0.9),
            choice(2, 0.8),
            choice(3, 0.7),
            choice(4, 0.6),
        ];
        assert_eq!(
            assign_example_slots(&ranked, [None, None, None]),
            [Some(1), Some(2), Some(3)]
        );
    }

    #[test]
    fn fewer_sentences_than_slots_leaves_the_tail_empty() {
        let ranked = vec![choice(1, 0.9), choice(2, 0.8)];
        assert_eq!(
            assign_example_slots(&ranked, [None, None, None]),
            [Some(1), Some(2), None]
        );
        assert_eq!(
            assign_example_slots(&[], [None, None, None]),
            [None, None, None]
        );
    }

    #[test]
    fn a_settled_assignment_is_left_exactly_as_it_is() {
        let ranked = vec![choice(1, 0.9), choice(2, 0.8), choice(3, 0.7)];
        let states = [filled(1, 0.9), filled(2, 0.8), filled(3, 0.7)];
        assert_eq!(
            assign_example_slots(&ranked, states),
            [Some(1), Some(2), Some(3)]
        );
    }

    /// The crash this replaced: a re-rank that swaps two slots is one
    /// permutation, and asking each slot on its own produced half of it.
    #[test]
    fn a_reorder_of_sentences_the_word_already_shows_is_applied_whole() {
        // Slot 1 holds 7, slot 2 holds 9; rescoring puts 9 ahead.
        let ranked = vec![choice(9, 0.90), choice(7, 0.88), choice(4, 0.40)];
        let states = [filled(7, 0.88), filled(9, 0.90), None];
        assert_eq!(
            assign_example_slots(&ranked, states),
            [Some(9), Some(7), Some(4)],
            "the swap must be complete, never half of it"
        );
    }

    /// Reordering costs nothing and cannot oscillate, so the margin — which
    /// exists to stop a challenger flip-flopping with an incumbent — does not
    /// hold a swap back even when the two are a hair apart.
    #[test]
    fn a_hairs_breadth_reorder_is_not_held_back() {
        let ranked = vec![choice(9, 0.801), choice(7, 0.800)];
        let states = [filled(7, 0.800), filled(9, 0.801)];
        assert_eq!(
            assign_example_slots(&ranked, [states[0], states[1], None]),
            [Some(9), Some(7), None]
        );
    }

    #[test]
    fn a_newcomer_must_clear_the_margin_before_it_takes_a_slot() {
        let ranked = vec![choice(50, 0.82), choice(7, 0.80), choice(9, 0.79)];
        let states = [filled(7, 0.80), filled(9, 0.79), None];
        assert_eq!(
            assign_example_slots(&ranked, states),
            [Some(7), Some(9), None],
            "0.82 does not beat 0.80 by the margin"
        );

        let ranked = vec![choice(50, 0.95), choice(7, 0.80), choice(9, 0.79)];
        assert_eq!(
            assign_example_slots(&ranked, states),
            [Some(50), Some(7), Some(9)],
            "0.95 does, and the incumbents shift down rather than vanish"
        );
    }

    #[test]
    fn a_newcomer_fills_an_empty_slot_outright() {
        // Rule 2: nothing to displace, so nothing to clear.
        let ranked = vec![choice(7, 0.80), choice(50, 0.10)];
        let states = [filled(7, 0.80), None, None];
        assert_eq!(
            assign_example_slots(&ranked, states),
            [Some(7), Some(50), None]
        );
    }

    #[test]
    fn a_pinned_slot_keeps_its_sentence_and_reserves_it() {
        let ranked = vec![choice(1, 0.9), choice(2, 0.8), choice(3, 0.7)];
        let states = [None, held_pin(1, 0.9), None];
        assert_eq!(
            assign_example_slots(&ranked, states),
            [Some(2), Some(1), Some(3)],
            "the pinned sentence stays put and is never dealt twice"
        );
    }

    #[test]
    fn every_assignment_is_free_of_duplicates() {
        let ranked = vec![
            choice(1, 0.9),
            choice(2, 0.8),
            choice(3, 0.7),
            choice(4, 0.6),
        ];
        let cases = [
            [None, None, None],
            [filled(3, 0.7), None, filled(1, 0.9)],
            [held_pin(4, 0.6), filled(2, 0.8), None],
            [filled(1, 0.9), filled(2, 0.8), filled(3, 0.7)],
            [held_pin(1, 0.9), held_pin(2, 0.8), held_pin(3, 0.7)],
        ];
        for states in cases {
            let wanted = assign_example_slots(&ranked, states);
            let occupied: Vec<i64> = wanted.iter().flatten().copied().collect();
            let unique: std::collections::HashSet<i64> = occupied.iter().copied().collect();
            assert_eq!(occupied.len(), unique.len(), "{states:?} -> {wanted:?}");
        }
    }

    /// Slot 1 is the only slot readiness depends on, so an assignment always
    /// compacts towards it rather than leaving a hole at the front.
    #[test]
    fn an_emptied_front_slot_is_compacted_into() {
        let ranked = vec![choice(2, 0.8), choice(3, 0.7)];
        let states = [None, filled(2, 0.8), filled(3, 0.7)];
        assert_eq!(
            assign_example_slots(&ranked, states),
            [Some(2), Some(3), None]
        );
    }

    /// Same inputs, same answer: nothing here depends on iteration order or on
    /// how many passes have run, which is what makes the loop converge.
    #[test]
    fn the_assignment_is_a_pure_function_of_the_ranking() {
        let ranked = vec![choice(9, 0.9), choice(7, 0.8), choice(4, 0.7)];
        let states = [filled(7, 0.8), filled(9, 0.9), None];
        let once = assign_example_slots(&ranked, states);
        assert_eq!(once, assign_example_slots(&ranked, states));
        // And applying it reaches a fixed point.
        let settled = [filled(9, 0.9), filled(7, 0.8), filled(4, 0.7)];
        assert_eq!(assign_example_slots(&ranked, settled), once);
    }

    fn evidence(pos: &str, candidates: usize, source_rank: u8, first: i64) -> PosEvidence {
        PosEvidence {
            pos: pos.to_string(),
            candidates,
            source_rank,
            first_cand_id: first,
        }
    }

    #[test]
    fn the_part_of_speech_with_the_most_senses_becomes_primary() {
        // "vivid": the Free Dictionary lists one noun sense (a felt-tip pen)
        // before three adjective ones. Sense count is the real evidence.
        let vivid = vec![evidence("noun", 1, 2, 10), evidence("adj", 3, 2, 11)];
        assert_eq!(strongest_pos(&vivid).as_deref(), Some("adj"));
    }

    #[test]
    fn an_authoritative_source_breaks_a_count_tie() {
        let tied = vec![evidence("noun", 2, 2, 10), evidence("verb", 2, 0, 40)];
        assert_eq!(strongest_pos(&tied).as_deref(), Some("verb"));
    }

    #[test]
    fn a_full_tie_breaks_on_the_lowest_candidate_id() {
        let tied = vec![evidence("verb", 2, 2, 40), evidence("noun", 2, 2, 10)];
        assert_eq!(strongest_pos(&tied).as_deref(), Some("noun"));
        // And the answer does not depend on input order.
        let reversed: Vec<PosEvidence> = tied.into_iter().rev().collect();
        assert_eq!(strongest_pos(&reversed).as_deref(), Some("noun"));
    }

    #[test]
    fn a_single_part_of_speech_is_trivially_primary() {
        assert_eq!(
            strongest_pos(&[evidence("adj", 1, 2, 5)]).as_deref(),
            Some("adj")
        );
        assert_eq!(strongest_pos(&[]), None);
    }

    #[test]
    fn a_highlight_covering_the_word_is_valid() {
        let text = "He is a benevolent man.";
        assert!(highlight_is_valid(text, 8, 18, "benevolent"));
    }

    #[test]
    fn a_highlight_covering_an_inflection_is_valid() {
        let text = "Species adapted quickly.";
        assert!(highlight_is_valid(text, 8, 15, "adapt"));
    }

    #[test]
    fn a_highlight_on_the_wrong_word_is_invalid() {
        let text = "He is a benevolent man.";
        assert!(!highlight_is_valid(text, 0, 2, "benevolent"));
    }

    #[test]
    fn an_out_of_range_highlight_is_invalid() {
        let text = "short";
        assert!(!highlight_is_valid(text, 0, 99, "short"));
        assert!(!highlight_is_valid(text, -1, 3, "short"));
        assert!(!highlight_is_valid(text, 3, 3, "short"));
    }

    #[test]
    fn a_highlight_splitting_a_character_is_invalid() {
        let text = "café time";
        // 'é' spans bytes 3..5.
        assert!(!highlight_is_valid(text, 0, 4, "café"));
    }
}
