import type { EventsQuery, OovQuery, PageParams, WordsQuery } from './types';

/**
 * Central query-key factory. Keys are structured so a single
 * `invalidateQueries({ queryKey: qk.words() })` sweeps every filtered word list
 * after a mutation, matching the reconciler's "one write, many consequences"
 * model.
 */
export const qk = {
  dashboard: () => ['dashboard'] as const,
  events: (query?: EventsQuery) => ['events', query ?? {}] as const,
  jobs: () => ['jobs'] as const,

  words: () => ['words'] as const,
  wordList: (query: WordsQuery) => ['words', 'list', query] as const,
  wordDetail: (wordId: number) => ['words', 'detail', wordId] as const,

  oov: () => ['oov'] as const,
  oovList: (query: OovQuery) => ['oov', 'list', query] as const,

  deadLetters: () => ['dead-letters'] as const,
  deadLetterList: (query: PageParams) => ['dead-letters', 'list', query] as const,

  plan: () => ['plan'] as const,
  planGroup: (seq: number) => ['plan', 'group', seq] as const,

  releases: () => ['releases'] as const,
  releaseList: () => ['releases', 'list'] as const,
  releasePreview: () => ['releases', 'preview'] as const,
} as const;

/** Every key family a word-scoped mutation can invalidate. */
export const wordMutationInvalidations = [
  qk.words(),
  qk.dashboard(),
  qk.oov(),
  qk.releases(),
  qk.plan(),
] as const;
