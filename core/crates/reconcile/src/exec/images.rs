//! Image fetching from the stock libraries, and SDXL generation.
//!
//! Both paths follow README Part 4 §"媒体写入": bytes first, hashed, renamed
//! into the content-addressed library, and only then the database rows. A crash
//! between the two leaves an orphan file that the janitor's GC pass will find —
//! never a row pointing at a file that is not there.
//!
//! Photos are fitted to 768×576 WebP before storage, so the library only ever
//! holds bytes the app can decode directly.

use std::sync::Arc;

use async_trait::async_trait;

use morpho_domain::error::{ErrorKind, TaskError};
use morpho_domain::event::Actor;
use morpho_domain::job::JobKind;
use morpho_domain::types::{FetchedImage, ImageSource, MediaKind};
use morpho_store::ops::{IngestImages, MediaRegistration};
use morpho_store::{Store, WriteOp};

use crate::config::ImageProvider;
use crate::engine::EngineContext;
use crate::exec::{store_error, wrong_payload, Executor};
use crate::rule::{JobPayload, JobSpec};
use crate::sources::{images, proc};

/// Negative prompt for SDXL. Text in a picture ruins a four-image quiz grid.
const SDXL_NEGATIVE: &str = "text, watermark, logo, caption, letters, signature";

pub struct FetchImagesExecutor {
    context: Arc<EngineContext>,
}

impl FetchImagesExecutor {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Executor for FetchImagesExecutor {
    fn kind(&self) -> JobKind {
        JobKind::FetchImages
    }

    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError> {
        let JobPayload::FetchImages {
            word_id,
            lemma,
            source,
            gloss,
        } = &job.payload
        else {
            return Err(wrong_payload(JobKind::FetchImages));
        };

        let query = search_query(lemma, gloss.as_deref());
        let photos = match images::search(
            &self.context.sources.http,
            &self.context.sources.config,
            *source,
            &query,
        )
        .await
        {
            Ok(photos) => photos,
            // A provider that answers "nothing for this word" has answered.
            Err(err) if err.kind() == ErrorKind::Permanent && err.message().contains("404") => {
                Vec::new()
            }
            Err(err) => return Err(err),
        };

        // One job is a search plus a download per candidate, and the downloads
        // usually go to a different host than the search — so the dispatcher's
        // lane, which meters jobs, does not meter them. The provider says how
        // far apart its file host wants them (see `ImageProvider`).
        let spacing = ImageProvider::for_source(*source).map(ImageProvider::download_spacing);

        let mut fetched = Vec::new();
        let mut media = Vec::new();
        for (index, photo) in photos.iter().enumerate() {
            if index > 0 {
                if let Some(spacing) = spacing.filter(|gap| !gap.is_zero()) {
                    tokio::time::sleep(spacing).await;
                }
            }
            let encoded = match images::download(&self.context.sources.http, photo).await {
                Ok(encoded) => encoded,
                Err(err) if err.kind() == ErrorKind::Permanent => {
                    // One unusable photo must not sink the other two.
                    tracing::warn!(lemma, source = %source, error = %err, "skipping unusable photo");
                    continue;
                }
                Err(err) => return Err(err),
            };
            let stored = self
                .context
                .media
                .put_bytes(&encoded.bytes, MediaKind::Image)
                .map_err(store_error)?;
            media.push(MediaRegistration {
                file_hash: stored.file_hash.clone(),
                kind: MediaKind::Image,
                rel_path: stored.rel_path,
                bytes: stored.bytes,
            });
            fetched.push(FetchedImage {
                file_hash: stored.file_hash,
                width: Some(i64::from(encoded.width)),
                height: Some(i64::from(encoded.height)),
                source: *source,
                source_ref: Some(photo.source_ref.clone()),
                license: photo.license.clone(),
                query_used: Some(query.clone()),
            });
        }

        store
            .write(
                Actor::Worker(JobKind::FetchImages),
                WriteOp::IngestImages(IngestImages {
                    word_id: *word_id,
                    source: *source,
                    images: fetched,
                    media,
                }),
            )
            .await
            .map_err(store_error)?;
        Ok(())
    }
}

pub struct GenImageSdxlExecutor {
    context: Arc<EngineContext>,
}

impl GenImageSdxlExecutor {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Executor for GenImageSdxlExecutor {
    fn kind(&self) -> JobKind {
        JobKind::GenImageSdxl
    }

    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError> {
        let JobPayload::GenImageSdxl {
            word_id,
            lemma,
            gloss,
        } = &job.payload
        else {
            return Err(wrong_payload(JobKind::GenImageSdxl));
        };

        let prompt = sdxl_prompt(lemma, gloss.as_deref());
        // The seed is derived from the prompt, so re-running a generation
        // reproduces the same picture instead of quietly making a new one.
        let seed = seed_from(&prompt);

        let staging = self
            .context
            .media
            .staging(&format!("sdxl-{word_id}"))
            .map_err(store_error)?;
        let out_path = staging.out_path("image.webp");

        let result = proc::sdxl_generate(
            &self.context.sources.adapters,
            proc::SdxlRequest {
                prompt: &prompt,
                negative_prompt: SDXL_NEGATIVE,
                seed,
                width: images::TARGET_WIDTH,
                height: images::TARGET_HEIGHT,
                out_path: out_path.to_string_lossy().into_owned(),
            },
        )
        .await?;

        let stored = self
            .context
            .media
            .put_file(&out_path, MediaKind::Image)
            .map_err(store_error)?;

        store
            .write(
                Actor::Worker(JobKind::GenImageSdxl),
                WriteOp::IngestImages(IngestImages {
                    word_id: *word_id,
                    source: ImageSource::Sdxl,
                    images: vec![FetchedImage {
                        file_hash: stored.file_hash.clone(),
                        width: Some(i64::from(images::TARGET_WIDTH)),
                        height: Some(i64::from(images::TARGET_HEIGHT)),
                        source: ImageSource::Sdxl,
                        source_ref: Some(
                            serde_json::json!({
                                "prompt": prompt,
                                "seed": result.seed,
                                "model": result.model,
                            })
                            .to_string(),
                        ),
                        license: Some("generated".to_string()),
                        query_used: Some(prompt.clone()),
                    }],
                    media: vec![MediaRegistration {
                        file_hash: stored.file_hash,
                        kind: MediaKind::Image,
                        rel_path: stored.rel_path,
                        bytes: stored.bytes,
                    }],
                }),
            )
            .await
            .map_err(store_error)?;
        Ok(())
    }
}

/// Stock-photo search query.
///
/// The lemma alone is ambiguous ("adapt" returns power adapters), so the
/// primary gloss's content words are appended when one is available.
fn search_query(lemma: &str, gloss: Option<&str>) -> String {
    let Some(gloss) = gloss else {
        return lemma.to_string();
    };
    let extra: Vec<&str> = gloss
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| word.len() > 3 && !word.eq_ignore_ascii_case(lemma))
        .take(3)
        .collect();
    if extra.is_empty() {
        lemma.to_string()
    } else {
        format!("{lemma} {}", extra.join(" "))
    }
}

/// SDXL prompt, following the shape in `docs/contracts/adapter-protocol.md`.
fn sdxl_prompt(lemma: &str, gloss: Option<&str>) -> String {
    match gloss {
        Some(gloss) => {
            format!("a clear photographic scene depicting the concept of '{lemma}': {gloss}")
        }
        None => format!("a clear photographic scene depicting the concept of '{lemma}'"),
    }
}

/// Deterministic seed from the prompt.
fn seed_from(prompt: &str) -> u64 {
    let digest = blake3::hash(prompt.as_bytes());
    u64::from_le_bytes(digest.as_bytes()[..8].try_into().expect("8 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_lemma_is_its_own_query() {
        assert_eq!(search_query("serene", None), "serene");
    }

    #[test]
    fn the_gloss_disambiguates_the_query() {
        let query = search_query("adapt", Some("to change in order to fit a new situation"));
        assert!(query.starts_with("adapt "), "{query}");
        assert!(query.contains("change"), "{query}");
        // Short function words carry no visual signal.
        assert!(!query.contains(" to "), "{query}");
    }

    #[test]
    fn the_query_never_repeats_the_lemma() {
        let query = search_query("change", Some("change something to change again"));
        assert_eq!(query.matches("change").count(), 1, "{query}");
    }

    #[test]
    fn a_gloss_of_only_short_words_falls_back_to_the_lemma() {
        assert_eq!(search_query("go", Some("to be on a way")), "go");
    }

    #[test]
    fn the_prompt_matches_the_protocol_example() {
        let prompt = sdxl_prompt("abandon", Some("to give up completely"));
        assert!(prompt.contains("the concept of 'abandon'"), "{prompt}");
        assert!(prompt.contains("to give up completely"), "{prompt}");
    }

    #[test]
    fn seeds_are_deterministic_and_prompt_specific() {
        let a = sdxl_prompt("serene", Some("calm"));
        let b = sdxl_prompt("serene", Some("peaceful"));
        assert_eq!(seed_from(&a), seed_from(&a));
        assert_ne!(seed_from(&a), seed_from(&b));
    }

    #[test]
    fn the_negative_prompt_blocks_text_in_the_picture() {
        assert!(SDXL_NEGATIVE.contains("text"));
        assert!(SDXL_NEGATIVE.contains("watermark"));
    }
}
