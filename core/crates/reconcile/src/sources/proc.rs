//! The adapter subprocess protocol (`docs/contracts/adapter-protocol.md`).
//!
//! One process per job (per *batch* for Morfessor): morphod writes a single
//! JSON request to stdin, closes it, reads one JSON response from stdout, and
//! kills the child at the contractual timeout. stderr is captured verbatim into
//! `job_state.last_error`, which is where a traceback ends up when a dependency
//! is missing.
//!
//! Ruling #1: the invocation is `uv run --project adapters/<name> <name>-adapter`
//! from the repository root, and morphod owns the `out_path` staging directory.
//! Ruling #3: exit code 2 is a protocol crash; **any** non-zero exit maps to
//! `Transient`.
//!
//! admin-api.md wave-3 ruling #17: that repository root comes from
//! `adapters.adapters_root`, never from the process working directory, so
//! `morphod` reaches the same adapters whether it was started from the repo, a
//! systemd unit or `/`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use morpho_domain::error::TaskError;

use crate::config::{AdapterConfig, ADAPTERS};

/// Contractual timeouts (adapter-protocol.md §Timeouts).
pub const TTS_TIMEOUT: Duration = Duration::from_secs(60);
pub const MORFESSOR_TIMEOUT: Duration = Duration::from_secs(120);
pub const SDXL_TIMEOUT: Duration = Duration::from_secs(600);
/// Codex generation goes out to a hosted model over somebody else's queue, so
/// it is given the same budget as a local render plus the round trip.
pub const CODEX_TIMEOUT: Duration = Duration::from_secs(900);
/// CLIP scores a handful of images against one sentence in milliseconds once
/// the model is loaded, but the model load itself — which happens once per
/// subprocess invocation — can take seconds on CPU and a few on GPU.
pub const CLIP_TIMEOUT: Duration = Duration::from_secs(120);

/// stderr kept for `last_error`. Enough for a traceback, bounded so one broken
/// adapter cannot bloat the database.
const MAX_STDERR: usize = 4_000;

/// `{"op": ..., "params": {...}}`
#[derive(Debug, Serialize)]
struct Request<'a, P: Serialize> {
    op: &'a str,
    params: P,
}

#[derive(Debug, Deserialize)]
struct Envelope<R> {
    #[serde(default)]
    ok: bool,
    // No `#[serde(default)]`: that would demand `R: Default`, and `Option<R>`
    // already defaults to `None` when the key is absent.
    result: Option<R>,
    error: Option<AdapterError>,
}

#[derive(Debug, Deserialize)]
struct AdapterError {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    retry_after_ms: u64,
}

impl AdapterError {
    fn into_task_error(self) -> TaskError {
        match self.kind.as_str() {
            "permanent" => TaskError::permanent(self.message),
            "rate_limited" => {
                // The contract says retry_after_ms is always present and 0 when
                // upstream named no cooldown; 60 s is the documented default.
                let ms = if self.retry_after_ms == 0 {
                    60_000
                } else {
                    self.retry_after_ms
                };
                TaskError::rate_limited_ms(ms)
            }
            // "transient" and anything unrecognized: back off.
            _ => TaskError::transient(self.message),
        }
    }
}

/// Run one adapter op and decode its result.
pub async fn call<P: Serialize, R: serde::de::DeserializeOwned>(
    config: &AdapterConfig,
    adapter: &str,
    op: &str,
    params: P,
    timeout: Duration,
) -> Result<R, TaskError> {
    let payload = serde_json::to_vec(&Request { op, params })
        .map_err(|err| TaskError::permanent(format!("cannot serialize {op} request: {err}")))?;
    let (stdout, stderr) = spawn(config, adapter, &payload, timeout).await?;

    let envelope: Envelope<R> = serde_json::from_slice(&stdout).map_err(|err| {
        // Ruling #3: a broken envelope is a protocol crash, and morphod maps
        // those to Transient so a flaky dependency still gets its retries.
        TaskError::transient(format!(
            "{adapter} produced an unreadable response ({err}); stderr: {}",
            truncate(&stderr)
        ))
    })?;

    if envelope.ok {
        return envelope.result.ok_or_else(|| {
            TaskError::transient(format!("{adapter} reported ok with no result payload"))
        });
    }
    Err(envelope
        .error
        .map(AdapterError::into_task_error)
        .unwrap_or_else(|| {
            TaskError::transient(format!(
                "{adapter} reported failure with no error payload; stderr: {}",
                truncate(&stderr)
            ))
        }))
}

/// Spawn the adapter, feed it `payload`, and collect `(stdout, stderr)`.
async fn spawn(
    config: &AdapterConfig,
    adapter: &str,
    payload: &[u8],
    timeout: Duration,
) -> Result<(Vec<u8>, String), TaskError> {
    let (program, args) = config.command(adapter);
    let mut command = tokio::process::Command::new(&program);
    command
        .args(&args)
        .current_dir(config.root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A timed-out adapter must not outlive the job that asked for it.
        .kill_on_drop(true);

    let mut child = command.spawn().map_err(|err| {
        // A launcher that is not installed will not install itself on a retry.
        TaskError::permanent(format!(
            "cannot launch `{program}` for the {adapter} adapter from {}: {err}",
            config.root().display()
        ))
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        let payload = payload.to_vec();
        // Write and close before waiting: the adapter reads one request to EOF.
        if let Err(err) = stdin.write_all(&payload).await {
            return Err(TaskError::transient(format!(
                "{adapter} closed stdin early: {err}"
            )));
        }
        let _ = stdin.shutdown().await;
        drop(stdin);
    }

    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            return Err(TaskError::transient(format!(
                "{adapter} could not be collected: {err}"
            )))
        }
        Err(_) => {
            return Err(TaskError::transient(format!(
                "{adapter} exceeded its {}s budget",
                timeout.as_secs()
            )))
        }
    };

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        // Ruling #3: every non-zero exit is Transient, including the exit-2
        // protocol crash.
        return Err(TaskError::transient(format!(
            "{adapter} exited with {}; stderr: {}",
            output.status,
            truncate(&stderr)
        )));
    }
    Ok((output.stdout, stderr))
}

fn truncate(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= MAX_STDERR {
        return trimmed.to_string();
    }
    let mut end = MAX_STDERR;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… (truncated)", &trimmed[..end])
}

// --- Typed op wrappers -------------------------------------------------------

/// `tts.synthesize` request.
#[derive(Debug, Serialize)]
pub struct TtsRequest<'a> {
    pub text: &'a str,
    pub voice: &'a str,
    pub rate: &'a str,
    pub pitch: &'a str,
    pub volume: &'a str,
    pub format: &'static str,
    pub bitrate_kbps: u32,
    pub out_path: String,
}

/// `tts.synthesize` result.
#[derive(Debug, Clone, Deserialize)]
pub struct TtsResult {
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub engine_ver: String,
}

pub async fn tts_synthesize(
    config: &AdapterConfig,
    request: TtsRequest<'_>,
) -> Result<TtsResult, TaskError> {
    call(config, "tts", "tts.synthesize", request, TTS_TIMEOUT).await
}

/// `morfessor.segment` result.
#[derive(Debug, Clone, Deserialize)]
pub struct SegmentResult {
    #[serde(default)]
    pub segments: std::collections::BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub model_ver: String,
}

pub async fn morfessor_segment(
    config: &AdapterConfig,
    words: &[String],
) -> Result<SegmentResult, TaskError> {
    #[derive(Serialize)]
    struct Params<'a> {
        words: &'a [String],
    }
    call(
        config,
        "morfessor",
        "morfessor.segment",
        Params { words },
        MORFESSOR_TIMEOUT,
    )
    .await
}

/// `sdxl.generate` request.
///
/// The three optional fields carry generation settings the engine has an
/// opinion about — scene mode drives a Turbo checkpoint, which wants four steps
/// and no guidance rather than the base model's thirty and seven. They are
/// omitted entirely when unset, so a request the engine has no opinion about is
/// byte-identical to the one the protocol example shows and an adapter that has
/// never heard of them behaves exactly as before.
#[derive(Debug, Serialize)]
pub struct SdxlRequest<'a> {
    pub prompt: &'a str,
    pub negative_prompt: &'a str,
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    pub out_path: String,
    /// Sampler steps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<u32>,
    /// Classifier-free guidance scale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cfg: Option<f32>,
    /// Workflow template name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<&'a str>,
}

/// `sdxl.generate` result.
#[derive(Debug, Clone, Deserialize)]
pub struct SdxlResult {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub seed: u64,
}

pub async fn sdxl_generate(
    config: &AdapterConfig,
    request: SdxlRequest<'_>,
) -> Result<SdxlResult, TaskError> {
    call(config, "sdxl", "sdxl.generate", request, SDXL_TIMEOUT).await
}

/// `codex.generate` request.
///
/// The fields are the ones `ops/genimg_cron.sh` wrote into its word list —
/// lemma, part of speech, primary definition, slot-1 sentence — because that is
/// what its prompt asks the model to draw, and this is the same ask made an op
/// instead of a batch file. The adapter owns the prompt text: a prompt version
/// belongs with the words it is phrased in, and passing it as a parameter would
/// let the engine and the adapter drift into two templates.
#[derive(Debug, Serialize)]
pub struct CodexRequest<'a> {
    pub word_id: i64,
    pub lemma: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_definition: Option<&'a str>,
    /// The scene the picture must depict. The whole point of the source, and
    /// the reason it is not optional: mode 1 asks the learner to match sentence
    /// to picture, and the CLIP score that decides whether the result wins the
    /// slot queries with this same sentence. A word without one is deferred by
    /// the rule rather than drawn from its lemma.
    pub slot1_sentence: &'a str,
    /// Prompt template version, echoed back so a stored candidate says which
    /// template drew it.
    pub prompt_ver: &'a str,
    pub width: u32,
    pub height: u32,
    pub out_path: String,
}

/// `codex.generate` result.
#[derive(Debug, Clone, Deserialize)]
pub struct CodexResult {
    #[serde(default)]
    pub model: String,
    /// The prompt the adapter actually sent, recorded on the candidate as
    /// `query_used` so a reviewer can see what was asked for.
    #[serde(default)]
    pub prompt: String,
}

pub async fn codex_generate(
    config: &AdapterConfig,
    request: CodexRequest<'_>,
) -> Result<CodexResult, TaskError> {
    call(config, "codex", "codex.generate", request, CODEX_TIMEOUT).await
}

/// `clip.score` request — a word's pictures scored against its text.
///
/// Unlike the HTTP sidecar, the subprocess adapter receives the media root from
/// the engine rather than configuring it itself, so both sides agree about where
/// the files are without a separate `MORPHO_CLIP_MEDIA_ROOT` variable.
#[derive(Debug, Serialize)]
pub struct ClipRequest<'a> {
    pub text: &'a str,
    pub images: &'a [String],
    pub media_root: String,
}

/// `clip.score` result — reuses [`super::clip::ScoreResponse`] for the
/// deserialization target since the subprocess adapter returns the same shape as
/// the HTTP sidecar.
pub async fn clip_score(
    config: &AdapterConfig,
    request: ClipRequest<'_>,
) -> Result<super::clip::ScoreResponse, TaskError> {
    call(config, "clip", "clip.score", request, CLIP_TIMEOUT).await
}

/// Is this executable reachable — on `PATH`, or at the absolute path given?
///
/// Used for backends an adapter shells out to, so the engine can answer "is
/// this source available" the same way the adapter will, and disable it rather
/// than dead-lettering every word (README part 4, "disabled ≡ waived").
pub fn binary_available(program: &str) -> bool {
    which(program).is_some()
}

/// Is the launcher for a given adapter present at all?
///
/// Used at startup to log an honest "adapter unavailable" instead of
/// discovering it eight retries later.
pub fn launcher_available(config: &AdapterConfig) -> bool {
    which(config.runner()).is_some()
}

/// What a startup probe found for one adapter (wave-3 ruling #17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterProbe {
    /// `tts`, `morfessor` or `sdxl`.
    pub adapter: &'static str,
    /// Launcher binary resolved on `PATH` (or at an absolute path).
    pub launcher: String,
    pub launcher_found: bool,
    /// Project directory the command template points at, when it names one.
    pub project: Option<PathBuf>,
    /// The project directory exists and holds a `pyproject.toml`. Always true
    /// when the template names no project — there is nothing to check.
    pub project_found: bool,
    /// Jobs that dead-letter while this adapter is unavailable.
    pub dead_letters: &'static str,
}

impl AdapterProbe {
    pub fn available(&self) -> bool {
        self.launcher_found && self.project_found
    }

    /// One line for `morphod status` and the startup log.
    pub fn state(&self) -> String {
        if self.available() {
            return match &self.project {
                Some(project) => format!("ready ({})", project.display()),
                None => format!("ready (`{}`)", self.launcher),
            };
        }
        let mut reasons = Vec::new();
        if !self.launcher_found {
            reasons.push(format!("`{}` is not on PATH", self.launcher));
        }
        if !self.project_found {
            match &self.project {
                Some(project) => {
                    reasons.push(format!("no pyproject.toml under {}", project.display()))
                }
                None => reasons.push("project directory is unknown".to_string()),
            }
        }
        format!("UNAVAILABLE ({})", reasons.join("; "))
    }
}

/// Probe every adapter: is its project on disk, and is the launcher runnable?
pub fn probe_adapters(config: &AdapterConfig) -> Vec<AdapterProbe> {
    let launcher = config.runner().to_string();
    let launcher_found = which(&launcher).is_some();
    ADAPTERS
        .iter()
        .map(|(adapter, dead_letters)| {
            let project = config.project_dir(adapter);
            let project_found = match &project {
                Some(dir) => dir.join("pyproject.toml").is_file(),
                None => true,
            };
            AdapterProbe {
                adapter,
                launcher: launcher.clone(),
                launcher_found,
                project,
                project_found,
                dead_letters,
            }
        })
        .collect()
}

fn which(program: &str) -> Option<std::path::PathBuf> {
    if program.contains('/') {
        let path = Path::new(program);
        return path.is_file().then(|| path.to_path_buf());
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use morpho_domain::error::ErrorKind;

    /// Build a config that runs a shell snippet instead of a real adapter, so
    /// the protocol can be exercised without uv or a network.
    ///
    /// The snippet is passed to `sh -c`, never written to disk as an
    /// executable: writing and immediately exec-ing a file races with other
    /// threads' `fork` calls and intermittently fails with `ETXTBSY`.
    fn fake_adapter(script: &str) -> (tempfile::TempDir, AdapterConfig) {
        let dir = tempfile::tempdir().unwrap();
        let config = AdapterConfig {
            adapters_root: Some(dir.path().to_path_buf()),
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                script.to_string(),
                "fake-{adapter}".to_string(),
            ],
            ..AdapterConfig::default()
        };
        (dir, config)
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Echo {
        value: String,
    }

    #[tokio::test]
    async fn decodes_a_successful_envelope() {
        let (_dir, config) = fake_adapter(r#"echo '{"ok":true,"result":{"value":"hi"}}'"#);
        let result: Echo = call(
            &config,
            "tts",
            "tts.synthesize",
            serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(result.value, "hi");
    }

    #[tokio::test]
    async fn the_request_reaches_the_adapter_on_stdin() {
        let (_dir, config) = fake_adapter(
            r#"payload=$(cat); printf '{"ok":true,"result":{"value":%s}}' "\"$(echo "$payload" | tr -d '\n' | sed 's/"/\\"/g')\"""#,
        );
        let result: Echo = call(
            &config,
            "morfessor",
            "morfessor.segment",
            serde_json::json!({"words": ["a"]}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert!(result.value.contains("morfessor.segment"), "{result:?}");
        assert!(result.value.contains("words"), "{result:?}");
    }

    #[tokio::test]
    async fn a_permanent_error_response_is_permanent() {
        let (_dir, config) = fake_adapter(
            r#"cat >/dev/null; echo '{"ok":false,"error":{"kind":"permanent","message":"sdxl backend not configured","retry_after_ms":0}}'"#,
        );
        let err = call::<_, Echo>(
            &config,
            "sdxl",
            "sdxl.generate",
            serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Permanent);
        assert_eq!(err.message(), "sdxl backend not configured");
    }

    #[tokio::test]
    async fn a_rate_limited_response_parks_the_lane() {
        let (_dir, config) = fake_adapter(
            r#"cat >/dev/null; echo '{"ok":false,"error":{"kind":"rate_limited","message":"slow down","retry_after_ms":4500}}'"#,
        );
        let err = call::<_, Echo>(
            &config,
            "tts",
            "tts.synthesize",
            serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::RateLimited);
        assert!(err.message().contains("4500"));
        assert!(!err.counts_as_attempt());
    }

    #[tokio::test]
    async fn rate_limited_without_a_cooldown_uses_the_documented_default() {
        let (_dir, config) = fake_adapter(
            r#"cat >/dev/null; echo '{"ok":false,"error":{"kind":"rate_limited","message":"x","retry_after_ms":0}}'"#,
        );
        let err = call::<_, Echo>(
            &config,
            "tts",
            "tts.synthesize",
            serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert!(err.message().contains("60000"));
    }

    #[tokio::test]
    async fn a_protocol_crash_maps_to_transient() {
        // Ruling #3: exit 2, empty stdout, explanation on stderr.
        let (_dir, config) =
            fake_adapter(r#"cat >/dev/null; echo "stdin was not JSON" >&2; exit 2"#);
        let err = call::<_, Echo>(
            &config,
            "morfessor",
            "morfessor.segment",
            serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Transient);
        assert!(err.message().contains("stdin was not JSON"), "{err}");
    }

    #[tokio::test]
    async fn any_non_zero_exit_maps_to_transient() {
        let (_dir, config) = fake_adapter(r#"cat >/dev/null; exit 137"#);
        let err = call::<_, Echo>(
            &config,
            "tts",
            "tts.synthesize",
            serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Transient);
    }

    #[tokio::test]
    async fn unreadable_stdout_is_transient() {
        let (_dir, config) = fake_adapter(r#"cat >/dev/null; echo 'not json'"#);
        let err = call::<_, Echo>(
            &config,
            "tts",
            "tts.synthesize",
            serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Transient);
    }

    #[tokio::test]
    async fn a_hung_adapter_is_killed_at_the_timeout() {
        let (_dir, config) = fake_adapter(r#"cat >/dev/null; sleep 30"#);
        let started = std::time::Instant::now();
        let err = call::<_, Echo>(
            &config,
            "sdxl",
            "sdxl.generate",
            serde_json::json!({}),
            Duration::from_millis(300),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Transient);
        assert!(err.message().contains("budget"));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "kill was not prompt"
        );
    }

    #[tokio::test]
    async fn a_missing_launcher_is_permanent() {
        let config = AdapterConfig {
            command: vec!["definitely-not-installed-morpho-runner".to_string()],
            ..AdapterConfig::default()
        };
        let err = call::<_, Echo>(
            &config,
            "tts",
            "tts.synthesize",
            serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Permanent);
        assert!(err.message().contains("cannot launch"));
    }

    #[tokio::test]
    async fn ok_without_a_result_is_transient_rather_than_a_panic() {
        let (_dir, config) = fake_adapter(r#"cat >/dev/null; echo '{"ok":true}'"#);
        let err = call::<_, Echo>(
            &config,
            "tts",
            "tts.synthesize",
            serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Transient);
    }

    #[test]
    fn stderr_is_bounded() {
        let long = "x".repeat(MAX_STDERR * 2);
        let truncated = truncate(&long);
        assert!(truncated.len() < long.len());
        assert!(truncated.ends_with("(truncated)"));
        assert_eq!(truncate("  short  "), "short");
    }

    #[test]
    fn contract_timeouts_match_the_protocol_document() {
        assert_eq!(TTS_TIMEOUT.as_secs(), 60);
        assert_eq!(MORFESSOR_TIMEOUT.as_secs(), 120);
        assert_eq!(SDXL_TIMEOUT.as_secs(), 600);
        assert!(
            CODEX_TIMEOUT > SDXL_TIMEOUT,
            "a hosted queue is slower than a local render"
        );
    }

    /// The codex request carries what the prompt draws from, and omits what the
    /// word does not have rather than sending an empty string the model would
    /// try to illustrate.
    #[test]
    fn the_codex_request_omits_what_a_word_does_not_have() {
        let body = serde_json::to_value(CodexRequest {
            word_id: 7,
            lemma: "abandon",
            pos: None,
            primary_definition: None,
            slot1_sentence: "She had to abandon the car.",
            prompt_ver: "codex/1",
            width: 768,
            height: 576,
            out_path: "/tmp/x.webp".into(),
        })
        .unwrap();
        assert_eq!(body["word_id"], 7);
        assert_eq!(body["slot1_sentence"], "She had to abandon the car.");
        assert_eq!(body["prompt_ver"], "codex/1");
        assert!(body.get("pos").is_none());
        assert!(body.get("primary_definition").is_none());
    }

    /// Ruling #17: the adapter runs from `adapters_root`, whatever the process
    /// working directory happens to be.
    #[tokio::test]
    async fn the_adapter_runs_from_the_configured_root() {
        let (dir, config) = fake_adapter(
            r#"cat >/dev/null; printf '{"ok":true,"result":{"value":"%s"}}' "$(pwd)""#,
        );
        let result: Echo = call(
            &config,
            "tts",
            "tts.synthesize",
            serde_json::json!({}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        let expected = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(std::path::Path::new(&result.value), expected);
    }

    /// The probe reads the project layout back out of the command template.
    #[test]
    fn the_project_directory_comes_from_the_command_template() {
        let config = AdapterConfig {
            adapters_root: Some(std::path::PathBuf::from("/srv/morpho")),
            ..AdapterConfig::default()
        };
        assert_eq!(
            config.project_dir("tts"),
            Some(std::path::PathBuf::from("/srv/morpho/adapters/tts"))
        );

        let vendored = AdapterConfig {
            command: vec!["python".into(), "-m".into(), "morpho_{adapter}".into()],
            ..AdapterConfig::default()
        };
        assert_eq!(
            vendored.project_dir("tts"),
            None,
            "a template with no project has no directory to probe"
        );
    }

    /// A checkout with all three adapter projects on disk.
    fn adapters_tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (adapter, _) in ADAPTERS {
            let project = dir.path().join("adapters").join(adapter);
            std::fs::create_dir_all(&project).unwrap();
            std::fs::write(project.join("pyproject.toml"), b"[project]\n").unwrap();
        }
        dir
    }

    #[test]
    fn a_complete_checkout_probes_as_available() {
        let dir = adapters_tree();
        let config = AdapterConfig {
            adapters_root: Some(dir.path().to_path_buf()),
            command: vec!["sh".into(), "--project".into(), "adapters/{adapter}".into()],
            ..AdapterConfig::default()
        };
        let probes = probe_adapters(&config);
        assert_eq!(probes.len(), ADAPTERS.len());
        for probe in &probes {
            assert!(probe.available(), "{probe:?}");
            assert!(probe.state().starts_with("ready"), "{}", probe.state());
        }
    }

    #[test]
    fn a_missing_project_directory_names_what_dead_letters() {
        let dir = adapters_tree();
        std::fs::remove_file(dir.path().join("adapters/tts/pyproject.toml")).unwrap();
        let config = AdapterConfig {
            adapters_root: Some(dir.path().to_path_buf()),
            command: vec!["sh".into(), "--project".into(), "adapters/{adapter}".into()],
            ..AdapterConfig::default()
        };
        let probes = probe_adapters(&config);
        let tts = probes.iter().find(|p| p.adapter == "tts").unwrap();
        assert!(!tts.available());
        assert!(tts.state().contains("no pyproject.toml"), "{}", tts.state());
        assert!(tts.dead_letters.contains("synth_tts"));
        assert!(probes.iter().filter(|p| p.available()).count() == ADAPTERS.len() - 1);
    }

    #[test]
    fn a_missing_launcher_takes_every_adapter_down() {
        let dir = adapters_tree();
        let config = AdapterConfig {
            adapters_root: Some(dir.path().to_path_buf()),
            command: vec![
                "definitely-not-installed-morpho-runner".into(),
                "--project".into(),
                "adapters/{adapter}".into(),
            ],
            ..AdapterConfig::default()
        };
        for probe in probe_adapters(&config) {
            assert!(!probe.available(), "{probe:?}");
            assert!(probe.state().contains("not on PATH"), "{}", probe.state());
            assert!(!probe.dead_letters.is_empty());
        }
    }

    /// A request the engine has no opinion about is the one the protocol
    /// example shows, key for key — no `steps`, no `cfg`, no `workflow`. An
    /// adapter that has never heard of them must see exactly what it saw
    /// before they existed.
    #[test]
    fn unset_generation_params_are_omitted_entirely() {
        let request = SdxlRequest {
            prompt: "a clear photographic scene",
            negative_prompt: "text, watermark",
            seed: 42,
            width: 768,
            height: 576,
            out_path: "/tmp/x.webp".to_string(),
            steps: None,
            cfg: None,
            workflow: None,
        };
        let json: serde_json::Value = serde_json::to_value(&request).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "height",
                "negative_prompt",
                "out_path",
                "prompt",
                "seed",
                "width"
            ]
        );
    }

    #[test]
    fn set_generation_params_ride_along_with_the_request() {
        let request = SdxlRequest {
            prompt: "photograph illustrating: ...",
            negative_prompt: "text, watermark",
            seed: 42,
            width: 768,
            height: 576,
            out_path: "/tmp/x.webp".to_string(),
            steps: Some(4),
            cfg: Some(1.0),
            workflow: Some("sdxl_turbo_v1"),
        };
        let json: serde_json::Value = serde_json::to_value(&request).unwrap();
        assert_eq!(json["steps"], 4);
        assert!((json["cfg"].as_f64().unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(json["workflow"], "sdxl_turbo_v1");
        // The params the protocol already names are untouched by their arrival.
        assert_eq!(json["seed"], 42);
        assert_eq!(json["width"], 768);
    }

    #[test]
    fn launcher_detection_finds_a_real_binary() {
        let config = AdapterConfig {
            command: vec!["sh".to_string()],
            ..AdapterConfig::default()
        };
        assert!(launcher_available(&config));
        let missing = AdapterConfig {
            command: vec!["definitely-not-installed-morpho-runner".to_string()],
            ..AdapterConfig::default()
        };
        assert!(!launcher_available(&missing));
    }
}
