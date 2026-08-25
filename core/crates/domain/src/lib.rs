//! Shared vocabulary for morphod: canonicalization, hashing, and the typed
//! domain enums that mirror `docs/contracts/working-db.sql`.
//!
//! This crate has no I/O and no database dependency on purpose — everything in
//! here is pure and unit-testable, which matters because the hashing rules are
//! the foundation of the whole staleness model (README Part 3).

pub mod canon;
pub mod change;
pub mod error;
pub mod event;
pub mod hash;
pub mod job;
pub mod time;
pub mod types;
pub mod version;

pub use canon::{canonicalize, fold_lemma};
pub use change::{ChangeEvent, ChangeSet, EntityType};
pub use error::{ErrorKind, TaskError};
pub use event::{Action, Actor, EventDraft, EventRecord};
pub use hash::{
    def_extraction_input_hash, file_hash, hash_fields, text_hash, tts_input_hash, HashInput,
};
pub use job::{
    JobKey, JobKind, JobStatus, JobView, JobsSnapshot, LaneView, Priority, RateKey, SubjectRef,
    SubjectType,
};
pub use time::{format_ts, now_ts, parse_ts};
pub use types::{
    AuxStatus, CandidateKind, CandidateStatus, CreatedBy, DefinitionCandidate, DefinitionSelection,
    DefinitionSource, ExampleSource, ExtractedToken, ImageSource, MediaKind, OosStatus,
    ParseEnumError, Role, SelectedBy, SlotRef, TtsKind, Word, WordImport,
};
pub use version::SCHEMA_USER_VERSION;
