/**
 * In-memory fixture database for MSW.
 *
 * Mirrors the tables of `docs/contracts/working-db.sql` closely enough that the
 * handlers can compute contract responses the same way morphod will: readiness
 * is recomputed from state (never stored as a flag), the OOV queue is derived
 * from the currently selected definitions, and every mutation appends an
 * `events` row. Approving a candidate really does flip readiness, resolving an
 * OOV entry really does remove it from the queue, and rejecting a selected
 * candidate really does trigger the pin-fallback re-selection.
 *
 * This is fixture code, not product code: hashing is a cheap deterministic
 * stand-in for blake3 and "tokenization" is a word-boundary scan.
 */

import type {
  AdminEvent,
  BlockerCode,
  CandidateStatus,
  CreatedBy,
  DefinitionSource,
  EtymologySource,
  ExampleSlotNumber,
  ExampleSource,
  ImageSource,
  JobStatus,
  JobSubjectType,
  PlanGroupType,
  Pos,
  SelectedBy,
  TtsKind,
  TtsStatusCode,
  WordRole,
} from '../api/types';
import {
  OOS_LEMMAS,
  REWRITE_DRAFTS,
  SEED_WORDS,
  type SeedStage,
  type SeedWord,
} from './fixtures/lexicon';
import { SILENT_OGG_BYTES } from './fixtures/media';

/* ------------------------------------------------------------------ */
/* Row shapes                                                          */
/* ------------------------------------------------------------------ */

export interface WordRow {
  word_id: number;
  lemma: string;
  role: WordRole;
  aux_status: 'active' | 'retired' | null;
  phonetic: string | null;
  frequency_rank: number | null;
  etymology: string | null;
  etymology_source: EtymologySource | null;
  created_by: CreatedBy;
  created_at: string;
  stage: SeedStage;
}

export interface DefCandRow {
  def_cand_id: number;
  word_id: number;
  pos: Pos;
  text: string;
  text_hash: string;
  source: DefinitionSource;
  source_ref: string | null;
  parent_cand_id: number | null;
  status: CandidateStatus;
  auto_score: number;
  score_detail: Record<string, number>;
  scorer_ver: string;
  created_by: string;
  created_at: string;
}

export interface DefSelRow {
  word_id: number;
  pos: Pos;
  def_cand_id: number;
  is_primary: boolean;
  enabled: boolean;
  selected_by: SelectedBy;
  pinned: boolean;
  approved: boolean;
  approved_hash: string | null;
  approved_by: string | null;
  approved_at: string | null;
  selection_rev: number;
  updated_at: string;
}

export interface ExCandRow {
  ex_cand_id: number;
  word_id: number;
  text: string;
  text_hash: string;
  hl_start: number;
  hl_end: number;
  source: ExampleSource;
  source_ref: string | null;
  status: CandidateStatus;
  auto_score: number;
  score_detail: Record<string, number>;
  scorer_ver: string;
  created_by: string;
  created_at: string;
}

export interface ExSelRow {
  word_id: number;
  slot: ExampleSlotNumber;
  ex_cand_id: number;
  selected_by: SelectedBy;
  pinned: boolean;
  approved: boolean;
  approved_hash: string | null;
  approved_by: string | null;
  approved_at: string | null;
  selection_rev: number;
  updated_at: string;
}

export interface ImgCandRow {
  img_cand_id: number;
  word_id: number;
  pos: Pos | null;
  file_hash: string;
  width: number;
  height: number;
  source: ImageSource;
  source_ref: string | null;
  license: string | null;
  query_used: string | null;
  status: CandidateStatus;
  auto_score: number;
  score_detail: Record<string, number>;
  scorer_ver: string;
  created_by: string;
  created_at: string;
  /** CLIP cosine against this word's own query text, or `null` when the
   * sidecar has not scored this pair yet — mirrors `clip_scores`. */
  clip_similarity: number | null;
}

export interface ImgSelRow {
  word_id: number;
  img_cand_id: number;
  selected_by: SelectedBy;
  pinned: boolean;
  approved: boolean;
  approved_hash: string | null;
  approved_by: string | null;
  approved_at: string | null;
  selection_rev: number;
  updated_at: string;
}

export interface MediaRow {
  file_hash: string;
  kind: 'image' | 'audio';
  rel_path: string;
  bytes: number;
  label: string;
  created_at: string;
}

export interface TtsRow {
  tts_id: number;
  input_hash: string;
  text: string;
  text_hash: string;
  kind: TtsKind;
  voice: string;
  engine: string;
  engine_ver: string;
  params_json: string;
  file_hash: string | null;
  duration_ms: number | null;
  status: 'ready' | 'failed';
  last_error: string | null;
  built_at: string;
}

export interface OosRow {
  oos_lemma: string;
  status: 'open' | 'resolved_rewrite' | 'resolved_promote' | 'auto_closed';
  first_seen: string;
  resolved_by: string | null;
  resolved_at: string | null;
  notes: string | null;
}

export interface JobRow {
  kind: string;
  subject_type: JobSubjectType;
  subject_id: string;
  rate_key: string;
  status: JobStatus;
  attempts: number;
  next_retry_at: string | null;
  last_error: string | null;
  updated_at: string;
}

export interface DistractorRow {
  word_id: number;
  rank: 1 | 2 | 3;
  distractor_word_id: number;
  algo_ver: string;
  bound_at: string;
  bound_by: string;
}

export interface PlanGroupRow {
  plan_id: number;
  group_seq: number;
  group_type: PlanGroupType;
}

export interface PlanWordRow {
  plan_id: number;
  word_id: number;
  learning_order: number;
  group_seq: number;
}

export interface PlanRow {
  plan_id: number;
  input_hash: string;
  algo_ver: string;
  params: Record<string, unknown>;
  is_current: boolean;
  built_at: string;
}

export interface ReleaseRow {
  release_id: number;
  version: string;
  plan_id: number;
  input_hash: string;
  db_file_hash: string;
  exported_at: string;
  exported_by: string;
  notes: string | null;
  word_count: number;
  media_count: number;
  total_bytes: number;
}

/* ------------------------------------------------------------------ */
/* Deterministic helpers                                               */
/* ------------------------------------------------------------------ */

/** Deterministic 32-hex digest; stands in for blake3 in fixtures only. */
export function fakeHash(input: string): string {
  let h1 = 0x811c9dc5;
  let h2 = 0x01000193;
  for (let i = 0; i < input.length; i += 1) {
    const c = input.charCodeAt(i);
    h1 = Math.imul(h1 ^ c, 16777619) >>> 0;
    h2 = Math.imul(h2 + c + i, 2246822519) >>> 0;
  }
  const part = (seed: number, rounds: number): string => {
    let v = seed >>> 0;
    let out = '';
    for (let i = 0; i < rounds; i += 1) {
      v = Math.imul(v ^ (v >>> 15), 2246822507) >>> 0;
      v = Math.imul(v ^ (v >>> 13), 3266489909) >>> 0;
      out += (v >>> 0).toString(16).padStart(8, '0');
    }
    return out;
  };
  return (part(h1, 4) + part(h2, 4)).slice(0, 64);
}

/** README: NFC, trimmed, internal whitespace collapsed, case preserved. */
export function canonical(text: string): string {
  return text.normalize('NFC').trim().replace(/\s+/g, ' ');
}

const BASE_TIME = Date.UTC(2026, 7, 18, 9, 0, 0);
let clockTick = 0;

/** Monotonic ISO timestamp so ordering in fixtures is stable across reloads. */
function ts(offsetMinutes = 0): string {
  clockTick += 1;
  return new Date(BASE_TIME + offsetMinutes * 60_000 + clockTick * 137).toISOString();
}

function nowIso(): string {
  return new Date().toISOString();
}

const SCORER_VER = 'scorer-1.4.2';
const PLAN_ALGO_VER = 'plan-2.1.0';
const DISTRACTOR_ALGO_VER = 'dist-1.0.0';
const TTS_VOICE = 'en-GB-SoniaNeural';
const TTS_ENGINE = 'edge-tts';
const TTS_ENGINE_VER = '6.1.12';
const TTS_PARAMS = '{"rate":"-4%","pitch":"+0Hz"}';

function scoreDetail(source: string, score: number): Record<string, number> {
  return {
    source_prior: Number((score * 0.4).toFixed(3)),
    readability: Number((score * 0.35).toFixed(3)),
    length_window: Number((score * 0.15).toFixed(3)),
    pos_match: Number((score * 0.1).toFixed(3)),
    oov_penalty: source === 'wordnet' ? -0.05 : 0,
  };
}

/** Word-boundary scan; the fixture stand-in for the tokenizer + lemmatizer. */
export function containsLemma(text: string, lemma: string): boolean {
  return new RegExp(`\\b${lemma.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\b`, 'i').test(text);
}

function oosLemmasIn(text: string): string[] {
  return OOS_LEMMAS.filter((lemma) => containsLemma(text, lemma));
}

/* ------------------------------------------------------------------ */
/* Database                                                            */
/* ------------------------------------------------------------------ */

export interface MockState {
  words: WordRow[];
  defCands: DefCandRow[];
  defSels: DefSelRow[];
  exCands: ExCandRow[];
  exSels: ExSelRow[];
  imgCands: ImgCandRow[];
  imgSels: ImgSelRow[];
  media: MediaRow[];
  tts: TtsRow[];
  oos: OosRow[];
  jobs: JobRow[];
  distractors: DistractorRow[];
  plans: PlanRow[];
  planGroups: PlanGroupRow[];
  planWords: PlanWordRow[];
  releases: ReleaseRow[];
  events: AdminEvent[];
  seq: Record<string, number>;
}

let state: MockState = emptyState();

function emptyState(): MockState {
  return {
    words: [],
    defCands: [],
    defSels: [],
    exCands: [],
    exSels: [],
    imgCands: [],
    imgSels: [],
    media: [],
    tts: [],
    oos: [],
    jobs: [],
    distractors: [],
    plans: [],
    planGroups: [],
    planWords: [],
    releases: [],
    events: [],
    seq: {},
  };
}

function nextId(table: string): number {
  const current = state.seq[table] ?? 0;
  const next = current + 1;
  state.seq[table] = next;
  return next;
}

export function db(): MockState {
  if (state.words.length === 0) seed();
  return state;
}

export function resetDb(): void {
  state = emptyState();
  clockTick = 0;
  seed();
}

/* ------------------------------------------------------------------ */
/* Events                                                              */
/* ------------------------------------------------------------------ */

export function recordEvent(
  actor: string,
  entityType: string,
  entityId: string | number,
  action: string,
  detail: Record<string, unknown> | null = null,
): AdminEvent {
  const event: AdminEvent = {
    event_id: nextId('events'),
    ts: nowIso(),
    actor,
    entity_type: entityType,
    entity_id: String(entityId),
    action,
    detail,
  };
  state.events.unshift(event);
  return event;
}

function seedEvent(
  offsetMinutes: number,
  actor: string,
  entityType: string,
  entityId: string | number,
  action: string,
  detail: Record<string, unknown> | null = null,
): void {
  state.events.push({
    event_id: nextId('events'),
    ts: ts(offsetMinutes),
    actor,
    entity_type: entityType,
    entity_id: String(entityId),
    action,
    detail,
  });
}

/* ------------------------------------------------------------------ */
/* Seeding                                                             */
/* ------------------------------------------------------------------ */

function addMedia(kind: 'image' | 'audio', key: string, label: string, bytes: number): string {
  const fileHash = fakeHash(`${kind}:${key}`);
  if (!state.media.some((m) => m.file_hash === fileHash)) {
    state.media.push({
      file_hash: fileHash,
      kind,
      rel_path: `media/${fileHash.slice(0, 2)}/${fileHash}.${kind === 'image' ? 'webp' : 'ogg'}`,
      bytes,
      label,
      created_at: ts(-600),
    });
  }
  return fileHash;
}

const IMAGE_SOURCES: Array<{ source: ImageSource; license: string }> = [
  { source: 'unsplash', license: 'Unsplash License' },
  { source: 'pexels', license: 'Pexels License' },
  { source: 'pixabay', license: 'Pixabay Content License' },
  { source: 'sdxl', license: 'Generated (SDXL 1.0, local)' },
];

function seedWordAssets(seedWord: SeedWord, wordId: number): void {
  const { stage } = seedWord;
  if (stage === 'base') return;

  /* -------- definitions -------- */
  const senses = seedWord.senses ?? [];
  senses.forEach((sense, senseIndex) => {
    const created: DefCandRow[] = sense.candidates.map((cand, candIndex) => {
      const text = canonical(cand.text);
      const row: DefCandRow = {
        def_cand_id: nextId('def_cand'),
        word_id: wordId,
        pos: sense.pos,
        text,
        text_hash: fakeHash(text),
        source: cand.source,
        source_ref:
          cand.source === 'freedict'
            ? `freedict:${seedWord.lemma}#${candIndex}`
            : cand.source === 'wordnet'
              ? `wn31:${seedWord.lemma}.${sense.pos}.${candIndex + 1}`
              : null,
        parent_cand_id: null,
        status: 'available',
        auto_score: cand.score ?? 0.7,
        score_detail: scoreDetail(cand.source, cand.score ?? 0.7),
        scorer_ver: SCORER_VER,
        created_by: cand.source === 'manual' ? 'admin:lin' : `worker:fetch_definitions`,
        created_at: ts(-540 + candIndex),
      };
      state.defCands.push(row);
      return row;
    });

    if (created.length === 0 || stage === 'fresh') return;

    // Auto-selection = highest score, unless this word is staged to expose an
    // out-of-scope token, in which case a human pinned the offending candidate.
    let chosen = created.reduce((best, row) => (row.auto_score > best.auto_score ? row : best));
    let selectedBy: SelectedBy = 'auto';
    let pinned = false;
    if (stage === 'oos') {
      const offender = created.find((row) => oosLemmasIn(row.text).length > 0);
      if (offender) {
        chosen = offender;
        selectedBy = 'human';
        pinned = true;
      }
    }

    const approved =
      stage === 'ready' ||
      stage === 'distractor_gap' ||
      stage === 'tts_failed' ||
      stage === 'no_image';
    state.defSels.push({
      word_id: wordId,
      pos: sense.pos,
      def_cand_id: chosen.def_cand_id,
      is_primary: senseIndex === 0,
      enabled: true,
      selected_by: selectedBy,
      pinned,
      approved,
      approved_hash: approved ? chosen.text_hash : null,
      approved_by: approved ? 'admin:lin' : null,
      approved_at: approved ? ts(-320) : null,
      selection_rev: pinned ? 2 : 1,
      updated_at: ts(-330),
    });
  });

  /* -------- examples -------- */
  const examples = seedWord.examples ?? [];
  const exRows: ExCandRow[] = examples.map((ex, index) => {
    const text = canonical(ex.text);
    const hlStart = text.indexOf(ex.highlight);
    const row: ExCandRow = {
      ex_cand_id: nextId('ex_cand'),
      word_id: wordId,
      text,
      text_hash: fakeHash(text),
      hl_start: hlStart < 0 ? 0 : hlStart,
      hl_end: hlStart < 0 ? 0 : hlStart + ex.highlight.length,
      source: ex.source,
      source_ref: ex.source === 'exam_corpus' ? `kaoyan-2019-${1000 + index}` : null,
      status: 'available',
      auto_score: ex.score ?? 0.7,
      score_detail: scoreDetail(ex.source, ex.score ?? 0.7),
      scorer_ver: SCORER_VER,
      created_by: ex.source === 'manual' ? 'admin:lin' : 'worker:fetch_examples',
      created_at: ts(-500 + index),
    };
    state.exCands.push(row);
    return row;
  });

  if (stage !== 'fresh') {
    const ranked = [...exRows].sort((a, b) => b.auto_score - a.auto_score);
    const approveExamples =
      stage === 'ready' ||
      stage === 'distractor_gap' ||
      stage === 'tts_failed' ||
      stage === 'no_image';
    ranked.slice(0, 3).forEach((row, index) => {
      const slot = (index + 1) as ExampleSlotNumber;
      state.exSels.push({
        word_id: wordId,
        slot,
        ex_cand_id: row.ex_cand_id,
        selected_by: 'auto',
        pinned: false,
        approved: approveExamples,
        approved_hash: approveExamples ? row.text_hash : null,
        approved_by: approveExamples ? 'admin:lin' : null,
        approved_at: approveExamples ? ts(-300) : null,
        selection_rev: 1,
        updated_at: ts(-305),
      });
    });
  }

  /* -------- images -------- */
  if (stage !== 'no_image' && stage !== 'fresh') {
    const query = seedWord.image_query ?? seedWord.lemma;
    const count = stage === 'thin' ? 2 : 4;
    const imgRows: ImgCandRow[] = [];
    for (let i = 0; i < count; i += 1) {
      const meta = IMAGE_SOURCES[i % IMAGE_SOURCES.length]!;
      const fileHash = addMedia('image', `${seedWord.lemma}:${i}`, query, 42_000 + i * 3_100);
      const row: ImgCandRow = {
        img_cand_id: nextId('img_cand'),
        word_id: wordId,
        pos: seedWord.senses?.[0]?.pos ?? null,
        file_hash: fileHash,
        width: 768,
        height: 576,
        source: meta.source,
        source_ref:
          meta.source === 'sdxl'
            ? JSON.stringify({ prompt: query, seed: 1000 + i, model: 'sdxl-1.0' })
            : `${meta.source}:${900000 + wordId * 17 + i}`,
        license: meta.license,
        query_used: query,
        status: 'available',
        auto_score: Number((0.88 - i * 0.09).toFixed(2)),
        score_detail: scoreDetail(meta.source, 0.88 - i * 0.09),
        scorer_ver: SCORER_VER,
        created_by: `worker:fetch_images`,
        created_at: ts(-470 + i),
        // Deterministic stand-in for the sidecar's cosine, spread across the
        // whole [0.05, 0.96] band the review mode cares about; one row in
        // seven is left unscored so the "—" badge has fixture coverage too.
        clip_similarity:
          (wordId + i) % 7 === 0
            ? null
            : Number((0.05 + ((wordId * 31 + i * 13) % 92) / 100).toFixed(3)),
      };
      state.imgCands.push(row);
      imgRows.push(row);
    }
    const best = imgRows.reduce((acc, row) => (row.auto_score > acc.auto_score ? row : acc));
    const approvedImg = stage === 'ready' || stage === 'distractor_gap' || stage === 'tts_failed';
    state.imgSels.push({
      word_id: wordId,
      img_cand_id: best.img_cand_id,
      selected_by: 'auto',
      pinned: false,
      approved: approvedImg,
      approved_hash: approvedImg ? best.file_hash : null,
      approved_by: approvedImg ? 'admin:lin' : null,
      approved_at: approvedImg ? ts(-280) : null,
      selection_rev: 1,
      updated_at: ts(-285),
    });
  }
}

function ttsInputHash(text: string, kind: TtsKind): string {
  return fakeHash(
    `${canonical(text)}|${kind}|${TTS_VOICE}|${TTS_ENGINE}|${TTS_ENGINE_VER}|${TTS_PARAMS}`,
  );
}

/** Materializes `tts_assets` rows for the current selected-text set. */
function seedTts(): void {
  const failedWords = new Set(
    state.words.filter((w) => w.stage === 'tts_failed').map((w) => w.word_id),
  );

  const upsert = (text: string, kind: TtsKind, failed: boolean): void => {
    const inputHash = ttsInputHash(text, kind);
    if (state.tts.some((row) => row.input_hash === inputHash)) return;
    const fileHash = failed ? null : addMedia('audio', inputHash, text, SILENT_OGG_BYTES);
    state.tts.push({
      tts_id: nextId('tts'),
      input_hash: inputHash,
      text: canonical(text),
      text_hash: fakeHash(canonical(text)),
      kind,
      voice: TTS_VOICE,
      engine: TTS_ENGINE,
      engine_ver: TTS_ENGINE_VER,
      params_json: TTS_PARAMS,
      file_hash: fileHash,
      duration_ms: failed ? null : 420 + (text.length % 40) * 55,
      status: failed ? 'failed' : 'ready',
      last_error: failed ? 'edge-tts exited 1: WebSocket closed before handshake completed' : null,
      built_at: ts(-240),
    });
  };

  for (const word of state.words) {
    if (word.role === 'base' || word.stage === 'fresh') continue;
    const failed = failedWords.has(word.word_id);
    upsert(word.lemma, 'word', false);
    for (const sel of state.defSels.filter((s) => s.word_id === word.word_id && s.enabled)) {
      const cand = state.defCands.find((c) => c.def_cand_id === sel.def_cand_id);
      if (cand) upsert(cand.text, 'definition', failed);
    }
    for (const sel of state.exSels.filter((s) => s.word_id === word.word_id)) {
      const cand = state.exCands.find((c) => c.ex_cand_id === sel.ex_cand_id);
      if (cand) upsert(cand.text, 'example', failed && sel.slot === 1);
    }
  }
}

function seedOosQueue(): void {
  const seen = new Set<string>();
  for (const sel of state.defSels) {
    if (!sel.enabled) continue;
    const cand = state.defCands.find((c) => c.def_cand_id === sel.def_cand_id);
    if (!cand) continue;
    for (const lemma of oosLemmasIn(cand.text)) {
      if (state.words.some((w) => w.lemma.toLowerCase() === lemma.toLowerCase())) continue;
      if (seen.has(lemma)) continue;
      seen.add(lemma);
      state.oos.push({
        oos_lemma: lemma,
        status: 'open',
        first_seen: ts(-420),
        resolved_by: null,
        resolved_at: null,
        notes: null,
      });
    }
  }
  // A couple of already-closed rows so the status filter has something to show.
  state.oos.push(
    {
      oos_lemma: 'serene',
      status: 'resolved_promote',
      first_seen: ts(-1400),
      resolved_by: 'admin:lin',
      resolved_at: ts(-1300),
      notes: 'Promoted to auxiliary; used by three selected definitions.',
    },
    {
      oos_lemma: 'unpretentious',
      status: 'auto_closed',
      first_seen: ts(-1500),
      resolved_by: null,
      resolved_at: ts(-1200),
      notes: 'Disappeared from oos_occurrences after the definition was re-selected.',
    },
  );
}

const DEAD_LETTER_SPECS: Array<{
  kind: string;
  lemma: string;
  rate_key: string;
  attempts: number;
  error: string;
  status: JobStatus;
}> = [
  {
    kind: 'fetch_images',
    lemma: 'alleviate',
    rate_key: 'unsplash',
    attempts: 8,
    error: 'HTTP 403 Rate Limit Exceeded — client id quota exhausted for the hour',
    status: 'dead',
  },
  {
    kind: 'fetch_images',
    lemma: 'curb',
    rate_key: 'pexels',
    attempts: 8,
    error: 'HTTP 429 Too Many Requests (retry-after absent)',
    status: 'dead',
  },
  {
    kind: 'fetch_images',
    lemma: 'gregarious',
    rate_key: 'pixabay',
    attempts: 8,
    error: 'connect ETIMEDOUT 104.18.24.1:443 after 30000ms',
    status: 'dead',
  },
  {
    kind: 'gen_image_sdxl',
    lemma: 'mundane',
    rate_key: 'sdxl',
    attempts: 3,
    error: 'ComfyUI /history returned status=error: CUDA out of memory (tried 2.15 GiB)',
    status: 'dead',
  },
  {
    kind: 'synth_tts',
    lemma: 'ascertain',
    rate_key: 'edge_tts',
    attempts: 5,
    error: 'edge-tts exited 1: WebSocket closed before handshake completed',
    status: 'dead',
  },
  {
    kind: 'synth_tts',
    lemma: 'endorse',
    rate_key: 'edge_tts',
    attempts: 5,
    error: 'adapter timeout after 60000ms (no stdout frame received)',
    status: 'dead',
  },
  {
    kind: 'synth_tts',
    lemma: 'scrutinize',
    rate_key: 'edge_tts',
    attempts: 5,
    error: 'edge-tts exited 1: NoAudioReceived — service returned an empty stream',
    status: 'dead',
  },
  {
    kind: 'rewrite_definition',
    lemma: 'benevolent',
    rate_key: 'llm',
    attempts: 4,
    error: 'Rewrite still contains out-of-scope token "altruistic" after 3 attempts (Permanent)',
    status: 'dead',
  },
  {
    kind: 'fetch_etymology',
    lemma: 'vindicate',
    rate_key: 'wiktionary',
    attempts: 8,
    error: 'HTTP 503 Service Unavailable (upstream restart)',
    status: 'dead',
  },
  {
    kind: 'fetch_definitions',
    lemma: 'steadfast',
    rate_key: 'freedict',
    attempts: 6,
    error: 'HTTP 502 Bad Gateway',
    status: 'backoff',
  },
];

function seedJobs(): void {
  for (const spec of DEAD_LETTER_SPECS) {
    const word = state.words.find((w) => w.lemma === spec.lemma);
    if (!word) continue;
    state.jobs.push({
      kind: spec.kind,
      subject_type: 'word',
      subject_id: String(word.word_id),
      rate_key: spec.rate_key,
      status: spec.status,
      attempts: spec.attempts,
      next_retry_at:
        spec.status === 'backoff' ? new Date(Date.now() + 480_000).toISOString() : null,
      last_error: spec.error,
      updated_at: ts(-90),
    });
  }
}

/** Distractors are bound once. Fixture rule: a designated set keeps its broken
 *  triple so the console shows the `distractor_*_not_ready` cascade; every other
 *  word gets a triple whose members are core-ready. */
function seedDistractors(): void {
  const byLemma = new Map(state.words.map((w) => [w.lemma, w] as const));
  const activeWords = state.words.filter(
    (w) => w.role === 'target' || (w.role === 'auxiliary' && w.aux_status === 'active'),
  );
  // The pool deliberately excludes the words staged to expose a distractor gap:
  // they are core-ready but never fully ready, so binding anyone else to them
  // would drag the whole healthy set out of the dependency closure.
  const coreReadyPool = activeWords.filter(
    (w) => w.stage !== 'distractor_gap' && computeCoreReady(w.word_id).core_ready,
  );
  const poolIds = new Set(coreReadyPool.map((w) => w.word_id));

  for (const word of activeWords) {
    const seedRow = SEED_WORDS.find((s) => s.lemma === word.lemma);
    const declared = seedRow?.distractors ?? [];
    const keepBroken = word.stage === 'distractor_gap';
    const chosen: number[] = [];

    for (const lemma of declared) {
      const target = byLemma.get(lemma);
      if (!target || target.word_id === word.word_id) continue;
      if (chosen.includes(target.word_id)) continue;
      if (!keepBroken && !poolIds.has(target.word_id)) continue;
      chosen.push(target.word_id);
    }

    if (!keepBroken) {
      for (const candidate of coreReadyPool) {
        if (chosen.length >= 3) break;
        if (candidate.word_id === word.word_id || chosen.includes(candidate.word_id)) continue;
        chosen.push(candidate.word_id);
      }
    }

    chosen.slice(0, 3).forEach((distractorId, index) => {
      state.distractors.push({
        word_id: word.word_id,
        rank: (index + 1) as 1 | 2 | 3,
        distractor_word_id: distractorId,
        algo_ver: DISTRACTOR_ALGO_VER,
        bound_at: ts(-260),
        bound_by: 'auto',
      });
    });
  }
}

function seedPlan(): void {
  const active = state.words
    .filter((w) => w.role === 'target' || (w.role === 'auxiliary' && w.aux_status === 'active'))
    .sort(
      (a, b) => (a.frequency_rank ?? 99999) - (b.frequency_rank ?? 99999) || a.word_id - b.word_id,
    );

  const buildPlan = (planId: number, isCurrent: boolean, offset: number): void => {
    state.plans.push({
      plan_id: planId,
      input_hash: fakeHash(`plan:${planId}:${active.map((w) => w.word_id).join(',')}`),
      algo_ver: PLAN_ALGO_VER,
      params: { group_min: 15, group_max: 20, semantic_threshold: 0.62, debounce_ms: 2000 },
      is_current: isCurrent,
      built_at: ts(offset),
    });

    // Three small SCC groups up front (mutually-referencing definitions), then
    // fill groups of 15-20 for the remainder.
    const sccTriples: PlanGroupType[] = ['scc', 'scc', 'root'];
    let cursor = 0;
    let groupSeq = 1;
    let order = 1;

    for (const groupType of sccTriples) {
      const size = groupType === 'scc' ? 3 : 4;
      const slice = active.slice(cursor, cursor + size);
      if (slice.length === 0) break;
      state.planGroups.push({ plan_id: planId, group_seq: groupSeq, group_type: groupType });
      for (const word of slice) {
        state.planWords.push({
          plan_id: planId,
          word_id: word.word_id,
          learning_order: order,
          group_seq: groupSeq,
        });
        order += 1;
      }
      cursor += slice.length;
      groupSeq += 1;
    }

    let alternate = 0;
    while (cursor < active.length) {
      const size = alternate % 2 === 0 ? 17 : 15;
      const slice = active.slice(cursor, cursor + size);
      state.planGroups.push({
        plan_id: planId,
        group_seq: groupSeq,
        group_type: alternate % 3 === 1 ? 'semantic' : 'fill',
      });
      for (const word of slice) {
        state.planWords.push({
          plan_id: planId,
          word_id: word.word_id,
          learning_order: order,
          group_seq: groupSeq,
        });
        order += 1;
      }
      cursor += slice.length;
      groupSeq += 1;
      alternate += 1;
    }
  };

  buildPlan(nextId('plan'), false, -2880);
  buildPlan(nextId('plan'), true, -45);
}

function seedReleases(): void {
  const planId = state.plans.find((p) => p.is_current)?.plan_id ?? 1;
  const history: Array<[string, string, number, number, number, string | null]> = [
    [
      '2026.06.14+7c1af03e',
      'admin:lin',
      21,
      84,
      6_412_880,
      'First internal build for the pilot group.',
    ],
    [
      '2026.07.02+b39d5c17',
      'admin:lin',
      27,
      108,
      8_204_331,
      'Added auxiliary batch after the OOV sweep.',
    ],
    ['2026.08.09+e5720a94', 'admin:wei', 31, 124, 9_338_607, null],
  ];
  history.forEach(([version, by, words, mediaCount, bytes, notes], index) => {
    state.releases.push({
      release_id: nextId('release'),
      version,
      plan_id: index === history.length - 1 ? planId : Math.max(1, planId - 1),
      input_hash: fakeHash(`release:${version}`),
      db_file_hash: fakeHash(`releasedb:${version}`),
      exported_at: ts(-4000 + index * 900),
      exported_by: by,
      notes,
      word_count: words,
      media_count: mediaCount,
      total_bytes: bytes,
    });
  });
}

function seedEvents(): void {
  const pick = (lemma: string) => state.words.find((w) => w.lemma === lemma);
  const benevolent = pick('benevolent');
  const abandon = pick('abandon');
  const serene = pick('serene');
  const alleviate = pick('alleviate');
  const endorse = pick('endorse');
  const meticulous = pick('meticulous');

  if (serene) {
    seedEvent(-1300, 'admin:lin', 'word', serene.word_id, 'aux_promoted', {
      from: 'oos_queue',
      lemma: 'serene',
      referenced_by: 3,
    });
  }
  if (benevolent) {
    seedEvent(-420, 'reconciler', 'word', benevolent.word_id, 'approval_invalidated', {
      reason: 'oos_pending',
      token: 'altruistic',
    });
    seedEvent(-410, 'worker:rewrite_definition', 'def_candidate', benevolent.word_id, 'job_dead', {
      attempts: 4,
      error: 'Rewrite still contains out-of-scope token "altruistic"',
    });
  }
  if (abandon) {
    seedEvent(-330, 'admin:lin', 'selection', `${abandon.word_id}:verb`, 'approved', {
      pos: 'verb',
      approved_hash: fakeHash('to leave a person, thing or place with no plan to return').slice(
        0,
        16,
      ),
    });
  }
  if (alleviate) {
    seedEvent(-120, 'worker:fetch_images', 'word', alleviate.word_id, 'job_dead', {
      rate_key: 'unsplash',
      attempts: 8,
    });
  }
  if (endorse) {
    seedEvent(-95, 'worker:synth_tts', 'tts_input', endorse.word_id, 'job_dead', {
      rate_key: 'edge_tts',
      attempts: 5,
    });
  }
  if (meticulous) {
    seedEvent(-60, 'admin:wei', 'selection', `${meticulous.word_id}:image`, 'selection_changed', {
      before: { source: 'pexels', score: 0.79 },
      after: { source: 'unsplash', score: 0.88 },
    });
  }
  seedEvent(-45, 'reconciler', 'plan', 2, 'plan_rebuilt', {
    algo_ver: PLAN_ALGO_VER,
    groups: state.planGroups.filter((g) => g.plan_id === 2).length,
    duration_ms: 7,
  });
  seedEvent(-30, 'reconciler', 'global', 'distractors', 'distractor_bound', {
    bound: state.distractors.length,
  });
  seedEvent(-4000, 'admin:lin', 'release', 1, 'release_exported', {
    version: '2026.06.14+7c1af03e',
  });

  state.events.sort((a, b) => (a.ts < b.ts ? 1 : a.ts > b.ts ? -1 : b.event_id - a.event_id));
}

function seed(): void {
  for (const seedWord of SEED_WORDS) {
    const wordId = nextId('word');
    state.words.push({
      word_id: wordId,
      lemma: seedWord.lemma,
      role: seedWord.role,
      aux_status: seedWord.role === 'auxiliary' ? 'active' : null,
      phonetic: seedWord.phonetic ?? null,
      frequency_rank: seedWord.rank ?? null,
      etymology: seedWord.etymology ?? null,
      etymology_source: seedWord.etymology_source ?? null,
      created_by: seedWord.role === 'auxiliary' ? 'promotion' : 'import',
      created_at: ts(-2000),
      stage: seedWord.stage,
    });
  }
  for (const seedWord of SEED_WORDS) {
    const word = state.words.find((w) => w.lemma === seedWord.lemma);
    if (word) seedWordAssets(seedWord, word.word_id);
  }
  seedTts();
  seedOosQueue();
  seedJobs();
  seedDistractors();
  seedPlan();
  seedReleases();
  seedEvents();
}

/* ------------------------------------------------------------------ */
/* Derived state: readiness                                            */
/* ------------------------------------------------------------------ */

export interface CoreReadiness {
  core_ready: boolean;
  blockers: BlockerCode[];
}

export function ttsFor(text: string, kind: TtsKind): TtsRow | undefined {
  db();
  return state.tts.find((row) => row.input_hash === ttsInputHash(text, kind));
}

export function ttsStatusOf(text: string, kind: TtsKind): TtsStatusCode {
  const row = ttsFor(text, kind);
  if (!row) return 'missing';
  return row.status;
}

/** README Part 3: core_ready caps the recursion depth at one. */
export function computeCoreReady(wordId: number): CoreReadiness {
  db();
  const word = state.words.find((w) => w.word_id === wordId);
  const blockers: BlockerCode[] = [];
  if (!word || word.role === 'base') return { core_ready: false, blockers: ['not_in_plan'] };

  const sels = state.defSels.filter((s) => s.word_id === wordId);
  if (sels.length === 0) {
    blockers.push('missing_definition');
  } else {
    const primary = sels.find((s) => s.is_primary);
    if (!primary) blockers.push('missing_primary_sense');
    if (sels.some((s) => s.enabled && !s.approved)) blockers.push('sense_not_approved');
    for (const sel of sels) {
      if (!sel.enabled) continue;
      const cand = state.defCands.find((c) => c.def_cand_id === sel.def_cand_id);
      if (!cand) continue;
      const oos = oosLemmasIn(cand.text).filter(
        (lemma) => !state.words.some((w) => w.lemma.toLowerCase() === lemma.toLowerCase()),
      );
      if (oos.length > 0 && !blockers.includes('oos_pending')) blockers.push('oos_pending');
    }
  }

  const slot1 = state.exSels.find((s) => s.word_id === wordId && s.slot === 1);
  if (!slot1) blockers.push('missing_example');
  else if (!slot1.approved) blockers.push('example_not_approved');

  const img = state.imgSels.find((s) => s.word_id === wordId);
  if (!img) blockers.push('missing_image');
  else if (!img.approved) blockers.push('image_not_approved');

  let ttsMissing = 0;
  let ttsFailed = 0;
  for (const view of ttsTextsOf(wordId)) {
    const status = ttsStatusOf(view.text, view.kind);
    if (status === 'missing') ttsMissing += 1;
    if (status === 'failed') ttsFailed += 1;
  }
  if (ttsMissing > 0) blockers.push('tts_missing');
  if (ttsFailed > 0) blockers.push('tts_failed');

  return { core_ready: blockers.length === 0, blockers };
}

export interface TtsTextRef {
  kind: TtsKind;
  text: string;
  ref: { pos?: Pos; slot?: ExampleSlotNumber } | null;
}

/** The `tts_desired` slice belonging to one word. */
export function ttsTextsOf(wordId: number): TtsTextRef[] {
  db();
  const word = state.words.find((w) => w.word_id === wordId);
  if (!word) return [];
  const out: TtsTextRef[] = [{ kind: 'word', text: word.lemma, ref: null }];
  for (const sel of state.defSels.filter((s) => s.word_id === wordId && s.enabled)) {
    const cand = state.defCands.find((c) => c.def_cand_id === sel.def_cand_id);
    if (cand) out.push({ kind: 'definition', text: cand.text, ref: { pos: sel.pos } });
  }
  for (const sel of state.exSels
    .filter((s) => s.word_id === wordId)
    .sort((a, b) => a.slot - b.slot)) {
    const cand = state.exCands.find((c) => c.ex_cand_id === sel.ex_cand_id);
    if (cand) out.push({ kind: 'example', text: cand.text, ref: { slot: sel.slot } });
  }
  return out;
}

export interface Readiness {
  ready: boolean;
  blockers: BlockerCode[];
}

export function computeReadiness(wordId: number): Readiness {
  db();
  const word = state.words.find((w) => w.word_id === wordId);
  if (!word || word.role === 'base') return { ready: false, blockers: [] };

  const core = computeCoreReady(wordId);
  const blockers = [...core.blockers];

  const bound = state.distractors
    .filter((d) => d.word_id === wordId)
    .sort((a, b) => a.rank - b.rank);
  if (bound.length < 3) {
    blockers.push('distractors_unbound');
  } else {
    for (const link of bound) {
      if (!computeCoreReady(link.distractor_word_id).core_ready) {
        blockers.push(`distractor_${link.rank}_not_ready` as BlockerCode);
      }
    }
  }

  const currentPlan = state.plans.find((p) => p.is_current);
  if (
    currentPlan &&
    !state.planWords.some((pw) => pw.plan_id === currentPlan.plan_id && pw.word_id === wordId)
  ) {
    blockers.push('not_in_plan');
  }

  return { ready: blockers.length === 0, blockers };
}

/* ------------------------------------------------------------------ */
/* Derived state: OOV queue synchronization                            */
/* ------------------------------------------------------------------ */

export interface OosOccurrenceRow {
  word_id: number;
  lemma: string;
  pos: Pos;
  def_cand_id: number;
  text: string;
  hits: number;
  suggested_rewrite: string | null;
}

/** The `oos_occurrences` view: OOV tokens of currently selected definitions. */
export function oosOccurrences(oosLemma: string): OosOccurrenceRow[] {
  db();
  const rows: OosOccurrenceRow[] = [];
  for (const sel of state.defSels) {
    if (!sel.enabled) continue;
    const cand = state.defCands.find((c) => c.def_cand_id === sel.def_cand_id);
    if (!cand || !containsLemma(cand.text, oosLemma)) continue;
    const word = state.words.find((w) => w.word_id === sel.word_id);
    if (!word) continue;
    const draft = REWRITE_DRAFTS[oosLemma.toLowerCase()] ?? null;
    rows.push({
      word_id: word.word_id,
      lemma: word.lemma,
      pos: sel.pos,
      def_cand_id: cand.def_cand_id,
      text: cand.text,
      hits:
        (cand.text.toLowerCase().match(new RegExp(`\\b${oosLemma.toLowerCase()}\\b`, 'g')) ?? [])
          .length || 1,
      suggested_rewrite: draft,
    });
  }
  return rows;
}

/** Reconciler sync rule: open rows whose occurrences vanished become auto_closed. */
export function syncOosQueue(): void {
  db();
  for (const row of state.oos) {
    if (row.status !== 'open') continue;
    if (oosOccurrences(row.oos_lemma).length === 0) {
      row.status = 'auto_closed';
      row.resolved_at = nowIso();
      row.notes = row.notes ?? 'No longer present in any selected definition.';
      recordEvent('reconciler', 'oos_queue', row.oos_lemma, 'oos_auto_closed', null);
    }
  }
}

/* ------------------------------------------------------------------ */
/* Mutations                                                           */
/* ------------------------------------------------------------------ */

export function findWordByLemma(lemma: string): WordRow | undefined {
  return state.words.find((w) => w.lemma.toLowerCase() === lemma.toLowerCase());
}

export function insertWord(lemma: string, role: WordRole, createdBy: CreatedBy): WordRow {
  const row: WordRow = {
    word_id: nextId('word'),
    lemma,
    role,
    aux_status: role === 'auxiliary' ? 'active' : null,
    phonetic: null,
    frequency_rank: null,
    etymology: null,
    etymology_source: null,
    created_by: createdBy,
    created_at: nowIso(),
    stage: 'fresh',
  };
  state.words.push(row);
  // A brand-new word has zero assets, so the reconciler immediately derives a
  // full fetch fan-out. We only model the observable part: it enters the plan.
  const current = state.plans.find((p) => p.is_current);
  if (current) {
    const lastGroup = state.planGroups
      .filter((g) => g.plan_id === current.plan_id)
      .reduce((max, g) => Math.max(max, g.group_seq), 0);
    const nextOrder =
      state.planWords
        .filter((pw) => pw.plan_id === current.plan_id)
        .reduce((max, pw) => Math.max(max, pw.learning_order), 0) + 1;
    state.planWords.push({
      plan_id: current.plan_id,
      word_id: row.word_id,
      learning_order: nextOrder,
      group_seq: lastGroup || 1,
    });
  }
  return row;
}

export function mintDefinitionCandidate(
  wordId: number,
  pos: Pos,
  text: string,
  parentCandId: number | null,
  source: DefinitionSource = 'manual',
): DefCandRow {
  const canonicalText = canonical(text);
  const row: DefCandRow = {
    def_cand_id: nextId('def_cand'),
    word_id: wordId,
    pos,
    text: canonicalText,
    text_hash: fakeHash(canonicalText),
    source,
    source_ref: null,
    parent_cand_id: parentCandId,
    status: 'available',
    auto_score: source === 'manual' ? 0.95 : 0.88,
    score_detail: scoreDetail(source, source === 'manual' ? 0.95 : 0.88),
    scorer_ver: SCORER_VER,
    created_by: 'admin:lin',
    created_at: nowIso(),
  };
  state.defCands.push(row);
  return row;
}

export function mintExampleCandidate(
  wordId: number,
  text: string,
  hlStart: number,
  hlEnd: number,
): ExCandRow {
  const canonicalText = canonical(text);
  const row: ExCandRow = {
    ex_cand_id: nextId('ex_cand'),
    word_id: wordId,
    text: canonicalText,
    text_hash: fakeHash(canonicalText),
    hl_start: hlStart,
    hl_end: hlEnd,
    source: 'manual',
    source_ref: null,
    status: 'available',
    auto_score: 0.95,
    score_detail: scoreDetail('manual', 0.95),
    scorer_ver: SCORER_VER,
    created_by: 'admin:lin',
    created_at: nowIso(),
  };
  state.exCands.push(row);
  return row;
}

export function mintImageCandidate(wordId: number, filename: string, bytes: number): ImgCandRow {
  const fileHash = addMedia('image', `upload:${wordId}:${filename}:${bytes}`, filename, bytes);
  const row: ImgCandRow = {
    img_cand_id: nextId('img_cand'),
    word_id: wordId,
    pos: null,
    file_hash: fileHash,
    width: 768,
    height: 576,
    source: 'manual',
    source_ref: `upload:${filename}`,
    license: 'Manually supplied',
    query_used: null,
    status: 'available',
    auto_score: 0.96,
    score_detail: scoreDetail('manual', 0.96),
    scorer_ver: SCORER_VER,
    created_by: 'admin:lin',
    created_at: nowIso(),
    // A fresh upload has not been through the CLIP sidecar yet.
    clip_similarity: null,
  };
  state.imgCands.push(row);
  return row;
}

/** Auto-selection fallback after a selected candidate is rejected. */
function reselectDefinition(wordId: number, pos: Pos): void {
  const pool = state.defCands.filter(
    (c) => c.word_id === wordId && c.pos === pos && c.status === 'available',
  );
  const sel = state.defSels.find((s) => s.word_id === wordId && s.pos === pos);
  if (pool.length === 0) {
    if (sel) state.defSels = state.defSels.filter((s) => s !== sel);
    return;
  }
  const best = pool.reduce((acc, row) => (row.auto_score > acc.auto_score ? row : acc));
  if (!sel) return;
  sel.def_cand_id = best.def_cand_id;
  sel.selected_by = 'auto';
  sel.pinned = false;
  sel.approved = false;
  sel.approved_hash = null;
  sel.approved_by = null;
  sel.approved_at = null;
  sel.selection_rev += 1;
  sel.updated_at = nowIso();
}

function reselectExample(wordId: number, slot: ExampleSlotNumber): void {
  const used = new Set(
    state.exSels.filter((s) => s.word_id === wordId && s.slot !== slot).map((s) => s.ex_cand_id),
  );
  const pool = state.exCands.filter(
    (c) => c.word_id === wordId && c.status === 'available' && !used.has(c.ex_cand_id),
  );
  const sel = state.exSels.find((s) => s.word_id === wordId && s.slot === slot);
  if (!sel) return;
  if (pool.length === 0) {
    state.exSels = state.exSels.filter((s) => s !== sel);
    return;
  }
  const best = pool.reduce((acc, row) => (row.auto_score > acc.auto_score ? row : acc));
  sel.ex_cand_id = best.ex_cand_id;
  sel.selected_by = 'auto';
  sel.pinned = false;
  sel.approved = false;
  sel.approved_hash = null;
  sel.approved_by = null;
  sel.approved_at = null;
  sel.selection_rev += 1;
  sel.updated_at = nowIso();
}

function reselectImage(wordId: number): void {
  const pool = state.imgCands.filter((c) => c.word_id === wordId && c.status === 'available');
  const sel = state.imgSels.find((s) => s.word_id === wordId);
  if (!sel) return;
  if (pool.length === 0) {
    state.imgSels = state.imgSels.filter((s) => s !== sel);
    return;
  }
  const best = pool.reduce((acc, row) => (row.auto_score > acc.auto_score ? row : acc));
  sel.img_cand_id = best.img_cand_id;
  sel.selected_by = 'auto';
  sel.pinned = false;
  sel.approved = false;
  sel.approved_hash = null;
  sel.approved_by = null;
  sel.approved_at = null;
  sel.selection_rev += 1;
  sel.updated_at = nowIso();
}

export function rejectCandidate(
  kind: 'definition' | 'example' | 'image',
  candId: number,
): number | null {
  if (kind === 'definition') {
    const cand = state.defCands.find((c) => c.def_cand_id === candId);
    if (!cand) return null;
    cand.status = 'rejected';
    const sel = state.defSels.find((s) => s.def_cand_id === candId);
    if (sel) reselectDefinition(cand.word_id, cand.pos);
    ensureTtsForWord(cand.word_id);
    return cand.word_id;
  }
  if (kind === 'example') {
    const cand = state.exCands.find((c) => c.ex_cand_id === candId);
    if (!cand) return null;
    cand.status = 'rejected';
    const sel = state.exSels.find((s) => s.ex_cand_id === candId);
    if (sel) reselectExample(cand.word_id, sel.slot);
    ensureTtsForWord(cand.word_id);
    return cand.word_id;
  }
  const cand = state.imgCands.find((c) => c.img_cand_id === candId);
  if (!cand) return null;
  cand.status = 'rejected';
  const sel = state.imgSels.find((s) => s.img_cand_id === candId);
  if (sel) reselectImage(cand.word_id);
  return cand.word_id;
}

/**
 * Human override: point the slot at another candidate, `selected_by='human'`,
 * `pinned=1`, approval reset because the approved triple changed.
 */
export function overrideSelection(
  kind: 'definition' | 'example' | 'image',
  wordId: number,
  candId: number,
  pos?: Pos,
  slot?: ExampleSlotNumber,
): void {
  const stamp = nowIso();
  if (kind === 'definition') {
    const cand = state.defCands.find((c) => c.def_cand_id === candId);
    if (!cand) return;
    const targetPos = pos ?? cand.pos;
    let sel = state.defSels.find((s) => s.word_id === wordId && s.pos === targetPos);
    if (!sel) {
      sel = {
        word_id: wordId,
        pos: targetPos,
        def_cand_id: candId,
        is_primary: !state.defSels.some((s) => s.word_id === wordId && s.is_primary),
        enabled: true,
        selected_by: 'human',
        pinned: true,
        approved: false,
        approved_hash: null,
        approved_by: null,
        approved_at: null,
        selection_rev: 1,
        updated_at: stamp,
      };
      state.defSels.push(sel);
    } else {
      sel.def_cand_id = candId;
      sel.selected_by = 'human';
      sel.pinned = true;
      sel.approved = false;
      sel.approved_hash = null;
      sel.approved_by = null;
      sel.approved_at = null;
      sel.selection_rev += 1;
      sel.updated_at = stamp;
    }
    ensureTtsForWord(wordId);
    return;
  }

  if (kind === 'example') {
    const cand = state.exCands.find((c) => c.ex_cand_id === candId);
    if (!cand) return;
    const targetSlot = (slot ?? 1) as ExampleSlotNumber;
    // A candidate may only occupy one slot (UNIQUE (word_id, ex_cand_id)).
    state.exSels = state.exSels.filter(
      (s) => !(s.word_id === wordId && s.ex_cand_id === candId && s.slot !== targetSlot),
    );
    let sel = state.exSels.find((s) => s.word_id === wordId && s.slot === targetSlot);
    if (!sel) {
      sel = {
        word_id: wordId,
        slot: targetSlot,
        ex_cand_id: candId,
        selected_by: 'human',
        pinned: true,
        approved: false,
        approved_hash: null,
        approved_by: null,
        approved_at: null,
        selection_rev: 1,
        updated_at: stamp,
      };
      state.exSels.push(sel);
    } else {
      sel.ex_cand_id = candId;
      sel.selected_by = 'human';
      sel.pinned = true;
      sel.approved = false;
      sel.approved_hash = null;
      sel.approved_by = null;
      sel.approved_at = null;
      sel.selection_rev += 1;
      sel.updated_at = stamp;
    }
    ensureTtsForWord(wordId);
    return;
  }

  let sel = state.imgSels.find((s) => s.word_id === wordId);
  if (!sel) {
    sel = {
      word_id: wordId,
      img_cand_id: candId,
      selected_by: 'human',
      pinned: true,
      approved: false,
      approved_hash: null,
      approved_by: null,
      approved_at: null,
      selection_rev: 1,
      updated_at: stamp,
    };
    state.imgSels.push(sel);
  } else {
    sel.img_cand_id = candId;
    sel.selected_by = 'human';
    sel.pinned = true;
    sel.approved = false;
    sel.approved_hash = null;
    sel.approved_by = null;
    sel.approved_at = null;
    sel.selection_rev += 1;
    sel.updated_at = stamp;
  }
}

export function setApproval(
  kind: 'definition' | 'example' | 'image',
  wordId: number,
  approved: boolean,
  actor: string,
  pos?: Pos,
  slot?: ExampleSlotNumber,
): boolean {
  const stamp = nowIso();
  if (kind === 'definition') {
    const sel = state.defSels.find((s) => s.word_id === wordId && s.pos === pos);
    if (!sel) return false;
    const cand = state.defCands.find((c) => c.def_cand_id === sel.def_cand_id);
    sel.approved = approved;
    sel.approved_hash = approved ? (cand?.text_hash ?? null) : null;
    sel.approved_by = approved ? actor : null;
    sel.approved_at = approved ? stamp : null;
    if (approved) sel.pinned = true; // approval implies a pin
    sel.updated_at = stamp;
    return true;
  }
  if (kind === 'example') {
    const sel = state.exSels.find((s) => s.word_id === wordId && s.slot === slot);
    if (!sel) return false;
    const cand = state.exCands.find((c) => c.ex_cand_id === sel.ex_cand_id);
    sel.approved = approved;
    sel.approved_hash = approved ? (cand?.text_hash ?? null) : null;
    sel.approved_by = approved ? actor : null;
    sel.approved_at = approved ? stamp : null;
    if (approved) sel.pinned = true;
    sel.updated_at = stamp;
    return true;
  }
  const sel = state.imgSels.find((s) => s.word_id === wordId);
  if (!sel) return false;
  const cand = state.imgCands.find((c) => c.img_cand_id === sel.img_cand_id);
  sel.approved = approved;
  sel.approved_hash = approved ? (cand?.file_hash ?? null) : null;
  sel.approved_by = approved ? actor : null;
  sel.approved_at = approved ? stamp : null;
  if (approved) sel.pinned = true;
  sel.updated_at = stamp;
  return true;
}

export function setPrimarySense(wordId: number, pos: Pos): boolean {
  const target = state.defSels.find((s) => s.word_id === wordId && s.pos === pos);
  if (!target) return false;
  for (const sel of state.defSels.filter((s) => s.word_id === wordId)) {
    sel.is_primary = sel === target;
    sel.updated_at = nowIso();
  }
  return true;
}

export function setSenseEnabled(wordId: number, pos: Pos, enabled: boolean): boolean {
  const sel = state.defSels.find((s) => s.word_id === wordId && s.pos === pos);
  if (!sel) return false;
  sel.enabled = enabled;
  sel.updated_at = nowIso();
  ensureTtsForWord(wordId);
  return true;
}

/**
 * Materializes any newly desired TTS rows as `ready`. The real engine queues a
 * synthesis job; for the mock, appearing instantly keeps the readiness cascade
 * observable without a fake worker loop.
 */
export function ensureTtsForWord(wordId: number): void {
  for (const view of ttsTextsOf(wordId)) {
    const inputHash = ttsInputHash(view.text, view.kind);
    if (state.tts.some((row) => row.input_hash === inputHash)) continue;
    const fileHash = addMedia('audio', inputHash, view.text, SILENT_OGG_BYTES);
    state.tts.push({
      tts_id: nextId('tts'),
      input_hash: inputHash,
      text: view.text,
      text_hash: fakeHash(view.text),
      kind: view.kind,
      voice: TTS_VOICE,
      engine: TTS_ENGINE,
      engine_ver: TTS_ENGINE_VER,
      params_json: TTS_PARAMS,
      file_hash: fileHash,
      duration_ms: 420 + (view.text.length % 40) * 55,
      status: 'ready',
      last_error: null,
      built_at: nowIso(),
    });
  }
}

export function clearJob(kind: string, subjectType: JobSubjectType, subjectId: string): boolean {
  const before = state.jobs.length;
  state.jobs = state.jobs.filter(
    (j) => !(j.kind === kind && j.subject_type === subjectType && j.subject_id === subjectId),
  );
  return state.jobs.length !== before;
}

export function waiveJob(kind: string, subjectType: JobSubjectType, subjectId: string): boolean {
  const row = state.jobs.find(
    (j) => j.kind === kind && j.subject_type === subjectType && j.subject_id === subjectId,
  );
  if (!row) return false;
  row.status = 'waived';
  row.updated_at = nowIso();
  // Waiving a dead TTS job satisfies the demand by absence: the failed asset is
  // replaced with a successful one from the fallback path.
  if (kind === 'synth_tts') {
    for (const asset of state.tts.filter((t) => t.status === 'failed')) {
      const owner = state.words.find((w) => String(w.word_id) === subjectId);
      if (!owner) continue;
      const owned = ttsTextsOf(owner.word_id).some((view) => view.text === asset.text);
      if (!owned) continue;
      asset.status = 'ready';
      asset.last_error = null;
      asset.file_hash = addMedia('audio', asset.input_hash, asset.text, SILENT_OGG_BYTES);
      asset.duration_ms = 420 + (asset.text.length % 40) * 55;
    }
  }
  return true;
}

export function resolveOosPromote(lemma: string, actor: string): OosRow {
  let row = state.oos.find((r) => r.oos_lemma.toLowerCase() === lemma.toLowerCase());
  if (!row) {
    row = {
      oos_lemma: lemma,
      status: 'open',
      first_seen: nowIso(),
      resolved_by: null,
      resolved_at: null,
      notes: null,
    };
    state.oos.push(row);
  }
  if (!findWordByLemma(lemma)) {
    const word = insertWord(lemma, 'auxiliary', 'promotion');
    recordEvent(actor, 'word', word.word_id, 'aux_promoted', { lemma, from: 'oos_queue' });
  }
  row.status = 'resolved_promote';
  row.resolved_by = actor;
  row.resolved_at = nowIso();
  row.notes = row.notes ?? 'Promoted to auxiliary from the OOV queue.';
  return row;
}

export function resolveOosRewrite(
  lemma: string,
  defCandId: number,
  text: string,
  actor: string,
): OosRow | null {
  const original = state.defCands.find((c) => c.def_cand_id === defCandId);
  if (!original) return null;
  const rewrite = mintDefinitionCandidate(
    original.word_id,
    original.pos,
    text,
    original.def_cand_id,
    'llm_rewrite',
  );
  overrideSelection('definition', original.word_id, rewrite.def_cand_id, original.pos);
  recordEvent(actor, 'def_candidate', rewrite.def_cand_id, 'candidate_added', {
    source: 'llm_rewrite',
    parent_cand_id: original.def_cand_id,
    avoids: lemma,
  });

  const row = state.oos.find((r) => r.oos_lemma.toLowerCase() === lemma.toLowerCase());
  if (row && oosOccurrences(lemma).length === 0) {
    row.status = 'resolved_rewrite';
    row.resolved_by = actor;
    row.resolved_at = nowIso();
    row.notes = `Rewrote definition #${original.def_cand_id} to avoid "${lemma}".`;
  }
  return row ?? null;
}

/* ------------------------------------------------------------------ */
/* Release computations                                                */
/* ------------------------------------------------------------------ */

export interface HoldbackRow {
  word_id: number;
  lemma: string;
  role: WordRole;
  root_cause: string;
  root_cause_detail: string;
  blocking_word_id: number | null;
  blocking_lemma: string | null;
  impact_count: number;
}

const BLOCKER_LABELS: Record<string, string> = {
  missing_primary_sense: 'No sense is marked primary, so no question can be built.',
  sense_not_approved: 'An enabled sense is still waiting for approval.',
  missing_definition: 'No definition candidate has been selected.',
  oos_pending: 'The selected definition still contains an unresolved out-of-scope token.',
  missing_example: 'Slot 1 has no selected example sentence.',
  example_not_approved: 'The slot 1 example is selected but not approved.',
  missing_image: 'No image candidate is selected for this word.',
  image_not_approved: 'The live image is selected but not approved.',
  tts_missing: 'One or more selected texts have no synthesized audio.',
  tts_failed: 'A TTS synthesis for this word ended in a failed state.',
  distractors_unbound: 'Fewer than three distractors are bound.',
  distractor_1_not_ready: 'Bound distractor 1 is not core-ready.',
  distractor_2_not_ready: 'Bound distractor 2 is not core-ready.',
  distractor_3_not_ready: 'Bound distractor 3 is not core-ready.',
  not_in_plan: 'The word has no position in the current learning plan.',
};

/**
 * Dependency-closure pruning (README Part 5). Edges: selected-definition
 * dependencies (excluding base words) and the three bound distractors.
 */
export function computeHoldback(): {
  shippable: Set<number>;
  exportable: Set<number>;
  rows: HoldbackRow[];
} {
  db();
  const active = state.words.filter(
    (w) => w.role === 'target' || (w.role === 'auxiliary' && w.aux_status === 'active'),
  );
  const shippable = new Set<number>();
  const perWordBlockers = new Map<number, BlockerCode[]>();

  for (const word of active) {
    const readiness = computeReadiness(word.word_id);
    perWordBlockers.set(word.word_id, readiness.blockers);
    if (readiness.ready) shippable.add(word.word_id);
  }

  const edges = new Map<number, Set<number>>();
  const lemmaIndex = new Map(active.map((w) => [w.lemma.toLowerCase(), w.word_id] as const));
  for (const word of active) {
    const out = new Set<number>();
    for (const sel of state.defSels.filter((s) => s.word_id === word.word_id && s.enabled)) {
      const cand = state.defCands.find((c) => c.def_cand_id === sel.def_cand_id);
      if (!cand) continue;
      for (const token of cand.text.toLowerCase().split(/[^a-z]+/)) {
        const dep = lemmaIndex.get(token);
        if (dep !== undefined && dep !== word.word_id) out.add(dep);
      }
    }
    for (const link of state.distractors.filter((d) => d.word_id === word.word_id)) {
      out.add(link.distractor_word_id);
    }
    edges.set(word.word_id, out);
  }

  const exportable = new Set(shippable);
  let changed = true;
  while (changed) {
    changed = false;
    for (const wordId of [...exportable]) {
      for (const dep of edges.get(wordId) ?? []) {
        if (!exportable.has(dep)) {
          exportable.delete(wordId);
          changed = true;
          break;
        }
      }
    }
  }

  // Reverse edges → how many exportable-eligible words each excluded word blocks.
  const reverse = new Map<number, Set<number>>();
  for (const [from, tos] of edges) {
    for (const to of tos) {
      if (!reverse.has(to)) reverse.set(to, new Set());
      reverse.get(to)!.add(from);
    }
  }

  const impact = (rootId: number): number => {
    const seen = new Set<number>();
    const stack = [rootId];
    while (stack.length > 0) {
      const current = stack.pop()!;
      for (const upstream of reverse.get(current) ?? []) {
        if (seen.has(upstream) || upstream === rootId) continue;
        if (exportable.has(upstream)) continue;
        seen.add(upstream);
        stack.push(upstream);
      }
    }
    return seen.size;
  };

  const rows: HoldbackRow[] = [];
  for (const word of active) {
    if (exportable.has(word.word_id)) continue;
    const blockers = perWordBlockers.get(word.word_id) ?? [];
    if (blockers.length > 0) {
      const root = blockers[0]!;
      rows.push({
        word_id: word.word_id,
        lemma: word.lemma,
        role: word.role,
        root_cause: root,
        root_cause_detail: BLOCKER_LABELS[root] ?? root,
        blocking_word_id: null,
        blocking_lemma: null,
        impact_count: impact(word.word_id),
      });
      continue;
    }
    // Passes every per-word gate but got pulled out by the closure fixpoint.
    let culprit: number | null = null;
    for (const dep of edges.get(word.word_id) ?? []) {
      if (!exportable.has(dep)) {
        culprit = dep;
        break;
      }
    }
    const culpritWord = culprit === null ? null : state.words.find((w) => w.word_id === culprit);
    rows.push({
      word_id: word.word_id,
      lemma: word.lemma,
      role: word.role,
      root_cause: 'dependency_holdback',
      root_cause_detail: culpritWord
        ? `Passes every gate, but depends on "${culpritWord.lemma}", which is held back.`
        : 'Removed by the dependency-closure fixpoint.',
      blocking_word_id: culprit,
      blocking_lemma: culpritWord?.lemma ?? null,
      impact_count: impact(word.word_id),
    });
  }

  rows.sort((a, b) => b.impact_count - a.impact_count || a.lemma.localeCompare(b.lemma));
  return { shippable, exportable, rows };
}

export interface GateFailure {
  gate: string;
  message: string;
  word_id: number | null;
  lemma: string | null;
}

/** Hard export gates from README Part 5; all must pass before bytes are written. */
export function computeGateFailures(exportable: Set<number>): GateFailure[] {
  db();
  const failures: GateFailure[] = [];

  if (exportable.size === 0) {
    failures.push({
      gate: 'non_empty_release',
      message: 'The dependency-closed subset is empty; there is nothing to export.',
      word_id: null,
      lemma: null,
    });
  }

  const openOos = state.oos.filter((row) => row.status === 'open');
  if (openOos.length > 0) {
    failures.push({
      gate: 'no_open_oos',
      message: `${openOos.length} out-of-scope lemma(s) are still open: ${openOos
        .map((r) => r.oos_lemma)
        .join(', ')}.`,
      word_id: null,
      lemma: null,
    });
  }

  const deadCount = state.jobs.filter((j) => j.status === 'dead').length;
  if (deadCount > 0) {
    failures.push({
      gate: 'no_dead_letters',
      message: `${deadCount} dead-letter job(s) must be retried or waived before an export.`,
      word_id: null,
      lemma: null,
    });
  }

  for (const wordId of exportable) {
    for (const link of state.distractors.filter((d) => d.word_id === wordId)) {
      if (!exportable.has(link.distractor_word_id)) {
        const word = state.words.find((w) => w.word_id === wordId);
        failures.push({
          gate: 'distractors_resolve',
          message: `Distractor ${link.rank} of "${word?.lemma ?? wordId}" is not in the export set.`,
          word_id: wordId,
          lemma: word?.lemma ?? null,
        });
      }
    }
  }

  return failures;
}

export function insertRelease(
  exportable: Set<number>,
  actor: string,
  notes: string | null,
): ReleaseRow {
  const planId = state.plans.find((p) => p.is_current)?.plan_id ?? 1;
  const mediaHashes = new Set<string>();
  for (const wordId of exportable) {
    const img = state.imgSels.find((s) => s.word_id === wordId);
    if (img) {
      const cand = state.imgCands.find((c) => c.img_cand_id === img.img_cand_id);
      if (cand) mediaHashes.add(cand.file_hash);
    }
    for (const view of ttsTextsOf(wordId)) {
      const asset = ttsFor(view.text, view.kind);
      if (asset?.file_hash) mediaHashes.add(asset.file_hash);
    }
  }
  const totalBytes = [...mediaHashes].reduce(
    (sum, hash) => sum + (state.media.find((m) => m.file_hash === hash)?.bytes ?? 0),
    0,
  );
  const inputHash = fakeHash(`export:${planId}:${[...exportable].sort((a, b) => a - b).join(',')}`);
  const date = new Date();
  const version = `${date.getUTCFullYear()}.${String(date.getUTCMonth() + 1).padStart(2, '0')}.${String(
    date.getUTCDate(),
  ).padStart(2, '0')}+${inputHash.slice(0, 8)}`;

  const row: ReleaseRow = {
    release_id: nextId('release'),
    version,
    plan_id: planId,
    input_hash: inputHash,
    db_file_hash: fakeHash(`releasedb:${version}`),
    exported_at: nowIso(),
    exported_by: actor,
    notes,
    word_count: exportable.size,
    media_count: mediaHashes.size,
    total_bytes: totalBytes + 10_485_760,
  };
  state.releases.push(row);
  return row;
}
