//! Wiktionary etymology.
//!
//! The REST `page/definition` endpoint returns senses but no etymology, so this
//! goes through the MediaWiki action API for the page's wikitext and reads the
//! English section's `===Etymology===` subsection.
//!
//! Wikitext is not prose: it is templates. Rendering it faithfully would mean
//! shipping Wiktionary's Lua modules, so instead the templates that actually
//! carry etymological content are unwrapped into readable English, and the rest
//! are dropped. Everything printed comes from the page — nothing is invented.

use serde::Deserialize;

use morpho_domain::canon::canonicalize;
use morpho_domain::error::TaskError;

use crate::config::SourcesConfig;
use crate::sources::http;

#[derive(Debug, Deserialize)]
struct ParseResponse {
    #[serde(default)]
    parse: Option<ParseBody>,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ParseBody {
    #[serde(default)]
    wikitext: String,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    code: String,
    #[serde(default)]
    info: String,
}

/// Fetch and render one word's etymology, or `None` when the page has no
/// English etymology section.
pub async fn fetch(
    client: &reqwest::Client,
    config: &SourcesConfig,
    word: &str,
) -> Result<Option<String>, TaskError> {
    let url = format!(
        "{}?action=parse&page={}&prop=wikitext&formatversion=2&format=json&redirects=1",
        config.wiktionary_url.0,
        encode_query(word)
    );
    let body = http::get_bytes(client, &url, &[], &format!("wiktionary {word}")).await?;
    let response: ParseResponse = serde_json::from_slice(&body).map_err(|err| {
        TaskError::permanent(format!("wiktionary returned unparsable JSON: {err}"))
    })?;

    if let Some(error) = response.error {
        // "missingtitle" is the normal answer for a word Wiktionary lacks:
        // a legitimate empty result, not a fault.
        if error.code == "missingtitle" || error.code == "invalidtitle" {
            return Ok(None);
        }
        return Err(TaskError::permanent(format!(
            "wiktionary {word}: {} ({})",
            error.info, error.code
        )));
    }

    let Some(parse) = response.parse else {
        return Ok(None);
    };
    Ok(extract_etymology(&parse.wikitext))
}

fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('_'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Pull the English `===Etymology===` body out of a page's wikitext.
pub fn extract_etymology(wikitext: &str) -> Option<String> {
    let english = english_section(wikitext)?;
    let body = subsection(english, "Etymology")?;
    let rendered = render_wikitext(body);
    let rendered = canonicalize(&rendered);
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

/// Slice out `==English==` … up to the next *language* heading.
///
/// Level-2 headings are languages; `===` and deeper are sections inside one.
/// The scan therefore has to reject a `\n==` that is really the start of a
/// `\n===`, or every page would end at its own Etymology heading.
fn english_section(wikitext: &str) -> Option<&str> {
    let start = wikitext.find("==English==")? + "==English==".len();
    let rest = &wikitext[start..];
    let end = rest
        .match_indices("\n==")
        .find(|(index, _)| !rest[index + 3..].starts_with('='))
        .map(|(index, _)| index)
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Slice out `===Name===` (or `===Name 1===`) up to the next `===` heading.
fn subsection<'a>(section: &'a str, name: &str) -> Option<&'a str> {
    let mut cursor = 0usize;
    while let Some(found) = section[cursor..].find("===") {
        let heading_start = cursor + found;
        let after = heading_start + 3;
        let heading_end = section[after..].find("===")? + after;
        let heading = section[after..heading_end].trim();
        let body_start = heading_end + 3;
        let body_end = section[body_start..]
            .find("\n===")
            .map(|i| i + body_start)
            .unwrap_or(section.len());

        // Match "Etymology" and the numbered variants Wiktionary uses when a
        // page has several unrelated origins.
        let matches = heading == name
            || heading
                .strip_prefix(name)
                .is_some_and(|tail| tail.trim().chars().all(|c| c.is_ascii_digit()));
        if matches {
            return Some(&section[body_start..body_end]);
        }
        cursor = body_start.max(heading_start + 3);
    }
    None
}

/// Wiktionary language codes that show up in etymologies often enough to be
/// worth naming. An unknown code is printed as-is rather than guessed.
fn language_name(code: &str) -> Option<&'static str> {
    Some(match code {
        "la" => "Latin",
        "grc" => "Ancient Greek",
        "el" => "Greek",
        "fro" => "Old French",
        "frm" => "Middle French",
        "fr" => "French",
        "ang" => "Old English",
        "enm" => "Middle English",
        "en" => "English",
        "gem-pro" => "Proto-Germanic",
        "ine-pro" => "Proto-Indo-European",
        "itc-pro" => "Proto-Italic",
        "goh" => "Old High German",
        "de" => "German",
        "nl" => "Dutch",
        "dum" => "Middle Dutch",
        "non" => "Old Norse",
        "it" => "Italian",
        "es" => "Spanish",
        "pt" => "Portuguese",
        "ML." | "ML" => "Medieval Latin",
        "LL." | "LL" => "Late Latin",
        "NL." | "NL" => "New Latin",
        "ar" => "Arabic",
        "he" => "Hebrew",
        "sa" => "Sanskrit",
        "ru" => "Russian",
        "gl" => "Galician",
        "ca" => "Catalan",
        "sco" => "Scots",
        "cel-pro" => "Proto-Celtic",
        "sla-pro" => "Proto-Slavic",
        _ => return None,
    })
}

/// Turn one wikitext fragment into readable English.
pub fn render_wikitext(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        // Template: {{ ... }} with nesting.
        if chars[i] == '{' && chars.get(i + 1) == Some(&'{') {
            let (body, next) = take_balanced(&chars, i, '{', '}');
            out.push_str(&render_template(&body));
            i = next;
            continue;
        }
        // Wiki link: [[target]] or [[target|label]].
        if chars[i] == '[' && chars.get(i + 1) == Some(&'[') {
            let (body, next) = take_balanced(&chars, i, '[', ']');
            let label = body.rsplit('|').next().unwrap_or(&body);
            out.push_str(label.trim());
            i = next;
            continue;
        }
        // HTML comment.
        if chars[i] == '<' && chars[i..].iter().collect::<String>().starts_with("<!--") {
            let rest: String = chars[i..].iter().collect();
            match rest.find("-->") {
                Some(end) => {
                    i += rest[..end + 3].chars().count();
                    continue;
                }
                None => break,
            }
        }
        // Bold/italic markers.
        if chars[i] == '\'' && chars.get(i + 1) == Some(&'\'') {
            i += 2;
            while chars.get(i) == Some(&'\'') {
                i += 1;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }

    // Collapse the punctuation gaps that dropped templates leave behind.
    let cleaned = out
        .replace(" ,", ",")
        .replace(" .", ".")
        .replace("( ", "(")
        .replace(" )", ")");
    let mut text = canonicalize(&cleaned);
    while text.contains("  ") {
        text = text.replace("  ", " ");
    }
    text.trim_matches(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .to_string()
}

/// Consume a balanced `open`/`close` run starting at `start`, returning the
/// inner text and the index just past the closer.
fn take_balanced(chars: &[char], start: usize, open: char, close: char) -> (String, usize) {
    let mut depth = 0usize;
    let mut i = start;
    let mut body = String::new();
    while i < chars.len() {
        if chars[i] == open && chars.get(i + 1) == Some(&open) {
            depth += 1;
            i += 2;
            if depth > 1 {
                body.push(open);
                body.push(open);
            }
            continue;
        }
        if chars[i] == close && chars.get(i + 1) == Some(&close) {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return (body, i);
            }
            body.push(close);
            body.push(close);
            continue;
        }
        body.push(chars[i]);
        i += 1;
    }
    // Unbalanced markup: swallow the rest rather than emitting braces.
    (body, chars.len())
}

/// Render one template's contents.
fn render_template(body: &str) -> String {
    let parts: Vec<&str> = split_top_level(body);
    let Some(name) = parts.first().map(|n| n.trim()) else {
        return String::new();
    };
    // Positional arguments only; `|nocat=1`-style named ones are metadata.
    // Each is rendered first, because Wiktionary nests `{{l}}` inside `{{bor}}`
    // and raw braces must never reach the database.
    let rendered: Vec<String> = parts[1..]
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.contains('=') && !p.is_empty())
        .map(render_wikitext)
        .filter(|p| !p.is_empty())
        .collect();
    let positional: Vec<&str> = rendered.iter().map(String::as_str).collect();

    match name {
        // Derivation templates: {{tpl|<target lang>|<source lang>|<term>|<gloss>}}
        "bor" | "der" | "inh" | "uder" | "bor+" | "der+" | "inh+" | "learned borrowing"
        | "lbor" | "slbor" | "calque" | "cal" => {
            let language = positional.get(1).copied().unwrap_or_default();
            let term = positional.get(2).copied().unwrap_or_default();
            let named = language_name(language).unwrap_or(language);
            match (named.is_empty(), term.is_empty()) {
                (true, true) => String::new(),
                (false, true) => named.to_string(),
                (true, false) => term.to_string(),
                (false, false) => format!("{named} {term}"),
            }
        }
        // Cognates and mentions: {{tpl|<lang>|<term>}}
        "cog" | "ncog" | "noncog" | "m" | "mention" | "l" | "link" => {
            let language = positional.first().copied().unwrap_or_default();
            let term = positional.get(1).copied().unwrap_or_default();
            if term.is_empty() {
                String::new()
            } else if language_name(language).is_some() && name.starts_with("cog") {
                format!("{} {term}", language_name(language).unwrap())
            } else {
                term.to_string()
            }
        }
        // Compounds: {{af|en|bene|-volent}} → "bene + -volent"
        "af" | "affix" | "com" | "compound" | "suf" | "suffix" | "pre" | "prefix" | "blend" => {
            let terms: Vec<&str> = positional.iter().skip(1).copied().collect();
            terms.join(" + ")
        }
        // Glosses and plain text passthroughs.
        "gloss" | "gl" | "q" | "qualifier" | "n" | "non-gloss definition" | "ngd" => {
            positional.first().copied().unwrap_or_default().to_string()
        }
        // Everything else — {{root}}, {{rfe}}, {{PIE root}}, categorization,
        // reference templates — carries no readable etymology.
        _ => String::new(),
    }
}

/// Split a template body on `|`, ignoring separators nested in `{{}}`/`[[]]`.
fn split_top_level(body: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let bytes = body.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' | b'[' if i + 1 < bytes.len() && bytes[i + 1] == bytes[i] => {
                depth += 1;
                i += 2;
                continue;
            }
            b'}' | b']' if i + 1 < bytes.len() && bytes[i + 1] == bytes[i] => {
                depth -= 1;
                i += 2;
                continue;
            }
            b'|' if depth == 0 => {
                parts.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(&body[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    const BENEVOLENT: &str = "==English==\n\n===Etymology===\n{{root|en|ine-pro|*welh₁-}}\nFrom \
{{uder|en|fro|benevolent}}, borrowed from {{bor|en|la|benevolēns}} (\"benevolent\"). Displaced \
native {{ncog|ang|welwillende}} (literally {{l|en|well}}-{{l|en|wishing}}).\n\n\
===Pronunciation===\n* {{IPA|en|/bəˈnɛvələnt/}}\n\n===Adjective===\n{{en-adj}}\n\n# Having a \
[[disposition]] to do [[good]].\n";

    #[test]
    fn extracts_a_real_page() {
        let text = extract_etymology(BENEVOLENT).expect("etymology");
        assert!(text.starts_with("From Old French benevolent"), "{text}");
        assert!(text.contains("Latin benevolēns"), "{text}");
        assert!(!text.contains("{{"), "no template markup survives: {text}");
        assert!(!text.contains("[["), "no link markup survives: {text}");
    }

    #[test]
    fn stops_at_the_next_subsection() {
        let text = extract_etymology(BENEVOLENT).unwrap();
        assert!(!text.contains("Pronunciation"));
        assert!(!text.contains("IPA"));
    }

    #[test]
    fn stops_at_the_next_language() {
        let page = "==English==\n\n===Etymology===\nFrom {{bor|en|la|serenus}}.\n\n\
==Spanish==\n\n===Etymology===\nDel latín.\n";
        let text = extract_etymology(page).unwrap();
        assert!(text.contains("Latin serenus"));
        assert!(!text.contains("Del latín"));
    }

    #[test]
    fn handles_numbered_etymology_sections() {
        let page = "==English==\n\n===Etymology 1===\nFrom {{bor|en|la|frangere}}.\n\n\
===Etymology 2===\nUnrelated.\n";
        let text = extract_etymology(page).unwrap();
        assert!(text.contains("Latin frangere"));
    }

    #[test]
    fn a_page_without_an_english_section_yields_nothing() {
        assert!(extract_etymology("==Spanish==\n\n===Etimología===\nDel latín.\n").is_none());
    }

    #[test]
    fn a_page_without_an_etymology_yields_nothing() {
        assert!(extract_etymology("==English==\n\n===Noun===\n# A thing.\n").is_none());
    }

    #[test]
    fn an_etymology_of_only_templates_yields_nothing_rather_than_punctuation() {
        assert!(extract_etymology("==English==\n\n===Etymology===\n{{rfe|en}}\n").is_none());
    }

    #[test]
    fn renders_affix_templates_as_compounds() {
        assert_eq!(render_wikitext("{{af|en|bene|-volent}}"), "bene + -volent");
        assert_eq!(render_wikitext("{{suffix|en|kind|ly}}"), "kind + ly");
    }

    #[test]
    fn renders_links_with_and_without_labels() {
        assert_eq!(render_wikitext("[[disposition]]"), "disposition");
        assert_eq!(render_wikitext("[[good|goodness]]"), "goodness");
    }

    #[test]
    fn strips_bold_and_italic_markers() {
        assert_eq!(
            render_wikitext("''serene'' and '''calm'''"),
            "serene and calm"
        );
    }

    #[test]
    fn drops_html_comments() {
        assert_eq!(
            render_wikitext("From Latin<!-- check this -->."),
            "From Latin."
        );
    }

    #[test]
    fn unknown_language_codes_are_printed_verbatim() {
        assert_eq!(render_wikitext("{{bor|en|xyz|foo}}"), "xyz foo");
    }

    #[test]
    fn nested_templates_do_not_leak_braces() {
        let rendered = render_wikitext("From {{bor|en|la|{{l|la|serenus}}}} today.");
        assert!(!rendered.contains('{'), "{rendered}");
        assert!(!rendered.contains('}'), "{rendered}");
    }

    #[test]
    fn unbalanced_markup_does_not_hang_or_leak() {
        let rendered = render_wikitext("From {{bor|en|la|serenus");
        assert!(!rendered.contains('{'));
    }

    #[test]
    fn query_encoding_handles_spaces_and_punctuation() {
        assert_eq!(encode_query("benevolent"), "benevolent");
        assert_eq!(encode_query("ad hoc"), "ad_hoc");
        assert_eq!(encode_query("don't"), "don%27t");
    }

    #[test]
    fn split_top_level_ignores_nested_separators() {
        assert_eq!(
            split_top_level("bor|en|la|{{l|la|x}}"),
            vec!["bor", "en", "la", "{{l|la|x}}"]
        );
    }
}
