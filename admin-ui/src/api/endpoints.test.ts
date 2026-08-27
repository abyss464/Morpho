import { describe, expect, it } from 'vitest';
import {
  approveSelection,
  getDashboard,
  getPlan,
  getReleasePreview,
  getWord,
  listDeadLetters,
  listGallery,
  listOov,
  listWords,
  rejectCandidate,
  resolveOov,
  retryDeadLetter,
} from './endpoints';

/**
 * These run the real client against the real MSW handlers, so they check the
 * contract end to end: request shape, response shape and the state transitions
 * the console depends on.
 */

async function findWord(lemma: string) {
  const page = await listWords({ q: lemma, page_size: 5 });
  const item = page.items.find((row) => row.lemma === lemma);
  if (!item) throw new Error(`fixture word "${lemma}" is missing`);
  return item;
}

describe('GET /words', () => {
  it('paginates and filters by role', async () => {
    const firstPage = await listWords({ page: 1, page_size: 10 });
    expect(firstPage.items).toHaveLength(10);
    expect(firstPage.total).toBeGreaterThan(50);

    const auxiliary = await listWords({ role: 'auxiliary', page_size: 100 });
    expect(auxiliary.items.length).toBeGreaterThan(0);
    expect(auxiliary.items.every((item) => item.role === 'auxiliary')).toBe(true);
  });

  it('filters by readiness and by blocker code', async () => {
    const blocked = await listWords({ ready: false, page_size: 100 });
    expect(blocked.items.every((item) => !item.ready)).toBe(true);

    const oos = await listWords({ blocker: 'oos_pending', page_size: 100 });
    expect(oos.items.length).toBeGreaterThan(0);
    expect(oos.items.every((item) => item.blockers.includes('oos_pending'))).toBe(true);
  });
});

describe('GET /gallery', () => {
  it('exposes clip_similarity per item, null when the pair is unscored (backlog #32)', async () => {
    const page = await listGallery({ page_size: 100 });
    expect(page.items.length).toBeGreaterThan(0);
    expect(
      page.items.every(
        (item) => typeof item.clip_similarity === 'number' || item.clip_similarity === null,
      ),
    ).toBe(true);
    // The fixture leaves some pairs unscored on purpose, so both branches
    // of the mapper actually run.
    expect(page.items.some((item) => item.clip_similarity === null)).toBe(true);
    expect(page.items.some((item) => item.clip_similarity !== null)).toBe(true);
  });

  it('sort=clip_asc ranks the worst match first, unscored items last', async () => {
    const page = await listGallery({ sort: 'clip_asc', page_size: 200 });
    const scores = page.items.map((item) => item.clip_similarity);
    const firstNull = scores.findIndex((score) => score === null);
    if (firstNull !== -1) {
      expect(scores.slice(firstNull).every((score) => score === null)).toBe(true);
    }
    const scored = scores.filter((score): score is number => score !== null);
    expect(scored).toEqual([...scored].sort((a, b) => a - b));
  });

  it('sort=clip_desc reverses the scored ranking but still keeps unscored items last', async () => {
    const page = await listGallery({ sort: 'clip_desc', page_size: 200 });
    const scores = page.items.map((item) => item.clip_similarity);
    const firstNull = scores.findIndex((score) => score === null);
    if (firstNull !== -1) {
      expect(scores.slice(firstNull).every((score) => score === null)).toBe(true);
    }
    const scored = scores.filter((score): score is number => score !== null);
    expect(scored).toEqual([...scored].sort((a, b) => b - a));
  });
});

describe('GET /words/{id}', () => {
  it('returns every slot group plus distractors and tts', async () => {
    const listed = await findWord('adapt');
    const detail = await getWord(listed.word_id);

    expect(detail.word.lemma).toBe('adapt');
    expect(detail.definitions.length).toBeGreaterThan(0);
    expect(detail.examples).toHaveLength(3);
    expect(detail.examples.map((slot) => slot.slot)).toEqual([1, 2, 3]);
    expect(detail.image.candidates.length).toBeGreaterThan(0);
    expect(detail.distractors).toHaveLength(3);
    expect(detail.tts.some((view) => view.kind === 'word')).toBe(true);
  });

  it('answers 404 for an unknown id', async () => {
    await expect(getWord(999999)).rejects.toMatchObject({ status: 404 });
  });
});

describe('approval flips derived readiness', () => {
  it('approving the last unapproved sense removes the blocker', async () => {
    const listed = await findWord('accumulate');
    const before = await getWord(listed.word_id);
    expect(before.word.blockers).toContain('sense_not_approved');

    let after = before;
    for (const slot of before.definitions) {
      after = await approveSelection('definition', { word_id: listed.word_id, pos: slot.pos });
    }
    expect(after.word.blockers).not.toContain('sense_not_approved');
    expect(after.definitions.every((slot) => slot.selection?.approved !== false)).toBe(true);
  });
});

describe('rejecting a selected candidate re-selects the slot', () => {
  it('moves the selection to the next best candidate and clears approval', async () => {
    const listed = await findWord('adapt');
    const before = await getWord(listed.word_id);
    const slot = before.definitions.find((entry) => (entry.selection?.def_cand_id ?? 0) > 0);
    const selectedId = slot?.selection?.def_cand_id;
    expect(selectedId).toBeDefined();

    const after = await rejectCandidate('definition', selectedId as number);
    const sameSlot = after.definitions.find((entry) => entry.pos === slot?.pos);

    expect(sameSlot?.selection?.def_cand_id).not.toBe(selectedId);
    expect(sameSlot?.selection?.approved).toBe(false);
    expect(sameSlot?.candidates.find((c) => c.def_cand_id === selectedId)?.status).toBe('rejected');
  });
});

describe('OOV queue', () => {
  it('lists open entries with highlighted occurrence contexts', async () => {
    const queue = await listOov({ status: 'open', page_size: 50 });
    expect(queue.items.length).toBeGreaterThan(0);
    const entry = queue.items[0]!;
    expect(entry.occurrence_count).toBe(entry.occurrences.length);
    expect(entry.occurrences[0]?.text.toLowerCase()).toContain(entry.oos_lemma.toLowerCase());
  });

  it('promote inserts an auxiliary word and closes the row', async () => {
    const queue = await listOov({ status: 'open', page_size: 50 });
    const target = queue.items.find((item) => item.occurrence_count > 0)!;

    const resolved = await resolveOov(target.oos_lemma, { mode: 'promote' });
    expect(resolved.status).toBe('resolved_promote');

    const words = await listWords({ q: target.oos_lemma, page_size: 5 });
    expect(words.items.some((item) => item.lemma === target.oos_lemma)).toBe(true);

    const stillOpen = await listOov({ status: 'open', page_size: 50 });
    expect(stillOpen.items.some((item) => item.oos_lemma === target.oos_lemma)).toBe(false);
  });

  it('refuses a rewrite that still contains the out-of-scope token', async () => {
    const queue = await listOov({ status: 'open', page_size: 50 });
    const target = queue.items.find((item) => item.occurrence_count > 0)!;
    const occurrence = target.occurrences[0]!;

    await expect(
      resolveOov(target.oos_lemma, {
        mode: 'rewrite',
        def_cand_id: occurrence.def_cand_id,
        text: occurrence.text,
      }),
    ).rejects.toMatchObject({ status: 422, code: 'rewrite_still_out_of_scope' });
  });

  it('a valid rewrite mints a linked candidate and selects it', async () => {
    const queue = await listOov({ status: 'open', page_size: 50 });
    const target = queue.items.find((item) =>
      item.occurrences.some((occurrence) => occurrence.suggested_rewrite !== null),
    )!;
    const occurrence = target.occurrences.find((item) => item.suggested_rewrite !== null)!;

    await resolveOov(target.oos_lemma, {
      mode: 'rewrite',
      def_cand_id: occurrence.def_cand_id,
      text: occurrence.suggested_rewrite as string,
    });

    const detail = await getWord(occurrence.word_id);
    const slot = detail.definitions.find((entry) => entry.pos === occurrence.pos)!;
    const selected = slot.candidates.find(
      (candidate) => candidate.def_cand_id === slot.selection?.def_cand_id,
    );
    expect(selected?.source).toBe('llm_rewrite');
    expect(selected?.parent_cand_id).toBe(occurrence.def_cand_id);
  });
});

describe('dead letters', () => {
  it('lists rows joined with their subject and retry removes one', async () => {
    const before = await listDeadLetters();
    expect(before.total).toBeGreaterThan(0);
    const row = before.items[0]!;
    expect(row.subject.label.length).toBeGreaterThan(0);

    const after = await retryDeadLetter({
      kind: row.kind,
      subject_type: row.subject_type,
      subject_id: row.subject_id,
    });
    expect(after.total).toBe(before.total - 1);
  });
});

describe('dashboard, plan and release preview', () => {
  it('reports consistent word counts', async () => {
    const dashboard = await getDashboard();
    expect(dashboard.words.total).toBeGreaterThan(0);
    expect(dashboard.words.ready + dashboard.words.blocked).toBeLessThanOrEqual(
      dashboard.words.total,
    );
    expect(dashboard.recent_events.length).toBeLessThanOrEqual(20);
  });

  it('returns a current plan whose group word counts add up', async () => {
    const plan = await getPlan();
    const summed = plan.groups.reduce((total, group) => total + group.word_count, 0);
    expect(summed).toBe(plan.stats.word_count);
    expect(plan.stats.group_count).toBe(plan.groups.length);
  });

  it('produces a holdback report sorted by impact with gate failures', async () => {
    const report = await getReleasePreview();
    expect(report.exportable_count).toBeLessThanOrEqual(report.shippable_count);
    const impacts = report.excluded.map((row) => row.impact_count);
    expect([...impacts].sort((a, b) => b - a)).toEqual(impacts);
    // Fixtures ship with open OOV rows and dead letters, so the gates must fail.
    expect(report.gates_pass).toBe(false);
    expect(report.gate_failures.length).toBeGreaterThan(0);
  });
});
