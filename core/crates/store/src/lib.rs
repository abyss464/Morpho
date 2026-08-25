//! The morphod store layer.
//!
//! Discipline (README Part 4, conventions §Rust):
//!   * exactly one writer task owns the sole read/write connection;
//!   * all mutations are typed [`WriteOp`]s sent over an mpsc channel with a
//!     oneshot acknowledgement, one transaction each;
//!   * reads come from a pool of read-only connections;
//!   * every commit publishes the touched entity keys on a broadcast bus.
//!
//! No other code path in the process opens the database for writing.

pub mod conn;
pub mod error;
pub mod media;
pub mod ops;
pub mod queries;
pub mod read;
pub mod schema;
mod store;
mod writer;

pub use error::{Result, StoreError};
pub use media::{MediaStore, StagingDir, StoredMedia};
pub use ops::{
    ApplyAutoSelections, ApplyReadiness, ApplyScores, AutoSelection, BindDistractors, CreateWord,
    DistractorBinding, ImportStats, ImportWords, IngestDefinitions, IngestExamples, IngestImages,
    MarkMediaGc, MediaRegistration, MintDefinitionCandidate, MintExampleCandidate,
    MintImageCandidate, OovResolution, PlanGroupRow, PlanWordRow, ReadinessRow,
    RecordDefExtraction, RecordRelease, RecordTtsAsset, ScoreUpdate, SetApproval, SetAuxStatus,
    SetEtymology, SetSelection, SyncOosQueue, UpsertJobState, WriteOp, WritePlan, WriteResult,
};
pub use read::{ReadPool, DEFAULT_READ_POOL_SIZE};
pub use schema::{ensure_schema, migrate, WORKING_DB_SQL};
pub use store::{Store, StoreConfig};
pub use writer::WriteOutcome;
