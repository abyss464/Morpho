//! The CLIP scoring sidecar client.
//!
//! ## Why this is a client and not an adapter subprocess
//!
//! CLIP needs a GPU and a torch build that matches it. morphod ships in a
//! `python:3.12-slim` container with no accelerator and no ROCm runtime, so
//! spawning `uv run --project adapters/clip` inside it — the shape `tts` and
//! `morfessor` use — would spawn a process that cannot load the model.
//!
//! `sdxl` already solves this problem, and this module copies its answer rather
//! than inventing one: **the GPU work runs outside the engine and is reached
//! over HTTP.** The one difference is where the HTTP server comes from. ComfyUI
//! is a server somebody else wrote, so `adapters/sdxl` exists as a thin client
//! translating the adapter envelope into ComfyUI's API. CLIP has no such server,
//! so `adapters/clip` *is* the server — it runs on the host under the same venv
//! ComfyUI uses (`~/Code/vendor/ComfyUI/.venv`, ROCm torch plus `open_clip`),
//! holds the model in memory across requests, and reads the media library
//! straight off disk. Putting a subprocess in front of it would add a process
//! spawn and a model load per job to buy nothing.
//!
//! So morphod talks to it directly with `reqwest`, exactly as it talks to the
//! Free Dictionary or Wikimedia: a typed request, a typed reply, and the
//! `Permanent | Transient | RateLimited` taxonomy in between.
//!
//! ## What crosses the wire
//!
//! Content hashes, never bytes. The media library is content addressed and the
//! sidecar is given its own root, so a request names `file_hash` values and the
//! sidecar resolves `{root}/{hash[:2]}/{hash}.webp` itself. Scoring a word's
//! whole candidate pool then costs one small JSON round trip instead of a
//! multi-megabyte upload, and the container's `/app/data` bind mount and the
//! host's `data/` are free to be spelled differently.
//!
//! The contract is `docs/contracts/clip-service.md`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use morpho_domain::error::TaskError;

use crate::sources::http;

/// A word's pool is a handful of pictures and one short sentence; a GPU answers
/// in well under a second. Anything past this is a sidecar in trouble.
pub const CLIP_TIMEOUT: Duration = Duration::from_secs(120);

/// Most pictures one request may ask about.
///
/// A word normally holds three to eight candidates, so this is headroom rather
/// than a limit anything reaches — it exists so a lexicon-wide bug cannot ask
/// the sidecar to embed the whole library in one call.
pub const MAX_IMAGES_PER_REQUEST: usize = 64;

/// What the sidecar is asked.
#[derive(Debug, Clone, Serialize)]
pub struct ScoreRequest<'a> {
    /// The query the pictures are judged against: the word's selected slot-1
    /// sentence, or its lemma when it has none. Same formula as
    /// `ops/clip_rematch.py`.
    pub text: &'a str,
    /// Content hashes of the pictures to score.
    pub images: &'a [String],
}

/// One picture's answer.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ImageScore {
    pub file_hash: String,
    /// Cosine of the two unit embeddings.
    pub similarity: f64,
}

/// What the sidecar answers.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ScoreResponse {
    /// Model identity, e.g. `ViT-B-32/laion2b_s34b_b79k`.
    #[serde(default)]
    pub model: String,
    /// Algorithm version the sidecar implements, e.g. `clip/1`.
    #[serde(default)]
    pub algo_ver: String,
    #[serde(default)]
    pub scores: Vec<ImageScore>,
    /// Hashes the sidecar could not find on disk. Reported rather than failed:
    /// one missing file must not cost the word its other candidates' scores.
    #[serde(default)]
    pub missing: Vec<String>,
}

impl ScoreResponse {
    /// The identity these scores must be stored under.
    pub fn model_ver(&self) -> String {
        format!("{}:{}", self.algo_ver, self.model)
    }
}

/// Score one word's pictures against one text.
pub async fn score(
    client: &reqwest::Client,
    base_url: &str,
    text: &str,
    images: &[String],
) -> Result<ScoreResponse, TaskError> {
    if text.trim().is_empty() {
        return Err(TaskError::permanent("clip scoring needs a query text"));
    }
    if images.is_empty() {
        return Ok(ScoreResponse::default());
    }
    if images.len() > MAX_IMAGES_PER_REQUEST {
        return Err(TaskError::permanent(format!(
            "clip scoring was asked about {} pictures, over the {MAX_IMAGES_PER_REQUEST} limit",
            images.len()
        )));
    }
    let url = format!("{}/score", base_url.trim_end_matches('/'));
    http::post_json(
        client,
        &url,
        &ScoreRequest { text, images },
        CLIP_TIMEOUT,
        "clip score",
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SourcesConfig;
    use morpho_domain::error::ErrorKind;

    fn client() -> reqwest::Client {
        http::build_client(&SourcesConfig::default()).unwrap()
    }

    #[tokio::test]
    async fn an_empty_pool_is_answered_without_a_round_trip() {
        // The URL is deliberately unreachable: reaching the network here would
        // mean the short-circuit is missing.
        let response = score(&client(), "http://127.0.0.1:1", "a serene lake", &[])
            .await
            .unwrap();
        assert!(response.scores.is_empty());
        assert!(response.missing.is_empty());
    }

    #[tokio::test]
    async fn a_query_with_nothing_in_it_is_permanent() {
        let err = score(&client(), "http://127.0.0.1:1", "   ", &["abc".into()])
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Permanent);
    }

    #[tokio::test]
    async fn an_absurd_batch_is_refused_before_it_is_sent() {
        let images: Vec<String> = (0..MAX_IMAGES_PER_REQUEST + 1)
            .map(|n| format!("hash{n}"))
            .collect();
        let err = score(&client(), "http://127.0.0.1:1", "text", &images)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Permanent);
        assert!(err.message().contains("limit"), "{}", err.message());
    }

    #[test]
    fn the_reply_names_the_identity_its_numbers_belong_to() {
        let parsed: ScoreResponse = serde_json::from_str(
            r#"{"model":"ViT-B-32/laion2b_s34b_b79k","algo_ver":"clip/1",
                "scores":[{"file_hash":"aa11","similarity":0.2731}],
                "missing":["bb22"]}"#,
        )
        .unwrap();
        assert_eq!(parsed.model_ver(), "clip/1:ViT-B-32/laion2b_s34b_b79k");
        assert_eq!(parsed.scores[0].file_hash, "aa11");
        assert!((parsed.scores[0].similarity - 0.2731).abs() < 1e-9);
        assert_eq!(parsed.missing, vec!["bb22".to_string()]);
    }

    /// A sidecar that answers with fewer fields than expected is a version skew,
    /// and the caller checks the identity — so parsing must survive to get
    /// there rather than failing as malformed JSON.
    #[test]
    fn a_sparse_reply_parses_so_the_identity_check_can_reject_it() {
        let parsed: ScoreResponse = serde_json::from_str(r#"{"scores":[]}"#).unwrap();
        assert_eq!(parsed.model_ver(), ":");
        assert!(parsed.scores.is_empty());
    }

    #[test]
    fn the_request_serializes_to_the_contract_shape() {
        let images = vec!["aa11".to_string(), "bb22".to_string()];
        let body = serde_json::to_value(ScoreRequest {
            text: "A serene lake at dawn.",
            images: &images,
        })
        .unwrap();
        assert_eq!(body["text"], "A serene lake at dawn.");
        assert_eq!(body["images"][1], "bb22");
    }
}
