import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { App } from 'antd';
import * as api from '../api/endpoints';
import { qk } from '../api/queryKeys';
import { useLiveStream } from '../app/liveStreamContext';
import { errorMessage } from '../lib/errors';
import type {
  AssetKind,
  EventsQuery,
  ExampleSlotNumber,
  ExportBody,
  JobKeyBody,
  MintDefinitionBody,
  MintExampleBody,
  OovQuery,
  OovResolveBody,
  PageParams,
  Pos,
  WordDetail,
  WordsQuery,
} from '../api/types';

/* ------------------------------------------------------------------ */
/* Reads                                                               */
/* ------------------------------------------------------------------ */

/**
 * The change stream drives every refresh while it is connected. The interval is
 * a fallback for the window where it is not: mock mode (nothing ever changes
 * behind our back) leaves it off entirely, and a live console falls back to a
 * slow poll only while reconnecting.
 */
export function useDashboard() {
  const live = useLiveStream();
  const fallbackPolling = live.enabled && live.status !== 'open';
  return useQuery({
    queryKey: qk.dashboard(),
    queryFn: ({ signal }) => api.getDashboard(signal),
    refetchInterval: fallbackPolling ? 30_000 : false,
  });
}

export function useWordList(query: WordsQuery, enabled = true) {
  return useQuery({
    queryKey: qk.wordList(query),
    queryFn: ({ signal }) => api.listWords(query, signal),
    placeholderData: (previous) => previous,
    enabled,
  });
}

export function useWordDetail(wordId: number, enabled = true) {
  return useQuery({
    queryKey: qk.wordDetail(wordId),
    queryFn: ({ signal }) => api.getWord(wordId, signal),
    enabled: enabled && Number.isFinite(wordId) && wordId > 0,
  });
}

export function useOovList(query: OovQuery) {
  return useQuery({
    queryKey: qk.oovList(query),
    queryFn: ({ signal }) => api.listOov(query, signal),
    placeholderData: (previous) => previous,
  });
}

export function useDeadLetters(query: PageParams = {}) {
  return useQuery({
    queryKey: qk.deadLetterList(query),
    queryFn: ({ signal }) => api.listDeadLetters(query, signal),
    placeholderData: (previous) => previous,
  });
}

export function usePlan() {
  return useQuery({ queryKey: qk.plan(), queryFn: ({ signal }) => api.getPlan(signal) });
}

export function usePlanGroup(seq: number | null) {
  return useQuery({
    queryKey: qk.planGroup(seq ?? 0),
    queryFn: ({ signal }) => api.getPlanGroup(seq as number, signal),
    enabled: seq !== null,
  });
}

export function useReleases() {
  return useQuery({
    queryKey: qk.releaseList(),
    queryFn: ({ signal }) => api.listReleases(signal),
  });
}

export function useReleasePreview() {
  return useQuery({
    queryKey: qk.releasePreview(),
    queryFn: ({ signal }) => api.getReleasePreview(signal),
  });
}

export function useEvents(query: EventsQuery) {
  return useQuery({
    queryKey: qk.events(query),
    queryFn: ({ signal }) => api.getEvents(query, signal),
    placeholderData: (previous) => previous,
  });
}

/* ------------------------------------------------------------------ */
/* Mutation plumbing                                                   */
/* ------------------------------------------------------------------ */

/** One write cascades; invalidate every family that can observe the change. */
function invalidateAfterWordWrite(client: QueryClient, wordId: number): void {
  void client.invalidateQueries({ queryKey: qk.words() });
  void client.invalidateQueries({ queryKey: qk.dashboard() });
  void client.invalidateQueries({ queryKey: qk.oov() });
  void client.invalidateQueries({ queryKey: qk.releases() });
  void client.invalidateQueries({ queryKey: qk.plan() });
  void client.invalidateQueries({ queryKey: qk.events() });
  void client.invalidateQueries({ queryKey: qk.wordDetail(wordId) });
}

export interface SelectionKey {
  pos?: Pos;
  slot?: ExampleSlotNumber;
}

/**
 * Every word-scoped operation for one word, wired to the same cache slot.
 * Approve/un-approve update optimistically because the flip is local, cheap to
 * revert, and the server answers with the authoritative detail anyway.
 */
export function useWordMutations(wordId: number) {
  const client = useQueryClient();
  const { message } = App.useApp();
  const detailKey = qk.wordDetail(wordId);

  const onSuccessDetail = (detail: WordDetail, toast: string) => {
    client.setQueryData(detailKey, detail);
    invalidateAfterWordWrite(client, wordId);
    message.success(toast);
  };

  const onFailure = (error: unknown) => {
    void client.invalidateQueries({ queryKey: detailKey });
    message.error(errorMessage(error));
  };

  const approve = useMutation({
    mutationFn: ({ kind, key }: { kind: AssetKind; key: SelectionKey }) =>
      api.approveSelection(kind, { word_id: wordId, ...key }),
    onMutate: async ({ kind, key }) => {
      await client.cancelQueries({ queryKey: detailKey });
      const previous = client.getQueryData<WordDetail>(detailKey);
      if (previous) client.setQueryData(detailKey, applyApproval(previous, kind, key, true));
      return { previous };
    },
    onError: (error, _vars, context) => {
      if (context?.previous) client.setQueryData(detailKey, context.previous);
      onFailure(error);
    },
    onSuccess: (detail) => onSuccessDetail(detail, 'Approved.'),
  });

  const unapprove = useMutation({
    mutationFn: ({ kind, key }: { kind: AssetKind; key: SelectionKey }) =>
      api.unapproveSelection(kind, { word_id: wordId, ...key }),
    onMutate: async ({ kind, key }) => {
      await client.cancelQueries({ queryKey: detailKey });
      const previous = client.getQueryData<WordDetail>(detailKey);
      if (previous) client.setQueryData(detailKey, applyApproval(previous, kind, key, false));
      return { previous };
    },
    onError: (error, _vars, context) => {
      if (context?.previous) client.setQueryData(detailKey, context.previous);
      onFailure(error);
    },
    onSuccess: (detail) => onSuccessDetail(detail, 'Approval withdrawn.'),
  });

  const select = useMutation({
    mutationFn: ({ kind, candId, key }: { kind: AssetKind; candId: number; key: SelectionKey }) =>
      api.overrideSelection(kind, { word_id: wordId, cand_id: candId, ...key }),
    onSuccess: (detail) => onSuccessDetail(detail, 'Selection pinned to this candidate.'),
    onError: onFailure,
  });

  const reject = useMutation({
    mutationFn: ({ kind, candId }: { kind: AssetKind; candId: number }) =>
      api.rejectCandidate(kind, candId),
    onSuccess: (detail) => onSuccessDetail(detail, 'Candidate rejected; the slot re-selected.'),
    onError: onFailure,
  });

  const setPrimary = useMutation({
    mutationFn: (pos: Pos) => api.setPrimarySense({ word_id: wordId, pos }),
    onSuccess: (detail) => onSuccessDetail(detail, 'Primary sense moved.'),
    onError: onFailure,
  });

  const setEnabled = useMutation({
    mutationFn: ({ pos, enabled }: { pos: Pos; enabled: boolean }) =>
      api.setSenseEnabled({ word_id: wordId, pos, enabled }),
    onSuccess: (detail, vars) =>
      onSuccessDetail(detail, vars.enabled ? 'Sense enabled.' : 'Sense disabled.'),
    onError: onFailure,
  });

  const mintDefinition = useMutation({
    mutationFn: (body: Omit<MintDefinitionBody, 'word_id'>) =>
      api.mintDefinitionCandidate({ word_id: wordId, ...body }),
    onSuccess: (detail) => onSuccessDetail(detail, 'Manual definition candidate minted.'),
    onError: onFailure,
  });

  const mintExample = useMutation({
    mutationFn: (body: Omit<MintExampleBody, 'word_id'>) =>
      api.mintExampleCandidate({ word_id: wordId, ...body }),
    onSuccess: (detail) => onSuccessDetail(detail, 'Manual example candidate minted.'),
    onError: onFailure,
  });

  const uploadImage = useMutation({
    mutationFn: (file: File) => api.uploadImageCandidate({ word_id: wordId, file }),
    onSuccess: (detail) => onSuccessDetail(detail, 'Image stored content-addressed.'),
    onError: onFailure,
  });

  return {
    approve,
    unapprove,
    select,
    reject,
    setPrimary,
    setEnabled,
    mintDefinition,
    mintExample,
    uploadImage,
    busy:
      approve.isPending ||
      unapprove.isPending ||
      select.isPending ||
      reject.isPending ||
      setPrimary.isPending ||
      setEnabled.isPending ||
      mintDefinition.isPending ||
      mintExample.isPending ||
      uploadImage.isPending,
  };
}

/** Pure helper so the optimistic path is unit-testable. */
export function applyApproval(
  detail: WordDetail,
  kind: AssetKind,
  key: SelectionKey,
  approved: boolean,
): WordDetail {
  if (kind === 'definition') {
    return {
      ...detail,
      definitions: detail.definitions.map((slot) =>
        slot.pos === key.pos && slot.selection
          ? { ...slot, selection: { ...slot.selection, approved } }
          : slot,
      ),
    };
  }
  if (kind === 'example') {
    return {
      ...detail,
      examples: detail.examples.map((slot) =>
        slot.slot === key.slot && slot.selection
          ? { ...slot, selection: { ...slot.selection, approved } }
          : slot,
      ),
    };
  }
  return {
    ...detail,
    image: detail.image.selection
      ? { ...detail.image, selection: { ...detail.image.selection, approved } }
      : detail.image,
  };
}

/* ------------------------------------------------------------------ */
/* Queue mutations                                                     */
/* ------------------------------------------------------------------ */

export function useResolveOov() {
  const client = useQueryClient();
  const { message } = App.useApp();
  return useMutation({
    mutationFn: ({ lemma, body }: { lemma: string; body: OovResolveBody }) =>
      api.resolveOov(lemma, body),
    onSuccess: (_row, { body }) => {
      void client.invalidateQueries({ queryKey: qk.oov() });
      void client.invalidateQueries({ queryKey: qk.words() });
      void client.invalidateQueries({ queryKey: qk.dashboard() });
      void client.invalidateQueries({ queryKey: qk.releases() });
      void client.invalidateQueries({ queryKey: qk.plan() });
      message.success(
        body.mode === 'promote'
          ? 'Promoted to an auxiliary word; asset fetches are now derived for it.'
          : 'Rewrite minted and selected.',
      );
    },
    onError: (error) => message.error(errorMessage(error)),
  });
}

export function useDeadLetterActions() {
  const client = useQueryClient();
  const { message } = App.useApp();

  const invalidate = () => {
    void client.invalidateQueries({ queryKey: qk.deadLetters() });
    void client.invalidateQueries({ queryKey: qk.dashboard() });
    void client.invalidateQueries({ queryKey: qk.words() });
    void client.invalidateQueries({ queryKey: qk.releases() });
  };

  // The write endpoints answer with the *first* page of the box. Seeding that
  // into the cache would replace whatever page the operator is on, so the
  // response is discarded and the list refetches for the page in view.
  const retry = useMutation({
    mutationFn: (body: JobKeyBody) => api.retryDeadLetter(body),
    onSuccess: () => {
      invalidate();
      message.success('job_state row deleted; the demand re-derives on the next pass.');
    },
    onError: (error) => message.error(errorMessage(error)),
  });

  const waive = useMutation({
    mutationFn: (body: JobKeyBody) => api.waiveDeadLetter(body),
    onSuccess: () => {
      invalidate();
      message.success('Waived — the fallback rule for this lane is now armed.');
    },
    onError: (error) => message.error(errorMessage(error)),
  });

  return { retry, waive };
}

export function useExportRelease() {
  const client = useQueryClient();
  const { message } = App.useApp();
  return useMutation({
    mutationFn: (body: ExportBody) => api.exportRelease(body),
    onSuccess: (release) => {
      void client.invalidateQueries({ queryKey: qk.releases() });
      void client.invalidateQueries({ queryKey: qk.dashboard() });
      message.success(`Exported ${release.version}.`);
    },
  });
}
