import { describe, expect, it } from 'vitest';
import { getWord, listWords } from '../../api/endpoints';
import { BULK_ACTIONS, bulkAction, resolveApprovalTarget, tally } from './bulkApprove';
import type { WordDetail } from '../../api/types';

async function detailOf(lemma: string): Promise<WordDetail> {
  const page = await listWords({ q: lemma, page_size: 5 });
  const row = page.items.find((item) => item.lemma === lemma);
  if (!row) throw new Error(`fixture word "${lemma}" is missing`);
  return getWord(row.word_id);
}

describe('BULK_ACTIONS', () => {
  it('covers exactly the three approvable slots', () => {
    expect(BULK_ACTIONS.map((action) => action.kind)).toEqual([
      'definition_primary',
      'example_slot_1',
      'image',
    ]);
    expect(bulkAction('image').blocker).toBe('image_not_approved');
  });
});

describe('resolveApprovalTarget against real fixture details', () => {
  it('targets the primary sense by its pos', async () => {
    const detail = await detailOf('accumulate');
    const primary = detail.definitions.find((slot) => slot.selection?.is_primary);
    expect(primary).toBeDefined();

    const target = resolveApprovalTarget('definition_primary', detail);
    expect(target).toEqual({
      action: 'approve',
      selectionKind: 'definition',
      key: { pos: primary?.pos },
    });
  });

  it('targets slot 1 only, never slots 2 or 3', async () => {
    const detail = await detailOf('adapt');
    const target = resolveApprovalTarget('example_slot_1', detail);
    if (target.action === 'approve') {
      expect(target.selectionKind).toBe('example');
      expect(target.key).toEqual({ slot: 1 });
    } else {
      expect(target.reason).toMatch(/slot 1/i);
    }
  });
});

describe('resolveApprovalTarget skip rules', () => {
  const base: WordDetail = {
    word: {
      word_id: 1,
      lemma: 'stub',
      role: 'target',
      aux_status: null,
      phonetic: null,
      frequency_rank: null,
      etymology: null,
      etymology_source: null,
      ready: false,
      blockers: [],
      created_by: 'import',
      created_at: '2026-01-01T00:00:00Z',
    },
    definitions: [],
    examples: [
      { slot: 1, selection: null, candidates: [] },
      { slot: 2, selection: null, candidates: [] },
      { slot: 3, selection: null, candidates: [] },
    ],
    image: { selection: null, candidates: [] },
    tts: [],
    distractors: [],
    recent_events: [],
  };

  const selection = {
    word_id: 1,
    selected_by: 'auto' as const,
    pinned: false,
    approved: false,
    approved_hash: null,
    approved_by: null,
    approved_at: null,
    selection_rev: 1,
    updated_at: '2026-01-01T00:00:00Z',
  };

  it('skips a word with no primary sense', () => {
    expect(resolveApprovalTarget('definition_primary', base)).toEqual({
      action: 'skip',
      reason: 'No primary sense is selected.',
    });
  });

  it('skips an already approved primary sense rather than re-posting it', () => {
    const detail: WordDetail = {
      ...base,
      definitions: [
        {
          pos: 'noun',
          selection: {
            ...selection,
            pos: 'noun',
            def_cand_id: 9,
            is_primary: true,
            enabled: true,
            approved: true,
          },
          candidates: [],
        },
      ],
    };
    expect(resolveApprovalTarget('definition_primary', detail)).toEqual({
      action: 'skip',
      reason: 'Primary sense is already approved.',
    });
  });

  it('skips a disabled primary sense', () => {
    const detail: WordDetail = {
      ...base,
      definitions: [
        {
          pos: 'noun',
          selection: {
            ...selection,
            pos: 'noun',
            def_cand_id: 9,
            is_primary: true,
            enabled: false,
          },
          candidates: [],
        },
      ],
    };
    expect(resolveApprovalTarget('definition_primary', detail)).toMatchObject({ action: 'skip' });
  });

  it('skips an empty image slot and approves a filled one', () => {
    expect(resolveApprovalTarget('image', base)).toEqual({
      action: 'skip',
      reason: 'No image is selected.',
    });

    const withImage: WordDetail = {
      ...base,
      image: { selection: { ...selection, img_cand_id: 4 }, candidates: [] },
    };
    expect(resolveApprovalTarget('image', withImage)).toEqual({
      action: 'approve',
      selectionKind: 'image',
      key: {},
    });
  });
});

describe('tally', () => {
  it('counts each terminal state and leaves pending out of done', () => {
    expect(
      tally([
        { word_id: 1, lemma: 'a', status: 'approved', note: '' },
        { word_id: 2, lemma: 'b', status: 'skipped', note: 'nothing to do' },
        { word_id: 3, lemma: 'c', status: 'failed', note: 'boom' },
        { word_id: 4, lemma: 'd', status: 'pending', note: '' },
        { word_id: 5, lemma: 'e', status: 'running', note: '' },
      ]),
    ).toEqual({ total: 5, approved: 1, skipped: 1, failed: 1, done: 3 });
  });
});
