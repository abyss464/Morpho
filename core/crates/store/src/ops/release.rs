//! Release bookkeeping.
//!
//! The `releases` row and its `release_manifests` rows land together: the
//! manifest is what pins every media file a shipped APK references, and GC must
//! never be able to observe a release without its pins (README Part 3 §"辅助词
//! 生命周期与媒体 GC").

use morpho_domain::change::EntityType;
use morpho_domain::event::{Action, EventDraft};

use super::{OpCtx, WriteResult};
use crate::error::{Result, StoreError};

/// A finished export, ready to be recorded.
#[derive(Debug, Clone)]
pub struct RecordRelease {
    /// `YYYY.MM.DD+<manifest-hash-8>`
    pub version: String,
    pub plan_id: i64,
    pub input_hash: String,
    pub db_file_hash: String,
    pub exported_by: String,
    pub notes: Option<String>,
    /// Every media file the release references, pinned against GC.
    pub media_hashes: Vec<String>,
    pub word_count: usize,
}

pub(super) fn record_release(req: RecordRelease, ctx: &mut OpCtx<'_, '_>) -> Result<WriteResult> {
    let existing: Option<i64> = {
        use rusqlite::OptionalExtension;
        ctx.tx
            .query_row(
                "SELECT release_id FROM releases WHERE version = ?1",
                rusqlite::params![req.version],
                |row| row.get(0),
            )
            .optional()?
    };
    if let Some(release_id) = existing {
        // Byte-identical inputs produce the same version string. Re-recording
        // it is not an error, it is the determinism guarantee showing up.
        return Ok(WriteResult::Release {
            release_id,
            created: false,
        });
    }

    // Ruling #15: the word count is a column of the history table, not
    // something `GET /releases` has to reconstruct from the audit log.
    ctx.tx.execute(
        "INSERT INTO releases (version, plan_id, input_hash, db_file_hash, exported_at, exported_by, notes, word_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            req.version,
            req.plan_id,
            req.input_hash,
            req.db_file_hash,
            ctx.now,
            req.exported_by,
            req.notes,
            req.word_count as i64,
        ],
    )?;
    let release_id = ctx.tx.last_insert_rowid();

    {
        let mut stmt = ctx.tx.prepare(
            "INSERT INTO release_manifests (release_id, file_hash) VALUES (?1, ?2)
             ON CONFLICT (release_id, file_hash) DO NOTHING",
        )?;
        for file_hash in &req.media_hashes {
            stmt.execute(rusqlite::params![release_id, file_hash])
                .map_err(|err| {
                    StoreError::conflict(format!(
                        "release manifest references unknown media file {file_hash}: {err}"
                    ))
                })?;
        }
    }
    // Anything a release pins is by definition referenced again.
    ctx.tx.execute(
        "UPDATE media_files SET gc_eligible_at = NULL
         WHERE file_hash IN (SELECT file_hash FROM release_manifests WHERE release_id = ?1)",
        rusqlite::params![release_id],
    )?;

    ctx.event(
        EventDraft::new(
            EntityType::Release,
            release_id.to_string(),
            Action::ReleaseExported,
        )
        .detail(serde_json::json!({
            "release_id": release_id,
            "version": req.version,
            "plan_id": req.plan_id,
            "input_hash": req.input_hash,
            "db_file_hash": req.db_file_hash,
            "word_count": req.word_count,
            "media_count": req.media_hashes.len(),
        })),
    )?;
    ctx.touch(EntityType::Release, release_id.to_string());

    Ok(WriteResult::Release {
        release_id,
        created: true,
    })
}
