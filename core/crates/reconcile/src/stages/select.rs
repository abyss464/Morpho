//! Scoring and automatic selection (README Part 3 §"选择语义").
//!
//! 1. candidates whose `scorer_ver` is behind get rescored;
//! 2. an empty slot takes the highest-scoring available candidate;
//! 3. an `auto`, unpinned slot only switches when a challenger clears the
//!    hysteresis margin;
//! 4. a pinned slot is never touched;
//! 5. the first sense a word gets is marked primary, chosen by the strongest
//!    frequency evidence available;
//! 6. a primary the reconciler picked moves when that evidence does — an
//!    `is_primary` set on a word's first selected sense is otherwise a fossil
//!    of whichever part of speech a source happened to deliver first. A primary
//!    an editor placed is never moved.
//!
//! A word's three example slots are the one place where a slot is not an
//! independent race: `UNIQUE (word_id, ex_cand_id)` makes them an assignment,
//! so they are decided together by [`assign_example_slots`], and rule 3 applies
//! to a newcomer entering the set rather than to the order within it.
//!
//! Both halves compute from one read snapshot and commit one write, and the
//! write re-checks what the read saw — so a human editing a slot mid-sweep
//! wins, and the discarded decision is simply recomputed next pass.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::Connection;

use morpho_domain::canon::fold_lemma;
use morpho_domain::event::Actor;
use morpho_domain::types::{
    CandidateKind, CandidateStatus, DefinitionSource, ExampleSource, ImageSource, Role, SelectedBy,
    SlotRef,
};
use morpho_domain::version::SCORER_ALGO_VER;
use morpho_store::error::Result;
use morpho_store::ops::{
    ApplyAutoSelections, ApplyScores, AutoSelection, PrimaryMove, ReconcilePrimaries,
    ReleaseInvalidSelection, ScoreUpdate,
};
use morpho_store::{Store, WriteOp};

use crate::engine::EngineContext;
use crate::facts;
use crate::score::{
    self, DefinitionFacts, ExampleFacts, ImageFacts, ImageStrategy, Scored, TokenCoverage,
    HYSTERESIS_DELTA,
};
#[cfg(test)]
use crate::score::{CLIP_CEIL, CLIP_FLOOR};
use crate::sources::wordnet::{WnPos, WordNet};
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
    //
    // Self-reference is read off the candidate's own text rather than off
    // `def_tokens`, which is a separate artifact that lands *after* the
    // candidate: scoring against it means scoring whatever the extraction
    // happened to hold at the time, and a `scorer_ver` that never changes
    // freezes that answer forever. `sense_rank` is the candidate's position in
    // its source's list for this word and part of speech — see
    // [`super::super::score::sense_rank_prior`] for why that is the best
    // frequency evidence available in-process.
    let mut stmt = conn.prepare(
        "SELECT dc.def_cand_id, dc.source, w.lemma, dc.text,
                (SELECT COUNT(*) FROM def_tokens t WHERE t.def_cand_id = dc.def_cand_id),
                (SELECT COUNT(*) FROM def_tokens t JOIN words x ON x.lemma = t.lemma
                  WHERE t.def_cand_id = dc.def_cand_id AND x.role = 'base'),
                (SELECT COUNT(*) FROM def_tokens t JOIN words x ON x.lemma = t.lemma
                  WHERE t.def_cand_id = dc.def_cand_id AND x.role IN ('target','auxiliary')),
                (SELECT COUNT(*) FROM definition_candidates p
                  WHERE p.word_id = dc.word_id AND p.pos = dc.pos AND p.source = dc.source
                    AND p.def_cand_id <= dc.def_cand_id)
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
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (def_cand_id, source, lemma, text, total, base, in_scope, rank) in rows {
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
            self_referential: score::is_self_referential(&lemma, &text),
            sense_rank: sense_rank(source, rank),
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
                  WHERE ds.word_id = ic.word_id AND ds.is_primary = 1),
                ic.source_ref
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
                row.get::<_, Option<String>>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (img_cand_id, source, width, height, pos, primary_pos, source_ref) in rows {
        let source = source.parse::<ImageSource>().unwrap_or(ImageSource::Manual);
        // No hint is neutral, not wrong.
        let pos_matches_primary = match (&pos, &primary_pos) {
            (Some(hint), Some(primary)) => hint == primary,
            _ => true,
        };
        // Which pass found it lives in `source_ref` rather than in a column of
        // its own: the provider is the source, and the strategy is provenance
        // about how it was asked, which is what that field already records.
        let strategy = ImageStrategy::from_source_ref(source_ref.as_deref());
        let scored = score::score_image(&ImageFacts {
            source,
            width,
            height,
            pos_matches_primary,
            strategy,
        });
        updates.push(update(CandidateKind::Image, img_cand_id, &scored));
    }

    Ok(updates)
}

/// Where a candidate sits in its source's sense list, if that list means
/// anything.
///
/// A dictionary orders the senses of a word by how common they are, and both
/// harvesters mint them in the order they were handed over — WordNet from
/// `index.pos`, which is corpus-frequency order by construction; the Free
/// Dictionary in the order it printed them. Candidate ids are monotonic and
/// minting is `ON CONFLICT DO NOTHING`, so the id order inside one
/// `(word, pos, source)` group *is* that list order.
///
/// A manual or rewritten candidate is in no such list — it was written for this
/// word by somebody who meant it — so it has no rank rather than a bad one.
const fn sense_rank(source: DefinitionSource, rank: i64) -> Option<usize> {
    match source {
        DefinitionSource::Freedict | DefinitionSource::Wordnet if rank > 0 => Some(rank as usize),
        _ => None,
    }
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

/// One slot's occupant, and whether it is still content the word may show.
///
/// Rule 4's exception: a selection whose candidate has left
/// `status = 'available'` is not an incumbent to be out-argued, it is an
/// **invalid occupant**. The ranking sees the slot as empty — no hysteresis
/// against a candidate nobody may show, and no pin protecting it — and the pin
/// and the approval that were holding it still are released. What the row
/// points at only moves when there is something available to move it to; the
/// reconciler never empties a slot.
#[derive(Debug, Clone, Copy)]
struct Occupant {
    state: SlotState,
    available: bool,
    approved: bool,
}

impl Occupant {
    /// The slot as the ranking must see it.
    fn incumbent(&self) -> Option<SlotState> {
        self.available.then_some(self.state)
    }

    /// Is there still a pin or an approval to release?
    ///
    /// False the second time round, which is what keeps a word with no
    /// available candidate at all from writing the same two audit rows on every
    /// single pass.
    fn needs_release(&self) -> bool {
        !self.available && (self.state.pinned || self.approved)
    }
}

/// Everything one pass decided.
#[derive(Debug, Default)]
struct Decisions {
    /// Rule 4's exception: slots whose occupant is no longer available.
    releases: Vec<ReleaseInvalidSelection>,
    selections: Vec<AutoSelection>,
    /// Rule 6: words whose primary sense no longer matches the evidence.
    primaries: Vec<PrimaryMove>,
}

/// Run automatic selection over every slot of every active word.
pub async fn auto_select(store: &Store, context: &EngineContext) -> Result<usize> {
    let wordnet = context.sources.wordnet.clone();
    let clip_model_ver = context.images.clip_model_ver();
    let decisions = store
        .read(move |conn| collect_selections(conn, wordnet.as_deref(), &clip_model_ver))
        .await?;
    let mut applied = 0usize;
    if !decisions.selections.is_empty() || !decisions.releases.is_empty() {
        let outcome = store
            .write(
                Actor::Reconciler,
                WriteOp::ApplyAutoSelections(ApplyAutoSelections {
                    releases: decisions.releases,
                    selections: decisions.selections,
                }),
            )
            .await?;
        if let morpho_store::WriteResult::Selected { applied: n, .. } = outcome.result {
            applied += n;
        }
    }
    // Second write, not part of the batch above: a primary move is a decision
    // about a slot that already exists, so it has to land after any selection
    // that creates one. Both are idempotent, so a pass that only gets half way
    // simply finishes on the next one.
    if !decisions.primaries.is_empty() {
        let outcome = store
            .write(
                Actor::Reconciler,
                WriteOp::ReconcilePrimaries(ReconcilePrimaries {
                    moves: decisions.primaries,
                }),
            )
            .await?;
        if let morpho_store::WriteResult::Selected { applied: n, .. } = outcome.result {
            applied += n;
        }
    }
    Ok(applied)
}

fn collect_selections(
    conn: &Connection,
    wordnet: Option<&WordNet>,
    clip_model_ver: &str,
) -> Result<Decisions> {
    let mut decisions = Vec::new();
    // Rule 4's exception, gathered from the slot tables rather than from the
    // pools below: a word whose every candidate was rejected has no pool at
    // all, and it is exactly the word whose pinned, approved slot is pointing
    // at content nobody may show.
    let mut releases = Vec::new();

    // -- definitions -------------------------------------------------------
    let mut stmt = conn.prepare(
        "SELECT dc.word_id, dc.pos, dc.def_cand_id, COALESCE(dc.auto_score, 0.0), dc.source
         FROM definition_candidates dc
         JOIN active_words w ON w.word_id = dc.word_id AND w.zh_gloss IS NULL
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

    let lemmas = word_lemmas(conn)?;
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
                wn_frequency: wordnet
                    .and_then(|db| {
                        let lemma = lemmas.get(&word_id)?;
                        db.pos_frequency(lemma)
                            .into_iter()
                            .find(|(wn_pos, _)| wn_pos.as_str() == pos)
                            .map(|(_, count)| count)
                    })
                    .unwrap_or(0),
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

    // WordNet's evidence is only admissible when it can see the whole word. A
    // lemma whose harvest includes a preposition, conjunction or interjection
    // has a part of speech WordNet models nothing of, and its corpus counts for
    // the remaining ones say only how often people used the half WordNet knows
    // about — "beyond" is tagged as an adverb eleven times and as a preposition
    // never, because there is no preposition to tag. Those words are decided by
    // the harvest, exactly as they were before this signal existed.
    for entries in evidence.values_mut() {
        if entries.iter().any(|entry| !WnPos::models(&entry.pos)) {
            for entry in entries.iter_mut() {
                entry.wn_frequency = 0;
            }
        }
    }

    let def_slots = definition_slots(conn)?;
    for ((word_id, pos), slot) in &def_slots {
        if slot.occupant.needs_release() {
            releases.push(ReleaseInvalidSelection {
                slot: SlotRef::Definition {
                    word_id: *word_id,
                    pos: pos.clone(),
                },
            });
        }
    }
    let primaries = current_primaries(conn)?;
    let hand_moved = words_whose_primary_a_human_moved(conn)?;
    let mut moves = Vec::new();

    for (word_id, positions) in &by_word {
        let ranked = evidence.get(word_id).map(|entries| ranked_pos(entries));
        let evidence_pos = ranked.as_ref().and_then(|order| order.first().cloned());
        let current_primary = primaries.get(word_id);
        let needs_primary = current_primary.is_none();
        // Apply the evidence slot first so it is the one that becomes primary.
        let mut ordered: Vec<&String> = positions.keys().collect();
        ordered.sort_by_key(|pos| (Some(*pos) != evidence_pos.as_ref(), (*pos).clone()));

        for pos in ordered {
            let choices = &positions[pos];
            let slot = def_slots.get(&(*word_id, pos.clone()));
            // An occupant that is no longer available is not an occupant: the
            // decision is taken as if the slot were empty, while the guard the
            // write checks still names the row the rule actually saw.
            let Some(choice) = pick(choices, slot.and_then(|slot| slot.occupant.incumbent()))
            else {
                continue;
            };
            decisions.push(AutoSelection {
                slot: SlotRef::Definition {
                    word_id: *word_id,
                    pos: pos.clone(),
                },
                cand_id: choice,
                expected_cand_id: slot.map(|slot| slot.occupant.state.cand_id),
                make_primary: needs_primary && Some(pos) == evidence_pos.as_ref(),
            });
        }

        // Rule 6: a word that already has a primary keeps it only while the
        // evidence still agrees. See [`ranked_pos`] and `reconcile_primaries`
        // for what "agrees" means and whose decision is never overruled.
        let (Some(current), Some(ranked)) = (current_primary, ranked.as_ref()) else {
            continue;
        };
        if hand_moved.contains(word_id) {
            continue;
        }
        let Some(wanted) = ranked.iter().find(|pos| {
            def_slots
                .get(&(*word_id, (*pos).clone()))
                .is_some_and(|slot| slot.enabled)
        }) else {
            continue;
        };
        if wanted != current {
            moves.push(PrimaryMove {
                word_id: *word_id,
                pos: wanted.clone(),
                expected_pos: Some(current.clone()),
            });
        }
    }

    // -- examples: three ordered slots ------------------------------------
    let mut stmt = conn.prepare(
        "SELECT ec.word_id, ec.ex_cand_id, COALESCE(ec.auto_score, 0.0)
         FROM example_candidates ec
         JOIN active_words w ON w.word_id = ec.word_id AND w.zh_gloss IS NULL
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
    for ((word_id, slot), occupant) in &example_slots {
        if occupant.needs_release() {
            releases.push(ReleaseInvalidSelection {
                slot: SlotRef::Example {
                    word_id: *word_id,
                    slot: *slot,
                },
            });
        }
    }

    for (word_id, choices) in &examples {
        let mut ranked = choices.clone();
        ranked.sort_by(rank_choices);

        let held: [Option<Occupant>; 3] =
            [1, 2, 3].map(|slot| example_slots.get(&(*word_id, slot)).copied());
        // Two views of the same three rows. The assignment is computed against
        // the occupants that are still available — an invalid one neither
        // reserves its sentence nor holds a slot with a pin — while the write
        // guard and the "already there" test read the rows as they stand.
        let states: [Option<SlotState>; 3] = held.map(|slot| slot.map(|slot| slot.state));
        let wanted =
            assign_example_slots(&ranked, held.map(|slot| slot.and_then(|s| s.incumbent())));

        for (index, want) in wanted.iter().enumerate() {
            let holds_a_valid_pin =
                held[index].is_some_and(|slot| slot.available && slot.state.pinned);
            let (Some(cand_id), false) = (*want, holds_a_valid_pin) else {
                continue;
            };
            if states[index].map(|s| s.cand_id) == Some(cand_id) {
                continue;
            }
            decisions.push(AutoSelection {
                slot: SlotRef::Example {
                    word_id: *word_id,
                    slot: index as i64 + 1,
                },
                cand_id,
                expected_cand_id: states[index].map(|s| s.cand_id),
                make_primary: false,
            });
        }
    }

    // -- images: exactly one slot ------------------------------------------
    //
    // Three things decide a picture, and only the first of them is cached on the
    // candidate:
    //
    // * how good a picture it is — resolution and primary-sense match, the
    //   `auto_score` [`score::score_image`] computed;
    // * how well it answers the word's own sentence, from `clip_scores`. This
    //   dominates: for two years nothing here knew what a picture *depicted*, so
    //   the library optimized for sharp pictures of the wrong thing (backlog #8);
    // * whether it is already somebody else's picture. Media is
    //   content-addressed, so two words searching for neighbouring ideas come
    //   back holding the same `file_hash`; a question renders the word beside its
    //   three fixed distractors, and two identical option pictures make the card
    //   unanswerable.
    //
    // The last one is enforced twice, both as hard exclusions (#56). Across the
    // lexicon, a picture another word already selected is removed from the pool
    // entirely. When two words share the same file_hash, the lowest word_id
    // keeps it — symmetric exclusion (both see the other and both relinquish)
    // caused indefinite oscillation. Inside one question card a picture a mate
    // shows is also removed. Both are idempotent: once a word is excluded from
    // a pool the pool does not change on the next pass, and the selection
    // converges.
    let taken = facts::selected_image_hashes(conn)?;
    let mates = question_mate_images(conn)?;
    let queries = clip_queries(conn)?;
    let clip = clip_scores(conn, clip_model_ver)?;
    let mut stmt = conn.prepare(
        "SELECT ic.word_id, ic.img_cand_id, COALESCE(ic.auto_score, 0.0), ic.file_hash
         FROM image_candidates ic
         JOIN active_words w ON w.word_id = ic.word_id AND w.zh_gloss IS NULL
         WHERE ic.status = 'available'
         ORDER BY ic.word_id, ic.img_cand_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut pools: BTreeMap<i64, Vec<ImageChoice>> = BTreeMap::new();
    for (word_id, cand_id, auto_score, file_hash) in rows {
        let clip_score = queries
            .get(&word_id)
            .and_then(|text_hash| clip.get(&(file_hash.clone(), text_hash.clone())))
            .copied();
        pools.entry(word_id).or_default().push(ImageChoice {
            cand_id,
            auto_score,
            clip: clip_score,
            duplicate: facts::is_duplicate_image(&taken, &file_hash, word_id),
            conflicts: mates
                .get(&word_id)
                .is_some_and(|held| held.contains(&file_hash)),
        });
    }
    let image_slots = image_slot_states(conn, &taken, &queries, &clip, &mates)?;
    for (word_id, slot) in &image_slots {
        if slot.needs_release() {
            releases.push(ReleaseInvalidSelection {
                slot: SlotRef::Image { word_id: *word_id },
            });
        }
    }
    for (word_id, pool) in &pools {
        let slot = image_slots.get(word_id);
        // One ruler for the whole word, incumbent included. Deciding that here
        // rather than inside each scorer is the point: measuring a scored
        // incumbent against unscored challengers would push it out of its own
        // slot on the strength of half the evidence.
        let semantic = uniformly_scored(pool);
        let choices = rank_images(pool, semantic);
        let state = slot.map(|slot| slot.state(semantic));
        // An incumbent that is a duplicate (another word shows the same hash)
        // or that a question mate also shows is not an occupant to be
        // out-argued — it is an invalid one, so the slot is decided as if it
        // were empty and the best admissible picture takes it outright. Without
        // that, the two words would each need to beat the other by the
        // hysteresis margin, and a card with one picture twice could sit there
        // through every future pass. A *pinned* slot is still never touched:
        // a human who chose that picture outranks this, and the export gate is
        // the right place for them to hear about it.
        //
        // A picture that has left `status = 'available'` is invalid the same
        // way and more plainly, and it is checked first: rule 4's exception
        // says a pin protecting a rejected candidate is a pin over nothing.
        let effective = match slot {
            Some(slot) if !slot.available => None,
            Some(slot) if slot.pinned => state,
            Some(slot) if slot.conflicts => None,
            Some(slot) if slot.duplicate => None,
            _ => state,
        };
        let Some(cand_id) = pick(&choices, effective) else {
            continue;
        };
        decisions.push(AutoSelection {
            slot: SlotRef::Image { word_id: *word_id },
            cand_id,
            expected_cand_id: slot.map(|slot| slot.cand_id),
            make_primary: false,
        });
    }

    Ok(Decisions {
        releases,
        selections: decisions,
        primaries: moves,
    })
}

/// One image candidate with everything the ranking needs, before the ranking
/// happens.
#[derive(Debug, Clone)]
struct ImageChoice {
    cand_id: i64,
    auto_score: f64,
    /// Raw cosine against the word's query, when the comparison exists.
    clip: Option<f64>,
    /// Some *other* word's selected picture, anywhere in the lexicon.
    duplicate: bool,
    /// A picture one of this word's question mates is currently showing.
    conflicts: bool,
}

/// Has every picture in this pool been compared against the word's sentence?
///
/// Semantic scoring is all or nothing, per word. A pool where some candidates
/// have been compared and some have not is ranked on quality alone, because
/// blending a scored candidate against an unscored one measures them with two
/// different rulers — and a word mid-backfill would otherwise flip its slot to
/// whichever candidate the sidecar happened to reach first, then flip back when
/// the rest arrived. As soon as every candidate has a score the whole pool is
/// ranked semantically; until then the word behaves exactly as it did before the
/// sidecar existed.
fn uniformly_scored(pool: &[ImageChoice]) -> bool {
    !pool.is_empty() && pool.iter().all(|choice| choice.clip.is_some())
}

/// Turn one word's pool into ranked [`Choice`]s.
///
/// `semantic` comes from [`uniformly_scored`] over this same pool, and the
/// incumbent is measured with the identical flag — see the caller.
///
/// Two kinds of picture are **removed, not docked**: a picture a question mate
/// shows (the per-question veto), and a picture a lower-numbered word already
/// selected (the duplicate exclusion, #56). If that empties
/// the pool the word makes no decision at all and keeps whatever it has, which
/// is honest: the reconciler never empties a slot, and the word is already
/// flagged as needing more candidates, so the image chain walks it down to a
/// picture nobody else holds — up to and including generating one.
fn rank_images(pool: &[ImageChoice], semantic: bool) -> Vec<Choice> {
    pool.iter()
        .filter(|choice| !choice.conflicts && !choice.duplicate)
        .map(|choice| Choice {
            cand_id: choice.cand_id,
            score: score::image_selection_score(
                choice.auto_score,
                semantic.then_some(choice.clip).flatten(),
            ),
        })
        .collect()
}

/// What the sources say about one part of speech of one word.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PosEvidence {
    pos: String,
    /// How strongly WordNet's tagged corpora tie the lemma to this part of
    /// speech, or `0` when WordNet is absent, does not model it, or cannot see
    /// the whole word. See [`super::super::sources::wordnet::WordNet::pos_frequency`].
    wn_frequency: usize,
    /// How many distinct senses the sources recorded under this part of speech.
    candidates: usize,
    /// Best (lowest) source rank seen: manual < llm_rewrite < freedict < wordnet.
    source_rank: u8,
    first_cand_id: i64,
}

/// Rules 5 and 6: every part of speech of one word, best claim on `is_primary`
/// first.
///
/// "Strongest frequency evidence available" is, in order of preference: how
/// often WordNet's tagged corpora used the lemma that way, then how many senses
/// the harvest recorded under each part of speech. Sense *order* is not
/// cross-part-of-speech evidence at all — the Free Dictionary orders senses
/// within a part of speech, and its first block is whichever one it happened to
/// list first, which is how "vivid" ends up defined as a felt-tip pen.
///
/// Counting harvested candidates measures how talkative one dictionary was, and
/// that is the whole "dominant (noun)" failure: five noun senses printed (a
/// gene, a note, a species) against three adjectives, and a word everybody uses
/// as an adjective gets filed as a noun. Corpus counts measure use, which is
/// what a learner meets.
///
/// The harvest still decides two cases, and both are the signal admitting it
/// cannot see: a lemma the corpora never tagged, and a word with a part of
/// speech WordNet models nothing of (see [`WnPos::models`]). Remaining ties fall
/// back to source authority and then to the lowest candidate id, so the answer
/// is a pure function of the lexicon.
fn ranked_pos(evidence: &[PosEvidence]) -> Vec<String> {
    let mut ordered: Vec<&PosEvidence> = evidence.iter().collect();
    ordered.sort_by(|a, b| {
        b.wn_frequency
            .cmp(&a.wn_frequency)
            .then_with(|| b.candidates.cmp(&a.candidates))
            .then_with(|| a.source_rank.cmp(&b.source_rank))
            .then_with(|| a.first_cand_id.cmp(&b.first_cand_id))
    });
    ordered.into_iter().map(|entry| entry.pos.clone()).collect()
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

/// One sense slot: what fills it, plus whether the word shows it at all.
#[derive(Debug, Clone, Copy)]
struct DefSlot {
    occupant: Occupant,
    enabled: bool,
}

fn definition_slots(conn: &Connection) -> Result<HashMap<(i64, String), DefSlot>> {
    let mut stmt = conn.prepare(
        "SELECT ds.word_id, ds.pos, ds.def_cand_id, ds.pinned, dc.auto_score, ds.enabled,
                dc.status, ds.approved
         FROM definition_selections ds
         JOIN definition_candidates dc ON dc.def_cand_id = ds.def_cand_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                (row.get::<_, i64>(0)?, row.get::<_, String>(1)?),
                DefSlot {
                    occupant: Occupant {
                        state: SlotState {
                            cand_id: row.get(2)?,
                            pinned: row.get::<_, i64>(3)? != 0,
                            score: row.get(4)?,
                        },
                        available: is_available(&row.get::<_, String>(6)?),
                        approved: row.get::<_, i64>(7)? != 0,
                    },
                    enabled: row.get::<_, i64>(5)? != 0,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

/// Is this `candidates.status` one a slot may point at?
fn is_available(status: &str) -> bool {
    status == CandidateStatus::Available.as_str()
}

fn word_lemmas(conn: &Connection) -> Result<HashMap<i64, String>> {
    let mut stmt = conn.prepare("SELECT word_id, lemma FROM words")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

fn example_slot_states(conn: &Connection) -> Result<HashMap<(i64, i64), Occupant>> {
    let mut stmt = conn.prepare(
        "SELECT es.word_id, es.slot, es.ex_cand_id, es.pinned, ec.auto_score,
                ec.status, es.approved
         FROM example_selections es
         JOIN example_candidates ec ON ec.ex_cand_id = es.ex_cand_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                (row.get::<_, i64>(0)?, row.get::<_, i64>(1)?),
                Occupant {
                    state: SlotState {
                        cand_id: row.get(2)?,
                        pinned: row.get::<_, i64>(3)? != 0,
                        score: row.get(4)?,
                    },
                    available: is_available(&row.get::<_, String>(5)?),
                    approved: row.get::<_, i64>(6)? != 0,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

/// One image slot, before it is scored.
///
/// The score is deliberately not here: it depends on whether the word's *pool*
/// is uniformly scored, which is a fact about the candidates rather than about
/// the slot. Computing it here would measure the incumbent on one ruler and its
/// challengers on another — and a scored incumbent against unscored challengers
/// would lose its own slot on half the evidence.
#[derive(Debug, Clone)]
struct ImageSlot {
    cand_id: i64,
    pinned: bool,
    auto_score: Option<f64>,
    /// This picture's cosine against the word's query, when it has one.
    clip: Option<f64>,
    /// Some other word in the lexicon also shows this picture.
    duplicate: bool,
    /// A word this one shares a question card with shows this picture.
    conflicts: bool,
    /// The candidate is still `status = 'available'`.
    available: bool,
    approved: bool,
}

impl ImageSlot {
    /// The incumbent as the selector sees it, on the ruler `semantic` names.
    ///
    /// Duplicate status is not folded into the score: an incumbent that is a
    /// duplicate is treated as if the slot were empty (see
    /// `collect_selections`), so scoring it would be pointless.
    fn state(&self, semantic: bool) -> SlotState {
        SlotState {
            cand_id: self.cand_id,
            pinned: self.pinned,
            score: self.auto_score.map(|score| {
                score::image_selection_score(score, semantic.then_some(self.clip).flatten())
            }),
        }
    }

    /// Rule 4's exception — see [`Occupant::needs_release`].
    fn needs_release(&self) -> bool {
        !self.available && (self.pinned || self.approved)
    }
}

fn image_slot_states(
    conn: &Connection,
    taken: &HashMap<String, Vec<i64>>,
    queries: &HashMap<i64, String>,
    clip: &HashMap<(String, String), f64>,
    mates: &HashMap<i64, HashSet<String>>,
) -> Result<HashMap<i64, ImageSlot>> {
    let mut stmt = conn.prepare(
        "SELECT s.word_id, s.img_cand_id, s.pinned, c.auto_score, c.file_hash,
                c.status, s.approved
         FROM image_selections s
         JOIN image_candidates c ON c.img_cand_id = s.img_cand_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, Option<f64>>(3)?,
                row.get::<_, String>(4)?,
                is_available(&row.get::<_, String>(5)?),
                row.get::<_, i64>(6)? != 0,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .map(
            |(word_id, cand_id, pinned, auto_score, file_hash, available, approved)| {
                // Everything the incumbent is judged on, gathered the same way its
                // challengers' is. What it *scores* is decided by the caller, which
                // is the only place that knows whether the pool is uniformly scored.
                let clip_score = queries
                    .get(&word_id)
                    .and_then(|text_hash| clip.get(&(file_hash.clone(), text_hash.clone())))
                    .copied();
                (
                    word_id,
                    ImageSlot {
                        cand_id,
                        pinned,
                        auto_score,
                        clip: clip_score,
                        duplicate: facts::is_duplicate_image(taken, &file_hash, word_id),
                        conflicts: mates
                            .get(&word_id)
                            .is_some_and(|held| held.contains(&file_hash)),
                        available,
                        approved,
                    },
                )
            },
        )
        .collect())
}

/// For every word, the pictures its question mates are currently showing.
///
/// The bindings are fixed for the life of a word (README Part 3 §"干扰项"), so
/// this graph never moves; what moves is which picture each mate has selected.
fn question_mate_images(conn: &Connection) -> Result<HashMap<i64, HashSet<String>>> {
    let mates = facts::question_mates(conn)?;
    let selected = facts::selected_images(conn)?;
    Ok(mates
        .into_iter()
        .map(|(word_id, others)| {
            let held: HashSet<String> = others
                .iter()
                .filter_map(|mate| selected.get(mate).cloned())
                .collect();
            (word_id, held)
        })
        .collect())
}

/// Word → the `text_hash` its pictures are scored against.
fn clip_queries(conn: &Connection) -> Result<HashMap<i64, String>> {
    Ok(facts::clip_queries(conn)?
        .into_iter()
        .map(|(word_id, query)| (word_id, query.text_hash))
        .collect())
}

/// Every comparison made under the model this build stores scores as.
fn clip_scores(conn: &Connection, model_ver: &str) -> Result<HashMap<(String, String), f64>> {
    facts::clip_scores(conn, model_ver)
}

/// Which part of speech currently holds `is_primary`, per word.
fn current_primaries(conn: &Connection) -> Result<HashMap<i64, String>> {
    let mut stmt =
        conn.prepare("SELECT word_id, pos FROM definition_selections WHERE is_primary = 1")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().collect())
}

/// Words whose primary sense somebody moved by hand.
///
/// The reconciler corrects its own guesses and never an editor's. Which slot a
/// human *filled* is on the row (`selected_by`), but moving the primary changes
/// no slot at all, so the only record that it happened is the audit log —
/// `entity_id` there is `"<word_id>:<pos>"`.
fn words_whose_primary_a_human_moved(conn: &Connection) -> Result<std::collections::HashSet<i64>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT entity_id FROM events
         WHERE action = 'primary_moved' AND actor <> 'reconciler'",
    )?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|id| id.split(':').next()?.parse().ok())
        .collect())
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

    // -- image pools: semantics, and the per-question veto -------------------

    fn image(cand_id: i64, auto_score: f64, clip: Option<f64>) -> ImageChoice {
        ImageChoice {
            cand_id,
            auto_score,
            clip,
            duplicate: false,
            conflicts: false,
        }
    }

    /// The incumbent, as the loop in `collect_selections` builds it — same
    /// candidate, same numbers, seen from the slot side.
    fn slot(choice: &ImageChoice) -> ImageSlot {
        ImageSlot {
            cand_id: choice.cand_id,
            pinned: false,
            auto_score: Some(choice.auto_score),
            clip: choice.clip,
            duplicate: choice.duplicate,
            conflicts: choice.conflicts,
            available: true,
            approved: false,
        }
    }

    fn occupant(cand_id: i64, pinned: bool, approved: bool, available: bool) -> Occupant {
        Occupant {
            state: SlotState {
                cand_id,
                pinned,
                score: Some(0.9),
            },
            available,
            approved,
        }
    }

    /// Rule 4's exception, at the level the whole heal turns on: an occupant
    /// nobody may show is not an incumbent, and whatever was protecting it has
    /// to be released exactly once.
    #[test]
    fn an_unavailable_occupant_is_no_incumbent_at_all() {
        let rejected = occupant(5, true, true, false);
        assert!(rejected.incumbent().is_none());
        assert!(rejected.needs_release());
        // With the slot seen as empty, the best available candidate takes it
        // outright — no margin against a candidate nobody may show.
        let choices = vec![choice(1, 0.10)];
        assert_eq!(pick(&choices, rejected.incumbent()), Some(1));
        // …and an available one is protected exactly as rule 4 says.
        let held = occupant(5, true, true, true);
        assert_eq!(pick(&choices, held.incumbent()), None);
        assert!(!held.needs_release());
    }

    /// The second pass over a word whose pool holds nothing available must be
    /// silent: the row still points where it points, and re-releasing it would
    /// write `pin_fallback` for ever.
    #[test]
    fn a_released_slot_is_not_released_again() {
        let released = occupant(5, false, false, false);
        assert!(!released.needs_release());
        assert_eq!(pick(&[], released.incumbent()), None);
    }

    /// A pool nothing has scored ranks exactly on the quality prior, with the
    /// scores passed through untouched — this is what a lexicon looks like
    /// before the sidecar has reached it, and it must be indistinguishable from
    /// the behaviour that predates semantic scoring.
    #[test]
    fn an_unscored_pool_is_ranked_on_quality_alone() {
        let pool = vec![image(1, 0.90, None), image(2, 0.60, None)];
        assert!(!uniformly_scored(&pool));
        let ranked = rank_images(&pool, false);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].score, 0.90);
        assert_eq!(ranked[1].score, 0.60);
        assert_eq!(pick(&ranked, None), Some(1));
    }

    /// A pool where the sidecar has answered for some candidates and not others
    /// is ranked on quality alone as well. Mixing the two would measure them
    /// with different rulers, and the word would flip to whichever picture was
    /// scored first and flip back when the rest arrived.
    #[test]
    fn a_half_scored_pool_falls_back_to_quality() {
        let pool = vec![image(1, 0.90, None), image(2, 0.60, Some(CLIP_CEIL))];
        assert!(!uniformly_scored(&pool));
        let ranked = rank_images(&pool, false);
        assert_eq!(ranked[0].score, 0.90, "the unscored candidate is untouched");
        assert_eq!(ranked[1].score, 0.60);
        assert_eq!(pick(&ranked, None), Some(1));

        // …and once the pool is complete, semantics decides.
        let pool = vec![
            image(1, 0.90, Some(CLIP_FLOOR)),
            image(2, 0.60, Some(CLIP_CEIL)),
        ];
        assert!(uniformly_scored(&pool));
        assert_eq!(pick(&rank_images(&pool, true), None), Some(2));
    }

    /// The half-scored case seen from the slot side, which is where it bites.
    ///
    /// A word whose incumbent happens to have been scored first and scored badly
    /// must not be pushed out of its own slot by an *unscored* challenger: that
    /// compares a semantic number against a quality number and calls the
    /// difference a preference. The incumbent and its challengers take the same
    /// `semantic` flag for exactly this reason.
    #[test]
    fn a_half_scored_pool_measures_the_incumbent_on_the_same_ruler() {
        // The incumbent is scored, and scored at the floor. The challenger has
        // no score at all and a picture a shade sharper — not by the switching
        // margin, so nothing here should move.
        let held = image(1, 0.90, Some(CLIP_FLOOR));
        let pool = vec![held.clone(), image(2, 0.93, None)];
        let semantic = uniformly_scored(&pool);
        assert!(!semantic);
        let ranked = rank_images(&pool, semantic);
        let incumbent = slot(&held).state(semantic);
        assert_eq!(incumbent.score, Some(0.90), "measured on the quality ruler");
        assert_eq!(
            pick(&ranked, Some(incumbent)),
            None,
            "0.93 does not beat 0.90 by the margin"
        );

        // On the wrong ruler it would have moved: the incumbent's blended score
        // is 0.36, which 0.93 clears by a mile — a slot lost to a comparison
        // between a semantic number and a quality one.
        let blended = slot(&held).state(true);
        assert!(blended.score.unwrap() < 0.4);
        assert_eq!(pick(&ranked, Some(blended)), Some(2), "the bug, pinned");
    }

    /// Once the pool is complete, both sides move to the semantic ruler
    /// together and the apt picture takes the slot.
    #[test]
    fn a_fully_scored_pool_moves_the_incumbent_and_its_challengers_together() {
        let held = image(1, 0.90, Some(CLIP_FLOOR));
        let pool = vec![held.clone(), image(2, 0.80, Some(CLIP_CEIL))];
        let semantic = uniformly_scored(&pool);
        assert!(semantic);
        let incumbent = slot(&held).state(semantic);
        assert_eq!(
            pick(&rank_images(&pool, semantic), Some(incumbent)),
            Some(2)
        );
    }

    /// The veto: a picture a question mate shows is not in the running at all,
    /// however good it is.
    #[test]
    fn a_picture_a_question_mate_shows_is_removed_from_the_pool() {
        let mut best = image(1, 1.0, Some(CLIP_CEIL));
        best.conflicts = true;
        let pool = vec![best, image(2, 0.10, Some(CLIP_FLOOR))];
        let ranked = rank_images(&pool, true);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].cand_id, 2);
    }

    /// #56: a picture another word already shows is removed from the pool, not
    /// penalised. A penalty oscillates when two words share candidates; hard
    /// exclusion is idempotent and the selection converges.
    #[test]
    fn a_duplicate_picture_is_removed_from_the_pool() {
        let mut dup = image(1, 1.0, Some(CLIP_CEIL));
        dup.duplicate = true;
        let pool = vec![dup, image(2, 0.10, Some(CLIP_FLOOR))];
        let ranked = rank_images(&pool, true);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].cand_id, 2);
    }

    /// An empty pool after the veto is no decision — never a fallback to the
    /// vetoed candidate. The word keeps whatever it has and the image chain goes
    /// looking for something nobody else holds.
    #[test]
    fn a_wholly_vetoed_pool_decides_nothing() {
        let pool: Vec<ImageChoice> = vec![1, 2]
            .into_iter()
            .map(|id| ImageChoice {
                conflicts: true,
                ..image(id, 0.9, Some(CLIP_CEIL))
            })
            .collect();
        let ranked = rank_images(&pool, true);
        assert!(ranked.is_empty());
        assert_eq!(pick(&ranked, None), None);
        let incumbent = SlotState {
            cand_id: 1,
            pinned: false,
            score: Some(0.9),
        };
        assert_eq!(pick(&ranked, Some(incumbent)), None);
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
            wn_frequency: 0,
            candidates,
            source_rank,
            first_cand_id: first,
        }
    }

    fn wn_evidence(pos: &str, wn_frequency: usize, candidates: usize) -> PosEvidence {
        PosEvidence {
            pos: pos.to_string(),
            wn_frequency,
            candidates,
            source_rank: 2,
            first_cand_id: 1,
        }
    }

    fn strongest_pos(evidence: &[PosEvidence]) -> Option<String> {
        ranked_pos(evidence).first().cloned()
    }

    #[test]
    fn the_part_of_speech_with_the_most_senses_becomes_primary() {
        // "vivid": the Free Dictionary lists one noun sense (a felt-tip pen)
        // before three adjective ones. Sense count is the real evidence.
        let vivid = vec![evidence("noun", 1, 2, 10), evidence("adj", 3, 2, 11)];
        assert_eq!(strongest_pos(&vivid).as_deref(), Some("adj"));
    }

    /// "dominant": the Free Dictionary prints five noun senses (a gene, a note,
    /// a species) against three adjective ones, so counting the harvest files
    /// an adjective as a noun. The corpora count use, not column inches.
    #[test]
    fn corpus_frequency_outranks_how_talkative_the_dictionary_was() {
        let dominant = vec![wn_evidence("noun", 4, 5), wn_evidence("adj", 21, 3)];
        assert_eq!(strongest_pos(&dominant).as_deref(), Some("adj"));
        // "attorney": both signals agree, and the answer is still the noun.
        let attorney = vec![wn_evidence("noun", 11, 5), wn_evidence("verb", 0, 2)];
        assert_eq!(strongest_pos(&attorney).as_deref(), Some("noun"));
    }

    /// A word the corpora never tagged has no frequency signal on any side, and
    /// the harvest decides exactly as it did before the signal existed. The
    /// caller zeroes the whole word the same way when one of its parts of
    /// speech is one WordNet models nothing of — see `collect_selections`.
    #[test]
    fn a_word_wordnet_cannot_speak_for_falls_back_to_the_harvest() {
        let unknown = vec![evidence("prep", 6, 2, 10), evidence("adv", 3, 2, 20)];
        assert_eq!(strongest_pos(&unknown).as_deref(), Some("prep"));
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

    /// Rule 6 needs the whole order, not just its head: the primary lands on
    /// the best part of speech whose slot the word actually shows.
    #[test]
    fn the_ranking_is_total_and_ordered_best_first() {
        let charm = vec![
            wn_evidence("verb", 3, 5),
            wn_evidence("noun", 5, 7),
            wn_evidence("adj", 0, 1),
        ];
        assert_eq!(ranked_pos(&charm), vec!["noun", "verb", "adj"]);
        assert!(ranked_pos(&[]).is_empty());
    }

    // -- sense rank ---------------------------------------------------------

    #[test]
    fn only_a_harvested_list_has_a_sense_rank() {
        assert_eq!(sense_rank(DefinitionSource::Freedict, 1), Some(1));
        assert_eq!(sense_rank(DefinitionSource::Wordnet, 4), Some(4));
        // Somebody wrote these for this word; there is no list to sit in.
        assert_eq!(sense_rank(DefinitionSource::Manual, 3), None);
        assert_eq!(sense_rank(DefinitionSource::LlmRewrite, 3), None);
        // A count of zero is impossible (the row counts itself) but must not
        // become a rank.
        assert_eq!(sense_rank(DefinitionSource::Freedict, 0), None);
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
