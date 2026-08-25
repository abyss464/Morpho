//! TTS synthesis through `adapters/tts`.
//!
//! One process per synthesis, writing into a morphod-owned staging directory
//! (adapter-protocol.md ruling #1). The produced Ogg Opus is hashed into the
//! content-addressed library, and the `media_files` row and the `tts_assets`
//! row are committed together — a TTS asset never points at a file that is not
//! registered.
//!
//! A permanent failure writes a `failed` asset row so the console can show it
//! and the desired-set diff stops asking. A transient failure writes nothing:
//! `job_state` owns the retry, and inventing a `failed` row on the first flaky
//! network hiccup would be a lie about a synthesis that has not been given up
//! on yet.

use std::sync::Arc;

use async_trait::async_trait;

use morpho_domain::error::{ErrorKind, TaskError};
use morpho_domain::event::Actor;
use morpho_domain::hash::text_hash;
use morpho_domain::job::JobKind;
use morpho_domain::types::MediaKind;
use morpho_store::ops::{MediaRegistration, RecordTtsAsset};
use morpho_store::{Store, WriteOp};

use crate::engine::EngineContext;
use crate::exec::{store_error, wrong_payload, Executor};
use crate::rule::{JobPayload, JobSpec};
use crate::sources::proc;

pub struct SynthTtsExecutor {
    context: Arc<EngineContext>,
}

impl SynthTtsExecutor {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Executor for SynthTtsExecutor {
    fn kind(&self) -> JobKind {
        JobKind::SynthTts
    }

    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError> {
        let JobPayload::SynthTts { desired } = &job.payload else {
            return Err(wrong_payload(JobKind::SynthTts));
        };
        let config = &self.context.tts;

        let staging = self
            .context
            .media
            .staging(&format!(
                "tts-{}",
                &desired.input_hash[..16.min(desired.input_hash.len())]
            ))
            .map_err(store_error)?;
        let out_path = staging.out_path("audio.ogg");

        let result = proc::tts_synthesize(
            &self.context.sources.adapters,
            proc::TtsRequest {
                text: &desired.text,
                voice: &config.voice,
                rate: &config.rate,
                pitch: &config.pitch,
                volume: &config.volume,
                format: "ogg_opus",
                bitrate_kbps: desired.bitrate_kbps,
                out_path: out_path.to_string_lossy().into_owned(),
            },
        )
        .await;

        let result = match result {
            Ok(result) => result,
            Err(err) if err.kind() == ErrorKind::Permanent => {
                // Record the failure so the console shows it and the diff stops
                // asking; the blocker becomes `tts_failed`.
                record(store, desired, config, None, None, &err.message()).await?;
                return Err(err);
            }
            Err(err) => return Err(err),
        };

        let stored = self
            .context
            .media
            .put_file(&out_path, MediaKind::Audio)
            .map_err(store_error)?;

        record(
            store,
            desired,
            config,
            Some(MediaRegistration {
                file_hash: stored.file_hash,
                kind: MediaKind::Audio,
                rel_path: stored.rel_path,
                bytes: stored.bytes,
            }),
            result.duration_ms,
            &result.engine_ver,
        )
        .await
    }
}

async fn record(
    store: &Store,
    desired: &morpho_domain::tts::DesiredTts,
    config: &morpho_domain::tts::TtsConfig,
    media: Option<MediaRegistration>,
    duration_ms: Option<i64>,
    engine_ver: &str,
) -> Result<(), TaskError> {
    store
        .write(
            Actor::Worker(JobKind::SynthTts),
            WriteOp::RecordTtsAsset(RecordTtsAsset {
                input_hash: desired.input_hash.clone(),
                text: desired.text.clone(),
                text_hash: text_hash(&desired.text),
                kind: desired.kind,
                voice: config.voice.clone(),
                engine: config.engine.clone(),
                // The configured version is what the hash was computed from;
                // what the adapter actually ran is recorded alongside it.
                engine_ver: if engine_ver.is_empty() {
                    config.engine_ver.clone()
                } else {
                    engine_ver.to_string()
                },
                params_json: desired.params_json.clone(),
                media,
                duration_ms,
            }),
        )
        .await
        .map_err(store_error)?;
    Ok(())
}
