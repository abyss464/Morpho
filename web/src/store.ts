// Learner progress in localStorage: every word's stage in the stream, FSRS cards for words in
// review, today's counters and the learner's own notes. Keys are release word_ids, which are
// stable across releases. The rules that move words between stages live in stream.ts.

import { useSyncExternalStore } from 'react';
import type { CardInput, Grade } from 'ts-fsrs';

const KEY = 'morpho-web-v2';
const OLD_KEY = 'morpho-web-v1';

/** FSRS card as stored: JSON round-trip turns its dates into ISO strings. */
export type StoredCard = Omit<CardInput, 'due' | 'last_review'> & { due: string; last_review?: string | null };

export interface Note {
  text: string;
  at: string;
}

export type StepKind = 'know' | 'explain1' | 'explain2' | 'use';

/** A word that is Learning (met, not graduated) or Relearning (a review was rated Again). */
export interface WordState {
  stage: 'learning' | 'relearning';
  /** The word's next step. */
  next: StepKind;
  /** The next step follows at once (know -> explain1, failed -> know) instead of after spacing. */
  immediate: boolean;
  /** Stream position when the next step was scheduled. */
  since: number;
  /** Clean explain2 results still required before moving on. */
  needClean: number;
  /** A Learning word goes on to `use` after its explain2 steps. */
  thenUse: boolean;
  /** Some step was shaky or failed: graduation rates Hard instead of Good. */
  flawed: boolean;
}

export interface Day {
  date: string;
  /** New words introduced today, and the extra allowance granted by "Meet 5 more words". */
  introduced: number;
  extra: number;
  steps: number;
  reviewed: number;
  reviewedClean: number;
  met: number;
  metClean: number;
  /** Position in the review, review, new rhythm. */
  cycle: number;
}

/** The step on screen, kept so that a paused stream resumes on the same step. */
export interface Current {
  word: number;
  kind: StepKind | 'review';
  task?: 'rebuild' | 'fill';
}

/** The last review, so its derived rating can be replaced. */
export interface LastReview {
  word: number;
  prev: StoredCard;
  rating: Grade;
}

export interface Progress {
  v: 2;
  cards: Record<string, StoredCard>;
  notes: Record<string, Note>;
  words: Record<string, WordState>;
  reviews: Record<string, number>;
  seq: number;
  recent: string[];
  lastWord: number | null;
  day: Day;
  history: Record<string, number>;
  newPerDay: number;
  current: Current | null;
  lastReview: LastReview | null;
}

export const localDate = (t = new Date()): string =>
  `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, '0')}-${String(t.getDate()).padStart(2, '0')}`;

export const emptyDay = (date = localDate()): Day => ({
  date,
  introduced: 0,
  extra: 0,
  steps: 0,
  reviewed: 0,
  reviewedClean: 0,
  met: 0,
  metClean: 0,
  cycle: 0,
});

const obj = <T>(v: unknown): Record<string, T> =>
  v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, T>) : {};

function fresh(): Progress {
  return {
    v: 2,
    cards: {},
    notes: {},
    words: {},
    reviews: {},
    seq: 0,
    recent: [],
    lastWord: null,
    day: emptyDay(),
    history: {},
    newPerDay: 20,
    current: null,
    lastReview: null,
  };
}

function read(): Progress {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return { ...fresh(), ...(JSON.parse(raw) as Partial<Progress>), v: 2 };
    // Progress from the unit-and-review version: studied words carry on in review.
    const old = localStorage.getItem(OLD_KEY);
    if (old) {
      const parsed = JSON.parse(old) as { cards?: unknown; notes?: unknown };
      return { ...fresh(), cards: obj<StoredCard>(parsed.cards), notes: obj<Note>(parsed.notes) };
    }
  } catch {
    /* unreadable: start over */
  }
  return fresh();
}

let state: Progress = read();
const listeners = new Set<() => void>();

export function commit(next: Progress): void {
  state = next;
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    /* storage unavailable: progress lives for this tab only */
  }
  listeners.forEach((l) => l());
}

try {
  window.addEventListener('storage', (e) => {
    if (e.key !== KEY) return;
    state = read();
    listeners.forEach((l) => l());
  });
} catch {
  /* no window */
}

/** Calls `l` after every change to the progress; returns the unsubscribe. */
export function onChange(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function useProgress(): Progress {
  return useSyncExternalStore(onChange, () => state);
}

export function getProgress(): Progress {
  return state;
}

export function saveNote(id: number, text: string): void {
  const t = text.trim();
  if (!t) return;
  commit({ ...state, notes: { ...state.notes, [id]: { text: t, at: new Date().toISOString() } } });
}

export function setNewPerDay(n: number): void {
  commit({ ...state, newPerDay: n });
}

export function formatInterval(ms: number): string {
  const min = Math.max(1, Math.round(ms / 60000));
  if (min < 60) return `${min} min`;
  const h = Math.round(min / 60);
  if (h < 24) return `${h} h`;
  const d = Math.round(h / 24);
  if (d < 31) return d === 1 ? '1 day' : `${d} days`;
  const mo = Math.round(d / 30.4);
  if (mo < 12) return mo === 1 ? '1 month' : `${mo} months`;
  return `${(d / 365).toFixed(1).replace(/\.0$/, '')} years`;
}
