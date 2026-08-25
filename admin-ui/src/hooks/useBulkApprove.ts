/**
 * Sequential driver for bulk approval.
 *
 * Sequential rather than parallel on purpose: morphod serializes writes behind a
 * single writer task, so fanning out only trades queueing for a thundering herd
 * and makes partial failure impossible to narrate. One word at a time gives an
 * honest cursor, a stoppable run, and a per-word failure the operator can act on.
 */

import { useCallback, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import * as api from '../api/endpoints';
import { qk } from '../api/queryKeys';
import { errorMessage } from '../lib/errors';
import {
  resolveApprovalTarget,
  type BulkApproveItem,
  type BulkApproveKind,
  type BulkApproveProgress,
} from '../features/words/bulkApprove';
import type { WordListItem, WordsQuery } from '../api/types';

/** Morphod clamps `page_size` at 200 (api/dto.rs MAX_PAGE_SIZE). */
export const MAX_PAGE_SIZE = 200;

export interface BulkTargetWord {
  word_id: number;
  lemma: string;
}

/**
 * Walks every page of the current filter so "select all matching" means the
 * whole worklist, not the 25 rows that happen to be on screen. Aborts with the
 * caller's signal, because at syllabus scale this is 30 round trips.
 */
export async function collectMatchingWords(
  query: WordsQuery,
  signal: AbortSignal,
  onProgress?: (loaded: number, total: number) => void,
): Promise<WordListItem[]> {
  const collected: WordListItem[] = [];
  let page = 1;
  let total = Infinity;

  while (collected.length < total) {
    if (signal.aborted) break;
    const chunk = await api.listWords({ ...query, page, page_size: MAX_PAGE_SIZE }, signal);
    total = chunk.total;
    collected.push(...chunk.items);
    onProgress?.(collected.length, total);
    if (chunk.items.length === 0) break;
    page += 1;
  }

  return collected;
}

const IDLE: BulkApproveProgress | null = null;

export interface BulkApproveController {
  progress: BulkApproveProgress | null;
  start: (kind: BulkApproveKind, words: readonly BulkTargetWord[]) => Promise<void>;
  cancel: () => void;
  reset: () => void;
}

export function useBulkApprove(): BulkApproveController {
  const client = useQueryClient();
  const [progress, setProgress] = useState<BulkApproveProgress | null>(IDLE);
  const cancelled = useRef(false);

  const cancel = useCallback(() => {
    cancelled.current = true;
    setProgress((current) => (current ? { ...current, cancelled: true } : current));
  }, []);

  const reset = useCallback(() => {
    cancelled.current = false;
    setProgress(IDLE);
  }, []);

  const start = useCallback(
    async (kind: BulkApproveKind, words: readonly BulkTargetWord[]) => {
      cancelled.current = false;

      const items: BulkApproveItem[] = words.map((word) => ({
        word_id: word.word_id,
        lemma: word.lemma,
        status: 'pending',
        note: '',
      }));
      setProgress({ kind, items, cursor: 0, running: true, cancelled: false });

      const patch = (index: number, next: Partial<BulkApproveItem>) =>
        setProgress((current) => {
          if (!current) return current;
          const at = current.items[index];
          if (!at) return current;
          const updated = current.items.slice();
          updated[index] = { ...at, ...next };
          return { ...current, items: updated };
        });

      for (const [index, item] of items.entries()) {
        if (cancelled.current) break;
        setProgress((current) => (current ? { ...current, cursor: index } : current));
        patch(index, { status: 'running' });

        try {
          const detail = await api.getWord(item.word_id);
          client.setQueryData(qk.wordDetail(item.word_id), detail);

          const target = resolveApprovalTarget(kind, detail);
          if (target.action === 'skip') {
            patch(index, { status: 'skipped', note: target.reason });
            continue;
          }

          const updated = await api.approveSelection(target.selectionKind, {
            word_id: item.word_id,
            ...target.key,
          });
          client.setQueryData(qk.wordDetail(item.word_id), updated);
          patch(index, { status: 'approved', note: '' });
        } catch (error) {
          patch(index, { status: 'failed', note: errorMessage(error) });
        }
      }

      setProgress((current) =>
        current ? { ...current, running: false, cursor: current.items.length } : current,
      );

      // One sweep at the end rather than per word: the change stream already
      // pushed the same invalidations, and a run of 200 should not refetch the
      // dashboard 200 times.
      void client.invalidateQueries({ queryKey: qk.words() });
      void client.invalidateQueries({ queryKey: qk.dashboard() });
      void client.invalidateQueries({ queryKey: qk.releases() });
      void client.invalidateQueries({ queryKey: qk.plan() });
      void client.invalidateQueries({ queryKey: qk.events() });
    },
    [client],
  );

  return { progress, start, cancel, reset };
}
