//! `morphod status`: a one-shot report of what is in the working database, and
//! which external sources this build can actually reach.

use anyhow::Result;

use morpho_domain::tts::TtsConfig;
use morpho_reconcile::SourcesConfig;
use morpho_store::queries::{
    asset_counts, current_plan, dead_letter_count, oos_open_count, tts_assets, tts_desired,
    word_counts, AssetCounts, AssetRollup, PlanSummary, WordCounts,
};
use morpho_store::Store;

pub struct StatusReport {
    pub words: WordCounts,
    pub assets: AssetCounts,
    pub oos_open: i64,
    pub dead_letters: i64,
    pub plan: Option<PlanSummary>,
    pub events: i64,
    pub releases: i64,
    pub blockers: Vec<(String, i64)>,
}

pub async fn collect(store: &Store, tts: &TtsConfig) -> Result<StatusReport> {
    let tts = tts.clone();
    let report = store
        .read(move |conn| {
            let mut assets = asset_counts(conn)?;
            assets.tts = tts_rollup(conn, &tts)?;
            Ok(StatusReport {
                words: word_counts(conn)?,
                assets,
                oos_open: oos_open_count(conn)?,
                dead_letters: dead_letter_count(conn)?,
                plan: current_plan(conn)?,
                events: conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?,
                releases: conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0))?,
                blockers: blocker_histogram(conn)?,
            })
        })
        .await?;
    Ok(report)
}

/// TTS coverage of the whole desired set against the configured voice.
fn tts_rollup(
    conn: &rusqlite::Connection,
    config: &TtsConfig,
) -> morpho_store::Result<AssetRollup> {
    let assets = tts_assets(conn)?;
    let mut out = AssetRollup::default();
    let mut seen = std::collections::HashSet::new();
    for (kind, text) in tts_desired(conn)? {
        let hash = config.input_hash(kind, &text);
        if !seen.insert(hash.clone()) {
            continue;
        }
        match assets.get(&hash).map(|asset| asset.status.as_str()) {
            Some("ready") => out.ready += 1,
            Some("failed") => out.failed += 1,
            _ => out.missing += 1,
        }
    }
    Ok(out)
}

/// How many active words carry each blocker code, commonest first.
fn blocker_histogram(conn: &rusqlite::Connection) -> morpho_store::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT je.value, COUNT(*)
         FROM active_words w, json_each(w.blockers) je
         GROUP BY je.value
         ORDER BY COUNT(*) DESC, je.value",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

impl StatusReport {
    pub fn render(&self, db_path: &str, sources: &SourcesConfig) -> String {
        let rollup = |label: &str, rollup: AssetRollup| {
            format!(
                "{label:<15} {} ready, {} missing, {} exhausted\n",
                rollup.ready, rollup.missing, rollup.failed
            )
        };

        let mut out = String::new();
        out.push_str(&format!("database        {db_path}\n"));
        out.push_str(&format!(
            "words           {} total ({} target, {} base, {} active auxiliary, {} retired)\n",
            self.words.total,
            self.words.target,
            self.words.base,
            self.words.auxiliary,
            self.words.auxiliary_retired,
        ));
        out.push_str(&format!(
            "readiness       {} ready, {} core-ready, {} blocked (of {} active)\n",
            self.words.ready, self.words.core_ready, self.words.blocked, self.words.active
        ));
        out.push_str(&rollup("definitions", self.assets.definitions));
        out.push_str(&rollup("examples", self.assets.examples));
        out.push_str(&rollup("images", self.assets.images));
        out.push_str(&rollup("tts", self.assets.tts));
        out.push_str(&format!(
            "candidates      {} definitions, {} examples, {} images\n",
            self.assets.definition_candidates,
            self.assets.example_candidates,
            self.assets.image_candidates
        ));
        out.push_str(&format!("oov queue       {} open\n", self.oos_open));
        out.push_str(&format!("dead letters    {}\n", self.dead_letters));
        match &self.plan {
            Some(plan) => out.push_str(&format!(
                "plan            #{} built {} ({} words, {} groups)\n",
                plan.plan_id, plan.built_at, plan.word_count, plan.group_count
            )),
            None => out.push_str("plan            none built yet\n"),
        }
        out.push_str(&format!("releases        {}\n", self.releases));
        out.push_str(&format!("events          {}\n", self.events));

        if !self.blockers.is_empty() {
            out.push_str("blockers\n");
            for (code, count) in &self.blockers {
                out.push_str(&format!("  {count:>6}  {code}\n"));
            }
        }

        out.push_str("sources\n");
        for (name, state) in sources.describe() {
            out.push_str(&format!("  {name:<14}{state}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_report() -> StatusReport {
        StatusReport {
            words: WordCounts::default(),
            assets: AssetCounts::default(),
            oos_open: 0,
            dead_letters: 0,
            plan: None,
            events: 0,
            releases: 0,
            blockers: vec![("missing_image".to_string(), 25)],
        }
    }

    #[test]
    fn the_report_names_every_section() {
        let text = empty_report().render("data/working.db", &SourcesConfig::default());
        for expected in [
            "database",
            "words",
            "readiness",
            "definitions",
            "examples",
            "images",
            "tts",
            "oov queue",
            "dead letters",
            "plan",
            "releases",
            "blockers",
            "sources",
        ] {
            assert!(text.contains(expected), "missing {expected} in:\n{text}");
        }
    }

    #[test]
    fn unconfigured_sources_are_reported_as_disabled() {
        let text = empty_report().render("db", &SourcesConfig::default());
        assert!(text.contains("wordnet"));
        assert!(text.contains("disabled"), "{text}");
    }

    #[test]
    fn the_blocker_histogram_is_rendered() {
        let text = empty_report().render("db", &SourcesConfig::default());
        assert!(text.contains("25  missing_image"), "{text}");
    }
}
