//! Semantic image-text scores.
//!
//! One row is "this picture, against this text, under this model, scored this".
//! Nothing about a *candidate* is in the key, which is what makes the artifact
//! content addressed in the same sense `tts_assets` is: two words holding the
//! same photograph and the same sentence share one row, a slot that switches
//! back to a sentence it once had finds its scores already computed, and a
//! rejected candidate invalidates nothing.
//!
//! A row is never updated in place by a *different* model — `model_ver` is part
//! of the key, so a model or algorithm change writes new rows and the old ones
//! simply lose their readers (README Part 3 §"哈希覆盖一览", the TTS line).
//! Re-scoring the same triple *is* an update, because a GPU is not bit-exact
//! across drivers and the freshest answer is the one to keep.

use morpho_domain::change::EntityType;

use crate::error::{Result, StoreError};

use super::{OpCtx, WriteResult};

/// One `(picture, text)` comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipScoreRow {
    pub file_hash: String,
    pub text_hash: String,
    pub similarity: f64,
}

/// Store a batch of comparisons made under one model.
#[derive(Debug, Clone)]
pub struct ApplyClipScores {
    /// `"<algo_ver>:<model>"`, the identity the scorer ran under.
    pub model_ver: String,
    pub rows: Vec<ClipScoreRow>,
    /// Candidates whose selection ranking these scores change. Touched on the
    /// change bus so the next pass re-ranks without waiting for the timer.
    pub touched_words: Vec<i64>,
}

pub(super) fn apply_clip_scores(
    req: ApplyClipScores,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    if req.model_ver.trim().is_empty() {
        return Err(StoreError::invalid("clip scores need a model version"));
    }
    let mut stmt = ctx.tx.prepare(
        "INSERT INTO clip_scores (file_hash, text_hash, model_ver, similarity, computed_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (file_hash, text_hash, model_ver)
           DO UPDATE SET similarity = excluded.similarity,
                         computed_at = excluded.computed_at",
    )?;
    let mut changed = 0usize;
    for row in &req.rows {
        if !row.similarity.is_finite() {
            return Err(StoreError::invalid(format!(
                "clip similarity for {} is not a number",
                row.file_hash
            )));
        }
        changed += stmt.execute(rusqlite::params![
            row.file_hash,
            row.text_hash,
            req.model_ver,
            row.similarity,
            ctx.now
        ])?;
    }
    drop(stmt);
    for word_id in &req.touched_words {
        ctx.touch(EntityType::ImageCandidate, word_id.to_string());
    }
    Ok(WriteResult::Scored { changed })
}
