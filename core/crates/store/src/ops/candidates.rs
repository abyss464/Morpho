//! Example and image candidates, plus the bulk ingest path used by fetch
//! executors.
//!
//! An ingest is one atomic write: every candidate the source returned **and**
//! the `source_fetch` completion marker land in the same transaction. That is
//! what makes a legitimately empty answer terminal — the desired-state rule
//! queries the marker, never "does a candidate exist" (README Part 4
//! §"完成标记").

use rusqlite::OptionalExtension;

use morpho_domain::canon::{canonicalize, fold_lemma};
use morpho_domain::change::EntityType;
use morpho_domain::event::{Action, EventDraft};
use morpho_domain::hash::text_hash;
use morpho_domain::sentence::locate;
use morpho_domain::types::{
    DefinitionSource, ExampleSource, FetchedDefinition, FetchedExample, FetchedImage, ImageSource,
    MediaKind, Pos,
};

use super::{OpCtx, WriteResult};
use crate::error::{Result, StoreError};

/// Insert an immutable example candidate.
#[derive(Debug, Clone)]
pub struct MintExampleCandidate {
    pub word_id: i64,
    pub text: String,
    /// UTF-8 byte offsets into the **canonicalized** text — advisory.
    ///
    /// The stored text is canonicalized, which collapses internal whitespace,
    /// so offsets a caller measured against the string it typed point at the
    /// wrong bytes of the string this writes. The op locates the word itself
    /// and keeps its own answer; these are read only when they agree with it,
    /// which is to say never in a way anybody can observe.
    pub hl_start: i64,
    pub hl_end: i64,
    pub source: ExampleSource,
    pub source_ref: Option<String>,
    pub created_by: Option<String>,
}

/// Insert an immutable image candidate whose bytes are already in the library.
#[derive(Debug, Clone)]
pub struct MintImageCandidate {
    pub word_id: i64,
    pub pos: Option<String>,
    pub file_hash: String,
    /// Registered alongside the candidate when the bytes are new to the store.
    pub media: Option<MediaRegistration>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub source: ImageSource,
    pub source_ref: Option<String>,
    pub license: Option<String>,
    pub query_used: Option<String>,
    pub created_by: Option<String>,
}

/// A `media_files` row to upsert in the same transaction as its referrer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRegistration {
    pub file_hash: String,
    pub kind: MediaKind,
    pub rel_path: String,
    pub bytes: i64,
}

/// Everything one definition fetch produced, plus its completion marker.
#[derive(Debug, Clone)]
pub struct IngestDefinitions {
    pub word_id: i64,
    pub source: DefinitionSource,
    pub definitions: Vec<FetchedDefinition>,
    /// IPA transcription the source happened to carry. Only fills a gap — an
    /// imported syllabus phonetic is authoritative and is never overwritten.
    pub phonetic: Option<String>,
}

/// Everything one example fetch produced, plus its completion marker.
#[derive(Debug, Clone)]
pub struct IngestExamples {
    pub word_id: i64,
    pub source: ExampleSource,
    pub examples: Vec<FetchedExample>,
}

/// Everything one image fetch produced, plus its completion marker.
#[derive(Debug, Clone)]
pub struct IngestImages {
    pub word_id: i64,
    pub source: ImageSource,
    pub images: Vec<FetchedImage>,
    pub media: Vec<MediaRegistration>,
    /// `source_fetch.source` for the completion marker, when it is not the
    /// candidates' own source.
    ///
    /// A second pass over a provider — the same library asked again on looser
    /// terms — has to mark itself separately, or it would overwrite the record
    /// of the first pass and the two would be indistinguishable ever after.
    /// The candidates it produces still name the provider they came from:
    /// `image_candidates.source` carries a `CHECK` union that these marks are
    /// not part of, and the picture really did come from that library.
    /// `source_fetch.source` is free text precisely so this can be recorded.
    pub mark_source: Option<String>,
}

/// Completion-marker kinds written to `source_fetch.kind`.
pub const FETCH_DEFINITIONS: &str = "definitions";
pub const FETCH_EXAMPLES: &str = "examples";
pub const FETCH_ETYMOLOGY: &str = "etymology";
pub const FETCH_IMAGES: &str = "images";

fn require_word(ctx: &OpCtx<'_, '_>, word_id: i64) -> Result<()> {
    word_lemma(ctx, word_id).map(|_| ())
}

/// The lemma of a word that must exist.
fn word_lemma(ctx: &OpCtx<'_, '_>, word_id: i64) -> Result<String> {
    let found: Option<String> = ctx
        .tx
        .query_row(
            "SELECT lemma FROM words WHERE word_id = ?1",
            rusqlite::params![word_id],
            |row| row.get(0),
        )
        .optional()?;
    found.ok_or_else(|| StoreError::not_found(format!("word {word_id}")))
}

pub(super) fn register_media(ctx: &mut OpCtx<'_, '_>, media: &MediaRegistration) -> Result<()> {
    ctx.tx.execute(
        "INSERT INTO media_files (file_hash, kind, rel_path, bytes, created_at, gc_eligible_at)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL)
         ON CONFLICT (file_hash) DO UPDATE SET gc_eligible_at = NULL",
        rusqlite::params![
            media.file_hash,
            media.kind.as_str(),
            media.rel_path,
            media.bytes,
            ctx.now
        ],
    )?;
    ctx.touch(EntityType::MediaFile, media.file_hash.clone());
    Ok(())
}

// ---------------------------------------------------------------------------
// Examples
// ---------------------------------------------------------------------------

pub(super) fn mint_example_candidate(
    req: MintExampleCandidate,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    let lemma = word_lemma(ctx, req.word_id)?;
    let text = canonicalize(&req.text);
    if text.is_empty() {
        return Err(StoreError::invalid("example text must not be empty"));
    }
    // The highlight is a property of (word, canonical text), so it is computed
    // from them rather than taken on trust. A caller measuring against the
    // string it typed is measuring against a different string: canonicalization
    // collapses internal whitespace, and a leading space alone shifts every
    // offset by one. A sentence that does not contain the word it claims to
    // illustrate has no highlight to compute and is refused — a wrong highlight
    // is worse than a missing example, and it is never guessed.
    let (hl_start, hl_end) = locate(&text, &fold_lemma(&lemma))
        .map(|(start, end)| (start as i64, end as i64))
        .ok_or_else(|| {
            StoreError::unprocessable(format!(
                "{text:?} does not contain {lemma:?} or an inflection of it"
            ))
        })?;
    validate_highlight(&text, hl_start, hl_end)?;

    let hash = text_hash(&text);
    let created_by = req.created_by.unwrap_or_else(|| ctx.actor.to_string());
    let inserted = ctx.tx.execute(
        "INSERT INTO example_candidates
             (word_id, text, text_hash, hl_start, hl_end, source, source_ref, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (word_id, text_hash) DO NOTHING",
        rusqlite::params![
            req.word_id,
            text,
            hash,
            hl_start,
            hl_end,
            req.source.as_str(),
            req.source_ref,
            created_by,
            ctx.now,
        ],
    )?;
    let ex_cand_id: i64 = ctx.tx.query_row(
        "SELECT ex_cand_id FROM example_candidates WHERE word_id = ?1 AND text_hash = ?2",
        rusqlite::params![req.word_id, hash],
        |row| row.get(0),
    )?;

    let created = inserted > 0;
    if created {
        ctx.event(
            EventDraft::new(
                EntityType::ExampleCandidate,
                ex_cand_id.to_string(),
                Action::CandidateAdded,
            )
            .detail(serde_json::json!({
                "word_id": req.word_id,
                "source": req.source.as_str(),
                "text_hash": hash,
            })),
        )?;
        ctx.touch(EntityType::ExampleCandidate, ex_cand_id.to_string());
        ctx.touch(EntityType::Word, req.word_id.to_string());
    }
    Ok(WriteResult::Candidate {
        cand_id: ex_cand_id,
        created,
    })
}

/// Delete one example candidate outright.
///
/// Rejection is the engine's answer to bad content: it is reversible, it keeps
/// the audit trail attached to a row that still exists, and it is what every
/// automatic path uses. Erasure is the administrator's, and README Part 3 says
/// so plainly — candidate rows are never garbage collected, only an explicit
/// admin purge deletes one.
///
/// A candidate a slot points at is refused rather than deleted. The reference
/// is a foreign key, so the delete would fail anyway; refusing says which slot
/// to move first instead of surfacing a constraint violation. Nothing else has
/// to be cleaned up: TTS rows and CLIP scores are content-addressed and
/// tolerate orphans by design, and the media library is reference-counted from
/// the live tables.
pub(super) fn purge_example_candidate(
    ex_cand_id: i64,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    let found: Option<(i64, String)> = ctx
        .tx
        .query_row(
            "SELECT word_id, text_hash FROM example_candidates WHERE ex_cand_id = ?1",
            rusqlite::params![ex_cand_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (word_id, text_hash) =
        found.ok_or_else(|| StoreError::not_found(format!("example candidate {ex_cand_id}")))?;

    let holder: Option<i64> = ctx
        .tx
        .query_row(
            "SELECT slot FROM example_selections WHERE ex_cand_id = ?1",
            rusqlite::params![ex_cand_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(slot) = holder {
        return Err(StoreError::conflict(format!(
            "example candidate {ex_cand_id} fills slot {slot} of word {word_id}"
        )));
    }

    ctx.tx.execute(
        "DELETE FROM example_candidates WHERE ex_cand_id = ?1",
        rusqlite::params![ex_cand_id],
    )?;
    ctx.event(
        EventDraft::new(
            EntityType::ExampleCandidate,
            ex_cand_id.to_string(),
            Action::CandidatePurged,
        )
        .detail(serde_json::json!({ "word_id": word_id, "text_hash": text_hash })),
    )?;
    ctx.touch(EntityType::ExampleCandidate, ex_cand_id.to_string());
    ctx.touch(EntityType::Word, word_id.to_string());
    Ok(WriteResult::Unit)
}

/// The highlight must be a byte range inside `text` that starts and ends on a
/// character boundary and is non-empty.
fn validate_highlight(text: &str, start: i64, end: i64) -> Result<()> {
    let len = text.len() as i64;
    if start < 0 || end <= start || end > len {
        return Err(StoreError::invalid(format!(
            "highlight [{start}, {end}) is outside the {len}-byte example text"
        )));
    }
    if !text.is_char_boundary(start as usize) || !text.is_char_boundary(end as usize) {
        return Err(StoreError::invalid(format!(
            "highlight [{start}, {end}) does not fall on UTF-8 character boundaries"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

pub(super) fn mint_image_candidate(
    req: MintImageCandidate,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    require_word(ctx, req.word_id)?;
    if let Some(media) = &req.media {
        register_media(ctx, media)?;
    }
    let pos = req
        .pos
        .as_deref()
        .map(|raw| Pos::normalize(raw).as_str().to_string());

    let inserted = ctx.tx.execute(
        "INSERT INTO image_candidates
             (word_id, pos, file_hash, width, height, source, source_ref, license, query_used,
              created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT (word_id, file_hash) DO NOTHING",
        rusqlite::params![
            req.word_id,
            pos,
            req.file_hash,
            req.width,
            req.height,
            req.source.as_str(),
            req.source_ref,
            req.license,
            req.query_used,
            req.created_by.unwrap_or_else(|| ctx.actor.to_string()),
            ctx.now,
        ],
    )?;
    let img_cand_id: i64 = ctx.tx.query_row(
        "SELECT img_cand_id FROM image_candidates WHERE word_id = ?1 AND file_hash = ?2",
        rusqlite::params![req.word_id, req.file_hash],
        |row| row.get(0),
    )?;

    let created = inserted > 0;
    if created {
        ctx.event(
            EventDraft::new(
                EntityType::ImageCandidate,
                img_cand_id.to_string(),
                Action::CandidateAdded,
            )
            .detail(serde_json::json!({
                "word_id": req.word_id,
                "source": req.source.as_str(),
                "file_hash": req.file_hash,
            })),
        )?;
        ctx.touch(EntityType::ImageCandidate, img_cand_id.to_string());
        ctx.touch(EntityType::Word, req.word_id.to_string());
    }
    Ok(WriteResult::Candidate {
        cand_id: img_cand_id,
        created,
    })
}

// ---------------------------------------------------------------------------
// Bulk ingest
// ---------------------------------------------------------------------------

pub(super) fn ingest_definitions(
    req: IngestDefinitions,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    require_word(ctx, req.word_id)?;

    if let Some(phonetic) = req
        .phonetic
        .as_deref()
        .map(canonicalize)
        .filter(|value| !value.is_empty())
    {
        ctx.tx.execute(
            "UPDATE words SET phonetic = ?2 WHERE word_id = ?1 AND phonetic IS NULL",
            rusqlite::params![req.word_id, phonetic],
        )?;
    }

    let mut created = 0usize;
    for definition in &req.definitions {
        let text = canonicalize(&definition.text);
        if text.is_empty() {
            continue;
        }
        let hash = text_hash(&text);
        let pos = definition.pos.as_str();
        let inserted = ctx.tx.execute(
            "INSERT INTO definition_candidates
                 (word_id, pos, text, text_hash, source, source_ref, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (word_id, pos, text_hash) DO NOTHING",
            rusqlite::params![
                req.word_id,
                pos,
                text,
                hash,
                req.source.as_str(),
                definition.source_ref,
                format!("worker:{}", req.source.as_str()),
                ctx.now,
            ],
        )?;
        if inserted > 0 {
            created += 1;
            let def_cand_id: i64 = ctx.tx.query_row(
                "SELECT def_cand_id FROM definition_candidates
                 WHERE word_id = ?1 AND pos = ?2 AND text_hash = ?3",
                rusqlite::params![req.word_id, pos, hash],
                |row| row.get(0),
            )?;
            ctx.touch(EntityType::DefinitionCandidate, def_cand_id.to_string());
        }
    }
    finish_ingest(
        ctx,
        FETCH_DEFINITIONS,
        req.word_id,
        req.source.as_str(),
        req.definitions.len(),
        created,
    )?;
    Ok(WriteResult::Ingest {
        received: req.definitions.len(),
        created,
    })
}

pub(super) fn ingest_examples(req: IngestExamples, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    require_word(ctx, req.word_id)?;
    let mut created = 0usize;
    for example in &req.examples {
        let outcome = mint_example_candidate(
            MintExampleCandidate {
                word_id: req.word_id,
                text: example.text.clone(),
                hl_start: example.hl_start,
                hl_end: example.hl_end,
                source: req.source,
                source_ref: example.source_ref.clone(),
                created_by: Some(format!("worker:{}", req.source.as_str())),
            },
            ctx,
        )?;
        if matches!(outcome, WriteResult::Candidate { created: true, .. }) {
            created += 1;
        }
    }
    finish_ingest(
        ctx,
        FETCH_EXAMPLES,
        req.word_id,
        req.source.as_str(),
        req.examples.len(),
        created,
    )?;
    Ok(WriteResult::Ingest {
        received: req.examples.len(),
        created,
    })
}

pub(super) fn ingest_images(req: IngestImages, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    require_word(ctx, req.word_id)?;
    for media in &req.media {
        register_media(ctx, media)?;
    }
    let mut created = 0usize;
    for image in &req.images {
        let outcome = mint_image_candidate(
            MintImageCandidate {
                word_id: req.word_id,
                pos: None,
                file_hash: image.file_hash.clone(),
                media: None,
                width: image.width,
                height: image.height,
                source: image.source,
                source_ref: image.source_ref.clone(),
                license: image.license.clone(),
                query_used: image.query_used.clone(),
                created_by: Some(format!("worker:{}", image.source.as_str())),
            },
            ctx,
        )?;
        if matches!(outcome, WriteResult::Candidate { created: true, .. }) {
            created += 1;
        }
    }
    finish_ingest(
        ctx,
        FETCH_IMAGES,
        req.word_id,
        req.mark_source.as_deref().unwrap_or(req.source.as_str()),
        req.images.len(),
        created,
    )?;
    Ok(WriteResult::Ingest {
        received: req.images.len(),
        created,
    })
}

/// Write the completion marker and, when the fetch actually produced something
/// new, one audit row saying so.
fn finish_ingest(
    ctx: &mut OpCtx<'_, '_>,
    kind: &str,
    word_id: i64,
    source: &str,
    received: usize,
    created: usize,
) -> Result<()> {
    super::derived::record_source_fetch(kind, word_id, source, received as i64, ctx)?;
    if created > 0 {
        ctx.event(
            EventDraft::new(EntityType::Word, word_id.to_string(), Action::SourceFetched).detail(
                serde_json::json!({
                    "kind": kind,
                    "source": source,
                    "received": received,
                    "created": created,
                }),
            ),
        )?;
        ctx.touch(EntityType::Word, word_id.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_must_lie_inside_the_canonical_text() {
        let text = "He is a benevolent man.";
        assert!(validate_highlight(text, 8, 18).is_ok());
        assert!(validate_highlight(text, -1, 5).is_err());
        assert!(validate_highlight(text, 5, 5).is_err());
        assert!(validate_highlight(text, 5, 4).is_err());
        assert!(validate_highlight(text, 0, text.len() as i64 + 1).is_err());
    }

    #[test]
    fn highlight_must_land_on_character_boundaries() {
        let text = "café benevolent";
        // 'é' occupies bytes 3..5.
        assert!(validate_highlight(text, 4, 6).is_err());
        assert!(validate_highlight(text, 0, 5).is_ok());
    }
}
