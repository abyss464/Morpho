//! `morphod export`: build a release bundle from the current working state.

use std::path::PathBuf;

use anyhow::{bail, Result};

use morpho_export::{ExportError, HoldbackReport};
use morpho_store::Store;

use crate::config::Config;

/// Run one export and print a human-readable summary.
pub async fn run(
    config: &Config,
    store: &Store,
    out_dir: Option<PathBuf>,
    actor: &str,
    notes: Option<String>,
    preview_only: bool,
) -> Result<()> {
    let settings = config.export_settings();

    if preview_only {
        let report = morpho_export::preview(store, &settings).await?;
        print!("{}", render_holdback(&report));
        return Ok(());
    }

    let out_dir = out_dir.unwrap_or_else(|| {
        config.releases_dir.join(format!(
            "export-{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
        ))
    });

    match morpho_export::export(store, &settings, &out_dir, actor, notes).await {
        Ok((written, report)) => {
            println!("release        {}", written.content_version);
            println!("bundle         {}", written.out_dir.display());
            println!("release.db     {}", written.db_file_hash);
            println!("input_hash     {}", written.content_hash);
            println!(
                "contents       {} words, {} media files, {} bytes",
                written.word_count,
                written.media_hashes.len(),
                written.manifest.total_bytes
            );
            print!("{}", render_holdback(&report));
            Ok(())
        }
        Err(ExportError::GatesFailed(failures)) => {
            eprintln!(
                "export refused: {} validation gate(s) failed",
                failures.len()
            );
            for failure in &failures {
                eprintln!("  [{}] {}", failure.gate, failure.message);
            }
            bail!("export validation failed")
        }
        Err(err) => Err(err.into()),
    }
}

/// The edit worklist, most blocking word first.
pub fn render_holdback(report: &HoldbackReport) -> String {
    const SHOWN: usize = 20;
    let mut out = String::new();
    out.push_str(&format!(
        "plan #{}: {} shippable, {} exportable after closure, {} held back\n",
        report.plan_id, report.shippable_count, report.exportable_count, report.excluded_count
    ));
    if !report.gates_pass {
        out.push_str(&format!(
            "validation FAILED: {} gate(s)\n",
            report.gate_failures.len()
        ));
        for failure in report.gate_failures.iter().take(SHOWN) {
            out.push_str(&format!("  [{}] {}\n", failure.gate, failure.message));
        }
    }
    if report.excluded.is_empty() {
        return out;
    }

    // Root-cause histogram first: with a fresh database every word fails the
    // same way, and a per-word list would bury that.
    let mut causes: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for entry in &report.excluded {
        *causes.entry(entry.root_cause.as_str()).or_default() += 1;
    }
    out.push_str("held back by root cause:\n");
    let mut ranked: Vec<(&str, usize)> = causes.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    for (cause, count) in ranked {
        out.push_str(&format!("  {count:>6}  {cause}\n"));
    }

    out.push_str("worst blockers (by downstream impact):\n");
    for entry in report.excluded.iter().take(SHOWN) {
        out.push_str(&format!(
            "  {:>4} blocked  {:<20} {}  {}\n",
            entry.impact_count, entry.lemma, entry.root_cause, entry.root_cause_detail
        ));
    }
    if report.excluded.len() > SHOWN {
        out.push_str(&format!(
            "  ... and {} more\n",
            report.excluded.len() - SHOWN
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use morpho_export::HoldbackEntry;

    fn entry(lemma: &str, cause: &str, impact: usize) -> HoldbackEntry {
        HoldbackEntry {
            word_id: 1,
            lemma: lemma.to_string(),
            role: "target".to_string(),
            root_cause: cause.to_string(),
            root_cause_detail: "detail".to_string(),
            blocking_word_id: None,
            blocking_lemma: None,
            impact_count: impact,
        }
    }

    #[test]
    fn an_empty_release_still_reports_its_plan() {
        let report = HoldbackReport {
            plan_id: 7,
            shippable_count: 0,
            exportable_count: 0,
            excluded_count: 0,
            excluded: Vec::new(),
            gates_pass: true,
            gate_failures: Vec::new(),
        };
        let text = render_holdback(&report);
        assert!(text.contains("plan #7"));
        assert!(text.contains("0 shippable"));
        assert!(!text.contains("root cause"));
    }

    #[test]
    fn the_histogram_groups_root_causes() {
        let report = HoldbackReport {
            plan_id: 1,
            shippable_count: 0,
            exportable_count: 0,
            excluded_count: 3,
            excluded: vec![
                entry("serene", "missing_image", 0),
                entry("tranquil", "missing_image", 0),
                entry("lucid", "missing_example", 0),
            ],
            gates_pass: true,
            gate_failures: Vec::new(),
        };
        let text = render_holdback(&report);
        assert!(text.contains("2  missing_image"), "{text}");
        assert!(text.contains("1  missing_example"), "{text}");
    }

    #[test]
    fn gate_failures_are_shouted_about() {
        let report = HoldbackReport {
            plan_id: 1,
            shippable_count: 1,
            exportable_count: 1,
            excluded_count: 0,
            excluded: Vec::new(),
            gates_pass: false,
            gate_failures: vec![morpho_export::GateFailure {
                gate: "three_distractors".into(),
                message: "serene has 2 usable distractors, expected 3".into(),
                word_id: Some(1),
                lemma: Some("serene".into()),
            }],
        };
        let text = render_holdback(&report);
        assert!(text.contains("validation FAILED"));
        assert!(text.contains("three_distractors"));
    }

    #[test]
    fn the_worst_blockers_come_first() {
        let report = HoldbackReport {
            plan_id: 1,
            shippable_count: 0,
            exportable_count: 0,
            excluded_count: 2,
            excluded: vec![
                entry("generous", "missing_image", 12),
                entry("serene", "missing_image", 1),
            ],
            gates_pass: true,
            gate_failures: Vec::new(),
        };
        let text = render_holdback(&report);
        let generous = text.find("generous").unwrap();
        let serene = text.rfind("serene").unwrap();
        assert!(generous < serene, "{text}");
    }
}
