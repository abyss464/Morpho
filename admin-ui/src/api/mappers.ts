/**
 * Wire → domain normalization.
 *
 * The working DB stores booleans as INTEGER 0/1 and structured payloads
 * (`words.blockers`, `*.score_detail`, `events.detail`, `plan_artifacts.*_json`)
 * as TEXT holding JSON. Whether morphod unwraps those before serializing is an
 * encoding detail, not a contract shape, so the client tolerates both forms and
 * every consumer above this layer sees real booleans and parsed objects.
 *
 * Pure functions only — this module is the unit-tested core of the API client.
 */

import type {
  AdminEvent,
  DashboardResponse,
  DeadLetter,
  DefinitionCandidate,
  DefinitionSelection,
  DefinitionSlotView,
  DistractorView,
  ExampleCandidate,
  ExampleSelection,
  ExampleSlotView,
  GalleryItem,
  HoldbackReport,
  ImageCandidate,
  ImageSelection,
  ImageSlotView,
  JobsSnapshot,
  JobView,
  OovOccurrence,
  OovQueueEntry,
  Paginated,
  PlanGroupDetail,
  PlanSummary,
  PlanWordView,
  Release,
  TtsStatusView,
  Word,
  WordDetail,
  WordListItem,
} from './types';

type Wire = Record<string, unknown>;

const asWire = (value: unknown): Wire =>
  typeof value === 'object' && value !== null ? (value as Wire) : {};

/** SQLite booleans arrive as 0/1; JSON APIs may already send true/false. */
export function toBool(value: unknown): boolean {
  if (typeof value === 'boolean') return value;
  if (typeof value === 'number') return value !== 0;
  if (typeof value === 'string') return value === '1' || value.toLowerCase() === 'true';
  return false;
}

export function toNumber(value: unknown, fallback = 0): number {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string') {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return fallback;
}

export function toNullableNumber(value: unknown): number | null {
  if (value === null || value === undefined || value === '') return null;
  const parsed = toNumber(value, Number.NaN);
  return Number.isFinite(parsed) ? parsed : null;
}

export function toString(value: unknown, fallback = ''): string {
  return typeof value === 'string'
    ? value
    : value === null || value === undefined
      ? fallback
      : String(value);
}

export function toNullableString(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  return typeof value === 'string' ? value : String(value);
}

/** Parses a TEXT column holding JSON; passes through already-decoded values. */
export function parseJsonColumn<T>(value: unknown, fallback: T): T {
  if (value === null || value === undefined || value === '') return fallback;
  if (typeof value !== 'string') return value as T;
  try {
    const parsed: unknown = JSON.parse(value);
    return parsed === null ? fallback : (parsed as T);
  } catch {
    return fallback;
  }
}

/** `words.blockers` is a JSON array of blocker codes stored as TEXT. */
export function toBlockers(value: unknown): string[] {
  const parsed = parseJsonColumn<unknown>(value, []);
  if (!Array.isArray(parsed)) return [];
  return parsed.filter((entry): entry is string => typeof entry === 'string');
}

function toScoreDetail(value: unknown): Record<string, number> | null {
  const parsed = parseJsonColumn<Record<string, unknown> | null>(value, null);
  if (!parsed || typeof parsed !== 'object') return null;
  const out: Record<string, number> = {};
  for (const [key, raw] of Object.entries(parsed)) {
    const num = toNullableNumber(raw);
    if (num !== null) out[key] = num;
  }
  return out;
}

function mapList<T>(value: unknown, map: (item: unknown) => T): T[] {
  return Array.isArray(value) ? value.map(map) : [];
}

/* ------------------------------------------------------------------ */
/* Envelopes                                                           */
/* ------------------------------------------------------------------ */

export function mapPaginated<T>(raw: unknown, map: (item: unknown) => T): Paginated<T> {
  const wire = asWire(raw);
  const items = mapList(wire.items, map);
  return { items, total: toNumber(wire.total, items.length) };
}

/* ------------------------------------------------------------------ */
/* Events                                                              */
/* ------------------------------------------------------------------ */

export function mapEvent(raw: unknown): AdminEvent {
  const wire = asWire(raw);
  return {
    event_id: toNumber(wire.event_id),
    ts: toString(wire.ts),
    actor: toString(wire.actor),
    entity_type: toString(wire.entity_type),
    entity_id: toString(wire.entity_id),
    action: toString(wire.action),
    detail: parseJsonColumn<Record<string, unknown> | null>(wire.detail, null),
  };
}

/* ------------------------------------------------------------------ */
/* Dashboard                                                           */
/* ------------------------------------------------------------------ */

function mapRollup(raw: unknown) {
  const wire = asWire(raw);
  return {
    ready: toNumber(wire.ready),
    missing: toNumber(wire.missing),
    failed: toNumber(wire.failed),
  };
}

export function mapDashboard(raw: unknown): DashboardResponse {
  const wire = asWire(raw);
  const words = asWire(wire.words);
  const assets = asWire(wire.assets);
  const plan = wire.plan ? asWire(wire.plan) : null;
  return {
    words: {
      total: toNumber(words.total),
      target: toNumber(words.target),
      auxiliary: toNumber(words.auxiliary),
      ready: toNumber(words.ready),
      blocked: toNumber(words.blocked),
    },
    assets: {
      definitions: mapRollup(assets.definitions),
      examples: mapRollup(assets.examples),
      images: mapRollup(assets.images),
      tts: mapRollup(assets.tts),
    },
    oos_open: toNumber(wire.oos_open),
    dead_letters: toNumber(wire.dead_letters),
    plan: plan
      ? {
          plan_id: toNumber(plan.plan_id),
          built_at: toString(plan.built_at),
          group_count: toNumber(plan.group_count),
        }
      : null,
    recent_events: mapList(wire.recent_events, mapEvent),
  };
}

/* ------------------------------------------------------------------ */
/* Jobs                                                                */
/* ------------------------------------------------------------------ */

export function mapJobView(raw: unknown): JobView {
  const wire = asWire(raw);
  return {
    kind: toString(wire.kind),
    subject_type: toString(wire.subject_type, 'global') as JobView['subject_type'],
    subject_id: toString(wire.subject_id),
    rate_key: toString(wire.rate_key),
    status: (toNullableString(wire.status) as JobView['status']) ?? null,
    attempts: toNumber(wire.attempts),
    next_retry_at: toNullableString(wire.next_retry_at),
    last_error: toNullableString(wire.last_error),
    subject_label: toNullableString(wire.subject_label),
  };
}

export function mapJobsSnapshot(raw: unknown): JobsSnapshot {
  const wire = asWire(raw);
  const lanesWire = asWire(wire.lanes);
  const lanes: JobsSnapshot['lanes'] = {};
  for (const [key, value] of Object.entries(lanesWire)) {
    const lane = asWire(value);
    lanes[key] = {
      queued: toNumber(lane.queued),
      running: toNumber(lane.running),
      limit: toNumber(lane.limit),
    };
  }
  return {
    in_flight: mapList(wire.in_flight, mapJobView),
    backoff: mapList(wire.backoff, mapJobView),
    lanes,
  };
}

/* ------------------------------------------------------------------ */
/* Words                                                               */
/* ------------------------------------------------------------------ */

export function mapWord(raw: unknown): Word {
  const wire = asWire(raw);
  return {
    word_id: toNumber(wire.word_id),
    lemma: toString(wire.lemma),
    role: toString(wire.role, 'target') as Word['role'],
    aux_status: (toNullableString(wire.aux_status) as Word['aux_status']) ?? null,
    phonetic: toNullableString(wire.phonetic),
    frequency_rank: toNullableNumber(wire.frequency_rank),
    etymology: toNullableString(wire.etymology),
    etymology_source: (toNullableString(wire.etymology_source) as Word['etymology_source']) ?? null,
    ready: toBool(wire.ready),
    blockers: toBlockers(wire.blockers),
    created_by: toString(wire.created_by, 'import') as Word['created_by'],
    created_at: toString(wire.created_at),
  };
}

export function mapWordListItem(raw: unknown): WordListItem {
  const wire = asWire(raw);
  return {
    word_id: toNumber(wire.word_id),
    lemma: toString(wire.lemma),
    role: toString(wire.role, 'target') as WordListItem['role'],
    ready: toBool(wire.ready),
    blockers: toBlockers(wire.blockers),
    has_image: toBool(wire.has_image),
    sense_count: toNumber(wire.sense_count),
    example_count: toNumber(wire.example_count),
    tts_missing: toNumber(wire.tts_missing),
  };
}

export function mapGalleryItem(raw: unknown): GalleryItem {
  const wire = asWire(raw);
  return {
    word_id: toNumber(wire.word_id),
    lemma: toString(wire.lemma),
    role: toString(wire.role, 'target') as GalleryItem['role'],
    img_cand_id: toNumber(wire.img_cand_id),
    file_hash: toString(wire.file_hash),
    source: toString(wire.source, 'unsplash') as GalleryItem['source'],
    auto_score: toNullableNumber(wire.auto_score),
    approved: toBool(wire.approved),
    selected_by: toString(wire.selected_by, 'auto') as GalleryItem['selected_by'],
    pinned: toBool(wire.pinned),
    clip_similarity: toNullableNumber(wire.clip_similarity),
  };
}

function mapCandidateBase(wire: Wire) {
  return {
    word_id: toNumber(wire.word_id),
    status: toString(wire.status, 'available') as DefinitionCandidate['status'],
    auto_score: toNullableNumber(wire.auto_score),
    score_detail: toScoreDetail(wire.score_detail),
    scorer_ver: toNullableString(wire.scorer_ver),
    created_by: toString(wire.created_by),
    created_at: toString(wire.created_at),
    source_ref: toNullableString(wire.source_ref),
  };
}

export function mapDefinitionCandidate(raw: unknown): DefinitionCandidate {
  const wire = asWire(raw);
  return {
    ...mapCandidateBase(wire),
    def_cand_id: toNumber(wire.def_cand_id),
    pos: toString(wire.pos, 'noun') as DefinitionCandidate['pos'],
    text: toString(wire.text),
    text_hash: toString(wire.text_hash),
    source: toString(wire.source, 'freedict') as DefinitionCandidate['source'],
    parent_cand_id: toNullableNumber(wire.parent_cand_id),
  };
}

export function mapExampleCandidate(raw: unknown): ExampleCandidate {
  const wire = asWire(raw);
  return {
    ...mapCandidateBase(wire),
    ex_cand_id: toNumber(wire.ex_cand_id),
    text: toString(wire.text),
    text_hash: toString(wire.text_hash),
    hl_start: toNumber(wire.hl_start),
    hl_end: toNumber(wire.hl_end),
    source: toString(wire.source, 'llm') as ExampleCandidate['source'],
  };
}

export function mapImageCandidate(raw: unknown): ImageCandidate {
  const wire = asWire(raw);
  return {
    ...mapCandidateBase(wire),
    img_cand_id: toNumber(wire.img_cand_id),
    pos: (toNullableString(wire.pos) as ImageCandidate['pos']) ?? null,
    file_hash: toString(wire.file_hash),
    width: toNullableNumber(wire.width),
    height: toNullableNumber(wire.height),
    source: toString(wire.source, 'unsplash') as ImageCandidate['source'],
    license: toNullableString(wire.license),
    query_used: toNullableString(wire.query_used),
  };
}

function mapSelectionBase(wire: Wire) {
  return {
    word_id: toNumber(wire.word_id),
    selected_by: toString(wire.selected_by, 'auto') as DefinitionSelection['selected_by'],
    pinned: toBool(wire.pinned),
    approved: toBool(wire.approved),
    approved_hash: toNullableString(wire.approved_hash),
    approved_by: toNullableString(wire.approved_by),
    approved_at: toNullableString(wire.approved_at),
    selection_rev: toNumber(wire.selection_rev, 1),
    updated_at: toString(wire.updated_at),
  };
}

export function mapDefinitionSelection(raw: unknown): DefinitionSelection {
  const wire = asWire(raw);
  return {
    ...mapSelectionBase(wire),
    pos: toString(wire.pos, 'noun') as DefinitionSelection['pos'],
    def_cand_id: toNumber(wire.def_cand_id),
    is_primary: toBool(wire.is_primary),
    enabled: toBool(wire.enabled),
  };
}

export function mapExampleSelection(raw: unknown): ExampleSelection {
  const wire = asWire(raw);
  return {
    ...mapSelectionBase(wire),
    slot: toNumber(wire.slot, 1) as ExampleSelection['slot'],
    ex_cand_id: toNumber(wire.ex_cand_id),
  };
}

export function mapImageSelection(raw: unknown): ImageSelection {
  const wire = asWire(raw);
  return {
    ...mapSelectionBase(wire),
    img_cand_id: toNumber(wire.img_cand_id),
  };
}

function mapDefinitionSlot(raw: unknown): DefinitionSlotView {
  const wire = asWire(raw);
  return {
    pos: toString(wire.pos, 'noun') as DefinitionSlotView['pos'],
    selection: wire.selection ? mapDefinitionSelection(wire.selection) : null,
    candidates: mapList(wire.candidates, mapDefinitionCandidate),
  };
}

function mapExampleSlot(raw: unknown): ExampleSlotView {
  const wire = asWire(raw);
  return {
    slot: toNumber(wire.slot, 1) as ExampleSlotView['slot'],
    selection: wire.selection ? mapExampleSelection(wire.selection) : null,
    candidates: mapList(wire.candidates, mapExampleCandidate),
  };
}

function mapImageSlot(raw: unknown): ImageSlotView {
  const wire = asWire(raw);
  return {
    selection: wire.selection ? mapImageSelection(wire.selection) : null,
    candidates: mapList(wire.candidates, mapImageCandidate),
  };
}

export function mapTtsStatus(raw: unknown): TtsStatusView {
  const wire = asWire(raw);
  const ref = wire.ref ? asWire(wire.ref) : null;
  return {
    kind: toString(wire.kind, 'word') as TtsStatusView['kind'],
    text: toString(wire.text),
    text_hash: toString(wire.text_hash),
    input_hash: toString(wire.input_hash),
    voice: toString(wire.voice),
    engine: toString(wire.engine),
    engine_ver: toString(wire.engine_ver),
    status: toString(wire.status, 'missing') as TtsStatusView['status'],
    file_hash: toNullableString(wire.file_hash),
    duration_ms: toNullableNumber(wire.duration_ms),
    ref: ref
      ? {
          ...(ref.pos !== undefined && ref.pos !== null
            ? { pos: toString(ref.pos) as NonNullable<TtsStatusView['ref']>['pos'] }
            : {}),
          ...(ref.slot !== undefined && ref.slot !== null
            ? { slot: toNumber(ref.slot) as NonNullable<TtsStatusView['ref']>['slot'] }
            : {}),
        }
      : null,
    last_error: toNullableString(wire.last_error),
  };
}

export function mapDistractor(raw: unknown): DistractorView {
  const wire = asWire(raw);
  return {
    rank: toNumber(wire.rank, 1) as DistractorView['rank'],
    word_id: toNumber(wire.word_id),
    lemma: toString(wire.lemma),
    core_ready: toBool(wire.core_ready),
    blockers: toBlockers(wire.blockers),
    bound_at: toString(wire.bound_at),
    bound_by: toString(wire.bound_by, 'auto'),
  };
}

export function mapWordDetail(raw: unknown): WordDetail {
  const wire = asWire(raw);
  return {
    word: mapWord(wire.word),
    definitions: mapList(wire.definitions, mapDefinitionSlot),
    examples: mapList(wire.examples, mapExampleSlot),
    image: mapImageSlot(wire.image),
    tts: mapList(wire.tts, mapTtsStatus),
    distractors: mapList(wire.distractors, mapDistractor),
    recent_events: mapList(wire.recent_events, mapEvent),
  };
}

/* ------------------------------------------------------------------ */
/* OOV                                                                 */
/* ------------------------------------------------------------------ */

export function mapOovOccurrence(raw: unknown): OovOccurrence {
  const wire = asWire(raw);
  return {
    word_id: toNumber(wire.word_id),
    lemma: toString(wire.lemma),
    pos: toString(wire.pos, 'noun') as OovOccurrence['pos'],
    def_cand_id: toNumber(wire.def_cand_id),
    text: toString(wire.text),
    hits: toNumber(wire.hits, 1),
    suggested_rewrite: toNullableString(wire.suggested_rewrite),
  };
}

export function mapOovEntry(raw: unknown): OovQueueEntry {
  const wire = asWire(raw);
  const occurrences = mapList(wire.occurrences, mapOovOccurrence);
  return {
    oos_lemma: toString(wire.oos_lemma),
    status: toString(wire.status, 'open') as OovQueueEntry['status'],
    first_seen: toString(wire.first_seen),
    resolved_by: toNullableString(wire.resolved_by),
    resolved_at: toNullableString(wire.resolved_at),
    notes: toNullableString(wire.notes),
    occurrences,
    occurrence_count: toNumber(wire.occurrence_count, occurrences.length),
  };
}

/* ------------------------------------------------------------------ */
/* Dead letters                                                        */
/* ------------------------------------------------------------------ */

export function mapDeadLetter(raw: unknown): DeadLetter {
  const wire = asWire(raw);
  const subject = asWire(wire.subject);
  return {
    kind: toString(wire.kind),
    subject_type: toString(wire.subject_type, 'global') as DeadLetter['subject_type'],
    subject_id: toString(wire.subject_id),
    rate_key: toString(wire.rate_key),
    status: toString(wire.status, 'dead') as DeadLetter['status'],
    attempts: toNumber(wire.attempts),
    next_retry_at: toNullableString(wire.next_retry_at),
    last_error: toNullableString(wire.last_error),
    updated_at: toString(wire.updated_at),
    subject: {
      word_id: toNullableNumber(subject.word_id),
      lemma: toNullableString(subject.lemma),
      label: toString(subject.label, toString(wire.subject_id)),
    },
  };
}

/* ------------------------------------------------------------------ */
/* Plan                                                                */
/* ------------------------------------------------------------------ */

export function mapPlanSummary(raw: unknown): PlanSummary {
  const wire = asWire(raw);
  const stats = parseJsonColumn<Wire>(wire.stats ?? wire.stats_json, {});
  const diff = asWire(wire.diff);
  return {
    plan_id: toNumber(wire.plan_id),
    input_hash: toString(wire.input_hash),
    algo_ver: toString(wire.algo_ver),
    params: parseJsonColumn<Record<string, unknown>>(wire.params ?? wire.params_json, {}),
    is_current: toBool(wire.is_current),
    built_at: toString(wire.built_at),
    stats: {
      word_count: toNumber(stats.word_count),
      group_count: toNumber(stats.group_count),
      edge_count: toNumber(stats.edge_count),
      scc_group_count: toNumber(stats.scc_group_count),
      largest_group: toNumber(stats.largest_group),
      avg_group_size: toNumber(stats.avg_group_size),
    },
    groups: mapList(wire.groups, (item) => {
      const group = asWire(item);
      return {
        group_seq: toNumber(group.group_seq),
        group_type: toString(
          group.group_type,
          'fill',
        ) as PlanSummary['groups'][number]['group_type'],
        word_count: toNumber(group.word_count),
        ready_count: toNumber(group.ready_count),
        first_lemma: toString(group.first_lemma),
        last_lemma: toString(group.last_lemma),
      };
    }),
    diff: {
      previous_plan_id: toNullableNumber(diff.previous_plan_id),
      added: toNumber(diff.added),
      removed: toNumber(diff.removed),
      reordered: toNumber(diff.reordered),
    },
  };
}

export function mapPlanWord(raw: unknown): PlanWordView {
  const wire = asWire(raw);
  return {
    word_id: toNumber(wire.word_id),
    lemma: toString(wire.lemma),
    role: toString(wire.role, 'target') as PlanWordView['role'],
    learning_order: toNumber(wire.learning_order),
    group_seq: toNumber(wire.group_seq),
    ready: toBool(wire.ready),
    blockers: toBlockers(wire.blockers),
  };
}

export function mapPlanGroupDetail(raw: unknown): PlanGroupDetail {
  const wire = asWire(raw);
  return {
    plan_id: toNumber(wire.plan_id),
    group_seq: toNumber(wire.group_seq),
    group_type: toString(wire.group_type, 'fill') as PlanGroupDetail['group_type'],
    words: mapList(wire.words, mapPlanWord),
  };
}

/* ------------------------------------------------------------------ */
/* Releases                                                            */
/* ------------------------------------------------------------------ */

export function mapRelease(raw: unknown): Release {
  const wire = asWire(raw);
  return {
    release_id: toNumber(wire.release_id),
    version: toString(wire.version),
    plan_id: toNumber(wire.plan_id),
    input_hash: toString(wire.input_hash),
    db_file_hash: toString(wire.db_file_hash),
    exported_at: toString(wire.exported_at),
    exported_by: toString(wire.exported_by),
    notes: toNullableString(wire.notes),
    word_count: toNumber(wire.word_count),
    media_count: toNumber(wire.media_count),
    total_bytes: toNumber(wire.total_bytes),
  };
}

export function mapHoldbackReport(raw: unknown): HoldbackReport {
  const wire = asWire(raw);
  return {
    plan_id: toNumber(wire.plan_id),
    shippable_count: toNumber(wire.shippable_count),
    exportable_count: toNumber(wire.exportable_count),
    excluded_count: toNumber(wire.excluded_count),
    excluded: mapList(wire.excluded, (item) => {
      const entry = asWire(item);
      return {
        word_id: toNumber(entry.word_id),
        lemma: toString(entry.lemma),
        role: toString(entry.role, 'target') as HoldbackReport['excluded'][number]['role'],
        root_cause: toString(
          entry.root_cause,
          'dependency_holdback',
        ) as HoldbackReport['excluded'][number]['root_cause'],
        root_cause_detail: toString(entry.root_cause_detail),
        blocking_word_id: toNullableNumber(entry.blocking_word_id),
        blocking_lemma: toNullableString(entry.blocking_lemma),
        impact_count: toNumber(entry.impact_count),
      };
    }).sort((a, b) => b.impact_count - a.impact_count || a.lemma.localeCompare(b.lemma)),
    gates_pass: toBool(wire.gates_pass),
    gate_failures: mapList(wire.gate_failures, (item) => {
      const failure = asWire(item);
      return {
        gate: toString(failure.gate),
        message: toString(failure.message),
        word_id: toNullableNumber(failure.word_id),
        lemma: toNullableString(failure.lemma),
      };
    }),
  };
}
