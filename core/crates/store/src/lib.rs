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
pub mod ops;
pub mod queries;
pub mod read;
pub mod schema;
mod store;
mod writer;

pub use error::{Result, StoreError};
pub use ops::{
    CreateWord, ImportStats, ImportWords, MintDefinitionCandidate, OovResolution,
    RecordDefExtraction, SetApproval, SetSelection, UpsertJobState, WriteOp, WriteResult,
};
pub use read::{ReadPool, DEFAULT_READ_POOL_SIZE};
pub use schema::{ensure_schema, WORKING_DB_SQL};
pub use store::{Store, StoreConfig};
pub use writer::WriteOutcome;
