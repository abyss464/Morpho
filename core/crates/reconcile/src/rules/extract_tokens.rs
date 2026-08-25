//! `ExtractTokens` — the reference rule.
//!
//! Desired state: every available definition candidate has a `def_extractions`
//! row whose `input_hash` equals `blake3(text_hash ‖ tokenizer_ver ‖
//! lemmatizer_ver)`. Because candidates are immutable, that hash only ever
//! changes when a tool version is bumped — which is exactly the invalidation
//! semantics the whitepaper asks for.
//!
//! This is the one rule that still queries directly rather than reading the
//! shared fact set: it needs every candidate's full text, which is the largest
//! table in the database and pointless to hold in memory when the answer is
//! usually "nothing to do".

use morpho_domain::job::{JobKey, JobKind, Priority, RateKey, SubjectRef};
use morpho_store::error::Result;

use crate::rule::{JobPayload, JobSpec, Rule, Snapshot};
use crate::text::TextPipeline;

pub struct ExtractTokensRule {
    pipeline: TextPipeline,
}

impl ExtractTokensRule {
    pub fn new(pipeline: TextPipeline) -> Self {
        Self { pipeline }
    }

    fn derive_from(&self, conn: &rusqlite::Connection) -> Result<Vec<JobSpec>> {
        let mut stmt = conn.prepare_cached(
            "SELECT dc.def_cand_id, dc.text, dc.text_hash, e.input_hash, w.frequency_rank
             FROM definition_candidates dc
             JOIN words w ON w.word_id = dc.word_id
             LEFT JOIN def_extractions e ON e.def_cand_id = dc.def_cand_id
             WHERE dc.status = 'available'",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })?;

        let mut jobs = Vec::new();
        for row in rows {
            let (def_cand_id, text, text_hash, recorded, frequency_rank) = row?;
            let expected = self.pipeline.input_hash(&text_hash);
            if recorded.as_deref() == Some(expected.as_str()) {
                continue;
            }
            jobs.push(
                JobSpec::new(
                    JobKey::new(
                        JobKind::ExtractTokens,
                        SubjectRef::def_candidate(def_cand_id),
                    ),
                    RateKey::Cpu,
                    // P0: cheap local computation that unblocks dependency
                    // edges, OOV detection and the plan hash.
                    Priority::P0,
                )
                .with_tiebreak(frequency_rank, def_cand_id)
                .with_payload(JobPayload::ExtractTokens {
                    def_cand_id,
                    text,
                    text_hash,
                }),
            );
        }
        Ok(jobs)
    }
}

impl Rule for ExtractTokensRule {
    fn name(&self) -> &'static str {
        "extract_tokens"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        self.derive_from(snapshot.conn)
    }
}
