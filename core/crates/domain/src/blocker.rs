//! Readiness blocker vocabulary.
//!
//! Normative source: the `BlockerCode` union in `admin-ui/src/api/types.ts`
//! (admin-api.md wave-2 ruling #4). Core emits exactly these codes and nothing
//! else, so the admin console can render every one of them.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::types::ParseEnumError;

/// One materialized reason a word is not ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockerCode {
    /// No sense carries `is_primary`.
    MissingPrimarySense,
    /// Some enabled sense is selected but not approved.
    SenseNotApproved,
    /// The word has no enabled definition selection at all.
    MissingDefinition,
    /// A selected definition contains an unresolved out-of-scope lemma.
    OosPending,
    /// A dependency of a selected definition is not covered by base words or by
    /// an earlier position in the current plan.
    DependencyNotReady,
    /// Example slot 1 (the mode-1 sentence) is empty.
    MissingExample,
    /// A selected example slot is not approved.
    ExampleNotApproved,
    /// No image selection.
    MissingImage,
    /// The selected image is not approved.
    ImageNotApproved,
    /// Some text this word needs spoken has no TTS asset for the current voice.
    TtsMissing,
    /// Some text this word needs spoken has a `failed` TTS asset.
    TtsFailed,
    /// Fewer than three distractors bound.
    DistractorsUnbound,
    // `rename_all = "snake_case"` does not put a separator before a digit, so
    // the numbered codes spell themselves out.
    /// Distractor 1 is not `core_ready`.
    #[serde(rename = "distractor_1_not_ready")]
    Distractor1NotReady,
    /// Distractor 2 is not `core_ready`.
    #[serde(rename = "distractor_2_not_ready")]
    Distractor2NotReady,
    /// Distractor 3 is not `core_ready`.
    #[serde(rename = "distractor_3_not_ready")]
    Distractor3NotReady,
    /// The word has no position in the current plan.
    NotInPlan,
}

impl BlockerCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingPrimarySense => "missing_primary_sense",
            Self::SenseNotApproved => "sense_not_approved",
            Self::MissingDefinition => "missing_definition",
            Self::OosPending => "oos_pending",
            Self::DependencyNotReady => "dependency_not_ready",
            Self::MissingExample => "missing_example",
            Self::ExampleNotApproved => "example_not_approved",
            Self::MissingImage => "missing_image",
            Self::ImageNotApproved => "image_not_approved",
            Self::TtsMissing => "tts_missing",
            Self::TtsFailed => "tts_failed",
            Self::DistractorsUnbound => "distractors_unbound",
            Self::Distractor1NotReady => "distractor_1_not_ready",
            Self::Distractor2NotReady => "distractor_2_not_ready",
            Self::Distractor3NotReady => "distractor_3_not_ready",
            Self::NotInPlan => "not_in_plan",
        }
    }

    /// Every code, in the order they are reported.
    pub const ALL: &'static [Self] = &[
        Self::MissingDefinition,
        Self::MissingPrimarySense,
        Self::SenseNotApproved,
        Self::OosPending,
        Self::DependencyNotReady,
        Self::MissingExample,
        Self::ExampleNotApproved,
        Self::MissingImage,
        Self::ImageNotApproved,
        Self::TtsMissing,
        Self::TtsFailed,
        Self::NotInPlan,
        Self::DistractorsUnbound,
        Self::Distractor1NotReady,
        Self::Distractor2NotReady,
        Self::Distractor3NotReady,
    ];

    /// Blockers that participate in `core_ready`.
    ///
    /// README Part 3 splits readiness so the distractor recursion is capped at
    /// depth 1: `core_ready` deliberately ignores everything about distractors,
    /// which is what keeps mutual distractors (adapt ↔ adopt) from deadlocking.
    pub const fn is_core(self) -> bool {
        !matches!(
            self,
            Self::DistractorsUnbound
                | Self::Distractor1NotReady
                | Self::Distractor2NotReady
                | Self::Distractor3NotReady
        )
    }

    /// `distractor_{rank}_not_ready` for rank 1..3.
    pub const fn distractor_not_ready(rank: i64) -> Option<Self> {
        match rank {
            1 => Some(Self::Distractor1NotReady),
            2 => Some(Self::Distractor2NotReady),
            3 => Some(Self::Distractor3NotReady),
            _ => None,
        }
    }
}

impl fmt::Display for BlockerCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BlockerCode {
    type Err = ParseEnumError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|code| code.as_str() == s)
            .ok_or_else(|| ParseEnumError {
                kind: "blocker_code",
                value: s.to_string(),
            })
    }
}

/// An ordered, de-duplicated blocker list ready for `words.blockers`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockerSet {
    codes: Vec<BlockerCode>,
}

impl BlockerSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, code: BlockerCode) {
        if !self.codes.contains(&code) {
            self.codes.push(code);
        }
    }

    pub fn insert_if(&mut self, condition: bool, code: BlockerCode) {
        if condition {
            self.insert(code);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }

    pub fn contains(&self, code: BlockerCode) -> bool {
        self.codes.contains(&code)
    }

    /// True when nothing outside the distractor recursion is blocking.
    pub fn core_ready(&self) -> bool {
        self.codes.iter().all(|code| !code.is_core())
    }

    pub fn ready(&self) -> bool {
        self.codes.is_empty()
    }

    /// Reporting order is [`BlockerCode::ALL`], not insertion order, so the
    /// serialized column is a deterministic function of the state.
    pub fn sorted(&self) -> Vec<BlockerCode> {
        BlockerCode::ALL
            .iter()
            .copied()
            .filter(|code| self.codes.contains(code))
            .collect()
    }

    pub fn to_strings(&self) -> Vec<String> {
        self.sorted()
            .into_iter()
            .map(|code| code.as_str().to_string())
            .collect()
    }

    /// JSON array written to `words.blockers`.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.to_strings()).unwrap_or_else(|_| "[]".to_string())
    }
}

impl FromIterator<BlockerCode> for BlockerSet {
    fn from_iter<I: IntoIterator<Item = BlockerCode>>(iter: I) -> Self {
        let mut set = Self::new();
        for code in iter {
            set.insert(code);
        }
        set
    }
}

/// Parse a stored `words.blockers` column.
pub fn parse_blockers(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_round_trip() {
        for code in BlockerCode::ALL {
            assert_eq!(BlockerCode::from_str(code.as_str()).unwrap(), *code);
        }
        assert!(BlockerCode::from_str("not_a_blocker").is_err());
    }

    #[test]
    fn vocabulary_matches_types_ts() {
        // Verbatim from admin-ui/src/api/types.ts `BlockerCode`.
        let expected = [
            "missing_primary_sense",
            "sense_not_approved",
            "missing_definition",
            "oos_pending",
            "dependency_not_ready",
            "missing_example",
            "example_not_approved",
            "missing_image",
            "image_not_approved",
            "tts_missing",
            "tts_failed",
            "distractors_unbound",
            "distractor_1_not_ready",
            "distractor_2_not_ready",
            "distractor_3_not_ready",
            "not_in_plan",
        ];
        let mut ours: Vec<&str> = BlockerCode::ALL.iter().map(|c| c.as_str()).collect();
        let mut theirs: Vec<&str> = expected.to_vec();
        ours.sort_unstable();
        theirs.sort_unstable();
        assert_eq!(ours, theirs);
    }

    #[test]
    fn serde_spelling_is_snake_case() {
        assert_eq!(
            serde_json::to_string(&BlockerCode::Distractor2NotReady).unwrap(),
            "\"distractor_2_not_ready\""
        );
    }

    #[test]
    fn core_ready_ignores_distractor_codes() {
        let mut set = BlockerSet::new();
        set.insert(BlockerCode::Distractor1NotReady);
        set.insert(BlockerCode::DistractorsUnbound);
        assert!(set.core_ready());
        assert!(!set.ready());
        set.insert(BlockerCode::MissingImage);
        assert!(!set.core_ready());
    }

    #[test]
    fn serialization_order_is_canonical_not_insertion_order() {
        let mut a = BlockerSet::new();
        a.insert(BlockerCode::TtsMissing);
        a.insert(BlockerCode::MissingImage);
        let mut b = BlockerSet::new();
        b.insert(BlockerCode::MissingImage);
        b.insert(BlockerCode::TtsMissing);
        assert_eq!(a.to_json(), b.to_json());
        assert_eq!(a.to_json(), r#"["missing_image","tts_missing"]"#);
    }

    #[test]
    fn duplicate_inserts_collapse() {
        let mut set = BlockerSet::new();
        set.insert(BlockerCode::OosPending);
        set.insert(BlockerCode::OosPending);
        assert_eq!(set.to_strings().len(), 1);
    }

    #[test]
    fn empty_set_is_ready() {
        let set = BlockerSet::new();
        assert!(set.ready());
        assert!(set.core_ready());
        assert_eq!(set.to_json(), "[]");
    }

    #[test]
    fn distractor_rank_mapping() {
        assert_eq!(
            BlockerCode::distractor_not_ready(2),
            Some(BlockerCode::Distractor2NotReady)
        );
        assert_eq!(BlockerCode::distractor_not_ready(4), None);
    }

    #[test]
    fn parses_stored_column() {
        assert_eq!(
            parse_blockers(r#"["missing_image"]"#),
            vec!["missing_image"]
        );
        assert!(parse_blockers("not json").is_empty());
    }
}
