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
use morpho_store::ops::{ApplyClipScores, ClipScoreRow, IngestImages, MediaRegistration};
use morpho_store::{Store, WriteOp};

use crate::config::ImageProvider;
use crate::engine::EngineContext;
use crate::exec::{store_error, wrong_payload, Executor};
use crate::rule::{JobPayload, JobSpec};
use crate::score::ImageStrategy;
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
            strategy,
            mark,
            gloss_tokens,
        } = &job.payload
        else {
            return Err(wrong_payload(JobKind::FetchImages));
        };

        let query = match strategy {
            ImageStrategy::WidenedQuery => widened_query(lemma, gloss_tokens),
            _ => search_query(lemma, gloss.as_deref()),
        };
        let photos = match images::search(
            &self.context.sources.http,
            &self.context.sources.config,
            *source,
            lemma,
            &query,
            *strategy,
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
                    // Whatever the pass found — including nothing at all — the
                    // mark this job was derived against is the one it writes.
                    // That is what stops the next stage deriving forever and
                    // what stops this one being derived again.
                    mark_source: Some(mark.clone()),
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
            scene,
        } = &job.payload
        else {
            return Err(wrong_payload(JobKind::GenImageSdxl));
        };

        let (prompt, seed) = match scene {
            // A scene generation keys its seed on the word and the template
            // version rather than on the prompt text, so a sentence that gets
            // re-selected under the same template reproduces the same picture,
            // and a template bump produces a different one on purpose.
            Some(scene) => (
                scene_prompt(&scene.sentence, lemma, gloss.as_deref()),
                scene_seed(*word_id, &scene.prompt_ver),
            ),
            // The bare-concept prompt seeds from its own text, so re-running a
            // generation reproduces the same picture instead of quietly making
            // a new one.
            None => {
                let prompt = sdxl_prompt(lemma, gloss.as_deref());
                let seed = seed_from(&prompt);
                (prompt, seed)
            }
        };

        let staging = self
            .context
            .media
            .staging(&format!("sdxl-{word_id}"))
            .map_err(store_error)?;
        let out_path = staging.out_path("image.webp");

        // Generation settings ride along only when the operator has turned
        // scene mode on, which is the same switch that says "my ComfyUI is set
        // up for the fast checkpoint". With it off, the request the adapter
        // receives is exactly the one it received before this existed.
        let params = self.context.images.scene_mode.then(|| &self.context.images);
        let result = proc::sdxl_generate(
            &self.context.sources.adapters,
            proc::SdxlRequest {
                prompt: &prompt,
                negative_prompt: SDXL_NEGATIVE,
                seed,
                width: images::TARGET_WIDTH,
                height: images::TARGET_HEIGHT,
                out_path: out_path.to_string_lossy().into_owned(),
                steps: params.map(|images| images.sdxl_steps),
                cfg: params.map(|images| images.sdxl_cfg),
                workflow: params.map(|images| &*images.sdxl_workflow),
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
                        source_ref: Some(match scene {
                            // The note is what a later pass reads to know this
                            // word already holds a scene image, and which
                            // template made it. The prompt is not repeated into
                            // it — `query_used` below already holds that.
                            Some(scene) => format!(
                                "sdxl:{} ({} {})",
                                result.seed,
                                crate::score::SCENE_NOTE,
                                scene.prompt_ver
                            ),
                            None => serde_json::json!({
                                "prompt": prompt,
                                "seed": result.seed,
                                "model": result.model,
                            })
                            .to_string(),
                        }),
                        license: Some("generated".to_string()),
                        query_used: Some(prompt.clone()),
                    }],
                    media: vec![MediaRegistration {
                        file_hash: stored.file_hash,
                        kind: MediaKind::Image,
                        rel_path: stored.rel_path,
                        bytes: stored.bytes,
                    }],
                    mark_source: None,
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

/// Content keywords the widened query may borrow from the gloss.
const WIDENED_KEYWORDS: usize = 2;

/// Function words that appear in a gloss because English needs them, not
/// because they say anything about the word.
///
/// The extraction has already dropped everything that resolves to no word at
/// all, and every one of these resolves perfectly well — they are base
/// vocabulary. What disqualifies them is that they describe nothing: a search
/// for "manner without something" returns the internet. The list is short on
/// purpose; length already filters most of the rest.
const GLOSS_STOPWORDS: &[&str] = &[
    "a",
    "about",
    "an",
    "and",
    "another",
    "any",
    "are",
    "as",
    "at",
    "be",
    "become",
    "been",
    "being",
    "but",
    "by",
    "can",
    "cause",
    "come",
    "do",
    "does",
    "each",
    "for",
    "from",
    "get",
    "give",
    "go",
    "had",
    "has",
    "have",
    "having",
    "he",
    "her",
    "him",
    "his",
    "how",
    "in",
    "into",
    "is",
    "it",
    "its",
    "like",
    "make",
    "many",
    "may",
    "more",
    "most",
    "much",
    "must",
    "not",
    "of",
    "on",
    "one",
    "onto",
    "or",
    "other",
    "out",
    "over",
    "own",
    "put",
    "same",
    "she",
    "should",
    "so",
    "some",
    "something",
    "such",
    "take",
    "than",
    "that",
    "the",
    "their",
    "them",
    "then",
    "there",
    "these",
    "they",
    "this",
    "those",
    "to",
    "too",
    "under",
    "up",
    "upon",
    "use",
    "used",
    "very",
    "was",
    "way",
    "we",
    "were",
    "what",
    "when",
    "where",
    "which",
    "while",
    "who",
    "will",
    "with",
    "would",
    "you",
    "your",
];

/// The gloss-widened query: the word, plus the two most contentful words of
/// what it means.
///
/// The strict query already appends gloss words, taken in the order the gloss
/// happens to say them and filtered on nothing but length — which is how
/// `desire` ends up searching for "desire wish that something happen". This one
/// starts from `def_tokens`, so every candidate keyword has been tokenized,
/// lemmatized and matched against the lexicon, and then picks by length.
///
/// Length is a crude salience proxy and an honest one: in a dictionary gloss
/// the long words are the ones carrying the sense — "condition", "behaviour",
/// "surface" — and the short ones are the scaffolding holding them together.
/// Two of them, because a third narrows a full-text search faster than it
/// sharpens it, and the point of this pass is to find *anything*.
///
/// Ties break alphabetically so the query a word gets is a pure function of its
/// gloss, and re-running the pass asks for the same thing.
fn widened_query(lemma: &str, gloss_tokens: &[String]) -> String {
    let mut keywords: Vec<&str> = gloss_tokens
        .iter()
        .map(String::as_str)
        .filter(|token| {
            token.chars().count() > 2
                && !token.eq_ignore_ascii_case(lemma)
                && !GLOSS_STOPWORDS
                    .iter()
                    .any(|stop| token.eq_ignore_ascii_case(stop))
        })
        .collect();
    keywords.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()).then(a.cmp(b)));
    keywords.dedup();
    keywords.truncate(WIDENED_KEYWORDS);

    if keywords.is_empty() {
        // Every word in the gloss was scaffolding, or there is no gloss. The
        // bare lemma is still a different question than the strict pass asked.
        return lemma.to_string();
    }
    format!("{lemma} {}", keywords.join(" "))
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

/// Words of the definition a scene prompt is allowed to borrow as steering.
///
/// The sentence is the subject of the picture and the definition is a nudge
/// about which sense of the word that sentence is using. Past a dozen words a
/// gloss stops steering and starts competing: the sampler weights the whole
/// prompt, and a long definition drags the composition towards the dictionary's
/// abstraction instead of the sentence's scene.
const SCENE_GLOSS_WORDS: usize = 12;

/// The scene prompt: the sentence first, the word second, the sense last.
///
/// Order is the point. Diffusion models weight the front of a prompt hardest,
/// and what the card needs is a picture of the moment the sentence describes —
/// so the sentence leads, the lemma follows to keep the subject in frame, and
/// the trimmed gloss trails as disambiguation for a lemma that has several
/// senses.
fn scene_prompt(sentence: &str, lemma: &str, gloss: Option<&str>) -> String {
    let sentence = collapse_whitespace(sentence);
    let base = format!("photograph illustrating: {sentence} — depicting the meaning of '{lemma}'");
    match gloss.map(trim_gloss).filter(|gloss| !gloss.is_empty()) {
        Some(gloss) => format!("{base} ({gloss})"),
        None => base,
    }
}

/// The first [`SCENE_GLOSS_WORDS`] words of a definition, whitespace collapsed.
fn trim_gloss(gloss: &str) -> String {
    gloss
        .split_whitespace()
        .take(SCENE_GLOSS_WORDS)
        .collect::<Vec<&str>>()
        .join(" ")
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// Deterministic seed for a scene generation: the word and the template
/// version, nothing else.
///
/// Keying on the word rather than on the prompt is what makes a scene image
/// per-word unique by construction — two words never draw the same seed, so
/// they never converge on the same bytes, so the duplicate-image pressure has
/// nothing to push against. Keying on the version as well is what makes a
/// template bump produce a genuinely different picture rather than the same one
/// under a new note.
fn scene_seed(word_id: i64, prompt_ver: &str) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&word_id.to_le_bytes());
    hasher.update(prompt_ver.as_bytes());
    u64::from_le_bytes(
        hasher.finalize().as_bytes()[..8]
            .try_into()
            .expect("8 bytes"),
    )
}

/// Deterministic seed from the prompt.
fn seed_from(prompt: &str) -> u64 {
    let digest = blake3::hash(prompt.as_bytes());
    u64::from_le_bytes(digest.as_bytes()[..8].try_into().expect("8 bytes"))
}

pub struct ScoreImageClipExecutor {
    context: Arc<EngineContext>,
}

impl ScoreImageClipExecutor {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Executor for ScoreImageClipExecutor {
    fn kind(&self) -> JobKind {
        JobKind::ScoreImageClip
    }

    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError> {
        let JobPayload::ScoreImageClip {
            word_id,
            text,
            text_hash,
            file_hashes,
        } = &job.payload
        else {
            return Err(wrong_payload(JobKind::ScoreImageClip));
        };
        if file_hashes.is_empty() {
            // The rule never derives an empty ask; a job that carries one has
            // nothing to do and nothing to report.
            return Ok(());
        }

        let media_root = self.context.media.root();
        let response = proc::clip_score(
            &self.context.sources.adapters,
            proc::ClipRequest {
                text,
                images: file_hashes,
                media_root: media_root.to_string_lossy().into_owned(),
            },
        )
        .await?;

        // An adapter quietly serving a different checkpoint would file two
        // models' cosines under one identity, and nothing downstream could tell
        // them apart — the exact failure `model_ver` exists to prevent. So the
        // mismatch is loud and permanent rather than a warning.
        let expected = self.context.images.clip_model_ver();
        if response.model_ver() != expected {
            return Err(TaskError::permanent(format!(
                "clip adapter serves {}, but this engine stores scores as {expected}",
                response.model_ver()
            )));
        }
        if !response.missing.is_empty() {
            // Bytes the library registered but the adapter cannot see: a media
            // root pointed somewhere else, or a file lost to a restore. Worth
            // saying out loud, not worth failing the other candidates over.
            tracing::warn!(
                word_id,
                missing = response.missing.len(),
                first = response.missing.first(),
                "clip adapter could not read some of this word's pictures"
            );
        }

        let rows: Vec<ClipScoreRow> = response
            .scores
            .into_iter()
            .map(|score| ClipScoreRow {
                file_hash: score.file_hash,
                text_hash: text_hash.clone(),
                similarity: score.similarity,
            })
            .collect();
        if rows.is_empty() {
            // Every picture asked about came back unreadable, so this run made
            // no progress — and the rule's trigger is "these are still
            // unscored", which has not changed. Reporting success would derive
            // the identical job on the very next sweep, forever, one GPU request
            // a minute for a word whose bytes are gone.
            //
            // Permanent is the honest classification: the same files will be
            // just as unreadable next time. The job backs off, dead-letters, and
            // shows up in the dead-letter box naming a word whose media the
            // adapter cannot see — which is a real thing an operator should fix
            // (usually a media root pointed somewhere else).
            return Err(TaskError::permanent(format!(
                "clip adapter could not read any of this word's {} picture(s); \
                 check the media root configuration",
                file_hashes.len()
            )));
        }
        store
            .write(
                Actor::Worker(JobKind::ScoreImageClip),
                WriteOp::ApplyClipScores(ApplyClipScores {
                    model_ver: expected,
                    rows,
                    touched_words: vec![*word_id],
                }),
            )
            .await
            .map_err(store_error)?;
        Ok(())
    }
}

pub struct GenImageCodexExecutor {
    context: Arc<EngineContext>,
}

impl GenImageCodexExecutor {
    pub fn new(context: Arc<EngineContext>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Executor for GenImageCodexExecutor {
    fn kind(&self) -> JobKind {
        JobKind::GenImageCodex
    }

    async fn run(&self, job: &JobSpec, store: &Store) -> Result<(), TaskError> {
        let JobPayload::GenImageCodex {
            word_id,
            lemma,
            pos,
            gloss,
            sentence,
            prompt_ver,
            mark,
        } = &job.payload
        else {
            return Err(wrong_payload(JobKind::GenImageCodex));
        };

        let staging = self
            .context
            .media
            .staging(&format!("codex-{word_id}"))
            .map_err(store_error)?;
        let out_path = staging.out_path("image.webp");

        let result = proc::codex_generate(
            &self.context.sources.adapters,
            proc::CodexRequest {
                word_id: *word_id,
                lemma,
                pos: pos.as_deref(),
                primary_definition: gloss.as_deref(),
                slot1_sentence: sentence,
                prompt_ver,
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
                Actor::Worker(JobKind::GenImageCodex),
                WriteOp::IngestImages(IngestImages {
                    word_id: *word_id,
                    source: ImageSource::Codex,
                    images: vec![FetchedImage {
                        file_hash: stored.file_hash.clone(),
                        width: Some(i64::from(images::TARGET_WIDTH)),
                        height: Some(i64::from(images::TARGET_HEIGHT)),
                        source: ImageSource::Codex,
                        // The note says which template drew it, exactly as the
                        // scene note does, so a later pass can tell a picture
                        // made under `codex/1` from one made under `codex/2` —
                        // and so the duplicate-image pressure leaves a picture
                        // drawn for one word alone.
                        source_ref: Some(format!(
                            "codex:{} ({} {prompt_ver})",
                            result.model,
                            crate::score::SCENE_NOTE
                        )),
                        license: Some("generated".to_string()),
                        query_used: Some(result.prompt.clone()),
                    }],
                    media: vec![MediaRegistration {
                        file_hash: stored.file_hash,
                        kind: MediaKind::Image,
                        rel_path: stored.rel_path,
                        bytes: stored.bytes,
                    }],
                    // Whatever came back, the mark this job was derived against
                    // is written: one generation per word per template version,
                    // which is what stops a word being asked for twice.
                    mark_source: Some(mark.clone()),
                }),
            )
            .await
            .map_err(store_error)?;
        Ok(())
    }
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

    // -- the widened second-pass query -------------------------------------

    fn tokens(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    #[test]
    fn the_widened_query_takes_the_two_longest_content_words() {
        // "manner": "a way in which a thing is done or happens".
        let query = widened_query(
            "manner",
            &tokens(&[
                "a", "way", "in", "which", "a", "thing", "be", "do", "or", "happen",
            ]),
        );
        assert_eq!(query, "manner happen thing");
    }

    #[test]
    fn the_widened_query_drops_the_scaffolding() {
        let query = widened_query(
            "desire",
            &tokens(&["to", "want", "something", "very", "much"]),
        );
        // "something", "very" and "much" are stopwords however long they are.
        assert_eq!(query, "desire want");
    }

    #[test]
    fn the_widened_query_never_takes_more_than_two_keywords() {
        let query = widened_query(
            "instance",
            &tokens(&[
                "a",
                "particular",
                "situation",
                "example",
                "or",
                "occurrence",
            ]),
        );
        assert_eq!(query.split_whitespace().count(), 1 + WIDENED_KEYWORDS);
        assert_eq!(query, "instance occurrence particular");
    }

    #[test]
    fn the_widened_query_never_repeats_the_word_it_is_searching_for() {
        let query = widened_query(
            "surface",
            &tokens(&["the", "outer", "surface", "of", "a", "structure"]),
        );
        assert_eq!(query.matches("surface").count(), 1, "{query}");
        assert_eq!(query, "surface structure outer");
    }

    #[test]
    fn a_gloss_of_pure_scaffolding_leaves_the_bare_word() {
        assert_eq!(
            widened_query("go", &tokens(&["to", "be", "on", "a", "way"])),
            "go"
        );
        assert_eq!(widened_query("go", &[]), "go");
    }

    /// Same gloss, same query — a pass that gets retried asks for exactly what
    /// it asked for the first time, whatever order the tokens arrive in.
    #[test]
    fn the_widened_query_is_a_pure_function_of_the_gloss() {
        let gloss = tokens(&["condition", "behaviour", "person", "state"]);
        let once = widened_query("temper", &gloss);
        assert_eq!(once, widened_query("temper", &gloss));
        let reversed: Vec<String> = gloss.iter().rev().cloned().collect();
        assert_eq!(once, widened_query("temper", &reversed));
        // "condition" and "behaviour" are both nine letters; the tie is broken
        // alphabetically rather than by arrival.
        assert_eq!(once, "temper behaviour condition");
    }

    /// The two queries are different questions, which is the entire reason the
    /// second pass is worth a request.
    #[test]
    fn the_widened_query_differs_from_the_strict_one() {
        let gloss = "a way in which a thing is done or happens";
        let strict = search_query("manner", Some(gloss));
        let widened = widened_query(
            "manner",
            &tokens(&[
                "a", "way", "in", "which", "a", "thing", "be", "do", "or", "happen",
            ]),
        );
        assert_ne!(strict, widened);
        assert!(strict.contains("which"), "{strict}");
        assert!(!widened.contains("which"), "{widened}");
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

    // -- the scene prompt --------------------------------------------------

    const SENTENCE: &str = "She had to abandon the car in the flood.";

    #[test]
    fn the_scene_prompt_describes_the_sentence_first() {
        let prompt = scene_prompt(SENTENCE, "abandon", Some("to give up completely"));
        assert!(prompt.starts_with("photograph illustrating: "), "{prompt}");
        assert!(prompt.contains(SENTENCE), "{prompt}");
        assert!(prompt.contains("the meaning of 'abandon'"), "{prompt}");
        assert!(prompt.ends_with("(to give up completely)"), "{prompt}");
        // The sentence leads, because the sampler weights the front hardest.
        let sentence_at = prompt.find(SENTENCE).unwrap();
        assert!(sentence_at < prompt.find("abandon'").unwrap());
        assert!(sentence_at < prompt.find("give up").unwrap());
    }

    #[test]
    fn a_long_definition_is_trimmed_to_steering_length() {
        let gloss = "to leave a place thing or person usually forever and \
                     without any intention of coming back to it at all";
        let prompt = scene_prompt(SENTENCE, "abandon", Some(gloss));
        let trimmed = trim_gloss(gloss);
        assert_eq!(trimmed.split_whitespace().count(), SCENE_GLOSS_WORDS);
        assert_eq!(
            trimmed,
            "to leave a place thing or person usually forever and without any"
        );
        assert!(prompt.ends_with(&format!("({trimmed})")), "{prompt}");
        // What got trimmed really is gone, rather than truncated mid-word.
        assert!(!prompt.contains("coming back"), "{prompt}");
    }

    #[test]
    fn a_word_with_no_definition_still_gets_a_scene_prompt() {
        let prompt = scene_prompt(SENTENCE, "abandon", None);
        assert!(prompt.contains(SENTENCE), "{prompt}");
        assert!(prompt.ends_with("the meaning of 'abandon'"), "{prompt}");
        // An empty parenthetical would read to the sampler as a hole.
        assert!(!prompt.contains("()"), "{prompt}");
        assert_eq!(scene_prompt(SENTENCE, "abandon", Some("   ")), prompt);
    }

    #[test]
    fn a_ragged_sentence_is_normalized_before_it_reaches_the_sampler() {
        let prompt = scene_prompt("  She had to\n abandon   the car.  ", "abandon", None);
        assert!(prompt.contains("She had to abandon the car."), "{prompt}");
        assert!(!prompt.contains("  "), "{prompt}");
    }

    /// Two words never draw the same seed, which is what makes a scene image
    /// per-word unique by construction and keeps the duplicate-image pressure
    /// with nothing to push against.
    #[test]
    fn scene_seeds_are_per_word_and_per_template() {
        assert_eq!(scene_seed(7, "scene/1"), scene_seed(7, "scene/1"));
        assert_ne!(scene_seed(7, "scene/1"), scene_seed(8, "scene/1"));
        // A template bump has to produce a genuinely different picture, not the
        // same one under a new note.
        assert_ne!(scene_seed(7, "scene/1"), scene_seed(7, "scene/2"));
        // And the seed does not move when the sentence is re-selected.
        assert_ne!(scene_seed(7, "scene/1"), seed_from(SENTENCE));
    }
}
