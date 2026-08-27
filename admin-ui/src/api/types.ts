/**
 * Wire types for the morphod admin API.
 *
 * This file mirrors `docs/contracts/admin-api.md` (shapes) and
 * `docs/contracts/working-db.sql` (field names / enums) one-to-one. It is the
 * single source of truth for admin-ui; every handler in `src/mocks/` and every
 * function in `src/api/endpoints.ts` is typed from here. Nothing else in the app
 * may invent a server-facing field.
 *
 * Convention notes (see README of this module and the wave-1 summary):
 *  - SQLite stores booleans as INTEGER 0/1 and JSON blobs as TEXT. `mappers.ts`
 *    normalizes both, so the types below are already the *domain* form
 *    (`boolean`, parsed objects). `Raw*` aliases describe the tolerated wire form.
 *  - Timestamps are UTC ISO-8601 strings, exactly as the DDL specifies.
 */

/* ------------------------------------------------------------------ */
/* Envelopes                                                           */
/* ------------------------------------------------------------------ */

/** `{"error": {"code": "string", "message": "string"}}` — admin-api.md preamble. */
export interface ApiErrorEnvelope {
  error: {
    code: string;
    message: string;
    /** Optional structured payload; export gate failures ride here or top-level. */
    details?: unknown;
  };
}

/** `?page=1&page_size=50` → `{"items": [...], "total": n}` */
export interface Paginated<T> {
  items: T[];
  total: number;
}

export interface PageParams {
  page?: number;
  page_size?: number;
}

/* ------------------------------------------------------------------ */
/* Enums (verbatim from working-db.sql CHECK constraints)              */
/* ------------------------------------------------------------------ */

export type WordRole = 'target' | 'base' | 'auxiliary';
export type AuxStatus = 'active' | 'retired';
export type CreatedBy = 'import' | 'promotion' | 'manual';
export type CandidateStatus = 'available' | 'rejected';
export type SelectedBy = 'auto' | 'human';

export type DefinitionSource = 'freedict' | 'wordnet' | 'llm_rewrite' | 'manual';
export type ExampleSource = 'exam_corpus' | 'freedict' | 'tatoeba' | 'llm' | 'manual';
export type ImageSource =
  'unsplash' | 'pexels' | 'pixabay' | 'wikimedia' | 'openverse' | 'sdxl' | 'codex' | 'manual';
export type CandidateSource = DefinitionSource | ExampleSource | ImageSource;

export type EtymologySource = 'wiktionary' | 'morfessor' | 'manual';

/** working-db.sql comment: noun/verb/adj/adv/prep/conj/interj/phrase (open TEXT column). */
export type Pos = 'noun' | 'verb' | 'adj' | 'adv' | 'prep' | 'conj' | 'interj' | 'phrase';

/** example_selections.slot CHECK (slot BETWEEN 1 AND 3) */
export type ExampleSlotNumber = 1 | 2 | 3;

export type TtsKind = 'word' | 'definition' | 'example';
/**
 * DDL allows `ready | failed`. The API additionally reports `missing` for a
 * desired text that has no `tts_assets` row yet (`tts_desired` minus `tts_assets`),
 * because the admin console must distinguish "not synthesized" from "failed".
 */
export type TtsStatusCode = 'ready' | 'failed' | 'missing';

export type OovStatus = 'open' | 'resolved_rewrite' | 'resolved_promote' | 'auto_closed';
export type JobStatus = 'backoff' | 'dead' | 'waived';
export type JobSubjectType = 'word' | 'def_candidate' | 'tts_input' | 'global';
export type PlanGroupType = 'scc' | 'root' | 'semantic' | 'fill';

/** Kind discriminator used by `/candidates/{kind}` and `/selections/{kind}`. */
export type AssetKind = 'definition' | 'example' | 'image';

/**
 * Readiness blocker codes materialized into `words.blockers` (README Part 3,
 * "未就绪原因物化为 blocker 列表"). The union is open-ended — core may add codes,
 * so unknown strings stay renderable.
 */
export type BlockerCode =
  | 'missing_primary_sense'
  | 'sense_not_approved'
  | 'missing_definition'
  | 'oos_pending'
  | 'dependency_not_ready'
  | 'missing_example'
  | 'example_not_approved'
  | 'missing_image'
  | 'image_not_approved'
  | 'tts_missing'
  | 'tts_failed'
  | 'distractors_unbound'
  | 'distractor_1_not_ready'
  | 'distractor_2_not_ready'
  | 'distractor_3_not_ready'
  | 'not_in_plan'
  | (string & {});

/* ------------------------------------------------------------------ */
/* Events                                                              */
/* ------------------------------------------------------------------ */

/**
 * `events` row. `detail` is a TEXT column holding a JSON before/after snapshot;
 * the mapper accepts either the raw string or an already-parsed object.
 */
export interface AdminEvent {
  event_id: number;
  ts: string;
  /** `reconciler | worker:<kind> | admin:<user>` */
  actor: string;
  entity_type: string;
  entity_id: string;
  action: string;
  detail: Record<string, unknown> | null;
}

export interface EventsQuery extends PageParams {
  entity_type?: string;
  entity_id?: string;
}

/* ------------------------------------------------------------------ */
/* GET /dashboard                                                      */
/* ------------------------------------------------------------------ */

export interface AssetRollup {
  ready: number;
  missing: number;
  failed: number;
}

export interface DashboardWordStats {
  total: number;
  target: number;
  auxiliary: number;
  ready: number;
  blocked: number;
}

export interface DashboardAssets {
  definitions: AssetRollup;
  examples: AssetRollup;
  images: AssetRollup;
  tts: AssetRollup;
}

export interface DashboardPlan {
  plan_id: number;
  built_at: string;
  group_count: number;
}

export interface DashboardResponse {
  words: DashboardWordStats;
  assets: DashboardAssets;
  oos_open: number;
  dead_letters: number;
  plan: DashboardPlan | null;
  /** Newest first, capped at 20 by the contract. */
  recent_events: AdminEvent[];
}

/* ------------------------------------------------------------------ */
/* GET /jobs                                                           */
/* ------------------------------------------------------------------ */

export interface JobView {
  kind: string;
  subject_type: JobSubjectType;
  subject_id: string;
  rate_key: string;
  status: JobStatus | null;
  attempts: number;
  next_retry_at: string | null;
  last_error: string | null;
  /** Human-facing label joined from the subject (lemma, text excerpt, ...). */
  subject_label: string | null;
}

export interface LaneView {
  queued: number;
  running: number;
  limit: number;
}

export interface JobsSnapshot {
  in_flight: JobView[];
  backoff: JobView[];
  lanes: Record<string, LaneView>;
}

/* ------------------------------------------------------------------ */
/* GET /stream (SSE)                                                   */
/* ------------------------------------------------------------------ */

export interface ChangeEvent {
  entity_type: string;
  entity_ids: Array<number | string>;
}

/* ------------------------------------------------------------------ */
/* Words                                                               */
/* ------------------------------------------------------------------ */

/** `words` table row, minus reconciler-private columns. */
export interface Word {
  word_id: number;
  lemma: string;
  role: WordRole;
  aux_status: AuxStatus | null;
  phonetic: string | null;
  frequency_rank: number | null;
  etymology: string | null;
  etymology_source: EtymologySource | null;
  ready: boolean;
  blockers: BlockerCode[];
  created_by: CreatedBy;
  created_at: string;
}

/** GET /words rollup row. */
export interface WordListItem {
  word_id: number;
  lemma: string;
  role: WordRole;
  ready: boolean;
  blockers: BlockerCode[];
  has_image: boolean;
  sense_count: number;
  example_count: number;
  tts_missing: number;
}

export interface WordsQuery extends PageParams {
  role?: WordRole;
  ready?: boolean;
  blocker?: BlockerCode;
  group?: number;
  q?: string;
}

export interface CreateWordBody {
  lemma: string;
  role: WordRole;
}

/* ---- candidates ---- */

interface CandidateBase {
  word_id: number;
  status: CandidateStatus;
  auto_score: number | null;
  score_detail: Record<string, number> | null;
  scorer_ver: string | null;
  created_by: string;
  created_at: string;
  source_ref: string | null;
}

export interface DefinitionCandidate extends CandidateBase {
  def_cand_id: number;
  pos: Pos;
  text: string;
  text_hash: string;
  source: DefinitionSource;
  parent_cand_id: number | null;
}

export interface ExampleCandidate extends CandidateBase {
  ex_cand_id: number;
  text: string;
  text_hash: string;
  /** Byte offsets into `text` (UTF-8), belonging to this exact candidate. */
  hl_start: number;
  hl_end: number;
  source: ExampleSource;
}

export interface ImageCandidate extends CandidateBase {
  img_cand_id: number;
  pos: Pos | null;
  file_hash: string;
  width: number | null;
  height: number | null;
  source: ImageSource;
  license: string | null;
  query_used: string | null;
}

/* ---- selections ---- */

interface SelectionBase {
  word_id: number;
  selected_by: SelectedBy;
  pinned: boolean;
  approved: boolean;
  approved_hash: string | null;
  approved_by: string | null;
  approved_at: string | null;
  selection_rev: number;
  updated_at: string;
}

export interface DefinitionSelection extends SelectionBase {
  pos: Pos;
  def_cand_id: number;
  is_primary: boolean;
  enabled: boolean;
}

export interface ExampleSelection extends SelectionBase {
  slot: ExampleSlotNumber;
  ex_cand_id: number;
}

export interface ImageSelection extends SelectionBase {
  img_cand_id: number;
}

/* ---- slot groupings returned by GET /words/{id} ---- */

export interface DefinitionSlotView {
  pos: Pos;
  selection: DefinitionSelection | null;
  candidates: DefinitionCandidate[];
}

export interface ExampleSlotView {
  slot: ExampleSlotNumber;
  selection: ExampleSelection | null;
  candidates: ExampleCandidate[];
}

export interface ImageSlotView {
  selection: ImageSelection | null;
  candidates: ImageCandidate[];
}

/** TTS status for one selected text, keyed content-addressed by input_hash. */
export interface TtsStatusView {
  kind: TtsKind;
  text: string;
  text_hash: string;
  input_hash: string;
  voice: string;
  engine: string;
  engine_ver: string;
  status: TtsStatusCode;
  file_hash: string | null;
  duration_ms: number | null;
  /** Which selected slot this text belongs to; null for the word lemma itself. */
  ref: { pos?: Pos; slot?: ExampleSlotNumber } | null;
  last_error: string | null;
}

export interface DistractorView {
  rank: 1 | 2 | 3;
  word_id: number;
  lemma: string;
  /** Recursion is capped at depth 1 by design (README Part 3). */
  core_ready: boolean;
  blockers: BlockerCode[];
  bound_at: string;
  bound_by: string;
}

/** GET /words/{id} */
export interface WordDetail {
  word: Word;
  /** Grouped by pos, primary sense first. */
  definitions: DefinitionSlotView[];
  /** Always three entries, slots 1..3 (slot 1 = mode-1 sentence). */
  examples: ExampleSlotView[];
  image: ImageSlotView;
  tts: TtsStatusView[];
  distractors: DistractorView[];
  recent_events: AdminEvent[];
}

/* ---- mutation bodies ---- */

export interface MintDefinitionBody {
  word_id: number;
  pos: Pos;
  text: string;
  parent_cand_id?: number;
}

export interface MintExampleBody {
  word_id: number;
  text: string;
  hl_start: number;
  hl_end: number;
}

export interface UploadImageBody {
  word_id: number;
  file: File;
}

/** POST /selections/{kind} — `pos` for definitions, `slot` for examples, neither for images. */
export interface SelectionKeyBody {
  word_id: number;
  pos?: Pos;
  slot?: ExampleSlotNumber;
}

export interface OverrideSelectionBody extends SelectionKeyBody {
  cand_id: number;
}

export interface SetPrimaryBody {
  word_id: number;
  pos: Pos;
}

export interface SetEnabledBody {
  word_id: number;
  pos: Pos;
  enabled: boolean;
}

export interface RejectCandidateParams {
  kind: AssetKind;
  cand_id: number;
}

/* ------------------------------------------------------------------ */
/* Gallery                                                             */
/* ------------------------------------------------------------------ */

/** GET /gallery — selected-image overview for visual review. */
export interface GalleryItem {
  word_id: number;
  lemma: string;
  role: WordRole;
  img_cand_id: number;
  file_hash: string;
  source: ImageSource;
  auto_score: number | null;
  approved: boolean;
  selected_by: SelectedBy;
  pinned: boolean;
  /** Cosine similarity to the word's own query text (its slot-1 sentence,
   * falling back to the lemma). `null` when the pair has not been scored. */
  clip_similarity: number | null;
  /** The word's selected slot-1 example sentence, for review-mode
   * image↔sentence judgment. `null` when no slot-1 selection exists yet. */
  slot1_sentence: string | null;
}

/** `sort=clip_asc|clip_desc` — worst/best semantic match first, unscored
 * items always last. Omitted keeps the gallery's default order. */
export type GallerySort = 'clip_asc' | 'clip_desc';

export interface GalleryQuery extends PageParams {
  source?: ImageSource;
  approved?: boolean;
  q?: string;
  sort?: GallerySort;
}

/* ------------------------------------------------------------------ */
/* OOV queue                                                           */
/* ------------------------------------------------------------------ */

/** One place the out-of-scope lemma occurs, i.e. one selected definition. */
export interface OovOccurrence {
  word_id: number;
  lemma: string;
  pos: Pos;
  def_cand_id: number;
  /** Full definition text so the UI can highlight the offending token. */
  text: string;
  hits: number;
  /**
   * LLM rewrite draft for this definition that avoids the OOV lemma, if the
   * `rewrite_definition` job has already produced one (README Part 4 example A:
   * "人打开队列时规避草稿已经躺在候选里"). Null while still queued.
   */
  suggested_rewrite: string | null;
}

export interface OovQueueEntry {
  oos_lemma: string;
  status: OovStatus;
  first_seen: string;
  resolved_by: string | null;
  resolved_at: string | null;
  notes: string | null;
  occurrences: OovOccurrence[];
  /** Denormalized `occurrences.length` for cheap table sorting. */
  occurrence_count: number;
}

export interface OovQuery extends PageParams {
  status?: OovStatus;
}

export type OovResolveBody =
  | { mode: 'promote'; notes?: string }
  | { mode: 'rewrite'; def_cand_id: number; text: string; notes?: string };

/* ------------------------------------------------------------------ */
/* Dead letters                                                        */
/* ------------------------------------------------------------------ */

export interface DeadLetter {
  kind: string;
  subject_type: JobSubjectType;
  subject_id: string;
  rate_key: string;
  status: JobStatus;
  attempts: number;
  next_retry_at: string | null;
  last_error: string | null;
  updated_at: string;
  /** Joined subject context so the table is readable without a second fetch. */
  subject: {
    word_id: number | null;
    lemma: string | null;
    label: string;
  };
}

/** Composite primary key of `job_state`. */
export interface JobKeyBody {
  kind: string;
  subject_type: JobSubjectType;
  subject_id: string;
}

/* ------------------------------------------------------------------ */
/* Plan                                                                */
/* ------------------------------------------------------------------ */

export interface PlanStats {
  word_count: number;
  group_count: number;
  edge_count: number;
  scc_group_count: number;
  largest_group: number;
  avg_group_size: number;
}

export interface PlanGroupSummary {
  group_seq: number;
  group_type: PlanGroupType;
  word_count: number;
  ready_count: number;
  first_lemma: string;
  last_lemma: string;
}

export interface PlanDiff {
  previous_plan_id: number | null;
  added: number;
  removed: number;
  reordered: number;
}

/** GET /plan */
export interface PlanSummary {
  plan_id: number;
  input_hash: string;
  algo_ver: string;
  params: Record<string, unknown>;
  is_current: boolean;
  built_at: string;
  stats: PlanStats;
  groups: PlanGroupSummary[];
  diff: PlanDiff;
}

export interface PlanWordView {
  word_id: number;
  lemma: string;
  role: WordRole;
  learning_order: number;
  group_seq: number;
  ready: boolean;
  blockers: BlockerCode[];
}

/** GET /plan/groups/{seq} */
export interface PlanGroupDetail {
  plan_id: number;
  group_seq: number;
  group_type: PlanGroupType;
  words: PlanWordView[];
}

/* ------------------------------------------------------------------ */
/* Releases                                                            */
/* ------------------------------------------------------------------ */

export interface Release {
  release_id: number;
  /** `YYYY.MM.DD+<manifest-hash-8>` */
  version: string;
  plan_id: number;
  input_hash: string;
  db_file_hash: string;
  exported_at: string;
  exported_by: string;
  notes: string | null;
  /** Derived from `release_manifests`, surfaced for the history table. */
  word_count: number;
  media_count: number;
  total_bytes: number;
}

/**
 * One excluded word in the holdback report. `root_cause` is either a factory
 * gate code (README Part 5) or `dependency_holdback` when the word itself passes
 * but is pulled out by the dependency-closure fixpoint.
 */
export interface HoldbackEntry {
  word_id: number;
  lemma: string;
  role: WordRole;
  root_cause: BlockerCode | 'dependency_holdback';
  root_cause_detail: string;
  /** Set when root_cause === 'dependency_holdback'. */
  blocking_word_id: number | null;
  blocking_lemma: string | null;
  /** How many otherwise-shippable words this one keeps off the boat. */
  impact_count: number;
}

/** GET /releases/preview */
export interface HoldbackReport {
  plan_id: number;
  /** Words passing every per-word gate, before closure pruning. */
  shippable_count: number;
  /** Size of the maximal dependency-closed subset R. */
  exportable_count: number;
  excluded_count: number;
  /** Sorted by impact_count descending — the edit worklist. */
  excluded: HoldbackEntry[];
  gates_pass: boolean;
  gate_failures: ExportGateFailure[];
}

export interface ExportGateFailure {
  gate: string;
  message: string;
  word_id: number | null;
  lemma: string | null;
}

export interface ExportBody {
  notes?: string;
}

/** 409 body of POST /releases/export. */
export interface ExportConflictBody extends ApiErrorEnvelope {
  failures: ExportGateFailure[];
}
