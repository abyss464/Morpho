/**
 * Live refresh: `GET /api/stream` frames become targeted TanStack Query
 * invalidations.
 *
 * The reconciler is the only thing that moves the working state forward, and it
 * moves constantly during a fetch run. Polling every screen would be both late
 * and wasteful, so the console subscribes to the change bus and invalidates only
 * the query families a given entity type can possibly affect. TanStack refetches
 * active queries and marks the rest stale, so an off-screen word detail costs
 * nothing until it is opened again.
 *
 * Mock mode keeps the no-op path: MSW's `/stream` handler is a silent stub, and
 * the fixture DB only changes in response to the console's own mutations, which
 * already invalidate inline.
 */

import { useEffect, useRef, useState } from 'react';
import { useQueryClient, type QueryClient } from '@tanstack/react-query';
import { subscribeToChanges, type StreamStatus } from '../api/changeStream';
import { STREAM_PATH } from '../api/endpoints';
import { qk } from '../api/queryKeys';
import type { ChangeEvent } from '../api/types';

/** Query families a change can touch. Names match the `qk` factory. */
export type InvalidationTarget =
  'words' | 'dashboard' | 'oov' | 'deadLetters' | 'jobs' | 'plan' | 'releases' | 'events';

/**
 * `EntityType` vocabulary of morpho-domain's change bus, mapped to the screens
 * that read the affected rows. `words` covers both the list and every cached
 * detail; word-scoped ids additionally pin the exact detail key.
 *
 * Readiness is derived, so almost every asset-level change can flip a word's
 * `ready`/`blockers` and therefore the dashboard rollup and the holdback report.
 * The plan is only rebuilt on its own entity, so it stays off the asset rows.
 */
export const CHANGE_TARGETS: Record<string, InvalidationTarget[]> = {
  word: ['words', 'dashboard', 'plan', 'releases', 'events'],
  definition_candidate: ['words', 'dashboard', 'releases', 'events'],
  definition_selection: ['words', 'dashboard', 'releases', 'events'],
  example_candidate: ['words', 'dashboard', 'releases', 'events'],
  example_selection: ['words', 'dashboard', 'releases', 'events'],
  image_candidate: ['words', 'dashboard', 'releases', 'events'],
  image_selection: ['words', 'dashboard', 'releases', 'events'],
  def_extraction: ['words', 'oov', 'dashboard'],
  oos_queue: ['oov', 'dashboard', 'words'],
  tts_asset: ['words', 'dashboard', 'releases'],
  // A job row moving between backoff/dead/waived never edits a word by itself;
  // when the retry finally lands, the write that lands with it touches the real
  // entity. Keeping `words` off this row is what stops a converging fetch run
  // from refetching the whole worklist on every queue tick.
  job_state: ['deadLetters', 'jobs', 'dashboard'],
  source_fetch: ['jobs'],
  plan: ['plan', 'words', 'dashboard', 'releases'],
  distractor: ['words', 'dashboard', 'releases'],
  release: ['releases', 'dashboard', 'events'],
  // `media_file` is content-addressed and immutable: the bytes behind a hash
  // never change, so a new row is only ever observed through the candidate that
  // references it. Nothing to invalidate.
  media_file: [],
};

/** Unknown entity types still refresh the two global rollups rather than nothing. */
export const FALLBACK_TARGETS: InvalidationTarget[] = ['dashboard', 'events'];

export function targetsFor(entityType: string): InvalidationTarget[] {
  return CHANGE_TARGETS[entityType] ?? FALLBACK_TARGETS;
}

export interface CollectedInvalidations {
  targets: Set<InvalidationTarget>;
  wordIds: Set<number>;
}

/** Folds a batch of frames into the minimal set of things to invalidate. */
export function collectInvalidations(events: readonly ChangeEvent[]): CollectedInvalidations {
  const targets = new Set<InvalidationTarget>();
  const wordIds = new Set<number>();

  for (const event of events) {
    for (const target of targetsFor(event.entity_type)) targets.add(target);
    if (event.entity_type !== 'word') continue;
    for (const id of event.entity_ids) {
      const numeric = typeof id === 'number' ? id : Number.parseInt(id, 10);
      if (Number.isFinite(numeric) && numeric > 0) wordIds.add(numeric);
    }
  }

  return { targets, wordIds };
}

const TARGET_KEYS: Record<InvalidationTarget, () => readonly unknown[]> = {
  words: qk.words,
  dashboard: qk.dashboard,
  oov: qk.oov,
  deadLetters: qk.deadLetters,
  jobs: qk.jobs,
  plan: qk.plan,
  releases: qk.releases,
  events: qk.events,
};

export function applyInvalidations(client: QueryClient, batch: CollectedInvalidations): void {
  for (const target of batch.targets) {
    void client.invalidateQueries({ queryKey: TARGET_KEYS[target]() });
  }
  // `qk.words()` already prefixes every detail key, but naming the exact word
  // keeps the intent legible and survives a future key-shape change.
  for (const wordId of batch.wordIds) {
    void client.invalidateQueries({ queryKey: qk.wordDetail(wordId) });
  }
}

/**
 * Client-side coalescing window. Morphod already batches over 250 ms, but a
 * single batch fans out into one frame per entity type; regrouping them here
 * turns a converging fetch run into one refetch pass rather than six.
 */
export const BATCH_WINDOW_MS = 300;

/**
 * Floor between two invalidation passes. A live import run publishes hundreds of
 * job_state and word frames a minute, and each pass costs a real refetch of
 * every screen currently mounted. One pass per second keeps the console visibly
 * live without turning the change bus into a load generator.
 */
export const MIN_FLUSH_INTERVAL_MS = 1_000;

const LIVE = import.meta.env.VITE_API_MOCK !== '1';

export interface ChangeStreamState {
  status: StreamStatus;
  /** False in mock mode, where the stream is a deliberate no-op. */
  enabled: boolean;
}

export function useChangeStream(): ChangeStreamState {
  const client = useQueryClient();
  const [status, setStatus] = useState<StreamStatus>(LIVE ? 'connecting' : 'closed');
  const pending = useRef<ChangeEvent[]>([]);
  const flushTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastFlush = useRef(0);
  const missedChanges = useRef(false);

  useEffect(() => {
    if (!LIVE) return;

    const flush = () => {
      flushTimer.current = null;
      lastFlush.current = Date.now();
      const batch = pending.current;
      pending.current = [];
      if (batch.length > 0) applyInvalidations(client, collectInvalidations(batch));
    };

    const unsubscribe = subscribeToChanges({
      url: STREAM_PATH,
      onStatus: (next) => {
        setStatus(next);
        if (next === 'reconnecting') {
          // Frames published while the socket was down are simply gone; the bus
          // has no replay and does not need one.
          missedChanges.current = true;
        } else if (next === 'open' && missedChanges.current) {
          missedChanges.current = false;
          // Everything on screen is suspect, including queries that failed while
          // morphod was away and would otherwise sit on their error forever.
          void client.invalidateQueries();
        }
      },
      onChange: (event) => {
        pending.current.push(event);
        if (flushTimer.current !== null) return;
        const sinceLast = Date.now() - lastFlush.current;
        const wait = Math.max(BATCH_WINDOW_MS, MIN_FLUSH_INTERVAL_MS - sinceLast);
        flushTimer.current = setTimeout(flush, wait);
      },
    });

    return () => {
      unsubscribe();
      if (flushTimer.current !== null) {
        clearTimeout(flushTimer.current);
        flushTimer.current = null;
      }
      pending.current = [];
    };
  }, [client]);

  return { status, enabled: LIVE };
}
