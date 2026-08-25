//! Change bus payloads.
//!
//! The writer task publishes the entity keys touched by a transaction *after*
//! it commits. Subscribers (reconciler, SSE `/api/stream`) treat these purely
//! as latency reduction — the periodic full pass is what guarantees
//! correctness (README Part 4).

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::types::ParseEnumError;

macro_rules! entity_enum {
    ({ $($variant:ident => $text:literal),+ $(,)? }) => {
        /// Coarse entity class of a touched row.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum EntityType {
            $($variant),+
        }

        impl EntityType {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
        }

        impl fmt::Display for EntityType {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for EntityType {
            type Err = ParseEnumError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($text => Ok(Self::$variant),)+
                    other => Err(ParseEnumError { kind: "entity_type", value: other.to_string() }),
                }
            }
        }
    };
}

entity_enum!({
    Word => "word",
    DefinitionCandidate => "definition_candidate",
    DefinitionSelection => "definition_selection",
    ExampleCandidate => "example_candidate",
    ExampleSelection => "example_selection",
    ImageCandidate => "image_candidate",
    ImageSelection => "image_selection",
    DefExtraction => "def_extraction",
    OosQueue => "oos_queue",
    TtsAsset => "tts_asset",
    MediaFile => "media_file",
    JobState => "job_state",
    SourceFetch => "source_fetch",
    Plan => "plan",
    Distractor => "distractor",
    Release => "release",
});

/// One published change: a set of ids of a single entity type.
///
/// Shape matches `docs/contracts/admin-api.md` §`GET /stream`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEvent {
    pub entity_type: EntityType,
    pub entity_ids: Vec<String>,
}

/// Accumulates touched keys during a transaction; the writer converts it into
/// [`ChangeEvent`]s once the commit succeeds.
#[derive(Debug, Default, Clone)]
pub struct ChangeSet {
    touched: BTreeMap<EntityType, Vec<String>>,
}

impl ChangeSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn touch(&mut self, entity_type: EntityType, id: impl Into<String>) {
        let id = id.into();
        let bucket = self.touched.entry(entity_type).or_default();
        if !bucket.contains(&id) {
            bucket.push(id);
        }
    }

    pub fn touch_many<I, S>(&mut self, entity_type: EntityType, ids: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for id in ids {
            self.touch(entity_type, id);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.touched.is_empty()
    }

    pub fn into_events(self) -> Vec<ChangeEvent> {
        self.touched
            .into_iter()
            .map(|(entity_type, entity_ids)| ChangeEvent {
                entity_type,
                entity_ids,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupes_and_groups_by_type() {
        let mut cs = ChangeSet::new();
        cs.touch(EntityType::Word, "1");
        cs.touch(EntityType::Word, "1");
        cs.touch(EntityType::Word, "2");
        cs.touch(EntityType::DefinitionCandidate, "9");
        let events = cs.into_events();
        assert_eq!(events.len(), 2);
        let words = events
            .iter()
            .find(|e| e.entity_type == EntityType::Word)
            .unwrap();
        assert_eq!(words.entity_ids, vec!["1", "2"]);
    }

    #[test]
    fn entity_type_strings_round_trip() {
        for t in EntityType::ALL {
            assert_eq!(EntityType::from_str(t.as_str()).unwrap(), *t);
        }
    }
}
