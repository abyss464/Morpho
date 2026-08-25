//! TTS synthesis: the desired-set diff.
//!
//! README Part 3 §"派生 · TTS": the desired set is every active lemma, every
//! selected definition and every selected example, each combined with the
//! current voice configuration into an `input_hash`. A hash with no `ready`
//! asset is a job. Nothing is ever recomputed in place — changing the voice
//! produces different hashes, so new work appears and the old rows go to GC.
//!
//! The job's subject is the `input_hash` itself, not a word, so two words whose
//! definitions happen to read identically share one synthesis and one dead
//! letter if it fails.

use std::sync::Arc;

use morpho_domain::job::{JobKey, JobKind, Priority, RateKey, SubjectRef};
use morpho_store::error::Result;

use crate::engine::EngineContext;
use crate::rule::{JobPayload, JobSpec, Rule, Snapshot};

pub struct SynthTtsRule {
    context: Arc<EngineContext>,
}

impl SynthTtsRule {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

impl Rule for SynthTtsRule {
    fn name(&self) -> &'static str {
        "synth_tts"
    }

    fn derive(&self, snapshot: &Snapshot<'_>) -> Result<Vec<JobSpec>> {
        let missing = snapshot.facts.missing_tts(&self.context.tts);
        Ok(missing
            .into_iter()
            .map(|desired| {
                // Content-addressed subjects have no frequency rank of their
                // own; ordering falls back to the hash, which is stable.
                let tiebreak = hash_prefix(&desired.input_hash);
                JobSpec::new(
                    JobKey::new(
                        JobKind::SynthTts,
                        SubjectRef::tts_input(desired.input_hash.clone()),
                    ),
                    RateKey::EdgeTts,
                    Priority::P2,
                )
                .with_tiebreak(None, tiebreak)
                .with_payload(JobPayload::SynthTts { desired })
            })
            .collect())
    }
}

/// A stable numeric ordering key derived from a hex hash.
fn hash_prefix(hash: &str) -> i64 {
    i64::from_str_radix(&hash.chars().take(12).collect::<String>(), 16).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_prefix_is_stable_and_total() {
        let hash = "0123456789abcdef".repeat(4);
        assert_eq!(hash_prefix(&hash), hash_prefix(&hash));
        assert_ne!(hash_prefix(&hash), hash_prefix("ffffffffffff0000"));
        assert_eq!(hash_prefix(""), 0);
        assert_eq!(hash_prefix("not-hex"), 0);
    }
}
