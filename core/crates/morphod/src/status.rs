//! `morphod status`: a one-shot report of what is in the working database, and
//! which external sources this build can actually reach.

use anyhow::Result;

use morpho_domain::tts::TtsConfig;
use morpho_reconcile::{probe_adapters, AdapterConfig, SourcesConfig};
use morpho_store::queries::{
    abandoned_tts_inputs, asset_counts, current_plan, dead_letter_count, oos_open_count,
    tts_assets, tts_desired, tts_given_up, word_counts, AssetCounts, AssetRollup, PlanSummary,
    WordCounts,
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
///
/// Buckets by the shared ruling-#13 rule, so the CLI, the dashboard and each
/// word's detail view all count the same clip the same way.
fn tts_rollup(
    conn: &rusqlite::Connection,
    config: &TtsConfig,
) -> morpho_store::Result<AssetRollup> {
    let assets = tts_assets(conn)?;
    let abandoned = abandoned_tts_inputs(conn)?;
    let mut out = AssetRollup::default();
    let mut seen = std::collections::HashSet::new();
    for (kind, text) in tts_desired(conn)? {
        let hash = config.input_hash(kind, &text);
        if !seen.insert(hash.clone()) {
            continue;
        }
        if assets.get(&hash).is_some_and(|a| a.status == "ready") {
            out.ready += 1;
        } else if tts_given_up(&hash, &assets, &abandoned) {
            out.failed += 1;
        } else {
            out.missing += 1;
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
    pub fn render(
        &self,
        db_path: &str,
        sources: &SourcesConfig,
        adapters: &AdapterConfig,
    ) -> String {
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

        // Ruling #17: the probe is part of the report, and an unavailable
        // adapter names the jobs it takes with it.
        out.push_str(&format!("adapters        {}\n", adapters.root().display()));
        for probe in probe_adapters(adapters) {
            out.push_str(&format!("  {:<14}{}\n", probe.adapter, probe.state()));
            if !probe.available() {
                out.push_str(&format!(
                    "                will dead-letter: {}\n",
                    probe.dead_letters
                ));
            }
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

    fn render(adapters: &AdapterConfig) -> String {
        empty_report().render("db", &SourcesConfig::default(), adapters)
    }

    /// A tree that looks like the repository: `adapters/<name>/pyproject.toml`.
    fn adapters_tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (adapter, _) in morpho_reconcile::ADAPTERS {
            let project = dir.path().join("adapters").join(adapter);
            std::fs::create_dir_all(&project).unwrap();
            std::fs::write(project.join("pyproject.toml"), b"[project]\n").unwrap();
        }
        dir
    }

    #[test]
    fn the_report_names_every_section() {
        let text = empty_report().render(
            "data/working.db",
            &SourcesConfig::default(),
            &AdapterConfig::default(),
        );
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
            "adapters",
        ] {
            assert!(text.contains(expected), "missing {expected} in:\n{text}");
        }
    }

    #[test]
    fn unconfigured_sources_are_reported_as_disabled() {
        let text = render(&AdapterConfig::default());
        assert!(text.contains("wordnet"));
        assert!(text.contains("disabled"), "{text}");
    }

    #[test]
    fn the_blocker_histogram_is_rendered() {
        let text = render(&AdapterConfig::default());
        assert!(text.contains("25  missing_image"), "{text}");
    }

    /// Ruling #17: every adapter gets its own line, rooted at `adapters_root`.
    #[test]
    fn status_reports_each_adapter_against_the_configured_root() {
        let dir = adapters_tree();
        let config = AdapterConfig {
            adapters_root: Some(dir.path().to_path_buf()),
            // `sh` always exists, so the launcher half of the probe passes and
            // the test is really about the project half.
            command: vec!["sh".into(), "--project".into(), "adapters/{adapter}".into()],
            ..AdapterConfig::default()
        };
        let text = render(&config);
        assert!(text.contains(&dir.path().display().to_string()), "{text}");
        for (adapter, _) in morpho_reconcile::ADAPTERS {
            assert!(text.contains(adapter), "missing {adapter} in:\n{text}");
        }
        assert!(!text.contains("UNAVAILABLE"), "{text}");
        assert!(!text.contains("will dead-letter"), "{text}");
    }

    /// A missing adapter names exactly what it takes down.
    #[test]
    fn a_missing_adapter_is_reported_with_its_damage() {
        let dir = tempfile::tempdir().unwrap();
        let config = AdapterConfig {
            adapters_root: Some(dir.path().to_path_buf()),
            command: vec!["sh".into(), "--project".into(), "adapters/{adapter}".into()],
            ..AdapterConfig::default()
        };
        let text = render(&config);
        assert!(text.contains("UNAVAILABLE"), "{text}");
        assert!(text.contains("no pyproject.toml"), "{text}");
        assert!(text.contains("synth_tts"), "{text}");
        assert!(text.contains("segment_morphology"), "{text}");
        assert!(text.contains("gen_image_sdxl"), "{text}");
    }
}
