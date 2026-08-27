//! `morphod publish`: automated export-to-APK pipeline.
//!
//! Orchestrates the six manual release steps in one command:
//!
//! 1. Export a release bundle (reuses `morpho_export`).
//! 2. Copy `release.db` into the Android assets.
//! 3. Sync media files and trash stale ones.
//! 4. Patch the test assertion to the new content version.
//! 5. Run the Gradle build (tests + APK assembly).
//! 6. Report the result.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;

use morpho_export::WrittenRelease;
use morpho_store::Store;

use crate::config::Config;

/// Result of a successful publish run.
#[derive(Debug, Serialize)]
pub struct PublishResult {
    pub content_version: String,
    pub word_count: usize,
    pub media_count: usize,
    pub apk_path: Option<String>,
    pub apk_size_bytes: Option<u64>,
    pub export_dir: String,
}

/// Run the full publish pipeline.
pub async fn publish(
    config: &Config,
    store: &Store,
    notes: Option<String>,
    no_build: bool,
    actor: String,
) -> Result<PublishResult> {
    let repo_root = config.repo_root();
    tracing::info!(repo_root = %repo_root.display(), "publish: resolved repo root");

    // ---------------------------------------------------------------
    // 1. Export
    // ---------------------------------------------------------------
    let settings = config.export_settings();
    let out_dir = config.releases_dir.join(format!(
        "export-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ));

    let (written, report) =
        match morpho_export::export(store, &settings, &out_dir, &actor, notes).await {
            Ok(pair) => pair,
            Err(morpho_export::ExportError::GatesFailed(failures)) => {
                for f in &failures {
                    eprintln!("  [{}] {}", f.gate, f.message);
                }
                bail!(
                    "export refused: {} validation gate(s) failed",
                    failures.len()
                )
            }
            Err(err) => return Err(err.into()),
        };

    println!("export         {}", written.content_version);
    println!("bundle         {}", written.out_dir.display());
    println!(
        "contents       {} words, {} media files",
        written.word_count,
        written.media_hashes.len()
    );
    print!("{}", crate::export::render_holdback(&report));

    // ---------------------------------------------------------------
    // 2. Copy release.db
    // ---------------------------------------------------------------
    let release_db_dest = repo_root.join("app/app/src/main/assets").join("release.db");
    let release_db_src = written.out_dir.join("release.db");
    std::fs::copy(&release_db_src, &release_db_dest).with_context(|| {
        format!(
            "copying release.db from {} to {}",
            release_db_src.display(),
            release_db_dest.display()
        )
    })?;
    println!("synced         release.db -> {}", release_db_dest.display());

    // ---------------------------------------------------------------
    // 3. Sync media
    // ---------------------------------------------------------------
    sync_media(&written, &repo_root)?;

    // ---------------------------------------------------------------
    // 4. Update test assertion
    // ---------------------------------------------------------------
    update_test_version(&written.content_version, &repo_root)?;

    // ---------------------------------------------------------------
    // 5. Gradle build
    // ---------------------------------------------------------------
    let apk_info = if no_build {
        println!("build          skipped (--no-build)");
        None
    } else {
        Some(gradle_build(&repo_root)?)
    };

    // ---------------------------------------------------------------
    // 6. Report
    // ---------------------------------------------------------------
    let result = PublishResult {
        content_version: written.content_version.clone(),
        word_count: written.word_count,
        media_count: written.media_hashes.len(),
        apk_path: apk_info.as_ref().map(|(p, _)| p.clone()),
        apk_size_bytes: apk_info.as_ref().map(|(_, s)| *s),
        export_dir: written.out_dir.display().to_string(),
    };

    println!();
    println!("=== publish complete ===");
    println!("version        {}", result.content_version);
    println!("words          {}", result.word_count);
    println!("media          {}", result.media_count);
    if let (Some(path), Some(size)) = (&result.apk_path, result.apk_size_bytes) {
        println!("apk            {path}");
        println!("apk size       {} MiB", size / (1024 * 1024));
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Media sync
// ---------------------------------------------------------------------------

/// Walk the manifest and sync img/ + audio/ to the Android content_media
/// assets directory. Stale files not in the manifest are moved to trash via
/// `gio trash`.
fn sync_media(written: &WrittenRelease, repo_root: &Path) -> Result<()> {
    let media_dest = repo_root.join("app/content_media/src/main/assets/content_media");
    std::fs::create_dir_all(&media_dest)
        .with_context(|| format!("creating media dir {}", media_dest.display()))?;

    let manifest_paths: HashSet<String> = written
        .manifest
        .files
        .iter()
        .filter(|entry| entry.path.starts_with("img/") || entry.path.starts_with("audio/"))
        .map(|entry| entry.path.clone())
        .collect();

    // Copy new/changed media files from the export bundle.
    let mut synced = 0usize;
    for rel_path in &manifest_paths {
        let src = written.out_dir.join(rel_path);
        let dst = media_dest.join(rel_path);

        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Skip files that already exist with the same size.
        if dst.exists() {
            if let (Ok(src_meta), Ok(dst_meta)) = (src.metadata(), dst.metadata()) {
                if src_meta.len() == dst_meta.len() {
                    continue;
                }
            }
        }

        std::fs::copy(&src, &dst)
            .with_context(|| format!("copying {} to {}", src.display(), dst.display()))?;
        synced += 1;
    }
    println!(
        "synced         {synced} media files to {}",
        media_dest.display()
    );

    // Trash stale files not in the manifest.
    let stale = find_stale_files(&media_dest, &manifest_paths)?;
    if !stale.is_empty() {
        trash_stale_files(&stale)?;
    }

    Ok(())
}

/// Collect files under `base` whose relative paths are not in `keep`.
fn find_stale_files(base: &Path, keep: &HashSet<String>) -> Result<Vec<PathBuf>> {
    let mut stale = Vec::new();
    for subdir in &["img", "audio"] {
        let dir = base.join(subdir);
        if !dir.is_dir() {
            continue;
        }
        for entry in
            std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let rel = format!(
                "{}/{}",
                subdir,
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            if !keep.contains(&rel) {
                stale.push(path);
            }
        }
    }
    Ok(stale)
}

/// Move stale files to trash using `gio trash`. Falls back to listing them
/// when `gio` is unavailable.
fn trash_stale_files(stale: &[PathBuf]) -> Result<()> {
    println!("trashing       {} stale media files", stale.len());

    // Test whether gio is available.
    let gio_available = std::process::Command::new("gio")
        .arg("version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if gio_available {
        for path in stale {
            let status = std::process::Command::new("gio")
                .args(["trash", "--"])
                .arg(path)
                .status()
                .with_context(|| format!("gio trash {}", path.display()))?;
            if !status.success() {
                tracing::warn!(path = %path.display(), "gio trash failed");
            }
        }
    } else {
        eprintln!("warning: `gio` not available — stale files NOT trashed");
        eprintln!("         remove these manually:");
        for path in stale {
            eprintln!("           {}", path.display());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Test file patching
// ---------------------------------------------------------------------------

/// Replace the content_version string in `ReleaseDatabaseTest.kt`.
fn update_test_version(new_version: &str, repo_root: &Path) -> Result<()> {
    let test_file =
        repo_root.join("app/app/src/test/kotlin/dev/morpho/data/db/ReleaseDatabaseTest.kt");

    let content = std::fs::read_to_string(&test_file)
        .with_context(|| format!("reading {}", test_file.display()))?;

    let updated = replace_content_version(&content, new_version);

    if updated == content {
        println!("test           version already up to date");
    } else {
        std::fs::write(&test_file, updated.as_bytes())
            .with_context(|| format!("writing {}", test_file.display()))?;
        println!("test           patched -> {new_version}");
    }

    Ok(())
}

/// Find a `"20YY.MM.DD+<8 hex>"` literal in `text` and replace the first
/// occurrence with `new_version`. Hand-rolled to avoid pulling in `regex`.
fn replace_content_version(text: &str, new_version: &str) -> String {
    // Inner: `YYYY.MM.DD+HHHHHHHH` = 4+1+2+1+2+1+8 = 19 chars.
    // Full quoted span: `"` + 19 + `"` = 21 bytes.
    const INNER_LEN: usize = 19;
    const TOTAL_LEN: usize = INNER_LEN + 2; // opening + closing quote

    let bytes = text.as_bytes();
    let mut i = 0;
    while i + TOTAL_LEN <= bytes.len() {
        if bytes[i] == b'"'
            && bytes[i + 1] == b'2'
            && bytes[i + 2] == b'0'
            && bytes[i + 5] == b'.'
            && bytes[i + 8] == b'.'
            && bytes[i + 11] == b'+'
            && bytes[i + TOTAL_LEN - 1] == b'"'
            && bytes[i + 3..i + 5].iter().all(|b| b.is_ascii_digit())
            && bytes[i + 6..i + 8].iter().all(|b| b.is_ascii_digit())
            && bytes[i + 9..i + 11].iter().all(|b| b.is_ascii_digit())
            && bytes[i + 12..i + 20].iter().all(|b| b.is_ascii_hexdigit())
        {
            let mut result = String::with_capacity(text.len());
            result.push_str(&text[..i]);
            result.push('"');
            result.push_str(new_version);
            result.push('"');
            result.push_str(&text[i + TOTAL_LEN..]);
            return result;
        }
        i += 1;
    }
    text.to_string()
}

// ---------------------------------------------------------------------------
// Gradle build
// ---------------------------------------------------------------------------

/// Run `./gradlew :domain:test :app:testFatApkDebugUnitTest :app:assembleFatApkDebug`
/// and return `(apk_path, apk_size)`.
fn gradle_build(repo_root: &Path) -> Result<(String, u64)> {
    let app_dir = repo_root.join("app");
    println!("build          ./gradlew (from {})", app_dir.display());

    let status = std::process::Command::new("./gradlew")
        .args([
            ":domain:test",
            ":app:testFatApkDebugUnitTest",
            ":app:assembleFatApkDebug",
        ])
        .current_dir(&app_dir)
        .status()
        .context("spawning gradlew")?;

    if !status.success() {
        bail!("gradle build failed with exit code {}", status);
    }

    // Find the APK.
    let apk_dir = app_dir.join("app/build/outputs/apk/fatApk/debug");
    let apk_path = find_apk(&apk_dir)?;
    let apk_size = std::fs::metadata(&apk_path)
        .with_context(|| format!("stat {}", apk_path.display()))?
        .len();

    let apk_str = apk_path.display().to_string();
    println!(
        "build          {apk_str} ({} MiB)",
        apk_size / (1024 * 1024)
    );

    Ok((apk_str, apk_size))
}

/// Locate the APK in the build output directory.
fn find_apk(dir: &Path) -> Result<PathBuf> {
    if !dir.is_dir() {
        bail!("APK output directory not found: {}", dir.display());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("apk") {
            return Ok(path);
        }
    }
    bail!("no .apk file found in {}", dir.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_content_version_swaps_the_first_match() {
        let input = r#"assertEquals("2026.08.27+27cea8f1", db.metaQueries)"#;
        let result = replace_content_version(input, "2026.09.01+deadbeef");
        assert!(
            result.contains("2026.09.01+deadbeef"),
            "version not replaced: {result}"
        );
        assert!(
            !result.contains("2026.08.27+27cea8f1"),
            "old version still present: {result}"
        );
    }

    #[test]
    fn replace_content_version_returns_unchanged_when_no_match() {
        let input = "no version here";
        let result = replace_content_version(input, "2026.09.01+deadbeef");
        assert_eq!(result, input);
    }

    #[test]
    fn update_test_version_writes_the_patched_file() {
        let dir = tempfile::tempdir().unwrap();
        let test_dir = dir
            .path()
            .join("app/app/src/test/kotlin/dev/morpho/data/db");
        std::fs::create_dir_all(&test_dir).unwrap();
        let test_file = test_dir.join("ReleaseDatabaseTest.kt");
        std::fs::write(
            &test_file,
            r#"assertEquals("2026.08.27+27cea8f1", db.metaQueries.selectValue(ContentMetaKeys.CONTENT_VERSION).executeAsOneOrNull())"#,
        )
        .unwrap();

        update_test_version("2026.09.01+deadbeef", dir.path()).unwrap();

        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(
            content.contains("2026.09.01+deadbeef"),
            "version not replaced: {content}"
        );
        assert!(
            !content.contains("2026.08.27+27cea8f1"),
            "old version still present: {content}"
        );
    }

    #[test]
    fn find_stale_files_identifies_files_not_in_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let img_dir = dir.path().join("img");
        std::fs::create_dir_all(&img_dir).unwrap();
        std::fs::write(img_dir.join("aaa.webp"), b"keep").unwrap();
        std::fs::write(img_dir.join("bbb.webp"), b"stale").unwrap();

        let keep: HashSet<String> = ["img/aaa.webp".to_string()].into_iter().collect();
        let stale = find_stale_files(dir.path(), &keep).unwrap();

        assert_eq!(stale.len(), 1);
        assert!(stale[0].ends_with("bbb.webp"));
    }

    #[test]
    fn find_stale_files_handles_missing_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        let keep: HashSet<String> = HashSet::new();
        let stale = find_stale_files(dir.path(), &keep).unwrap();
        assert!(stale.is_empty());
    }
}
