//! Typed mirrors of the working-database enumerations.
//!
//! Every enum here serializes to exactly the string the SQL `CHECK` constraints
//! accept, so a value that round-trips through the type system cannot violate
//! the schema.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Error returned when a database/API string does not match a known variant.
#[derive(Debug, Clone, thiserror::Error)]
#[error("invalid {kind} value: {value:?}")]
pub struct ParseEnumError {
    pub kind: &'static str,
    pub value: String,
}

impl ParseEnumError {
    fn new(kind: &'static str, value: &str) -> Self {
        Self {
            kind,
            value: value.to_string(),
        }
    }
}

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident, $kind:literal, { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            /// Database / API string form.
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }

            /// Every variant, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = ParseEnumError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($text => Ok(Self::$variant),)+
                    other => Err(ParseEnumError::new($kind, other)),
                }
            }
        }
    };
}

string_enum!(
    /// `words.role`
    Role, "role", {
        Target => "target",
        Base => "base",
        Auxiliary => "auxiliary",
    }
);

string_enum!(
    /// `words.aux_status`
    AuxStatus, "aux_status", {
        Active => "active",
        Retired => "retired",
    }
);

string_enum!(
    /// `words.created_by`
    CreatedBy, "created_by", {
        Import => "import",
        Promotion => "promotion",
        Manual => "manual",
    }
);

string_enum!(
    /// `*_selections.selected_by`
    SelectedBy, "selected_by", {
        Auto => "auto",
        Human => "human",
    }
);

string_enum!(
    /// `*_candidates.status`
    CandidateStatus, "candidate_status", {
        Available => "available",
        Rejected => "rejected",
    }
);

string_enum!(
    /// Which candidate/selection family an operation addresses.
    CandidateKind, "candidate_kind", {
        Definition => "definition",
        Example => "example",
        Image => "image",
    }
);

string_enum!(
    /// `definition_candidates.source`
    DefinitionSource, "definition_source", {
        Freedict => "freedict",
        Wordnet => "wordnet",
        LlmRewrite => "llm_rewrite",
        Manual => "manual",
    }
);

string_enum!(
    /// `example_candidates.source`
    ExampleSource, "example_source", {
        ExamCorpus => "exam_corpus",
        Llm => "llm",
        Manual => "manual",
    }
);

string_enum!(
    /// `image_candidates.source`
    ImageSource, "image_source", {
        Unsplash => "unsplash",
        Pexels => "pexels",
        Pixabay => "pixabay",
        Sdxl => "sdxl",
        Manual => "manual",
    }
);

string_enum!(
    /// `media_files.kind`
    MediaKind, "media_kind", {
        Image => "image",
        Audio => "audio",
    }
);

string_enum!(
    /// `tts_assets.kind`
    TtsKind, "tts_kind", {
        Word => "word",
        Definition => "definition",
        Example => "example",
    }
);

string_enum!(
    /// `oos_queue.status`
    OosStatus, "oos_status", {
        Open => "open",
        ResolvedRewrite => "resolved_rewrite",
        ResolvedPromote => "resolved_promote",
        AutoClosed => "auto_closed",
    }
);

impl MediaKind {
    /// File extension used by the content-addressed store.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Image => "webp",
            Self::Audio => "ogg",
        }
    }

    /// MIME type served by `GET /api/media/{hash}`.
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Image => "image/webp",
            Self::Audio => "audio/ogg",
        }
    }
}

/// Addresses one selection slot. Definitions are keyed per part of speech,
/// examples per slot 1..3, images have exactly one slot per word.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SlotRef {
    Definition { word_id: i64, pos: String },
    Example { word_id: i64, slot: i64 },
    Image { word_id: i64 },
}

impl SlotRef {
    pub fn word_id(&self) -> i64 {
        match self {
            Self::Definition { word_id, .. }
            | Self::Example { word_id, .. }
            | Self::Image { word_id } => *word_id,
        }
    }

    pub fn candidate_kind(&self) -> CandidateKind {
        match self {
            Self::Definition { .. } => CandidateKind::Definition,
            Self::Example { .. } => CandidateKind::Example,
            Self::Image { .. } => CandidateKind::Image,
        }
    }

    /// Stable identifier used in `events.entity_id`.
    pub fn entity_id(&self) -> String {
        match self {
            Self::Definition { word_id, pos } => format!("{word_id}:{pos}"),
            Self::Example { word_id, slot } => format!("{word_id}:{slot}"),
            Self::Image { word_id } => word_id.to_string(),
        }
    }
}

impl fmt::Display for SlotRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.candidate_kind(), self.entity_id())
    }
}

/// A word row as morphod reads it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub word_id: i64,
    pub lemma: String,
    pub role: Role,
    pub aux_status: Option<AuxStatus>,
    pub phonetic: Option<String>,
    pub frequency_rank: Option<i64>,
    pub etymology: Option<String>,
    pub etymology_source: Option<String>,
    pub ready: bool,
    pub blockers: Vec<String>,
    pub created_by: CreatedBy,
    pub created_at: String,
}

impl Word {
    /// Mirrors the `active_words` view.
    pub fn is_active(&self) -> bool {
        match self.role {
            Role::Target => true,
            Role::Auxiliary => self.aux_status == Some(AuxStatus::Active),
            Role::Base => false,
        }
    }
}

/// One row of a word list import.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WordImport {
    #[serde(alias = "lemma")]
    pub word: String,
    #[serde(default)]
    pub phonetic: Option<String>,
    #[serde(default)]
    pub frequency_rank: Option<i64>,
}

/// A definition candidate row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionCandidate {
    pub def_cand_id: i64,
    pub word_id: i64,
    pub pos: String,
    pub text: String,
    pub text_hash: String,
    pub source: DefinitionSource,
    pub source_ref: Option<String>,
    pub parent_cand_id: Option<i64>,
    pub status: CandidateStatus,
    pub auto_score: Option<f64>,
    pub scorer_ver: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

/// A definition selection row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionSelection {
    pub word_id: i64,
    pub pos: String,
    pub def_cand_id: i64,
    pub is_primary: bool,
    pub enabled: bool,
    pub selected_by: SelectedBy,
    pub pinned: bool,
    pub approved: bool,
    pub approved_hash: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
    pub selection_rev: i64,
    pub updated_at: String,
}

/// One token of a cached definition extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedToken {
    pub position: i64,
    pub surface: String,
    pub lemma: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_strings() {
        for role in Role::ALL {
            assert_eq!(Role::from_str(role.as_str()).unwrap(), *role);
        }
        for src in DefinitionSource::ALL {
            assert_eq!(DefinitionSource::from_str(src.as_str()).unwrap(), *src);
        }
        assert!(Role::from_str("nonsense").is_err());
    }

    #[test]
    fn serde_uses_the_contract_spelling() {
        let json = serde_json::to_string(&DefinitionSource::LlmRewrite).unwrap();
        assert_eq!(json, "\"llm_rewrite\"");
        let back: DefinitionSource = serde_json::from_str("\"llm_rewrite\"").unwrap();
        assert_eq!(back, DefinitionSource::LlmRewrite);
    }

    #[test]
    fn active_words_view_semantics() {
        let mut w = Word {
            word_id: 1,
            lemma: "abandon".into(),
            role: Role::Target,
            aux_status: None,
            phonetic: None,
            frequency_rank: None,
            etymology: None,
            etymology_source: None,
            ready: false,
            blockers: vec![],
            created_by: CreatedBy::Import,
            created_at: String::new(),
        };
        assert!(w.is_active());
        w.role = Role::Base;
        assert!(!w.is_active());
        w.role = Role::Auxiliary;
        w.aux_status = Some(AuxStatus::Retired);
        assert!(!w.is_active());
        w.aux_status = Some(AuxStatus::Active);
        assert!(w.is_active());
    }

    #[test]
    fn slot_ref_entity_ids_are_distinct() {
        let d = SlotRef::Definition {
            word_id: 7,
            pos: "noun".into(),
        };
        let e = SlotRef::Example {
            word_id: 7,
            slot: 1,
        };
        let i = SlotRef::Image { word_id: 7 };
        assert_eq!(d.entity_id(), "7:noun");
        assert_eq!(e.entity_id(), "7:1");
        assert_eq!(i.entity_id(), "7");
        assert_ne!(d.to_string(), e.to_string());
    }
}
