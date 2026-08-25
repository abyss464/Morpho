/**
 * MSW v2 handlers implementing `docs/contracts/admin-api.md` verbatim against
 * the stateful fixture DB. Response shapes here and the types in `src/api/types.ts`
 * are two views of the same contract; if one changes without the other, the
 * typecheck breaks, which is the point.
 */

import { HttpResponse, delay, http } from 'msw';
import type {
  AdminEvent,
  AssetKind,
  DashboardResponse,
  DeadLetter,
  DefinitionSlotView,
  DistractorView,
  ExampleSlotNumber,
  ExampleSlotView,
  HoldbackReport,
  ImageSlotView,
  JobsSnapshot,
  OovQueueEntry,
  PlanGroupDetail,
  PlanSummary,
  Pos,
  TtsStatusView,
  Word,
  WordDetail,
  WordListItem,
} from '../api/types';
import {
  clearJob,
  computeCoreReady,
  computeGateFailures,
  computeHoldback,
  computeReadiness,
  db,
  ensureTtsForWord,
  findWordByLemma,
  insertRelease,
  insertWord,
  mintDefinitionCandidate,
  mintExampleCandidate,
  mintImageCandidate,
  oosOccurrences,
  overrideSelection,
  recordEvent,
  rejectCandidate,
  resolveOosPromote,
  resolveOosRewrite,
  setApproval,
  setPrimarySense,
  setSenseEnabled,
  syncOosQueue,
  ttsFor,
  ttsTextsOf,
  waiveJob,
  type DefCandRow,
  type ExCandRow,
  type ImgCandRow,
  type WordRow,
} from './db';
import { placeholderImageSvg, silentOggBytes } from './fixtures/media';

const BASE = '/api';

/** Small latency so skeletons and optimistic updates are actually observable. */
const LATENCY_MS = 180;

function actorOf(request: Request): string {
  return `admin:${request.headers.get('X-Admin-User') ?? 'admin'}`;
}

function errorResponse(
  status: number,
  code: string,
  message: string,
  extra?: Record<string, unknown>,
) {
  return HttpResponse.json({ error: { code, message }, ...extra }, { status });
}

function paginate<T>(items: T[], page: number, pageSize: number) {
  const start = (page - 1) * pageSize;
  return { items: items.slice(start, start + pageSize), total: items.length };
}

function readPage(url: URL): { page: number; pageSize: number } {
  const page = Math.max(1, Number(url.searchParams.get('page') ?? '1') || 1);
  const pageSize = Math.min(
    200,
    Math.max(1, Number(url.searchParams.get('page_size') ?? '50') || 50),
  );
  return { page, pageSize };
}

/* ------------------------------------------------------------------ */
/* Projections                                                         */
/* ------------------------------------------------------------------ */

function toWord(row: WordRow): Word {
  const readiness = computeReadiness(row.word_id);
  return {
    word_id: row.word_id,
    lemma: row.lemma,
    role: row.role,
    aux_status: row.aux_status,
    phonetic: row.phonetic,
    frequency_rank: row.frequency_rank,
    etymology: row.etymology,
    etymology_source: row.etymology_source,
    ready: readiness.ready,
    blockers: readiness.blockers,
    created_by: row.created_by,
    created_at: row.created_at,
  };
}

function toWordListItem(row: WordRow): WordListItem {
  const s = db();
  const readiness = computeReadiness(row.word_id);
  const senseCount = s.defSels.filter((sel) => sel.word_id === row.word_id && sel.enabled).length;
  const exampleCount = s.exSels.filter((sel) => sel.word_id === row.word_id).length;
  const ttsMissing = ttsTextsOf(row.word_id).filter((view) => {
    const asset = ttsFor(view.text, view.kind);
    return !asset || asset.status !== 'ready';
  }).length;
  return {
    word_id: row.word_id,
    lemma: row.lemma,
    role: row.role,
    ready: readiness.ready,
    blockers: readiness.blockers,
    has_image: s.imgSels.some((sel) => sel.word_id === row.word_id),
    sense_count: senseCount,
    example_count: exampleCount,
    tts_missing: ttsMissing,
  };
}

const stripDefCand = (row: DefCandRow) => ({ ...row });
const stripExCand = (row: ExCandRow) => ({ ...row });
const stripImgCand = (row: ImgCandRow) => ({ ...row });

function buildDefinitionSlots(wordId: number): DefinitionSlotView[] {
  const s = db();
  const posList: Pos[] = [];
  for (const cand of s.defCands.filter((c) => c.word_id === wordId)) {
    if (!posList.includes(cand.pos)) posList.push(cand.pos);
  }
  for (const sel of s.defSels.filter((sel) => sel.word_id === wordId)) {
    if (!posList.includes(sel.pos)) posList.push(sel.pos);
  }
  const slots = posList.map<DefinitionSlotView>((pos) => {
    const selection = s.defSels.find((sel) => sel.word_id === wordId && sel.pos === pos) ?? null;
    return {
      pos,
      selection,
      candidates: s.defCands
        .filter((c) => c.word_id === wordId && c.pos === pos)
        .sort((a, b) => b.auto_score - a.auto_score)
        .map(stripDefCand),
    };
  });
  // Primary sense first — the console always leads with the sense that drives
  // every question type.
  return slots.sort(
    (a, b) => Number(b.selection?.is_primary ?? 0) - Number(a.selection?.is_primary ?? 0),
  );
}

function buildExampleSlots(wordId: number): ExampleSlotView[] {
  const s = db();
  const candidates = s.exCands
    .filter((c) => c.word_id === wordId)
    .sort((a, b) => b.auto_score - a.auto_score)
    .map(stripExCand);
  return ([1, 2, 3] as ExampleSlotNumber[]).map((slot) => ({
    slot,
    selection: s.exSels.find((sel) => sel.word_id === wordId && sel.slot === slot) ?? null,
    candidates,
  }));
}

function buildImageSlot(wordId: number): ImageSlotView {
  const s = db();
  return {
    selection: s.imgSels.find((sel) => sel.word_id === wordId) ?? null,
    candidates: s.imgCands
      .filter((c) => c.word_id === wordId)
      .sort((a, b) => b.auto_score - a.auto_score)
      .map(stripImgCand),
  };
}

function buildTtsViews(wordId: number): TtsStatusView[] {
  return ttsTextsOf(wordId).map<TtsStatusView>((view) => {
    const asset = ttsFor(view.text, view.kind);
    return {
      kind: view.kind,
      text: view.text,
      text_hash: asset?.text_hash ?? '',
      input_hash: asset?.input_hash ?? '',
      voice: asset?.voice ?? 'en-GB-SoniaNeural',
      engine: asset?.engine ?? 'edge-tts',
      engine_ver: asset?.engine_ver ?? '6.1.12',
      status: asset ? asset.status : 'missing',
      file_hash: asset?.file_hash ?? null,
      duration_ms: asset?.duration_ms ?? null,
      ref: view.ref,
      last_error: asset?.last_error ?? null,
    };
  });
}

function buildDistractors(wordId: number): DistractorView[] {
  const s = db();
  return s.distractors
    .filter((d) => d.word_id === wordId)
    .sort((a, b) => a.rank - b.rank)
    .map<DistractorView>((link) => {
      const target = s.words.find((w) => w.word_id === link.distractor_word_id);
      const core = computeCoreReady(link.distractor_word_id);
      return {
        rank: link.rank,
        word_id: link.distractor_word_id,
        lemma: target?.lemma ?? `#${link.distractor_word_id}`,
        core_ready: core.core_ready,
        blockers: core.blockers,
        bound_at: link.bound_at,
        bound_by: link.bound_by,
      };
    });
}

function eventsFor(entityType: string | null, entityId: string | null): AdminEvent[] {
  const s = db();
  return s.events.filter(
    (e) => (!entityType || e.entity_type === entityType) && (!entityId || e.entity_id === entityId),
  );
}

function wordEvents(wordId: number, limit = 25): AdminEvent[] {
  const s = db();
  const id = String(wordId);
  return s.events
    .filter((e) => e.entity_id === id || e.entity_id.startsWith(`${id}:`))
    .slice(0, limit);
}

function buildWordDetail(wordId: number): WordDetail | null {
  const s = db();
  const row = s.words.find((w) => w.word_id === wordId);
  if (!row) return null;
  return {
    word: toWord(row),
    definitions: buildDefinitionSlots(wordId),
    examples: buildExampleSlots(wordId),
    image: buildImageSlot(wordId),
    tts: buildTtsViews(wordId),
    distractors: buildDistractors(wordId),
    recent_events: wordEvents(wordId),
  };
}

function wordDetailResponse(wordId: number) {
  syncOosQueue();
  const detail = buildWordDetail(wordId);
  if (!detail) return errorResponse(404, 'word_not_found', `No word with id ${wordId}.`);
  return HttpResponse.json(detail);
}

/* ------------------------------------------------------------------ */
/* Handlers                                                            */
/* ------------------------------------------------------------------ */

export const handlers = [
  /* ---------------- Dashboard & observability ---------------- */

  http.get(`${BASE}/dashboard`, async () => {
    await delay(LATENCY_MS);
    const s = db();
    const active = s.words.filter(
      (w) => w.role === 'target' || (w.role === 'auxiliary' && w.aux_status === 'active'),
    );
    let ready = 0;
    for (const word of active) if (computeReadiness(word.word_id).ready) ready += 1;

    const defReady = s.defSels.filter((sel) => sel.enabled && sel.approved).length;
    const defPending = s.defSels.filter((sel) => sel.enabled && !sel.approved).length;
    const defMissing = active.filter(
      (w) => !s.defSels.some((sel) => sel.word_id === w.word_id),
    ).length;

    const exReady = s.exSels.filter((sel) => sel.slot === 1 && sel.approved).length;
    const exPending = s.exSels.filter((sel) => sel.slot === 1 && !sel.approved).length;
    const exMissing = active.filter(
      (w) => !s.exSels.some((sel) => sel.word_id === w.word_id && sel.slot === 1),
    ).length;

    const imgReady = s.imgSels.filter((sel) => sel.approved).length;
    const imgPending = s.imgSels.filter((sel) => !sel.approved).length;
    const imgMissing = active.filter(
      (w) => !s.imgSels.some((sel) => sel.word_id === w.word_id),
    ).length;

    let ttsReady = 0;
    let ttsMissing = 0;
    let ttsFailed = 0;
    for (const word of active) {
      for (const view of ttsTextsOf(word.word_id)) {
        const asset = ttsFor(view.text, view.kind);
        if (!asset) ttsMissing += 1;
        else if (asset.status === 'failed') ttsFailed += 1;
        else ttsReady += 1;
      }
    }

    const currentPlan = s.plans.find((p) => p.is_current) ?? null;

    const body: DashboardResponse = {
      words: {
        total: s.words.length,
        target: s.words.filter((w) => w.role === 'target').length,
        auxiliary: s.words.filter((w) => w.role === 'auxiliary').length,
        ready,
        blocked: active.length - ready,
      },
      assets: {
        definitions: { ready: defReady, missing: defMissing, failed: defPending },
        examples: { ready: exReady, missing: exMissing, failed: exPending },
        images: { ready: imgReady, missing: imgMissing, failed: imgPending },
        tts: { ready: ttsReady, missing: ttsMissing, failed: ttsFailed },
      },
      oos_open: s.oos.filter((row) => row.status === 'open').length,
      dead_letters: s.jobs.filter((row) => row.status === 'dead').length,
      plan: currentPlan
        ? {
            plan_id: currentPlan.plan_id,
            built_at: currentPlan.built_at,
            group_count: s.planGroups.filter((g) => g.plan_id === currentPlan.plan_id).length,
          }
        : null,
      recent_events: s.events.slice(0, 20),
    };
    return HttpResponse.json(body);
  }),

  http.get(`${BASE}/events`, async ({ request }) => {
    await delay(LATENCY_MS);
    const url = new URL(request.url);
    const { page, pageSize } = readPage(url);
    const filtered = eventsFor(
      url.searchParams.get('entity_type'),
      url.searchParams.get('entity_id'),
    );
    return HttpResponse.json(paginate(filtered, page, pageSize));
  }),

  http.get(`${BASE}/jobs`, async () => {
    await delay(LATENCY_MS);
    const s = db();
    const label = (subjectId: string) =>
      s.words.find((w) => String(w.word_id) === subjectId)?.lemma ?? subjectId;

    const backoff = s.jobs
      .filter((j) => j.status === 'backoff')
      .map((j) => ({ ...j, subject_label: label(j.subject_id) }));

    const inFlight = s.words.slice(0, 4).map((word, index) => ({
      kind: ['fetch_definitions', 'fetch_images', 'synth_tts', 'extract_tokens'][index % 4]!,
      subject_type: 'word' as const,
      subject_id: String(word.word_id),
      rate_key: ['freedict', 'unsplash', 'edge_tts', 'cpu'][index % 4]!,
      status: null,
      attempts: 0,
      next_retry_at: null,
      last_error: null,
      subject_label: word.lemma,
    }));

    const body: JobsSnapshot = {
      in_flight: inFlight,
      backoff,
      lanes: {
        freedict: { queued: 3, running: 1, limit: 4 },
        wiktionary: { queued: 0, running: 0, limit: 2 },
        unsplash: { queued: 11, running: 2, limit: 2 },
        pexels: { queued: 6, running: 1, limit: 2 },
        pixabay: { queued: 4, running: 0, limit: 2 },
        sdxl: { queued: 2, running: 1, limit: 1 },
        edge_tts: { queued: 18, running: 3, limit: 5 },
        llm: { queued: 1, running: 0, limit: 4 },
        cpu: { queued: 0, running: 2, limit: 8 },
      },
    };
    return HttpResponse.json(body);
  }),

  // SSE is stubbed: the stream opens and stays quiet. Wave 1 refreshes through
  // TanStack Query invalidation, so no ChangeEvent frames are required yet.
  http.get(`${BASE}/stream`, () => {
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode(': morpho change stream (mock, no-op)\n\n'));
      },
    });
    return new HttpResponse(stream, {
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        Connection: 'keep-alive',
      },
    });
  }),

  /* ---------------- Words ---------------- */

  http.get(`${BASE}/words`, async ({ request }) => {
    await delay(LATENCY_MS);
    const s = db();
    const url = new URL(request.url);
    const { page, pageSize } = readPage(url);
    const role = url.searchParams.get('role');
    const readyParam = url.searchParams.get('ready');
    const blocker = url.searchParams.get('blocker');
    const group = url.searchParams.get('group');
    const q = (url.searchParams.get('q') ?? '').trim().toLowerCase();

    const currentPlanId = s.plans.find((p) => p.is_current)?.plan_id;
    const groupIds =
      group && currentPlanId
        ? new Set(
            s.planWords
              .filter((pw) => pw.plan_id === currentPlanId && pw.group_seq === Number(group))
              .map((pw) => pw.word_id),
          )
        : null;

    const rows = s.words
      .filter((w) => (role ? w.role === role : true))
      .filter((w) => (q ? w.lemma.toLowerCase().includes(q) : true))
      .filter((w) => (groupIds ? groupIds.has(w.word_id) : true))
      .map(toWordListItem)
      .filter((item) => {
        if (readyParam === null || readyParam === '') return true;
        const wantReady = readyParam === 'true' || readyParam === '1';
        return item.ready === wantReady;
      })
      .filter((item) => (blocker ? item.blockers.includes(blocker) : true))
      .sort((a, b) => a.lemma.localeCompare(b.lemma));

    return HttpResponse.json(paginate(rows, page, pageSize));
  }),

  http.get(`${BASE}/words/:id`, async ({ params }) => {
    await delay(LATENCY_MS);
    return wordDetailResponse(Number(params.id));
  }),

  http.post(`${BASE}/words`, async ({ request }) => {
    await delay(LATENCY_MS);
    const body = (await request.json()) as { lemma?: string; role?: string };
    const lemma = (body.lemma ?? '').trim();
    if (!lemma) return errorResponse(400, 'invalid_body', 'lemma is required.');
    if (findWordByLemma(lemma)) {
      return errorResponse(409, 'lemma_exists', `"${lemma}" already exists in the lexicon.`);
    }
    const role = (body.role ?? 'target') as WordRow['role'];
    const row = insertWord(lemma, role, 'manual');
    recordEvent(actorOf(request), 'word', row.word_id, 'word_created', { lemma, role });
    return HttpResponse.json(toWord(row), { status: 201 });
  }),

  /* ---------------- Candidates ---------------- */

  http.post(`${BASE}/candidates/definition`, async ({ request }) => {
    await delay(LATENCY_MS);
    const body = (await request.json()) as {
      word_id?: number;
      pos?: Pos;
      text?: string;
      parent_cand_id?: number;
    };
    if (!body.word_id || !body.pos || !body.text?.trim()) {
      return errorResponse(400, 'invalid_body', 'word_id, pos and text are required.');
    }
    const row = mintDefinitionCandidate(
      body.word_id,
      body.pos,
      body.text,
      body.parent_cand_id ?? null,
    );
    recordEvent(actorOf(request), 'def_candidate', row.def_cand_id, 'candidate_added', {
      word_id: body.word_id,
      pos: body.pos,
      source: 'manual',
    });
    return wordDetailResponse(body.word_id);
  }),

  http.post(`${BASE}/candidates/example`, async ({ request }) => {
    await delay(LATENCY_MS);
    const body = (await request.json()) as {
      word_id?: number;
      text?: string;
      hl_start?: number;
      hl_end?: number;
    };
    if (!body.word_id || !body.text?.trim()) {
      return errorResponse(400, 'invalid_body', 'word_id and text are required.');
    }
    if (
      typeof body.hl_start !== 'number' ||
      typeof body.hl_end !== 'number' ||
      body.hl_end <= body.hl_start ||
      body.hl_end > body.text.length
    ) {
      return errorResponse(
        400,
        'invalid_highlight',
        'hl_start/hl_end must be a valid range in text.',
      );
    }
    const row = mintExampleCandidate(body.word_id, body.text, body.hl_start, body.hl_end);
    recordEvent(actorOf(request), 'ex_candidate', row.ex_cand_id, 'candidate_added', {
      word_id: body.word_id,
      source: 'manual',
    });
    return wordDetailResponse(body.word_id);
  }),

  http.post(`${BASE}/candidates/image`, async ({ request }) => {
    await delay(LATENCY_MS * 2);
    const form = await request.formData();
    const wordId = Number(form.get('word_id'));
    const file = form.get('file');
    if (!wordId || !(file instanceof File)) {
      return errorResponse(400, 'invalid_body', 'word_id and file are required.');
    }
    if (!file.type.startsWith('image/')) {
      return errorResponse(
        415,
        'unsupported_media_type',
        `${file.type || 'unknown'} is not an image.`,
      );
    }
    const row = mintImageCandidate(wordId, file.name, file.size);
    recordEvent(actorOf(request), 'img_candidate', row.img_cand_id, 'candidate_added', {
      word_id: wordId,
      source: 'manual',
      file_hash: row.file_hash,
    });
    return wordDetailResponse(wordId);
  }),

  http.post(`${BASE}/candidates/:kind/:candId/reject`, async ({ params, request }) => {
    await delay(LATENCY_MS);
    const kind = params.kind as AssetKind;
    if (!['definition', 'example', 'image'].includes(kind)) {
      return errorResponse(400, 'invalid_kind', `Unknown candidate kind "${kind}".`);
    }
    const wordId = rejectCandidate(kind, Number(params.candId));
    if (wordId === null) {
      return errorResponse(404, 'candidate_not_found', `No ${kind} candidate ${params.candId}.`);
    }
    recordEvent(
      actorOf(request),
      `${kind}_candidate`,
      String(params.candId),
      'candidate_rejected',
      {
        word_id: wordId,
      },
    );
    return wordDetailResponse(wordId);
  }),

  /* ---------------- Selections ---------------- */

  http.post(`${BASE}/selections/definition/primary`, async ({ request }) => {
    await delay(LATENCY_MS);
    const body = (await request.json()) as { word_id?: number; pos?: Pos };
    if (!body.word_id || !body.pos) {
      return errorResponse(400, 'invalid_body', 'word_id and pos are required.');
    }
    if (!setPrimarySense(body.word_id, body.pos)) {
      return errorResponse(404, 'selection_not_found', 'No definition selection for that pos.');
    }
    recordEvent(actorOf(request), 'selection', `${body.word_id}:${body.pos}`, 'primary_moved', {
      pos: body.pos,
    });
    return wordDetailResponse(body.word_id);
  }),

  http.post(`${BASE}/selections/definition/enabled`, async ({ request }) => {
    await delay(LATENCY_MS);
    const body = (await request.json()) as { word_id?: number; pos?: Pos; enabled?: boolean };
    if (!body.word_id || !body.pos || typeof body.enabled !== 'boolean') {
      return errorResponse(400, 'invalid_body', 'word_id, pos and enabled are required.');
    }
    if (!setSenseEnabled(body.word_id, body.pos, body.enabled)) {
      return errorResponse(404, 'selection_not_found', 'No definition selection for that pos.');
    }
    recordEvent(
      actorOf(request),
      'selection',
      `${body.word_id}:${body.pos}`,
      'sense_enabled_changed',
      {
        pos: body.pos,
        enabled: body.enabled,
      },
    );
    return wordDetailResponse(body.word_id);
  }),

  http.post(`${BASE}/selections/:kind/approve`, async ({ params, request }) => {
    await delay(LATENCY_MS);
    const kind = params.kind as AssetKind;
    const body = (await request.json()) as {
      word_id?: number;
      pos?: Pos;
      slot?: ExampleSlotNumber;
    };
    if (!body.word_id) return errorResponse(400, 'invalid_body', 'word_id is required.');
    const actor = actorOf(request);
    if (!setApproval(kind, body.word_id, true, actor, body.pos, body.slot)) {
      return errorResponse(404, 'selection_not_found', 'Nothing selected in that slot to approve.');
    }
    recordEvent(
      actor,
      'selection',
      `${body.word_id}:${body.pos ?? body.slot ?? kind}`,
      'approved',
      {
        kind,
        pos: body.pos ?? null,
        slot: body.slot ?? null,
      },
    );
    return wordDetailResponse(body.word_id);
  }),

  http.delete(`${BASE}/selections/:kind/approve`, async ({ params, request }) => {
    await delay(LATENCY_MS);
    const kind = params.kind as AssetKind;
    const body = (await request.json()) as {
      word_id?: number;
      pos?: Pos;
      slot?: ExampleSlotNumber;
    };
    if (!body.word_id) return errorResponse(400, 'invalid_body', 'word_id is required.');
    const actor = actorOf(request);
    if (!setApproval(kind, body.word_id, false, actor, body.pos, body.slot)) {
      return errorResponse(
        404,
        'selection_not_found',
        'Nothing selected in that slot to un-approve.',
      );
    }
    recordEvent(
      actor,
      'selection',
      `${body.word_id}:${body.pos ?? body.slot ?? kind}`,
      'approval_invalidated',
      { kind, reason: 'admin_unapprove' },
    );
    return wordDetailResponse(body.word_id);
  }),

  http.post(`${BASE}/selections/:kind`, async ({ params, request }) => {
    await delay(LATENCY_MS);
    const kind = params.kind as AssetKind;
    if (!['definition', 'example', 'image'].includes(kind)) {
      return errorResponse(400, 'invalid_kind', `Unknown selection kind "${kind}".`);
    }
    const body = (await request.json()) as {
      word_id?: number;
      cand_id?: number;
      pos?: Pos;
      slot?: ExampleSlotNumber;
    };
    if (!body.word_id || !body.cand_id) {
      return errorResponse(400, 'invalid_body', 'word_id and cand_id are required.');
    }
    overrideSelection(kind, body.word_id, body.cand_id, body.pos, body.slot);
    ensureTtsForWord(body.word_id);
    recordEvent(
      actorOf(request),
      'selection',
      `${body.word_id}:${body.pos ?? body.slot ?? kind}`,
      'selection_changed',
      { kind, cand_id: body.cand_id, selected_by: 'human', pinned: true },
    );
    return wordDetailResponse(body.word_id);
  }),

  /* ---------------- OOV queue ---------------- */

  http.get(`${BASE}/oov`, async ({ request }) => {
    await delay(LATENCY_MS);
    syncOosQueue();
    const s = db();
    const url = new URL(request.url);
    const { page, pageSize } = readPage(url);
    const status = url.searchParams.get('status');

    const rows = s.oos
      .filter((row) => (status ? row.status === status : true))
      .map<OovQueueEntry>((row) => {
        const occurrences = oosOccurrences(row.oos_lemma);
        return {
          oos_lemma: row.oos_lemma,
          status: row.status,
          first_seen: row.first_seen,
          resolved_by: row.resolved_by,
          resolved_at: row.resolved_at,
          notes: row.notes,
          occurrences,
          occurrence_count: occurrences.length,
        };
      })
      .sort(
        (a, b) => b.occurrence_count - a.occurrence_count || a.oos_lemma.localeCompare(b.oos_lemma),
      );

    return HttpResponse.json(paginate(rows, page, pageSize));
  }),

  http.post(`${BASE}/oov/:lemma/resolve`, async ({ params, request }) => {
    await delay(LATENCY_MS);
    const lemma = decodeURIComponent(String(params.lemma));
    const body = (await request.json()) as {
      mode?: 'promote' | 'rewrite';
      def_cand_id?: number;
      text?: string;
      notes?: string;
    };
    const actor = actorOf(request);

    if (body.mode === 'promote') {
      const row = resolveOosPromote(lemma, actor);
      if (body.notes) row.notes = body.notes;
      recordEvent(actor, 'oos_queue', lemma, 'oos_resolved', { mode: 'promote' });
      const occurrences = oosOccurrences(lemma);
      return HttpResponse.json({ ...row, occurrences, occurrence_count: occurrences.length });
    }

    if (body.mode === 'rewrite') {
      if (!body.def_cand_id || !body.text?.trim()) {
        return errorResponse(
          400,
          'invalid_body',
          'def_cand_id and text are required for a rewrite.',
        );
      }
      if (new RegExp(`\\b${lemma}\\b`, 'i').test(body.text)) {
        return errorResponse(
          422,
          'rewrite_still_out_of_scope',
          `The rewrite still contains the out-of-scope token "${lemma}".`,
        );
      }
      const row = resolveOosRewrite(lemma, body.def_cand_id, body.text, actor);
      if (!row)
        return errorResponse(
          404,
          'candidate_not_found',
          `No definition candidate ${body.def_cand_id}.`,
        );
      if (body.notes) row.notes = body.notes;
      recordEvent(actor, 'oos_queue', lemma, 'oos_resolved', {
        mode: 'rewrite',
        def_cand_id: body.def_cand_id,
      });
      const occurrences = oosOccurrences(lemma);
      return HttpResponse.json({ ...row, occurrences, occurrence_count: occurrences.length });
    }

    return errorResponse(400, 'invalid_mode', 'mode must be "promote" or "rewrite".');
  }),

  /* ---------------- Dead letters ---------------- */

  http.get(`${BASE}/dead-letters`, async () => {
    await delay(LATENCY_MS);
    const s = db();
    const rows = s.jobs
      .filter((job) => job.status === 'dead')
      .map<DeadLetter>((job) => {
        const word = s.words.find((w) => String(w.word_id) === job.subject_id);
        return {
          ...job,
          subject: {
            word_id: word?.word_id ?? null,
            lemma: word?.lemma ?? null,
            label: word ? `${word.lemma} (${word.role})` : `${job.subject_type} ${job.subject_id}`,
          },
        };
      })
      .sort((a, b) => a.rate_key.localeCompare(b.rate_key) || a.kind.localeCompare(b.kind));
    return HttpResponse.json({ items: rows, total: rows.length });
  }),

  http.post(`${BASE}/dead-letters/retry`, async ({ request }) => {
    await delay(LATENCY_MS);
    const body = (await request.json()) as {
      kind?: string;
      subject_type?: DeadLetter['subject_type'];
      subject_id?: string;
    };
    if (!body.kind || !body.subject_type || !body.subject_id) {
      return errorResponse(400, 'invalid_body', 'kind, subject_type and subject_id are required.');
    }
    if (!clearJob(body.kind, body.subject_type, body.subject_id)) {
      return errorResponse(404, 'job_not_found', 'No job_state row with that key.');
    }
    recordEvent(actorOf(request), 'job', `${body.kind}:${body.subject_id}`, 'job_retry_requested', {
      kind: body.kind,
    });
    return listDeadLetters();
  }),

  http.post(`${BASE}/dead-letters/waive`, async ({ request }) => {
    await delay(LATENCY_MS);
    const body = (await request.json()) as {
      kind?: string;
      subject_type?: DeadLetter['subject_type'];
      subject_id?: string;
    };
    if (!body.kind || !body.subject_type || !body.subject_id) {
      return errorResponse(400, 'invalid_body', 'kind, subject_type and subject_id are required.');
    }
    if (!waiveJob(body.kind, body.subject_type, body.subject_id)) {
      return errorResponse(404, 'job_not_found', 'No job_state row with that key.');
    }
    recordEvent(actorOf(request), 'job', `${body.kind}:${body.subject_id}`, 'job_waived', {
      kind: body.kind,
    });
    return listDeadLetters();
  }),

  /* ---------------- Plan ---------------- */

  http.get(`${BASE}/plan`, async () => {
    await delay(LATENCY_MS);
    const s = db();
    const plan = s.plans.find((p) => p.is_current);
    if (!plan) return errorResponse(404, 'no_current_plan', 'No plan artifact is marked current.');

    const groups = s.planGroups
      .filter((g) => g.plan_id === plan.plan_id)
      .sort((a, b) => a.group_seq - b.group_seq)
      .map((group) => {
        const members = s.planWords
          .filter((pw) => pw.plan_id === plan.plan_id && pw.group_seq === group.group_seq)
          .sort((a, b) => a.learning_order - b.learning_order);
        const lemmas = members.map(
          (pw) => s.words.find((w) => w.word_id === pw.word_id)?.lemma ?? `#${pw.word_id}`,
        );
        return {
          group_seq: group.group_seq,
          group_type: group.group_type,
          word_count: members.length,
          ready_count: members.filter((pw) => computeReadiness(pw.word_id).ready).length,
          first_lemma: lemmas[0] ?? '',
          last_lemma: lemmas[lemmas.length - 1] ?? '',
        };
      });

    const planWords = s.planWords.filter((pw) => pw.plan_id === plan.plan_id);
    const previous = s.plans.filter((p) => p.plan_id !== plan.plan_id).at(-1) ?? null;
    const previousIds = previous
      ? new Set(s.planWords.filter((pw) => pw.plan_id === previous.plan_id).map((pw) => pw.word_id))
      : new Set<number>();
    const currentIds = new Set(planWords.map((pw) => pw.word_id));

    let edgeCount = 0;
    const lemmaIndex = new Map(s.words.map((w) => [w.lemma.toLowerCase(), w] as const));
    for (const sel of s.defSels.filter((sel) => sel.enabled)) {
      const cand = s.defCands.find((c) => c.def_cand_id === sel.def_cand_id);
      if (!cand) continue;
      const seen = new Set<number>();
      for (const token of cand.text.toLowerCase().split(/[^a-z]+/)) {
        const dep = lemmaIndex.get(token);
        if (!dep || dep.role === 'base' || dep.word_id === sel.word_id) continue;
        if (seen.has(dep.word_id)) continue;
        seen.add(dep.word_id);
        edgeCount += 1;
      }
    }

    const body: PlanSummary = {
      plan_id: plan.plan_id,
      input_hash: plan.input_hash,
      algo_ver: plan.algo_ver,
      params: plan.params,
      is_current: plan.is_current,
      built_at: plan.built_at,
      stats: {
        word_count: planWords.length,
        group_count: groups.length,
        edge_count: edgeCount,
        scc_group_count: groups.filter((g) => g.group_type === 'scc').length,
        largest_group: groups.reduce((max, g) => Math.max(max, g.word_count), 0),
        avg_group_size: groups.length ? Number((planWords.length / groups.length).toFixed(1)) : 0,
      },
      groups,
      diff: {
        previous_plan_id: previous?.plan_id ?? null,
        added: [...currentIds].filter((id) => !previousIds.has(id)).length,
        removed: [...previousIds].filter((id) => !currentIds.has(id)).length,
        reordered: previous ? 4 : 0,
      },
    };
    return HttpResponse.json(body);
  }),

  http.get(`${BASE}/plan/groups/:seq`, async ({ params }) => {
    await delay(LATENCY_MS);
    const s = db();
    const plan = s.plans.find((p) => p.is_current);
    if (!plan) return errorResponse(404, 'no_current_plan', 'No plan artifact is marked current.');
    const seq = Number(params.seq);
    const group = s.planGroups.find((g) => g.plan_id === plan.plan_id && g.group_seq === seq);
    if (!group)
      return errorResponse(404, 'group_not_found', `Group ${seq} is not in plan ${plan.plan_id}.`);

    const body: PlanGroupDetail = {
      plan_id: plan.plan_id,
      group_seq: seq,
      group_type: group.group_type,
      words: s.planWords
        .filter((pw) => pw.plan_id === plan.plan_id && pw.group_seq === seq)
        .sort((a, b) => a.learning_order - b.learning_order)
        .map((pw) => {
          const word = s.words.find((w) => w.word_id === pw.word_id);
          const readiness = computeReadiness(pw.word_id);
          return {
            word_id: pw.word_id,
            lemma: word?.lemma ?? `#${pw.word_id}`,
            role: word?.role ?? 'target',
            learning_order: pw.learning_order,
            group_seq: pw.group_seq,
            ready: readiness.ready,
            blockers: readiness.blockers,
          };
        }),
    };
    return HttpResponse.json(body);
  }),

  /* ---------------- Releases ---------------- */

  http.get(`${BASE}/releases`, async () => {
    await delay(LATENCY_MS);
    const s = db();
    const rows = [...s.releases].sort((a, b) => (a.exported_at < b.exported_at ? 1 : -1));
    return HttpResponse.json({ items: rows, total: rows.length });
  }),

  http.get(`${BASE}/releases/preview`, async () => {
    await delay(LATENCY_MS * 2);
    const s = db();
    const { shippable, exportable, rows } = computeHoldback();
    const failures = computeGateFailures(exportable);
    const body: HoldbackReport = {
      plan_id: s.plans.find((p) => p.is_current)?.plan_id ?? 0,
      shippable_count: shippable.size,
      exportable_count: exportable.size,
      excluded_count: rows.length,
      excluded: rows,
      gates_pass: failures.length === 0,
      gate_failures: failures,
    };
    return HttpResponse.json(body);
  }),

  http.post(`${BASE}/releases/export`, async ({ request }) => {
    await delay(LATENCY_MS * 3);
    const body = (await request.json().catch(() => ({}))) as { notes?: string };
    const { exportable } = computeHoldback();
    const failures = computeGateFailures(exportable);
    if (failures.length > 0) {
      return errorResponse(
        409,
        'export_gates_failed',
        `${failures.length} validation gate(s) failed; nothing was written.`,
        { failures },
      );
    }
    const actor = actorOf(request);
    const release = insertRelease(exportable, actor, body.notes ?? null);
    recordEvent(actor, 'release', release.release_id, 'release_exported', {
      version: release.version,
      word_count: release.word_count,
    });
    return HttpResponse.json(release, { status: 201 });
  }),

  /* ---------------- Media ---------------- */

  http.get(`${BASE}/media/:hash`, async ({ params }) => {
    const s = db();
    const hash = String(params.hash);
    const file = s.media.find((m) => m.file_hash === hash);
    if (!file) return errorResponse(404, 'media_not_found', `No media file ${hash}.`);

    if (file.kind === 'audio') {
      const bytes = silentOggBytes();
      return new HttpResponse(bytes.slice().buffer as ArrayBuffer, {
        headers: {
          'Content-Type': 'audio/ogg',
          'Content-Length': String(bytes.byteLength),
          'Cache-Control': 'public, max-age=31536000, immutable',
        },
      });
    }

    return new HttpResponse(placeholderImageSvg(hash, file.label), {
      headers: {
        'Content-Type': 'image/svg+xml',
        'Cache-Control': 'public, max-age=31536000, immutable',
      },
    });
  }),
];

function listDeadLetters() {
  const s = db();
  const rows = s.jobs
    .filter((job) => job.status === 'dead')
    .map<DeadLetter>((job) => {
      const word = s.words.find((w) => String(w.word_id) === job.subject_id);
      return {
        ...job,
        subject: {
          word_id: word?.word_id ?? null,
          lemma: word?.lemma ?? null,
          label: word ? `${word.lemma} (${word.role})` : `${job.subject_type} ${job.subject_id}`,
        },
      };
    })
    .sort((a, b) => a.rate_key.localeCompare(b.rate_key) || a.kind.localeCompare(b.kind));
  return HttpResponse.json({ items: rows, total: rows.length });
}
