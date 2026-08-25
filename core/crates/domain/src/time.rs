//! UTC ISO-8601 timestamps, byte-identical to SQLite's
//! `strftime('%Y-%m-%dT%H:%M:%fZ','now')` used by the schema defaults.

use chrono::{DateTime, NaiveDateTime, SecondsFormat, TimeZone, Utc};

/// Format used by every timestamp column in the working database.
pub fn format_ts(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Current time in contract format.
pub fn now_ts() -> String {
    format_ts(Utc::now())
}

/// Parse a timestamp written by either morphod or SQLite's `strftime`.
///
/// SQLite defaults produce `2026-08-26T01:02:03.456Z`; be lenient about a
/// missing fractional part or a missing `Z` so hand-edited rows still load.
pub fn parse_ts(raw: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, fmt) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_matches_sqlite_strftime_shape() {
        let ts = Utc.with_ymd_and_hms(2026, 8, 26, 1, 2, 3).unwrap()
            + chrono::Duration::milliseconds(456);
        assert_eq!(format_ts(ts), "2026-08-26T01:02:03.456Z");
    }

    #[test]
    fn round_trips() {
        let raw = now_ts();
        let parsed = parse_ts(&raw).expect("parse");
        assert_eq!(format_ts(parsed), raw);
    }

    #[test]
    fn parses_lenient_variants() {
        assert!(parse_ts("2026-08-26T01:02:03Z").is_some());
        assert!(parse_ts("2026-08-26T01:02:03.456").is_some());
        assert!(parse_ts("2026-08-26 01:02:03").is_some());
        assert!(parse_ts("nonsense").is_none());
    }
}
