//! Shared HTTP plumbing for the network sources.
//!
//! One `reqwest::Client` for the whole process (connection pooling matters when
//! 6 000 words each need a fetch), plus the mapping from HTTP reality onto the
//! `Permanent | Transient | RateLimited` taxonomy that drives retries
//! (README Part 4 §"任务生命周期").

use std::time::Duration;

use morpho_domain::error::TaskError;
use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER, USER_AGENT};
use reqwest::{Response, StatusCode};

use crate::config::SourcesConfig;

/// Build the process-wide HTTP client.
pub fn build_client(config: &SourcesConfig) -> reqwest::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&config.user_agent.0) {
        headers.insert(USER_AGENT, value);
    }
    reqwest::Client::builder()
        .timeout(config.http_timeout())
        .connect_timeout(Duration::from_secs(10))
        .default_headers(headers)
        .build()
}

/// Percent-encode a value for use in a query string (form style: a space
/// becomes `+`).
///
/// Search queries are lemmas and short glosses, so this only has to survive
/// spaces and the occasional apostrophe; anything outside the unreserved set is
/// escaped rather than guessed at.
pub fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Classify a transport-level failure.
pub fn transport_error(err: &reqwest::Error) -> TaskError {
    // Everything at this layer — DNS, TLS, connect, read timeout — is the kind
    // of thing that works on the next attempt.
    TaskError::transient(err.to_string())
}

/// Classify an HTTP status.
///
/// * 404 / 410 are legitimate empty results: the word simply is not in that
///   dictionary. Permanent, and the caller writes a completion marker.
/// * 401 / 403 mean the credentials are wrong. Permanent, because retrying
///   cannot fix a bad key and a dead letter is the honest signal.
/// * 429 parks the whole lane until `Retry-After`.
/// * everything else 4xx is a request bug (permanent); 5xx is transient.
pub fn classify_status(
    status: StatusCode,
    response: &Response,
    context: &str,
) -> Option<TaskError> {
    if status.is_success() {
        return None;
    }
    if status == StatusCode::NOT_FOUND || status == StatusCode::GONE {
        return Some(TaskError::permanent(format!("{context}: {status}")));
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
            // No Retry-After means "back off for a sensible while"; the
            // adapters use 60 s for the same case.
            .unwrap_or(60);
        return Some(TaskError::rate_limited(Duration::from_secs(
            retry_after.clamp(1, 3_600),
        )));
    }
    if status.is_server_error() {
        return Some(TaskError::transient(format!("{context}: {status}")));
    }
    Some(TaskError::permanent(format!("{context}: {status}")))
}

/// Fetch a URL and decode JSON, applying the taxonomy above.
pub async fn get_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&str, String)],
    context: &str,
) -> Result<T, TaskError> {
    let body = get_bytes(client, url, headers, context).await?;
    serde_json::from_slice(&body).map_err(|err| {
        // A body that does not parse is a contract break with the upstream, not
        // a blip: retrying the same request gets the same bytes.
        TaskError::permanent(format!("{context}: malformed JSON: {err}"))
    })
}

/// Fetch a URL and return the raw body.
pub async fn get_bytes(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&str, String)],
    context: &str,
) -> Result<Vec<u8>, TaskError> {
    let mut request = client.get(url);
    for (name, value) in headers {
        request = request.header(*name, value);
    }
    let response = request.send().await.map_err(|err| transport_error(&err))?;
    let status = response.status();
    if let Some(err) = classify_status(status, &response, context) {
        return Err(err);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| transport_error(&err))?;
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use morpho_domain::error::ErrorKind;

    fn response(status: StatusCode, retry_after: Option<&str>) -> Response {
        let mut builder = http::Response::builder().status(status);
        if let Some(value) = retry_after {
            builder = builder.header(RETRY_AFTER, value);
        }
        Response::from(builder.body(Vec::<u8>::new()).unwrap())
    }

    #[test]
    fn success_is_not_an_error() {
        let r = response(StatusCode::OK, None);
        assert!(classify_status(StatusCode::OK, &r, "ctx").is_none());
    }

    #[test]
    fn a_missing_entry_is_permanent() {
        for status in [StatusCode::NOT_FOUND, StatusCode::GONE] {
            let r = response(status, None);
            let err = classify_status(status, &r, "freedict benevolent").unwrap();
            assert_eq!(err.kind(), ErrorKind::Permanent);
            assert!(err.message().contains("benevolent"));
        }
    }

    #[test]
    fn bad_credentials_are_permanent() {
        for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
            let r = response(status, None);
            assert_eq!(
                classify_status(status, &r, "unsplash").unwrap().kind(),
                ErrorKind::Permanent
            );
        }
    }

    #[test]
    fn server_errors_are_transient() {
        for status in [
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
        ] {
            let r = response(status, None);
            assert_eq!(
                classify_status(status, &r, "ctx").unwrap().kind(),
                ErrorKind::Transient
            );
        }
    }

    #[test]
    fn throttling_parks_the_lane_for_the_advertised_time() {
        let r = response(StatusCode::TOO_MANY_REQUESTS, Some("90"));
        let err = classify_status(StatusCode::TOO_MANY_REQUESTS, &r, "ctx").unwrap();
        assert_eq!(err.kind(), ErrorKind::RateLimited);
        assert!(err.message().contains("90000"));
        assert!(
            !err.counts_as_attempt(),
            "a lane park is not the word's fault"
        );
    }

    #[test]
    fn throttling_without_a_header_uses_the_adapter_default() {
        let r = response(StatusCode::TOO_MANY_REQUESTS, None);
        let err = classify_status(StatusCode::TOO_MANY_REQUESTS, &r, "ctx").unwrap();
        assert!(err.message().contains("60000"));
    }

    #[test]
    fn an_absurd_retry_after_is_clamped_to_an_hour() {
        let r = response(StatusCode::TOO_MANY_REQUESTS, Some("999999"));
        let err = classify_status(StatusCode::TOO_MANY_REQUESTS, &r, "ctx").unwrap();
        assert!(err.message().contains("3600000"));
    }

    #[test]
    fn a_non_numeric_retry_after_falls_back_rather_than_failing() {
        let r = response(
            StatusCode::TOO_MANY_REQUESTS,
            Some("Wed, 21 Oct 2026 07:28:00 GMT"),
        );
        let err = classify_status(StatusCode::TOO_MANY_REQUESTS, &r, "ctx").unwrap();
        assert_eq!(err.kind(), ErrorKind::RateLimited);
    }

    #[test]
    fn the_client_carries_a_descriptive_user_agent() {
        // Wikimedia rejects anonymous requests and asks for a tool name plus a
        // way to make contact, so this is load-bearing rather than politeness.
        let config = SourcesConfig::default();
        assert!(build_client(&config).is_ok());
        assert!(
            config
                .user_agent
                .starts_with("Morpho/0.1 vocabulary content builder"),
            "{}",
            config.user_agent.0
        );
        assert!(config.user_agent.contains("https://"), "no contact URL");
    }

    #[test]
    fn query_encoding_is_form_style() {
        assert_eq!(encode_query("serene"), "serene");
        assert_eq!(encode_query("ad hoc"), "ad+hoc");
        assert_eq!(encode_query("a&b"), "a%26b");
        assert_eq!(encode_query("don't"), "don%27t");
    }
}
