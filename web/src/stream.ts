// The stream: one mixed sequence of steps that takes every word from first sight to long-term
// review. Implements docs/contracts/stream.md; the Android client implements the same file.

import { createEmptyCard, fsrs, generatorParameters, Rating } from 'ts-fsrs';
import type { Card, Grade } from 'ts-fsrs';
import type { Index } from './api';
import { emptyDay, localDate } from './store';
import type { Current, Progress, StepKind, StoredCard, WordState } from './store';

/** Learning window, spacing between a word's steps, backlog guard (contract §5). */
export const WINDOW = 5;
export const SPACING = 3;
export const BACKLOG = 50;

export type Outcome = 'clean' | 'shaky' | 'failed';

// Same scheduling as the app's FSRS: the stream's own steps are the short-term learning, so a
// graduated word goes straight to day-based review, without fuzz.
const scheduler = fsrs(generatorParameters({ enable_fuzz: false, enable_short_term: false }));
const toStored = (c: Card): StoredCard => JSON.parse(JSON.stringify(c)) as StoredCard;

/** Today's counters, reset when the date changes. */
export function today(p: Progress): Progress {
  const date = localDate();
  return p.day.date === date ? p : { ...p, day: emptyDay(date) };
}

/** Words in review that are due, the most likely to be forgotten first. */
export function dueReviews(p: Progress, index: Index, now = Date.now()): number[] {
  const at = new Date(now);
  return Object.entries(p.cards)
    .filter(([id, c]) => !p.words[id] && index.pos.has(Number(id)) && Date.parse(c.due) <= now)
    .map(([id, c]) => [Number(id), scheduler.get_retrievability(c, at, false) as number] as const)
    .sort((a, b) => a[1] - b[1])
    .map(([id]) => id);
}

/** The next word not met yet, in release learning order. */
export function nextNewWord(p: Progress, index: Index): number | null {
  for (const [id] of index.words) if (!p.cards[id] && !p.words[id]) return id;
  return null;
}

export function newAllowance(p: Progress): number {
  return Math.max(0, p.newPerDay + p.day.extra - p.day.introduced);
}

/** Whether a new word may enter the stream now. */
function mayIntroduce(p: Progress, index: Index, due: number): boolean {
  return (
    Object.keys(p.words).length < WINDOW && newAllowance(p) > 0 && due < BACKLOG && nextNewWord(p, index) !== null
  );
}

const reviewTask = (p: Progress, id: number): 'rebuild' | 'fill' => ((p.reviews[id] ?? 0) % 2 === 0 ? 'rebuild' : 'fill');

/** Chooses the next step, or null when today's stream is done (contract §5). */
export function nextStep(input: Progress, index: Index, now = Date.now()): Current | null {
  const p = today(input);
  const learning = Object.entries(p.words).map(([id, ws]) => ({ id: Number(id), ws }));

  const immediate = learning.filter((x) => x.ws.immediate).sort((a, b) => a.ws.since - b.ws.since)[0];
  if (immediate) return { word: immediate.id, kind: immediate.ws.next };

  const delayed = learning.filter((x) => !x.ws.immediate).sort((a, b) => a.ws.since - b.ws.since);
  const ready = delayed.filter((x) => p.seq - x.ws.since >= SPACING);
  const waiting = delayed.filter((x) => p.seq - x.ws.since < SPACING);

  const due = dueReviews(p, index, now);
  const fresh = mayIntroduce(p, index, due.length) ? nextNewWord(p, index) : null;
  const reviews: Current[] = due.slice(0, 3).map((id) => ({ word: id, kind: 'review', task: reviewTask(p, id) }));
  const meet: Current[] = fresh !== null ? [{ word: fresh, kind: 'know' }] : [];
  const mixed = p.day.cycle % 3 < 2 ? [...reviews, ...meet] : [...meet, ...reviews];

  const candidates: Current[] = [
    ...ready.map((x) => ({ word: x.id, kind: x.ws.next })),
    ...mixed,
    ...waiting.map((x) => ({ word: x.id, kind: x.ws.next })),
  ];
  if (!candidates.length) return null;

  // Guards: never the same word twice in a row; never four of one step type in a row.
  const fine = (c: Current) =>
    c.word !== p.lastWord && !(p.recent.length >= 3 && p.recent.every((k) => k === c.kind));
  return candidates.find(fine) ?? candidates[0]!;
}

/** Steps still ahead of a Learning or Relearning word. */
function stepsLeft(ws: WordState): number {
  const after = ws.stage === 'learning' && ws.thenUse ? 1 : 0;
  switch (ws.next) {
    case 'know':
      return 1 + ws.needClean + after;
    case 'explain1':
      return 1 + ws.needClean + after;
    case 'explain2':
      return ws.needClean + after;
    case 'use':
      return 1;
  }
}

/** Steps done today and the estimated total, for the progress bar (contract §5). */
export function streamProgress(input: Progress, index: Index, now = Date.now()): { done: number; total: number } {
  const p = today(input);
  const due = dueReviews(p, index, now).length;
  const pending = Object.values(p.words).reduce((n, ws) => n + stepsLeft(ws), 0);
  const fresh = due < BACKLOG ? Math.min(newAllowance(p), index.words.length) : 0;
  return { done: p.day.steps, total: p.day.steps + due + pending + 4 * fresh };
}

/** Rating derived from a review task (contract §6). */
export function deriveRating(outcome: Outcome, ms: number, task: 'rebuild' | 'fill'): Grade {
  if (outcome === 'failed') return Rating.Again;
  if (outcome === 'shaky') return Rating.Hard;
  // A rebuild review is the rebuild plus spelling the word: 10 s and 8 s.
  return ms < (task === 'rebuild' ? 18_000 : 4_000) ? Rating.Easy : Rating.Good;
}

/** Applies a finished step and returns the new progress (contract §3). */
export function complete(input: Progress, cur: Current, outcome: Outcome, ms: number, now = new Date()): Progress {
  const p: Progress = structuredClone(today(input));
  const id = cur.word;
  p.seq += 1;
  p.lastWord = id;
  p.recent = [...p.recent, cur.kind].slice(-3);
  p.day.steps += 1;
  p.history[p.day.date] = (p.history[p.day.date] ?? 0) + 1;
  p.current = null;
  const ws = p.words[id];
  const later = (next: StepKind, extra: Partial<WordState> = {}) => {
    p.words[id] = { ...p.words[id]!, next, immediate: false, since: p.seq, ...extra };
  };

  switch (cur.kind) {
    case 'know':
      if (!ws) {
        p.words[id] = {
          stage: 'learning',
          next: 'explain1',
          immediate: true,
          since: p.seq,
          needClean: 1,
          thenUse: true,
          flawed: false,
        };
        p.day.introduced += 1;
        p.day.cycle += 1;
      } else {
        later('explain2');
      }
      break;
    case 'explain1':
      later('explain2', outcome === 'failed' ? { needClean: 2, flawed: true } : { flawed: ws!.flawed || outcome === 'shaky' });
      break;
    case 'explain2':
      if (outcome === 'clean') {
        const needClean = ws!.needClean - 1;
        if (needClean > 0) later('explain2', { needClean });
        else if (ws!.stage === 'learning' && ws!.thenUse) later('use', { needClean: 1 });
        else delete p.words[id];
      } else if (outcome === 'shaky') {
        later('explain2', { flawed: true });
      } else {
        p.words[id] = { ...ws!, next: 'know', immediate: true, since: p.seq, flawed: true };
      }
      break;
    case 'use':
      if (outcome === 'clean') {
        const rating = ws!.flawed ? Rating.Hard : Rating.Good;
        p.cards[id] = toStored(scheduler.next(createEmptyCard(now), now, rating).card);
        delete p.words[id];
        p.day.met += 1;
        if (!ws!.flawed) p.day.metClean += 1;
      } else {
        later('explain2', { needClean: 1, thenUse: true, flawed: true });
      }
      break;
    case 'review': {
      const prev = p.cards[id]!;
      const rating = deriveRating(outcome, ms, cur.task ?? 'rebuild');
      p.cards[id] = toStored(scheduler.next(prev, now, rating).card);
      p.reviews[id] = (p.reviews[id] ?? 0) + 1;
      p.day.reviewed += 1;
      if (outcome === 'clean') p.day.reviewedClean += 1;
      p.day.cycle += 1;
      p.lastReview = { word: id, prev, rating };
      if (rating === Rating.Again) relearn(p, id);
      break;
    }
  }
  return p;
}

function relearn(p: Progress, id: number): void {
  p.words[id] = {
    stage: 'relearning',
    next: 'know',
    immediate: true,
    since: p.seq,
    needClean: 1,
    thenUse: false,
    flawed: true,
  };
}

/** Replaces the derived rating of the last review with the learner's own choice. */
export function overrideRating(input: Progress, rating: Grade, now = new Date()): Progress {
  const last = input.lastReview;
  if (!last || last.rating === rating) return input;
  const p: Progress = structuredClone(input);
  p.cards[last.word] = toStored(scheduler.next(last.prev, now, rating).card);
  if (rating === Rating.Again) relearn(p, last.word);
  else if (p.words[last.word]?.stage === 'relearning') delete p.words[last.word];
  p.lastReview = { ...last, rating };
  return p;
}

/** When the next review would fall for each rating, from the card as it was before the review. */
export function intervals(prev: StoredCard, now = new Date()): Record<Grade, number> {
  const r = scheduler.repeat(prev, now);
  const at = (g: Grade) => r[g].card.due.getTime() - now.getTime();
  return {
    [Rating.Again]: at(Rating.Again),
    [Rating.Hard]: at(Rating.Hard),
    [Rating.Good]: at(Rating.Good),
    [Rating.Easy]: at(Rating.Easy),
  } as Record<Grade, number>;
}

/** Consecutive days with at least one step, counting back from today (or yesterday). */
export function streak(p: Progress): number {
  const d = new Date();
  if (!p.history[localDate(d)]) d.setDate(d.getDate() - 1);
  let n = 0;
  while (p.history[localDate(d)]) {
    n += 1;
    d.setDate(d.getDate() - 1);
  }
  return n;
}

/** Reviews due by the end of tomorrow, for the done screen. */
export function dueTomorrow(p: Progress, index: Index): number {
  const end = new Date();
  end.setDate(end.getDate() + 1);
  end.setHours(23, 59, 59, 999);
  return dueReviews(p, index, end.getTime()).length;
}

/** Steps per day for the last seven days, oldest first. */
export function week(p: Progress): { day: string; steps: number; today: boolean }[] {
  const out = [];
  for (let k = 6; k >= 0; k -= 1) {
    const d = new Date();
    d.setDate(d.getDate() - k);
    out.push({ day: 'SMTWTFS'[d.getDay()]!, steps: p.history[localDate(d)] ?? 0, today: k === 0 });
  }
  return out;
}
