//! `morphod status`: a one-shot report of what is in the working database.

use anyhow::Result;

use morpho_store::queries::{
    asset_counts, current_plan, dead_letter_count, oos_open_count, word_counts, AssetCounts,
    PlanSummary, WordCounts,
};
use morpho_store::Store;

pub struct StatusReport {
    pub words: WordCounts,
    pub assets: AssetCounts,
    pub oos_open: i64,
    pub dead_letters: i64,
    pub plan: Option<PlanSummary>,
    pub events: i64,
}

pub async fn collect(store: &Store) -> Result<StatusReport> {
    let report = store
        .read(|conn| {
            Ok(StatusReport {
                words: word_counts(conn)?,
                assets: asset_counts(conn)?,
                oos_open: oos_open_count(conn)?,
                dead_letters: dead_letter_count(conn)?,
                plan: current_plan(conn)?,
                events: conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?,
            })
        })
        .await?;
    Ok(report)
}

impl StatusReport {
    pub fn render(&self, db_path: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("database        {db_path}\n"));
        out.push_str(&format!(
            "words           {} total ({} target, {} base, {} auxiliary / {} active)\n",
            self.words.total,
            self.words.target,
            self.words.base,
            self.words.auxiliary,
            self.words.auxiliary_active,
        ));
        out.push_str(&format!(
            "readiness       {} ready, {} blocked\n",
            self.words.ready, self.words.blocked
        ));
        out.push_str(&format!(
            "definitions     {} candidates, {} selected\n",
            self.assets.definition_candidates, self.assets.definitions
        ));
        out.push_str(&format!(
            "examples        {} candidates, {} selected\n",
            self.assets.example_candidates, self.assets.examples
        ));
        out.push_str(&format!(
            "images          {} candidates, {} selected\n",
            self.assets.image_candidates, self.assets.images
        ));
        out.push_str(&format!(
            "tts             {} ready, {} missing, {} failed\n",
            self.assets.tts_ready, self.assets.tts_missing, self.assets.tts_failed
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
        out.push_str(&format!("events          {}\n", self.events));
        out
    }
}
