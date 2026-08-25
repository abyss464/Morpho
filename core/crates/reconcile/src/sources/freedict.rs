//! Free Dictionary API (`https://api.dictionaryapi.dev`).
//!
//! Response shape: an array of entries, each with `meanings[]`, each with a
//! `partOfSpeech` and a list of `definitions[]`. A word the dictionary does not
//! know answers 404 with a `{"title": "No Definitions Found"}` body — the HTTP
//! layer already turns that into `Permanent`, and the caller records a
//! completion marker so the word is never re-fetched from this source.
//!
//! Parsing is deliberately tolerant of extra fields: the API is community-run
//! and adds keys without notice, and a new key must not stall the pipeline.

use serde::Deserialize;

use morpho_domain::canon::canonicalize;
use morpho_domain::error::TaskError;
use morpho_domain::types::{FetchedDefinition, Pos};

use crate::config::SourcesConfig;
use crate::sources::http;

/// Cap per word. The API returns everything it has, and a word with forty
/// senses would drown the console; the scorer only ever picks one per part of
/// speech anyway.
const MAX_DEFINITIONS: usize = 12;

#[derive(Debug, Deserialize)]
struct Entry {
    #[serde(default)]
    phonetic: Option<String>,
    #[serde(default)]
    phonetics: Vec<Phonetic>,
    #[serde(default)]
    meanings: Vec<Meaning>,
}

#[derive(Debug, Deserialize)]
struct Phonetic {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Meaning {
    #[serde(rename = "partOfSpeech", default)]
    part_of_speech: String,
    #[serde(default)]
    definitions: Vec<Definition>,
}

#[derive(Debug, Deserialize)]
struct Definition {
    #[serde(default)]
    definition: String,
}

/// What one Free Dictionary lookup produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FreedictEntry {
    pub definitions: Vec<FetchedDefinition>,
    /// IPA transcription, when the entry carries one.
    pub phonetic: Option<String>,
}

/// Fetch and parse one word.
pub async fn fetch(
    client: &reqwest::Client,
    config: &SourcesConfig,
    word: &str,
) -> Result<FreedictEntry, TaskError> {
    let url = format!(
        "{}/{}",
        config.freedict_url.trim_end_matches('/'),
        urlencode(word)
    );
    let body = http::get_bytes(client, &url, &[], &format!("freedict {word}")).await?;
    parse(&body)
}

/// Percent-encode a lemma for use as a path segment.
///
/// Lemmas are words, so this only has to handle spaces and the odd apostrophe;
/// anything non-alphanumeric is escaped rather than guessed at.
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Parse a Free Dictionary response body.
pub fn parse(body: &[u8]) -> Result<FreedictEntry, TaskError> {
    let entries: Vec<Entry> = serde_json::from_slice(body)
        .map_err(|err| TaskError::permanent(format!("freedict returned unparsable JSON: {err}")))?;

    let mut out = FreedictEntry::default();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

    for entry in &entries {
        if out.phonetic.is_none() {
            out.phonetic = entry
                .phonetic
                .as_deref()
                .map(canonicalize)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    entry
                        .phonetics
                        .iter()
                        .filter_map(|p| p.text.as_deref())
                        .map(canonicalize)
                        .find(|value| !value.is_empty())
                });
        }
        for meaning in &entry.meanings {
            let pos = Pos::normalize(&meaning.part_of_speech);
            for definition in &meaning.definitions {
                let text = canonicalize(&definition.definition);
                if text.is_empty() {
                    continue;
                }
                if !seen.insert((pos.as_str().to_string(), text.to_lowercase())) {
                    continue;
                }
                out.definitions.push(FetchedDefinition {
                    pos,
                    text,
                    // Keep the upstream label: the normalization above is
                    // lossy for closed-class words, and the audit trail should
                    // show what was actually said.
                    source_ref: Some(format!("freedict:{}", meaning.part_of_speech)),
                });
                if out.definitions.len() >= MAX_DEFINITIONS {
                    return Ok(out);
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BENEVOLENT: &str = r#"[{
        "word": "benevolent",
        "phonetic": "/bəˈnɛvələnt/",
        "phonetics": [{"text": "/bəˈnɛvələnt/", "audio": ""}],
        "meanings": [{
            "partOfSpeech": "adjective",
            "definitions": [
                {"definition": "Having a disposition to do good.",
                 "example": "Chinese and Eastern mythologies describe dragons as benevolent."},
                {"definition": "Possessing or manifesting love for mankind."}
            ],
            "antonyms": ["malevolent"]
        }]
    }]"#;

    #[test]
    fn parses_a_real_response() {
        let entry = parse(BENEVOLENT.as_bytes()).unwrap();
        assert_eq!(entry.phonetic.as_deref(), Some("/bəˈnɛvələnt/"));
        assert_eq!(entry.definitions.len(), 2);
        assert_eq!(entry.definitions[0].pos, Pos::Adj);
        assert_eq!(
            entry.definitions[0].text,
            "Having a disposition to do good."
        );
        assert_eq!(
            entry.definitions[0].source_ref.as_deref(),
            Some("freedict:adjective")
        );
    }

    #[test]
    fn falls_back_to_the_phonetics_array() {
        let body = r#"[{"phonetics":[{"text":""},{"text":"/səˈriːn/"}],"meanings":[]}]"#;
        let entry = parse(body.as_bytes()).unwrap();
        assert_eq!(entry.phonetic.as_deref(), Some("/səˈriːn/"));
    }

    #[test]
    fn deduplicates_repeated_senses() {
        let body = r#"[
          {"meanings":[{"partOfSpeech":"noun","definitions":[{"definition":"A thing."}]}]},
          {"meanings":[{"partOfSpeech":"noun","definitions":[{"definition":"a thing."}]}]}
        ]"#;
        let entry = parse(body.as_bytes()).unwrap();
        assert_eq!(entry.definitions.len(), 1);
    }

    #[test]
    fn the_same_text_under_two_parts_of_speech_survives() {
        let body = r#"[{"meanings":[
          {"partOfSpeech":"noun","definitions":[{"definition":"A record."}]},
          {"partOfSpeech":"verb","definitions":[{"definition":"A record."}]}
        ]}]"#;
        let entry = parse(body.as_bytes()).unwrap();
        assert_eq!(entry.definitions.len(), 2);
    }

    #[test]
    fn canonicalizes_definition_text() {
        let body = r#"[{"meanings":[{"partOfSpeech":"verb","definitions":[{"definition":"  give   up\n"}]}]}]"#;
        let entry = parse(body.as_bytes()).unwrap();
        assert_eq!(entry.definitions[0].text, "give up");
    }

    #[test]
    fn empty_definitions_are_dropped_not_stored_blank() {
        let body = r#"[{"meanings":[{"partOfSpeech":"verb","definitions":[{"definition":"   "},{"definition":"go"}]}]}]"#;
        let entry = parse(body.as_bytes()).unwrap();
        assert_eq!(entry.definitions.len(), 1);
        assert_eq!(entry.definitions[0].text, "go");
    }

    #[test]
    fn unknown_parts_of_speech_fold_into_the_contract_vocabulary() {
        let body =
            r#"[{"meanings":[{"partOfSpeech":"pronoun","definitions":[{"definition":"x y"}]}]}]"#;
        let entry = parse(body.as_bytes()).unwrap();
        assert_eq!(entry.definitions[0].pos, Pos::Phrase);
        assert_eq!(
            entry.definitions[0].source_ref.as_deref(),
            Some("freedict:pronoun")
        );
    }

    #[test]
    fn caps_the_number_of_senses() {
        let definitions: Vec<String> = (0..40)
            .map(|i| format!(r#"{{"definition":"sense number {i}"}}"#))
            .collect();
        let body = format!(
            r#"[{{"meanings":[{{"partOfSpeech":"noun","definitions":[{}]}}]}}]"#,
            definitions.join(",")
        );
        let entry = parse(body.as_bytes()).unwrap();
        assert_eq!(entry.definitions.len(), MAX_DEFINITIONS);
    }

    #[test]
    fn an_empty_array_is_a_legitimate_empty_result() {
        let entry = parse(b"[]").unwrap();
        assert!(entry.definitions.is_empty());
        assert!(entry.phonetic.is_none());
    }

    #[test]
    fn a_not_found_body_is_a_parse_failure_not_a_silent_empty() {
        // The 404 body is an object, not an array; the HTTP layer classifies
        // the status first, so reaching the parser at all means something odd.
        let err = parse(br#"{"title":"No Definitions Found"}"#).unwrap_err();
        assert_eq!(err.kind(), morpho_domain::error::ErrorKind::Permanent);
    }

    #[test]
    fn extra_upstream_fields_are_ignored() {
        let body = r#"[{"word":"x","license":{"name":"CC"},"sourceUrls":["u"],
                        "meanings":[{"partOfSpeech":"noun","synonyms":["y"],
                                     "definitions":[{"definition":"a thing","synonyms":[]}]}]}]"#;
        assert_eq!(parse(body.as_bytes()).unwrap().definitions.len(), 1);
    }

    #[test]
    fn urlencoding_escapes_everything_unsafe() {
        assert_eq!(urlencode("benevolent"), "benevolent");
        assert_eq!(urlencode("well-meaning"), "well-meaning");
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("don't"), "don%27t");
        assert_eq!(urlencode("../etc"), "..%2Fetc");
    }
}
