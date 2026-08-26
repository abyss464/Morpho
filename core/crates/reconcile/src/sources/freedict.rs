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
use morpho_domain::types::{FetchedDefinition, FetchedExample, Pos};

use crate::config::SourcesConfig;
use crate::sources::{http, sentence};

/// Cap per word. The API returns everything it has, and a word with forty
/// senses would drown the console; the scorer only ever picks one per part of
/// speech anyway.
const MAX_DEFINITIONS: usize = 12;

/// Cap per word for the sentences mined out of the same payload (ruling #18).
/// Three slots get filled; a couple of spares let the scorer choose.
pub const MAX_EXAMPLES: usize = 6;

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
    /// Per-sense usage sentence. Present on a minority of senses, which is why
    /// mining it is worth doing for free rather than paying for a second call.
    #[serde(default)]
    example: Option<String>,
}

/// What one Free Dictionary lookup produced.
///
/// One payload, three products. The API answers with every sense the dictionary
/// has *and* the usage sentence attached to each, so the examples are already in
/// hand by the time the definitions are parsed; fetching them again over a
/// separate job would double this lane's traffic for no new information.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FreedictEntry {
    pub definitions: Vec<FetchedDefinition>,
    /// Usage sentences mined from `meanings[].definitions[].example`, already
    /// canonicalized with highlight offsets into that text (ruling #18).
    pub examples: Vec<FetchedExample>,
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
    parse(&body, word)
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

/// Parse a Free Dictionary response body for `word`.
///
/// The lemma is needed because the mined sentences carry highlight offsets, and
/// a sentence in which the word cannot be located is dropped rather than stored
/// with a guessed range.
pub fn parse(body: &[u8], word: &str) -> Result<FreedictEntry, TaskError> {
    let entries: Vec<Entry> = serde_json::from_slice(body)
        .map_err(|err| TaskError::permanent(format!("freedict returned unparsable JSON: {err}")))?;

    let mut out = FreedictEntry::default();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut seen_examples: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut definitions_full = false;

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
                // Sentences are mined even after the sense cap is reached: the
                // two lists fill independently, and cutting one short must not
                // silently cut the other.
                if out.examples.len() < MAX_EXAMPLES {
                    if let Some(raw) = definition.example.as_deref() {
                        if let Some(example) = sentence::candidate(
                            raw,
                            word,
                            Some(format!("freedict:{}", meaning.part_of_speech)),
                        ) {
                            if seen_examples.insert(example.text.to_lowercase()) {
                                out.examples.push(example);
                            }
                        }
                    }
                }
                if definitions_full {
                    continue;
                }
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
                definitions_full = out.definitions.len() >= MAX_DEFINITIONS;
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
        let entry = parse(BENEVOLENT.as_bytes(), "benevolent").unwrap();
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
        let entry = parse(body.as_bytes(), "x").unwrap();
        assert_eq!(entry.phonetic.as_deref(), Some("/səˈriːn/"));
    }

    #[test]
    fn deduplicates_repeated_senses() {
        let body = r#"[
          {"meanings":[{"partOfSpeech":"noun","definitions":[{"definition":"A thing."}]}]},
          {"meanings":[{"partOfSpeech":"noun","definitions":[{"definition":"a thing."}]}]}
        ]"#;
        let entry = parse(body.as_bytes(), "x").unwrap();
        assert_eq!(entry.definitions.len(), 1);
    }

    #[test]
    fn the_same_text_under_two_parts_of_speech_survives() {
        let body = r#"[{"meanings":[
          {"partOfSpeech":"noun","definitions":[{"definition":"A record."}]},
          {"partOfSpeech":"verb","definitions":[{"definition":"A record."}]}
        ]}]"#;
        let entry = parse(body.as_bytes(), "x").unwrap();
        assert_eq!(entry.definitions.len(), 2);
    }

    #[test]
    fn canonicalizes_definition_text() {
        let body = r#"[{"meanings":[{"partOfSpeech":"verb","definitions":[{"definition":"  give   up\n"}]}]}]"#;
        let entry = parse(body.as_bytes(), "x").unwrap();
        assert_eq!(entry.definitions[0].text, "give up");
    }

    #[test]
    fn empty_definitions_are_dropped_not_stored_blank() {
        let body = r#"[{"meanings":[{"partOfSpeech":"verb","definitions":[{"definition":"   "},{"definition":"go"}]}]}]"#;
        let entry = parse(body.as_bytes(), "x").unwrap();
        assert_eq!(entry.definitions.len(), 1);
        assert_eq!(entry.definitions[0].text, "go");
    }

    #[test]
    fn unknown_parts_of_speech_fold_into_the_contract_vocabulary() {
        let body =
            r#"[{"meanings":[{"partOfSpeech":"pronoun","definitions":[{"definition":"x y"}]}]}]"#;
        let entry = parse(body.as_bytes(), "x").unwrap();
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
        let entry = parse(body.as_bytes(), "x").unwrap();
        assert_eq!(entry.definitions.len(), MAX_DEFINITIONS);
    }

    #[test]
    fn an_empty_array_is_a_legitimate_empty_result() {
        let entry = parse(b"[]", "x").unwrap();
        assert!(entry.definitions.is_empty());
        assert!(entry.phonetic.is_none());
    }

    #[test]
    fn a_not_found_body_is_a_parse_failure_not_a_silent_empty() {
        // The 404 body is an object, not an array; the HTTP layer classifies
        // the status first, so reaching the parser at all means something odd.
        let err = parse(br#"{"title":"No Definitions Found"}"#, "x").unwrap_err();
        assert_eq!(err.kind(), morpho_domain::error::ErrorKind::Permanent);
    }

    #[test]
    fn extra_upstream_fields_are_ignored() {
        let body = r#"[{"word":"x","license":{"name":"CC"},"sourceUrls":["u"],
                        "meanings":[{"partOfSpeech":"noun","synonyms":["y"],
                                     "definitions":[{"definition":"a thing","synonyms":[]}]}]}]"#;
        assert_eq!(parse(body.as_bytes(), "x").unwrap().definitions.len(), 1);
    }

    // -- example mining (ruling #18) ---------------------------------------

    #[test]
    fn usage_sentences_are_mined_from_the_definition_payload() {
        let entry = parse(BENEVOLENT.as_bytes(), "benevolent").unwrap();
        assert_eq!(entry.examples.len(), 1);
        let example = &entry.examples[0];
        assert_eq!(
            example.text,
            "Chinese and Eastern mythologies describe dragons as benevolent."
        );
        assert_eq!(
            &example.text[example.hl_start as usize..example.hl_end as usize],
            "benevolent"
        );
        assert_eq!(example.source_ref.as_deref(), Some("freedict:adjective"));
    }

    #[test]
    fn a_sentence_that_never_says_the_word_is_not_mined() {
        let body = r#"[{"meanings":[{"partOfSpeech":"verb","definitions":[
            {"definition":"to give up","example":"He walked away from all of it."}]}]}]"#;
        let entry = parse(body.as_bytes(), "abandon").unwrap();
        assert_eq!(entry.definitions.len(), 1);
        assert!(entry.examples.is_empty(), "{:?}", entry.examples);
    }

    #[test]
    fn an_inflected_sentence_is_mined_with_the_inflected_span() {
        let body = r#"[{"meanings":[{"partOfSpeech":"verb","definitions":[
            {"definition":"to change","example":"Species adapted to a warming climate."}]}]}]"#;
        let entry = parse(body.as_bytes(), "adapt").unwrap();
        let example = &entry.examples[0];
        assert_eq!(
            &example.text[example.hl_start as usize..example.hl_end as usize],
            "adapted"
        );
    }

    #[test]
    fn repeated_sentences_are_mined_once() {
        let body = r#"[{"meanings":[{"partOfSpeech":"adj","definitions":[
            {"definition":"a","example":"A serene lake lay below us."},
            {"definition":"b","example":"a serene lake lay below us."}]}]}]"#;
        let entry = parse(body.as_bytes(), "serene").unwrap();
        assert_eq!(entry.examples.len(), 1);
    }

    #[test]
    fn caps_the_number_of_mined_sentences() {
        let definitions: Vec<String> = (0..20)
            .map(|i| {
                format!(
                    r#"{{"definition":"sense {i}","example":"A serene number {i} lake lay below."}}"#
                )
            })
            .collect();
        let body = format!(
            r#"[{{"meanings":[{{"partOfSpeech":"adj","definitions":[{}]}}]}}]"#,
            definitions.join(",")
        );
        let entry = parse(body.as_bytes(), "serene").unwrap();
        assert_eq!(entry.examples.len(), MAX_EXAMPLES);
    }

    /// The two caps are independent: hitting the sense limit must not silently
    /// stop the sentence mining, which is a different budget.
    #[test]
    fn sentences_are_still_mined_past_the_sense_cap() {
        let definitions: Vec<String> = (0..MAX_DEFINITIONS + 4)
            .map(|i| {
                format!(
                    r#"{{"definition":"distinct sense {i}","example":"A serene number {i} lake lay below."}}"#
                )
            })
            .collect();
        let body = format!(
            r#"[{{"meanings":[{{"partOfSpeech":"adj","definitions":[{}]}}]}}]"#,
            definitions.join(",")
        );
        let entry = parse(body.as_bytes(), "serene").unwrap();
        assert_eq!(entry.definitions.len(), MAX_DEFINITIONS);
        assert_eq!(entry.examples.len(), MAX_EXAMPLES);
    }

    #[test]
    fn a_payload_with_no_usage_sentences_mines_nothing() {
        let body = r#"[{"meanings":[{"partOfSpeech":"noun","definitions":[
            {"definition":"A thing that is."}]}]}]"#;
        let entry = parse(body.as_bytes(), "thing").unwrap();
        assert_eq!(entry.definitions.len(), 1);
        assert!(entry.examples.is_empty());
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
