/**
 * One function per row of `docs/contracts/admin-api.md`. Nothing else in the app
 * calls `fetch` against `/api`. Every response goes through `mappers.ts` so the
 * rest of the UI only ever sees normalized domain types.
 */

import { request, type QueryValue } from './client';
import {
  mapDashboard,
  mapDeadLetter,
  mapEvent,
  mapHoldbackReport,
  mapJobsSnapshot,
  mapOovEntry,
  mapPaginated,
  mapPlanGroupDetail,
  mapPlanSummary,
  mapRelease,
  mapWord,
  mapWordDetail,
  mapWordListItem,
} from './mappers';
import type {
  AdminEvent,
  AssetKind,
  CreateWordBody,
  DashboardResponse,
  DeadLetter,
  EventsQuery,
  ExportBody,
  HoldbackReport,
  JobKeyBody,
  JobsSnapshot,
  MintDefinitionBody,
  MintExampleBody,
  OovQuery,
  OovQueueEntry,
  OovResolveBody,
  OverrideSelectionBody,
  Paginated,
  PlanGroupDetail,
  PlanSummary,
  Release,
  SelectionKeyBody,
  SetEnabledBody,
  SetPrimaryBody,
  UploadImageBody,
  Word,
  WordDetail,
  WordListItem,
  WordsQuery,
} from './types';

/** Widens a typed query interface into the flat map the client encodes. */
function toQuery<T extends object>(params: T): Record<string, QueryValue> {
  const out: Record<string, QueryValue> = {};
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null) continue;
    out[key] = value as QueryValue;
  }
  return out;
}

/* ------------------------------------------------------------------ */
/* Dashboard & observability                                           */
/* ------------------------------------------------------------------ */

/** GET /dashboard */
export async function getDashboard(signal?: AbortSignal): Promise<DashboardResponse> {
  return mapDashboard(await request<unknown>('/dashboard', { signal }));
}

/** GET /events?entity_type=&entity_id=&page= — audit log, newest first. */
export async function getEvents(
  query: EventsQuery = {},
  signal?: AbortSignal,
): Promise<Paginated<AdminEvent>> {
  const raw = await request<unknown>('/events', { query: toQuery(query), signal });
  return mapPaginated(raw, mapEvent);
}

/** GET /jobs — live queue snapshot from morphod memory. */
export async function getJobs(signal?: AbortSignal): Promise<JobsSnapshot> {
  return mapJobsSnapshot(await request<unknown>('/jobs', { signal }));
}

/** GET /stream — SSE endpoint URL for `ChangeEvent` frames. */
export const STREAM_PATH = '/api/stream';

/* ------------------------------------------------------------------ */
/* Words                                                               */
/* ------------------------------------------------------------------ */

/** GET /words?role=&ready=&blocker=&group=&q=&page= */
export async function listWords(
  query: WordsQuery = {},
  signal?: AbortSignal,
): Promise<Paginated<WordListItem>> {
  const raw = await request<unknown>('/words', { query: toQuery(query), signal });
  return mapPaginated(raw, mapWordListItem);
}

/** GET /words/{id} */
export async function getWord(wordId: number, signal?: AbortSignal): Promise<WordDetail> {
  return mapWordDetail(await request<unknown>(`/words/${wordId}`, { signal }));
}

/** POST /words */
export async function createWord(body: CreateWordBody): Promise<Word> {
  return mapWord(await request<unknown>('/words', { method: 'POST', json: body }));
}

/* ------------------------------------------------------------------ */
/* Candidates & selections                                             */
/* ------------------------------------------------------------------ */
/*
 * Every word-scoped mutation answers with the refreshed `WordDetail`. The
 * contract says "all mutations return the updated resource"; for this engine the
 * meaningful resource is the word, because one write cascades (rejecting a
 * candidate re-selects the slot, re-selecting invalidates approval, readiness
 * and blockers recompute). Returning the row alone would force an immediate
 * refetch every time.
 */

/** POST /candidates/definition */
export async function mintDefinitionCandidate(body: MintDefinitionBody): Promise<WordDetail> {
  return mapWordDetail(
    await request<unknown>('/candidates/definition', { method: 'POST', json: body }),
  );
}

/** POST /candidates/example */
export async function mintExampleCandidate(body: MintExampleBody): Promise<WordDetail> {
  return mapWordDetail(
    await request<unknown>('/candidates/example', { method: 'POST', json: body }),
  );
}

/** POST /candidates/image — multipart, stored content-addressed. */
export async function uploadImageCandidate({
  word_id,
  file,
}: UploadImageBody): Promise<WordDetail> {
  const formData = new FormData();
  formData.append('word_id', String(word_id));
  formData.append('file', file);
  return mapWordDetail(await request<unknown>('/candidates/image', { method: 'POST', formData }));
}

/** POST /candidates/{kind}/{cand_id}/reject */
export async function rejectCandidate(kind: AssetKind, candId: number): Promise<WordDetail> {
  return mapWordDetail(
    await request<unknown>(`/candidates/${kind}/${candId}/reject`, { method: 'POST' }),
  );
}

/** POST /selections/{kind} — override: selected_by=human, pinned=1. */
export async function overrideSelection(
  kind: AssetKind,
  body: OverrideSelectionBody,
): Promise<WordDetail> {
  return mapWordDetail(
    await request<unknown>(`/selections/${kind}`, { method: 'POST', json: body }),
  );
}

/** POST /selections/{kind}/approve */
export async function approveSelection(
  kind: AssetKind,
  body: SelectionKeyBody,
): Promise<WordDetail> {
  return mapWordDetail(
    await request<unknown>(`/selections/${kind}/approve`, { method: 'POST', json: body }),
  );
}

/** DELETE /selections/{kind}/approve */
export async function unapproveSelection(
  kind: AssetKind,
  body: SelectionKeyBody,
): Promise<WordDetail> {
  return mapWordDetail(
    await request<unknown>(`/selections/${kind}/approve`, { method: 'DELETE', json: body }),
  );
}

/** POST /selections/definition/primary */
export async function setPrimarySense(body: SetPrimaryBody): Promise<WordDetail> {
  return mapWordDetail(
    await request<unknown>('/selections/definition/primary', { method: 'POST', json: body }),
  );
}

/** POST /selections/definition/enabled */
export async function setSenseEnabled(body: SetEnabledBody): Promise<WordDetail> {
  return mapWordDetail(
    await request<unknown>('/selections/definition/enabled', { method: 'POST', json: body }),
  );
}

/* ------------------------------------------------------------------ */
/* OOV queue                                                           */
/* ------------------------------------------------------------------ */

/** GET /oov?status=open&page= */
export async function listOov(
  query: OovQuery = {},
  signal?: AbortSignal,
): Promise<Paginated<OovQueueEntry>> {
  const raw = await request<unknown>('/oov', { query: toQuery(query), signal });
  return mapPaginated(raw, mapOovEntry);
}

/** POST /oov/{lemma}/resolve */
export async function resolveOov(lemma: string, body: OovResolveBody): Promise<OovQueueEntry> {
  const raw = await request<unknown>(`/oov/${encodeURIComponent(lemma)}/resolve`, {
    method: 'POST',
    json: body,
  });
  return mapOovEntry(raw);
}

/* ------------------------------------------------------------------ */
/* Dead letters                                                        */
/* ------------------------------------------------------------------ */

/** GET /dead-letters */
export async function listDeadLetters(signal?: AbortSignal): Promise<Paginated<DeadLetter>> {
  const raw = await request<unknown>('/dead-letters', { signal });
  return mapPaginated(raw, mapDeadLetter);
}

/** POST /dead-letters/retry — deletes the job_state row; demand re-derives. */
export async function retryDeadLetter(body: JobKeyBody): Promise<Paginated<DeadLetter>> {
  const raw = await request<unknown>('/dead-letters/retry', { method: 'POST', json: body });
  return mapPaginated(raw, mapDeadLetter);
}

/** POST /dead-letters/waive — status=waived, which arms the fallback rules. */
export async function waiveDeadLetter(body: JobKeyBody): Promise<Paginated<DeadLetter>> {
  const raw = await request<unknown>('/dead-letters/waive', { method: 'POST', json: body });
  return mapPaginated(raw, mapDeadLetter);
}

/* ------------------------------------------------------------------ */
/* Plan & releases                                                     */
/* ------------------------------------------------------------------ */

/** GET /plan */
export async function getPlan(signal?: AbortSignal): Promise<PlanSummary> {
  return mapPlanSummary(await request<unknown>('/plan', { signal }));
}

/** GET /plan/groups/{seq} */
export async function getPlanGroup(seq: number, signal?: AbortSignal): Promise<PlanGroupDetail> {
  return mapPlanGroupDetail(await request<unknown>(`/plan/groups/${seq}`, { signal }));
}

/** GET /releases */
export async function listReleases(signal?: AbortSignal): Promise<Paginated<Release>> {
  const raw = await request<unknown>('/releases', { signal });
  return mapPaginated(raw, mapRelease);
}

/** GET /releases/preview — holdback report, sorted by downstream impact. */
export async function getReleasePreview(signal?: AbortSignal): Promise<HoldbackReport> {
  return mapHoldbackReport(await request<unknown>('/releases/preview', { signal }));
}

/** POST /releases/export — 409 with gate failures when validation fails. */
export async function exportRelease(body: ExportBody = {}): Promise<Release> {
  return mapRelease(await request<unknown>('/releases/export', { method: 'POST', json: body }));
}
