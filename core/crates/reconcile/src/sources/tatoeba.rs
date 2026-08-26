//! Tatoeba sentence search (`https://tatoeba.org/en/api_v0/search`).
//!
//! A community corpus of natural sentences, free of credentials and free of
//! charge, which makes it the honest second example source behind the Free
//! Dictionary's own usage lines (admin-api.md ruling #18).
//!
//! Two things are worth knowing about the payload. Each result carries its own
//! `license` — sentences are contributed individually, and CC BY 2.0 FR sits
//! next to CC0 1.0 in the same response — so the licence is recorded per
//! candidate rather than assumed for the source. And the search is a relevance
//! ranking, not an exact-match filter: a query for "serene" happily returns
//! "Walk serenely." Every sentence is therefore re-checked against the lemma
//! locally, and one that does not actually contain the word (or a simple
//! inflection of it) is dropped rather than stored with a guessed highlight.

use serde::Deserialize;

use morpho_domain::error::TaskError;
use morpho_domain::types::FetchedExample;

use crate::config::SourcesConfig;
use crate::sources::{http, sentence};

/// Candidates kept per word. Matches the Free Dictionary cap: three slots get
/// filled, and the spares give the scorer something to choose between.
pub const MAX_PER_WORD: usize = 6;

/// Sentences requested before local filtering. The API pages at 10 and most of
/// a page survives the lemma check, so one page is enough to fill the cap.
const REQUEST_LIMIT: usize = 10;

/// Search English sentences containing `word`.
pub async fn search(
    client: &reqwest::Client,
    config: &SourcesConfig,
    word: &str,
) -> Result<Vec<FetchedExample>, TaskError> {
    let url = format!(
        "{}?from=eng&query={}&sort=relevance&limit={REQUEST_LIMIT}",
        config.tatoeba_url.trim_end_matches('/'),
        http::encode_query(word)
    );
    let body: SearchResponse =
        http::get_json(client, &url, &[], &format!("tatoeba {word}")).await?;
    Ok(collect(&body, word))
}

/// Parse a response body. Split out so the shape is testable without a socket.
pub fn parse(body: &[u8], word: &str) -> Result<Vec<FetchedExample>, TaskError> {
    let parsed: SearchResponse = serde_json::from_slice(body)
        .map_err(|err| TaskError::permanent(format!("tatoeba returned unparsable JSON: {err}")))?;
    Ok(collect(&parsed, word))
}

fn collect(body: &SearchResponse, word: &str) -> Vec<FetchedExample> {
    let mut out: Vec<FetchedExample> = Vec::new();
    for result in &body.results {
        if out.len() >= MAX_PER_WORD {
            break;
        }
        // The `from=eng` filter is the server's promise, not a guarantee.
        if !result.lang.is_empty() && result.lang != "eng" {
            continue;
        }
        let source_ref = match result.license.as_deref().map(str::trim) {
            Some(license) if !license.is_empty() => {
                format!("tatoeba:{}; {license}", result.id)
            }
            _ => format!("tatoeba:{}", result.id),
        };
        let Some(example) = sentence::candidate(&result.text, word, Some(source_ref)) else {
            continue;
        };
        if out.iter().any(|kept| kept.text == example.text) {
            continue;
        }
        out.push(example);
    }
    out
}

#[derive(Debug, Default, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<Sentence>,
}

#[derive(Debug, Deserialize)]
struct Sentence {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    text: String,
    #[serde(default)]
    lang: String,
    /// Per-sentence licence, e.g. `CC BY 2.0 FR` or `CC0 1.0`.
    #[serde(default)]
    license: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from a live `?from=eng&query=serene` response.
    const SERENE: &str = r#"{
      "paging": {"Sentences": {"count": 67, "page": 1}},
      "results": [
        {"id": 12668483, "text": "Walk serenely.", "lang": "eng",
         "license": "CC BY 2.0 FR", "user": {"username": "anzart"}},
        {"id": 12680232, "text": "The weather is serene today.", "lang": "eng",
         "license": "CC BY 2.0 FR", "translations": [[]]},
        {"id": 12768923, "text": "A serene lake lay below the ridge.", "lang": "eng",
         "license": "CC0 1.0", "user": {"username": "Igider"}}
      ]
    }"#;

    fn span(example: &FetchedExample) -> &str {
        &example.text[example.hl_start as usize..example.hl_end as usize]
    }

    #[test]
    fn keeps_only_sentences_that_contain_the_word() {
        let examples = parse(SERENE.as_bytes(), "serene").unwrap();
        // "Walk serenely." is both a non-match and too short to be a sentence.
        assert_eq!(examples.len(), 2);
        assert!(examples.iter().all(|e| span(e) == "serene"));
    }

    #[test]
    fn the_sentence_id_and_licence_ride_in_the_source_ref() {
        let examples = parse(SERENE.as_bytes(), "serene").unwrap();
        assert_eq!(
            examples[0].source_ref.as_deref(),
            Some("tatoeba:12680232; CC BY 2.0 FR")
        );
        assert_eq!(
            examples[1].source_ref.as_deref(),
            Some("tatoeba:12768923; CC0 1.0")
        );
    }

    #[test]
    fn a_sentence_without_a_licence_still_records_its_id() {
        let body = r#"{"results":[{"id":7,"text":"A serene lake lay below.","lang":"eng"}]}"#;
        let examples = parse(body.as_bytes(), "serene").unwrap();
        assert_eq!(examples[0].source_ref.as_deref(), Some("tatoeba:7"));
    }

    #[test]
    fn non_english_results_are_dropped() {
        let body = r#"{"results":[
            {"id":1,"text":"La vetero estas serena hodiau.","lang":"epo","license":"CC BY 2.0 FR"},
            {"id":2,"text":"The weather is serene today.","lang":"eng","license":"CC0 1.0"}
        ]}"#;
        let examples = parse(body.as_bytes(), "serene").unwrap();
        assert_eq!(examples.len(), 1);
        assert_eq!(
            examples[0].source_ref.as_deref(),
            Some("tatoeba:2; CC0 1.0")
        );
    }

    #[test]
    fn inflections_count_as_the_word() {
        let body = r#"{"results":[
            {"id":3,"text":"Species adapted to a warming climate.","lang":"eng"}
        ]}"#;
        let examples = parse(body.as_bytes(), "adapt").unwrap();
        assert_eq!(span(&examples[0]), "adapted");
    }

    #[test]
    fn duplicate_sentences_collapse() {
        let body = r#"{"results":[
            {"id":1,"text":"A serene lake lay below.","lang":"eng"},
            {"id":2,"text":"A  serene   lake lay below.","lang":"eng"}
        ]}"#;
        assert_eq!(parse(body.as_bytes(), "serene").unwrap().len(), 1);
    }

    #[test]
    fn caps_the_number_of_candidates() {
        let rows: Vec<String> = (0..20)
            .map(|i| {
                format!(r#"{{"id":{i},"text":"A serene number {i} lake lay below.","lang":"eng"}}"#)
            })
            .collect();
        let body = format!(r#"{{"results":[{}]}}"#, rows.join(","));
        assert_eq!(
            parse(body.as_bytes(), "serene").unwrap().len(),
            MAX_PER_WORD
        );
    }

    #[test]
    fn an_empty_result_set_is_a_legitimate_answer() {
        assert!(parse(br#"{"paging":{},"results":[]}"#, "serene")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn extra_upstream_fields_are_ignored() {
        let body = r#"{"results":[{"id":9,"text":"A serene lake lay below.","lang":"eng",
            "correctness":0,"script":null,"transcriptions":[],"audios":[],
            "translations":[[{"id":1,"text":"x","lang":"fra"}]],"is_favorite":null}]}"#;
        assert_eq!(parse(body.as_bytes(), "serene").unwrap().len(), 1);
    }

    #[test]
    fn a_body_that_is_not_a_search_response_is_permanent() {
        let err = parse(b"<html>rate limited</html>", "serene").unwrap_err();
        assert_eq!(err.kind(), morpho_domain::error::ErrorKind::Permanent);
    }
}
