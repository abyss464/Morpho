//! Derived-artifact writes: tokenization cache, fetch completion markers and
//! the media registry.
//!
//! These rows are engine bookkeeping rather than human-visible decisions, so
//! they deliberately do **not** append audit-log rows — the log would drown in
//! them. Optimistic concurrency lives here: a result computed from a snapshot
//! carries the input hash it was computed from and is discarded if the input
//! drifted in the meantime (README Part 4 §"对账循环").

use rusqlite::OptionalExtension;

use morpho_domain::change::EntityType;
use morpho_domain::types::{ExtractedToken, MediaKind};

use super::{OpCtx, WriteResult};
use crate::error::{Result, StoreError};

/// Store the tokenization of one immutable definition candidate.
#[derive(Debug, Clone)]
pub struct RecordDefExtraction {
    pub def_cand_id: i64,
    /// `text_hash` the tokens were computed from; the write is dropped if the
    /// candidate no longer matches.
    pub expected_text_hash: String,
    pub input_hash: String,
    pub tokenizer_ver: String,
    pub lemmatizer_ver: String,
    pub tokens: Vec<ExtractedToken>,
}

pub(super) fn record_def_extraction(
    req: RecordDefExtraction,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    let current: Option<String> = ctx
        .tx
        .query_row(
            "SELECT text_hash FROM definition_candidates WHERE def_cand_id = ?1",
            rusqlite::params![req.def_cand_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(current) = current else {
        return Err(StoreError::not_found(format!(
            "definition candidate {}",
            req.def_cand_id
        )));
    };
    if current != req.expected_text_hash {
        tracing::debug!(
            def_cand_id = req.def_cand_id,
            "discarding extraction computed from a drifted input"
        );
        return Ok(WriteResult::Extraction { applied: false });
    }

    ctx.tx.execute(
        "DELETE FROM def_tokens WHERE def_cand_id = ?1",
        rusqlite::params![req.def_cand_id],
    )?;
    {
        let mut stmt = ctx.tx.prepare(
            "INSERT INTO def_tokens (def_cand_id, position, surface, lemma)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for token in &req.tokens {
            stmt.execute(rusqlite::params![
                req.def_cand_id,
                token.position,
                token.surface,
                token.lemma
            ])?;
        }
    }
    ctx.tx.execute(
        "INSERT INTO def_extractions (def_cand_id, input_hash, tokenizer_ver, lemmatizer_ver, extracted_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (def_cand_id) DO UPDATE SET
             input_hash = excluded.input_hash,
             tokenizer_ver = excluded.tokenizer_ver,
             lemmatizer_ver = excluded.lemmatizer_ver,
             extracted_at = excluded.extracted_at",
        rusqlite::params![
            req.def_cand_id,
            req.input_hash,
            req.tokenizer_ver,
            req.lemmatizer_ver,
            ctx.now
        ],
    )?;

    ctx.touch(EntityType::DefExtraction, req.def_cand_id.to_string());
    Ok(WriteResult::Extraction { applied: true })
}

pub(super) fn record_source_fetch(
    kind: &str,
    word_id: i64,
    source: &str,
    result_count: i64,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    ctx.tx.execute(
        "INSERT INTO source_fetch (kind, word_id, source, fetched_at, result_count)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (kind, word_id, source) DO UPDATE SET
             fetched_at = excluded.fetched_at,
             result_count = excluded.result_count",
        rusqlite::params![kind, word_id, source, ctx.now, result_count],
    )?;
    ctx.touch(
        EntityType::SourceFetch,
        format!("{kind}:{word_id}:{source}"),
    );
    Ok(WriteResult::Unit)
}

pub(super) fn register_media_file(
    file_hash: &str,
    kind: MediaKind,
    rel_path: &str,
    bytes: i64,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    ctx.tx.execute(
        "INSERT INTO media_files (file_hash, kind, rel_path, bytes, created_at, gc_eligible_at)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL)
         ON CONFLICT (file_hash) DO UPDATE SET gc_eligible_at = NULL",
        rusqlite::params![file_hash, kind.as_str(), rel_path, bytes, ctx.now],
    )?;
    ctx.touch(EntityType::MediaFile, file_hash);
    Ok(WriteResult::Unit)
}
