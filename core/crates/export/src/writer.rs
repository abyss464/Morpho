//! The deterministic release bundle.
//!
//! `docs/contracts/release-db.sql` fixes the shape; README Part 5 fixes the
//! reproducibility rules: `page_size = 4096`, `journal_mode = DELETE`, inserts
//! ordered by primary key, `VACUUM` at the end, and no timestamps in rows.
//!
//! ## Bundle layout
//!
//! ```text
//! <out>/release.db
//! <out>/manifest.json
//! <out>/img/{file_hash}.webp
//! <out>/audio/{file_hash}.ogg
//! ```
//!
//! `examples.image_file` and the three `*_audio_file` columns hold exactly those
//! relative paths, so the app's `ContentStore` resolves a hash name to a stream
//! with no lookup table.
//!
//! ## Version identity
//!
//! `content_version = YYYY.MM.DD+<content-hash-8>`. The hash covers the whole
//! logical payload — every exported row in order, plus every media file's hash
//! and size — and nothing else, so it is a pure function of the content. The
//! date is the only wall-clock input, exactly as the README's format demands;
//! two exports of identical content on the same UTC day are byte-identical,
//! and on different days differ only in that prefix. Computing the hash from
//! the payload rather than from the finished file is what keeps it
//! non-circular: the version has to be *inside* `release.db`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use morpho_domain::hash::{file_hash, HashInput};
use morpho_domain::version::{EXPORT_ALGO_VER, RELEASE_SCHEMA_VER};

use crate::error::{ExportError, ExportResult};
use crate::model::ExportPayload;

/// The normative release DDL, embedded at compile time.
pub const RELEASE_DB_SQL: &str = include_str!("../../../../docs/contracts/release-db.sql");

/// One file in the bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Path relative to the bundle root.
    pub path: String,
    pub bytes: u64,
    /// blake3 of the file's bytes.
    pub file_hash: String,
}

/// `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub content_version: String,
    pub schema_ver: String,
    pub exporter: String,
    pub plan_id: i64,
    pub word_count: usize,
    pub media_count: usize,
    pub total_bytes: u64,
    /// Sorted by path.
    pub files: Vec<ManifestEntry>,
}

/// What one export produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenRelease {
    pub out_dir: PathBuf,
    pub content_version: String,
    /// Date-free digest of the payload; `releases.input_hash`.
    pub content_hash: String,
    pub db_file_hash: String,
    pub manifest: Manifest,
    /// Every media file hash the release references.
    pub media_hashes: Vec<String>,
    pub word_count: usize,
}

/// The rows of one release, already cut and ordered.
#[derive(Debug, Clone)]
pub struct ReleaseRows<'a> {
    pub plan_id: i64,
    pub words: Vec<&'a crate::model::ExportWord>,
    pub senses: Vec<&'a crate::model::ExportSense>,
    pub examples: Vec<&'a crate::model::ExportExample>,
    pub groups: Vec<&'a crate::model::ExportGroup>,
    pub distractors: Vec<(i64, i64, i64)>,
    /// The gloss anchors this release's words actually mention. Never an orphan:
    /// an anchor nobody reads is dead weight in the app's popover index.
    pub gloss_anchors: Vec<&'a crate::model::GlossAnchor>,
}

/// Select and order everything the cut kept.
pub fn rows_for<'a>(payload: &'a ExportPayload, exportable: &BTreeSet<i64>) -> ReleaseRows<'a> {
    let mut words: Vec<&crate::model::ExportWord> = payload
        .words
        .iter()
        .filter(|word| exportable.contains(&word.word_id))
        .collect();
    words.sort_by_key(|word| word.word_id);

    let mut senses: Vec<&crate::model::ExportSense> = payload
        .senses
        .iter()
        .filter(|sense| exportable.contains(&sense.word_id))
        .collect();
    // Primary first inside a word, then part of speech: the sense_id the
    // writer assigns is then a deterministic function of the content.
    senses.sort_by(|a, b| {
        a.word_id
            .cmp(&b.word_id)
            .then_with(|| b.is_primary.cmp(&a.is_primary))
            .then_with(|| a.pos.cmp(&b.pos))
    });

    let mut examples: Vec<&crate::model::ExportExample> = payload
        .examples
        .iter()
        .filter(|example| exportable.contains(&example.word_id))
        .collect();
    examples.sort_by_key(|example| (example.word_id, example.display_order));

    let used_groups: BTreeSet<i64> = words.iter().map(|word| word.group_seq).collect();
    let groups: Vec<&crate::model::ExportGroup> = payload
        .groups
        .iter()
        .filter(|group| used_groups.contains(&group.group_seq))
        .collect();

    let mut distractors: Vec<(i64, i64, i64)> = payload
        .distractors
        .iter()
        .copied()
        .filter(|(word_id, _, distractor)| {
            exportable.contains(word_id) && exportable.contains(distractor)
        })
        .collect();
    distractors.sort_unstable();

    // Only the anchors a shipped word actually mentions. `anchor_refs` carries
    // the edges the dependency closure deliberately dropped, so this is the one
    // place that knows which Chinese glosses the release has to carry.
    let referenced: BTreeSet<i64> = payload
        .anchor_refs
        .iter()
        .filter(|(word_id, _)| exportable.contains(word_id))
        .map(|(_, anchor)| *anchor)
        .collect();
    let mut gloss_anchors: Vec<&crate::model::GlossAnchor> = payload
        .gloss_anchors
        .iter()
        .filter(|anchor| referenced.contains(&anchor.word_id))
        .collect();
    gloss_anchors.sort_by_key(|anchor| anchor.word_id);

    ReleaseRows {
        plan_id: payload.plan_id,
        words,
        senses,
        examples,
        groups,
        distractors,
        gloss_anchors,
    }
}

/// Media file hashes the given rows reference, deduplicated and sorted.
pub fn media_hashes(rows: &ReleaseRows<'_>) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for word in &rows.words {
        if let Some(hash) = &word.image_file_hash {
            set.insert(hash.clone());
        }
        if let Some(hash) = &word.word_audio_hash {
            set.insert(hash.clone());
        }
    }
    for sense in &rows.senses {
        if let Some(hash) = &sense.audio_hash {
            set.insert(hash.clone());
        }
    }
    for example in &rows.examples {
        if let Some(hash) = &example.audio_hash {
            set.insert(hash.clone());
        }
    }
    set.into_iter().collect()
}

/// Relative bundle path of a media file.
pub fn media_path(kind: &str, file_hash: &str) -> String {
    match kind {
        "image" => format!("img/{file_hash}.webp"),
        _ => format!("audio/{file_hash}.ogg"),
    }
}

/// Date-free digest of the whole payload.
pub fn content_hash(rows: &ReleaseRows<'_>, media: &[(String, String, u64)]) -> String {
    let mut hasher = HashInput::new(EXPORT_ALGO_VER)
        .field(RELEASE_SCHEMA_VER)
        .field(rows.plan_id.to_string())
        .field(rows.words.len().to_string());

    for word in &rows.words {
        hasher = hasher.field(format!(
            "w|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            word.word_id,
            word.lemma,
            word.phonetic.as_deref().unwrap_or(""),
            word.frequency_rank.unwrap_or(-1),
            word.role.as_str(),
            word.group_seq,
            word.learning_order,
            word.etymology.as_deref().unwrap_or(""),
            word.image_file_hash.as_deref().unwrap_or(""),
            word.word_audio_hash.as_deref().unwrap_or(""),
        ));
    }
    for sense in &rows.senses {
        hasher = hasher.field(format!(
            "s|{}|{}|{}|{}|{}",
            sense.word_id,
            sense.pos,
            sense.definition,
            i32::from(sense.is_primary),
            sense.audio_hash.as_deref().unwrap_or(""),
        ));
    }
    for example in &rows.examples {
        hasher = hasher.field(format!(
            "e|{}|{}|{}|{}|{}|{}",
            example.word_id,
            example.display_order,
            example.sentence,
            example.hl_start,
            example.hl_end,
            example.audio_hash.as_deref().unwrap_or(""),
        ));
    }
    for group in &rows.groups {
        hasher = hasher.field(format!("g|{}|{}", group.group_seq, group.group_type));
    }
    for (word_id, rank, distractor) in &rows.distractors {
        hasher = hasher.field(format!("d|{word_id}|{rank}|{distractor}"));
    }
    // Appended after the distractors, so a release with no anchors hashes
    // exactly as it did before ruling #18a existed.
    for anchor in &rows.gloss_anchors {
        hasher = hasher.field(format!(
            "ga|{}|{}|{}",
            anchor.word_id, anchor.lemma, anchor.zh_gloss
        ));
    }
    for (path, hash, bytes) in media {
        hasher = hasher.field(format!("m|{path}|{hash}|{bytes}"));
    }
    hasher.finish()
}

/// `YYYY.MM.DD+<hash8>`
pub fn content_version(date: chrono::NaiveDate, content_hash: &str) -> String {
    format!(
        "{}+{}",
        date.format("%Y.%m.%d"),
        &content_hash[..8.min(content_hash.len())]
    )
}

/// Write the whole bundle.
pub fn write_bundle(
    out_dir: &Path,
    data_dir: &Path,
    payload: &ExportPayload,
    rows: &ReleaseRows<'_>,
    date: chrono::NaiveDate,
    exporter: &str,
) -> ExportResult<WrittenRelease> {
    if out_dir.exists() {
        // A release directory is written once. Refusing to merge into an
        // existing one is what keeps "the bundle is exactly this export".
        return Err(ExportError::OutputExists(out_dir.to_path_buf()));
    }
    std::fs::create_dir_all(out_dir)?;

    // 1. Copy media, collecting manifest entries as we go.
    let hashes = media_hashes(rows);
    let mut media_entries: Vec<(String, String, u64)> = Vec::with_capacity(hashes.len());
    let mut manifest_files: Vec<ManifestEntry> = Vec::with_capacity(hashes.len() + 1);
    for hash in &hashes {
        let entry = payload
            .media
            .get(hash)
            .ok_or_else(|| ExportError::MissingMedia(hash.clone()))?;
        let source = data_dir.join(&entry.rel_path);
        let bytes = std::fs::read(&source)
            .map_err(|err| ExportError::UnreadableMedia(source.display().to_string(), err))?;
        // Trust nothing: the registry says this is the content address, so it
        // had better be.
        let actual = file_hash(&bytes);
        if &actual != hash {
            return Err(ExportError::MediaHashMismatch {
                expected: hash.clone(),
                actual,
            });
        }
        let path = media_path(&entry.kind, hash);
        let target = out_dir.join(&path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, &bytes)?;
        media_entries.push((path.clone(), hash.clone(), bytes.len() as u64));
        manifest_files.push(ManifestEntry {
            path,
            bytes: bytes.len() as u64,
            file_hash: hash.clone(),
        });
    }

    // 2. Version identity, from the payload alone.
    let content_hash = content_hash(rows, &media_entries);
    let content_version = content_version(date, &content_hash);

    // 3. release.db.
    let db_path = out_dir.join("release.db");
    write_release_db(&db_path, rows, &content_version, payload, date)?;
    let db_bytes = std::fs::read(&db_path)?;
    let db_file_hash = file_hash(&db_bytes);
    manifest_files.push(ManifestEntry {
        path: "release.db".to_string(),
        bytes: db_bytes.len() as u64,
        file_hash: db_file_hash.clone(),
    });
    manifest_files.sort_by(|a, b| a.path.cmp(&b.path));

    // 4. manifest.json.
    let manifest = Manifest {
        content_version: content_version.clone(),
        schema_ver: RELEASE_SCHEMA_VER.to_string(),
        exporter: exporter.to_string(),
        plan_id: rows.plan_id,
        word_count: rows.words.len(),
        media_count: hashes.len(),
        total_bytes: manifest_files.iter().map(|file| file.bytes).sum(),
        files: manifest_files,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)? + "\n";
    std::fs::write(out_dir.join("manifest.json"), manifest_json)?;

    Ok(WrittenRelease {
        out_dir: out_dir.to_path_buf(),
        content_version,
        content_hash,
        db_file_hash,
        manifest,
        media_hashes: hashes,
        word_count: rows.words.len(),
    })
}

/// Build `release.db` from scratch, deterministically.
pub fn write_release_db(
    path: &Path,
    rows: &ReleaseRows<'_>,
    content_version: &str,
    payload: &ExportPayload,
    date: chrono::NaiveDate,
) -> ExportResult<()> {
    let conn = Connection::open(path)?;
    // page_size must be set before the first table exists, and journal_mode
    // DELETE keeps the artifact a single file with no sidecars.
    conn.pragma_update(None, "page_size", 4096)?;
    let _: String = conn.query_row("PRAGMA journal_mode = DELETE", [], |row| row.get(0))?;
    conn.execute_batch(RELEASE_DB_SQL)?;

    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO groups (group_id, group_order, group_type) VALUES (?1, ?2, ?3)",
        )?;
        for group in &rows.groups {
            stmt.execute(rusqlite::params![
                group.group_seq,
                group.group_seq,
                group.group_type
            ])?;
        }
    }
    // Build the word → image path lookup for the example writer below.
    let mut word_image: std::collections::HashMap<i64, String> =
        std::collections::HashMap::with_capacity(rows.words.len());
    {
        let mut stmt = tx.prepare(
            "INSERT INTO words (word_id, word, phonetic, frequency_rank, role, group_id,
                                learning_order, etymology, word_audio_file)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for word in &rows.words {
            if let Some(hash) = &word.image_file_hash {
                word_image.insert(word.word_id, media_path("image", hash));
            }
            let audio = word
                .word_audio_hash
                .as_ref()
                .map(|hash| media_path("audio", hash))
                .ok_or_else(|| ExportError::MissingAsset(word.word_id, "word audio"))?;
            stmt.execute(rusqlite::params![
                word.word_id,
                word.lemma,
                word.phonetic,
                word.frequency_rank,
                word.role.as_str(),
                word.group_seq,
                word.learning_order,
                word.etymology,
                audio,
            ])?;
        }
    }
    {
        let mut stmt = tx.prepare(
            "INSERT INTO senses (sense_id, word_id, pos, definition, is_primary, def_audio_file)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (index, sense) in rows.senses.iter().enumerate() {
            let audio = sense
                .audio_hash
                .as_ref()
                .map(|hash| media_path("audio", hash))
                .ok_or_else(|| ExportError::MissingAsset(sense.word_id, "sense audio"))?;
            stmt.execute(rusqlite::params![
                index as i64 + 1,
                sense.word_id,
                sense.pos,
                sense.definition,
                i64::from(sense.is_primary),
                audio,
            ])?;
        }
    }
    {
        let mut stmt = tx.prepare(
            "INSERT INTO examples (example_id, word_id, display_order, sentence, hl_start,
                                   hl_end, ex_audio_file, image_file)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for (index, example) in rows.examples.iter().enumerate() {
            let audio = example
                .audio_hash
                .as_ref()
                .map(|hash| media_path("audio", hash))
                .ok_or_else(|| ExportError::MissingAsset(example.word_id, "example audio"))?;
            // Only the slot-1 example carries the word's image.
            let image: Option<&str> = if example.display_order == 1 {
                word_image.get(&example.word_id).map(|s| s.as_str())
            } else {
                None
            };
            stmt.execute(rusqlite::params![
                index as i64 + 1,
                example.word_id,
                example.display_order,
                example.sentence,
                example.hl_start,
                example.hl_end,
                audio,
                image,
            ])?;
        }
    }
    {
        let mut stmt = tx.prepare(
            "INSERT INTO distractors (word_id, rank, distractor_word_id) VALUES (?1, ?2, ?3)",
        )?;
        for (word_id, rank, distractor) in &rows.distractors {
            stmt.execute(rusqlite::params![word_id, rank, distractor])?;
        }
    }
    {
        let mut stmt =
            tx.prepare("INSERT INTO gloss_anchors (word_id, word, zh_gloss) VALUES (?1, ?2, ?3)")?;
        for anchor in &rows.gloss_anchors {
            stmt.execute(rusqlite::params![
                anchor.word_id,
                anchor.lemma,
                anchor.zh_gloss
            ])?;
        }
    }
    {
        let mut stmt = tx.prepare("INSERT INTO meta (key, value) VALUES (?1, ?2)")?;
        // Ordered by key, and `exported_at` is the export *date* only: a wall
        // clock in here would make two identical exports differ.
        for (key, value) in [
            ("content_version", content_version.to_string()),
            ("exported_at", date.format("%Y-%m-%d").to_string()),
            ("plan_id", payload.plan_id.to_string()),
            ("schema_ver", RELEASE_SCHEMA_VER.to_string()),
        ] {
            stmt.execute(rusqlite::params![key, value])?;
        }
    }
    tx.commit()?;

    // Compact and drop free pages so the file is a pure function of its rows.
    conn.execute_batch("VACUUM")?;
    drop(conn);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_ddl_is_the_contract_file() {
        assert!(RELEASE_DB_SQL.contains("CREATE TABLE words ("));
        assert!(RELEASE_DB_SQL.contains("CREATE TABLE distractors"));
        assert!(RELEASE_DB_SQL.contains("CREATE TABLE gloss_anchors"));
        assert!(RELEASE_DB_SQL.contains("CREATE TABLE meta"));
    }

    #[test]
    fn media_paths_follow_the_release_ddl_comments() {
        assert_eq!(media_path("image", "abc"), "img/abc.webp");
        assert_eq!(media_path("audio", "abc"), "audio/abc.ogg");
    }

    #[test]
    fn the_version_format_matches_the_whitepaper() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 8, 26).unwrap();
        let version = content_version(date, "0123456789abcdef");
        assert_eq!(version, "2026.08.26+01234567");
    }

    #[test]
    fn a_short_hash_does_not_panic() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap();
        assert_eq!(content_version(date, "abc"), "2026.01.02+abc");
    }
}
