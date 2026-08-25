//! `tts_assets` writes.
//!
//! TTS is keyed by **what was synthesized** (README Part 3 §"派生 · TTS"), so a
//! row is never updated in place by a text edit: a different text produces a
//! different `input_hash` and therefore a different row. The only in-place
//! transition is `failed → ready` when a retry finally succeeds.
//!
//! The media registration and the asset row share one transaction, so there is
//! never a `tts_assets` row pointing at a `media_files` row that does not exist.

use morpho_domain::change::EntityType;

use super::candidates::{register_media, MediaRegistration};
use super::{OpCtx, WriteResult};
use crate::error::{Result, StoreError};

/// Result of one `tts.synthesize` call, ready to be committed.
#[derive(Debug, Clone)]
pub struct RecordTtsAsset {
    pub input_hash: String,
    pub text: String,
    pub text_hash: String,
    pub kind: morpho_domain::types::TtsKind,
    pub voice: String,
    pub engine: String,
    pub engine_ver: String,
    /// Serialized exactly as it was hashed into `input_hash`.
    pub params_json: String,
    /// `None` when the synthesis failed; the row is still written so the
    /// desired-set diff stops asking and the console can show the failure.
    pub media: Option<MediaRegistration>,
    pub duration_ms: Option<i64>,
}

impl RecordTtsAsset {
    pub fn status(&self) -> &'static str {
        if self.media.is_some() {
            "ready"
        } else {
            "failed"
        }
    }
}

pub(super) fn record_tts_asset(
    req: RecordTtsAsset,
    ctx: &mut OpCtx<'_, '_>,
) -> Result<WriteResult> {
    if req.input_hash.is_empty() {
        return Err(StoreError::invalid("tts input_hash must not be empty"));
    }
    let status = req.status();
    let file_hash = match &req.media {
        Some(media) => {
            register_media(ctx, media)?;
            Some(media.file_hash.clone())
        }
        None => None,
    };

    ctx.tx.execute(
        "INSERT INTO tts_assets
             (input_hash, text, text_hash, kind, voice, engine, engine_ver, params_json,
              file_hash, duration_ms, status, built_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT (input_hash) DO UPDATE SET
             file_hash = excluded.file_hash,
             duration_ms = excluded.duration_ms,
             status = excluded.status,
             engine_ver = excluded.engine_ver,
             built_at = excluded.built_at",
        rusqlite::params![
            req.input_hash,
            req.text,
            req.text_hash,
            req.kind.as_str(),
            req.voice,
            req.engine,
            req.engine_ver,
            req.params_json,
            file_hash,
            req.duration_ms,
            status,
            ctx.now,
        ],
    )?;

    ctx.touch(EntityType::TtsAsset, req.input_hash.clone());
    Ok(WriteResult::Unit)
}
