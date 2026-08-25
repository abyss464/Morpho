import { describe, expect, it } from 'vitest';
import { applyApproval } from './queries';
import type { WordDetail } from '../api/types';

/** The optimistic-update helper: flip only the addressed slot, touch nothing else. */

const detail = {
  word: {
    word_id: 1,
    lemma: 'adapt',
    role: 'target',
    aux_status: null,
    phonetic: null,
    frequency_rank: null,
    etymology: null,
    etymology_source: null,
    ready: false,
    blockers: [],
    created_by: 'import',
    created_at: '',
  },
  definitions: [
    {
      pos: 'verb',
      selection: { approved: false, pos: 'verb' },
      candidates: [],
    },
    {
      pos: 'noun',
      selection: { approved: false, pos: 'noun' },
      candidates: [],
    },
  ],
  examples: [
    { slot: 1, selection: { approved: false, slot: 1 }, candidates: [] },
    { slot: 2, selection: null, candidates: [] },
    { slot: 3, selection: null, candidates: [] },
  ],
  image: { selection: { approved: false }, candidates: [] },
  tts: [],
  distractors: [],
  recent_events: [],
} as unknown as WordDetail;

describe('applyApproval', () => {
  it('flips only the addressed sense', () => {
    const next = applyApproval(detail, 'definition', { pos: 'verb' }, true);
    expect(next.definitions[0]?.selection?.approved).toBe(true);
    expect(next.definitions[1]?.selection?.approved).toBe(false);
    expect(next.examples[0]?.selection?.approved).toBe(false);
  });

  it('flips only the addressed example slot', () => {
    const next = applyApproval(detail, 'example', { slot: 1 }, true);
    expect(next.examples[0]?.selection?.approved).toBe(true);
    expect(next.definitions[0]?.selection?.approved).toBe(false);
  });

  it('flips the single image selection', () => {
    expect(applyApproval(detail, 'image', {}, true).image.selection?.approved).toBe(true);
    expect(applyApproval(detail, 'image', {}, false).image.selection?.approved).toBe(false);
  });

  it('is a no-op on an empty slot rather than inventing a selection', () => {
    const next = applyApproval(detail, 'example', { slot: 2 }, true);
    expect(next.examples[1]?.selection).toBeNull();
  });

  it('does not mutate the input', () => {
    applyApproval(detail, 'definition', { pos: 'verb' }, true);
    expect(detail.definitions[0]?.selection?.approved).toBe(false);
  });
});
