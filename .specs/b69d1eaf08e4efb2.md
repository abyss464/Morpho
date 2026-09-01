---
file: core/crates/domain/src/lib.rs
---

# domain package entry point

A shared vocabulary package of pure types and pure functions. No I/O, no database dependencies—all content can be unit-tested directly. Hashing rules are the foundation of the entire expiration model, so they live here.

## Export groups

**Text processing**: canonicalize, fold_lemma (from canon); locate (from sentence)

**Hashing**: HashInput, text_hash, file_hash, hash_fields, def_extraction_input_hash, tts_input_hash (from hash)

**Blocking & readiness**: BlockerCode, BlockerSet, parse_blockers (from blocker)

**Change notification**: ChangeEvent, ChangeSet, EntityType (from change)

**Error categorization**: ErrorKind, TaskError (from error)

**Audit logging**: Actor, Action, EventDraft, EventRecord (from event)

**Task scheduling**: JobKind, JobKey, JobStatus, JobView, JobsSnapshot, LaneView, Priority, RateKey, SubjectRef, SubjectType (from job)

**Time**: format_ts, now_ts, parse_ts (from time)

**TTS configuration**: TtsConfig, DesiredTts (from tts)

**Database enums and data structures**: Role, AuxStatus, CreatedBy, SelectedBy, CandidateStatus, CandidateKind, DefinitionSource, ExampleSource, ImageSource, MediaKind, TtsKind, OosStatus, GlossSource, EtymologySource, Pos, ParseEnumError, SlotRef, Word, WordImport, DefinitionCandidate, DefinitionSelection, ExtractedToken, FetchedDefinition, FetchedExample, FetchedImage (from types)

**Version**: SCHEMA_USER_VERSION (from version)
