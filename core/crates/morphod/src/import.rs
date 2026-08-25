//! Word-list import.
//!
//! Two accepted shapes, decided per line so a mixed file still works:
//!   * JSONL — `{"word": "...", "phonetic": "...", "frequency_rank": n}`
//!   * plain text — one word per line
//!
//! Blank lines and `#` comments are ignored. The whole file lands in a single
//! transaction, and re-importing the same list is a no-op.

use std::path::Path;

use anyhow::{bail, Context, Result};

use morpho_domain::event::Actor;
use morpho_domain::types::{CreatedBy, Role, WordImport};
use morpho_store::{ImportStats, Store, WriteOp};

/// Parse a word list from disk.
pub fn parse_wordlist(path: &Path) -> Result<Vec<WordImport>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading word list {}", path.display()))?;
    parse_wordlist_str(&raw, &path.display().to_string())
}

pub fn parse_wordlist_str(raw: &str, origin: &str) -> Result<Vec<WordImport>> {
    let mut out = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('{') {
            let entry: WordImport = serde_json::from_str(line)
                .with_context(|| format!("{origin}:{}: invalid JSONL row", index + 1))?;
            if entry.word.trim().is_empty() {
                bail!("{origin}:{}: JSONL row has an empty word", index + 1);
            }
            out.push(entry);
        } else {
            out.push(WordImport {
                word: line.to_string(),
                ..Default::default()
            });
        }
    }
    Ok(out)
}

/// Import a word list into the working database.
pub async fn import_wordlist(store: &Store, path: &Path, role: Role) -> Result<ImportStats> {
    let words = parse_wordlist(path)?;
    if words.is_empty() {
        bail!("{} contains no words", path.display());
    }
    let outcome = store
        .write(
            Actor::Cli,
            WriteOp::import_words(role, CreatedBy::Import, words),
        )
        .await?;
    outcome
        .result
        .import_stats()
        .copied()
        .context("import did not return statistics")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_text() {
        let words = parse_wordlist_str("kind\n well \n\n# a comment\nmeaning\n", "test").unwrap();
        let lemmas: Vec<&str> = words.iter().map(|w| w.word.as_str()).collect();
        assert_eq!(lemmas, ["kind", "well", "meaning"]);
        assert!(words[0].phonetic.is_none());
    }

    #[test]
    fn parses_jsonl() {
        let raw = r#"
            {"word":"benevolent","phonetic":"/bəˈnevələnt/","frequency_rank":4312}
            {"word":"serene"}
        "#;
        let words = parse_wordlist_str(raw, "test").unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word, "benevolent");
        assert_eq!(words[0].frequency_rank, Some(4312));
        assert_eq!(words[1].phonetic, None);
    }

    #[test]
    fn accepts_the_lemma_alias() {
        let words = parse_wordlist_str(r#"{"lemma":"adept"}"#, "test").unwrap();
        assert_eq!(words[0].word, "adept");
    }

    #[test]
    fn reports_the_offending_line() {
        let err = parse_wordlist_str("ok\n{\"word\":}\n", "list.jsonl").unwrap_err();
        assert!(err.to_string().contains("list.jsonl:2"), "{err}");
    }
}
