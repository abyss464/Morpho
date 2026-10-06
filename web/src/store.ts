// Learner progress in localStorage: FSRS cards (a card exists once its word was studied)
// and the latest self-written explanation per word. Keys are release word_ids, which are
// stable across releases.

import { useSyncExternalStore } from 'react';
import { createEmptyCard, fsrs, generatorParameters, Rating, State } from 'ts-fsrs';
import type { Card, CardInput, Grade } from 'ts-fsrs';

const KEY = 'morpho-web-v1';

/** FSRS card as stored: JSON round-trip turns its dates into ISO strings. */
export type StoredCard = Omit<CardInput, 'due' | 'last_review'> & { due: string; last_review?: string | null };

export interface Note {
  text: string;
  at: string;
}

export interface Progress {
  v: 1;
  cards: Record<string, StoredCard>;
  notes: Record<string, Note>;
}

const scheduler = fsrs(generatorParameters({ enable_fuzz: true }));

const obj = <T>(v: unknown): Record<string, T> =>
  v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, T>) : {};

function read(): Progress {
  try {
    const raw = localStorage.getItem(KEY);
    const parsed = raw ? (JSON.parse(raw) as Partial<Progress>) : {};
    return { v: 1, cards: obj<StoredCard>(parsed.cards), notes: obj<Note>(parsed.notes) };
  } catch {
    return { v: 1, cards: {}, notes: {} };
  }
}

let state: Progress = read();
const listeners = new Set<() => void>();

function commit(next: Progress): void {
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

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function useProgress(): Progress {
  return useSyncExternalStore(subscribe, () => state);
}

export function getProgress(): Progress {
  return state;
}

const toStored = (c: Card): StoredCard => JSON.parse(JSON.stringify(c)) as StoredCard;

/** Adds a fresh FSRS card (due now) for every word not yet studied. */
export function markStudied(ids: number[]): number {
  const now = new Date();
  const cards = { ...state.cards };
  let added = 0;
  for (const id of ids) {
    if (cards[id]) continue;
    cards[id] = toStored(createEmptyCard(now));
    added++;
  }
  if (added) commit({ ...state, cards });
  return added;
}

/** Schedules `prev` (the card as it was before this review) with `grade` and stores the result. */
export function rate(id: number, prev: StoredCard, grade: Grade): void {
  const { card } = scheduler.next(prev, new Date(), grade);
  commit({ ...state, cards: { ...state.cards, [id]: toStored(card) } });
}

/** Due date of each grade if chosen now, for the rating buttons. */
export function preview(prev: StoredCard): Record<Grade, Date> {
  const p = scheduler.repeat(prev, new Date());
  return {
    [Rating.Again]: p[Rating.Again].card.due,
    [Rating.Hard]: p[Rating.Hard].card.due,
    [Rating.Good]: p[Rating.Good].card.due,
    [Rating.Easy]: p[Rating.Easy].card.due,
  } as Record<Grade, Date>;
}

export function saveNote(id: number, text: string): void {
  const t = text.trim();
  if (!t) return;
  commit({ ...state, notes: { ...state.notes, [id]: { text: t, at: new Date().toISOString() } } });
}

/* ---------- derived ---------- */

export const dueAt = (c: StoredCard): number => Date.parse(c.due);

function byDue(p: Progress, keep: (due: number) => boolean, order?: Map<number, number>): number[] {
  const rank = (id: number) => order?.get(id) ?? Number.MAX_SAFE_INTEGER;
  return Object.entries(p.cards)
    .map(([id, c]) => [Number(id), dueAt(c)] as const)
    .filter(([, due]) => keep(due))
    .sort((a, b) => a[1] - b[1] || rank(a[0]) - rank(b[0]))
    .map(([id]) => id);
}

/** Studied word ids due at `now`, earliest first; ties follow `order` (word id -> learning position). */
export function dueIds(p: Progress, now: number, order?: Map<number, number>): number[] {
  return byDue(p, (due) => due <= now, order);
}

/** Studied word ids not yet due, soonest first; ties follow `order`. */
export function upcomingIds(p: Progress, now: number, order?: Map<number, number>): number[] {
  return byDue(p, (due) => due > now, order);
}

/** 0 = not studied, 1 = new or learning, 2..4 = in review with growing stability. */
export function strength(c: StoredCard | undefined): number {
  if (!c) return 0;
  const st = typeof c.state === 'string' ? State[c.state] : c.state;
  if (st !== State.Review) return 1;
  if (c.stability < 7) return 2;
  if (c.stability < 30) return 3;
  return 4;
}

export const STRENGTH_LABEL = ['not studied yet', 'just learned', 'in review', 'getting strong', 'well known'];

export function formatInterval(ms: number): string {
  const min = Math.max(1, Math.round(ms / 60000));
  if (min < 60) return `${min}m`;
  const h = Math.round(min / 60);
  if (h < 24) return `${h}h`;
  const d = Math.round(h / 24);
  if (d < 31) return `${d}d`;
  const mo = Math.round(d / 30.4);
  if (mo < 12) return `${mo}mo`;
  return `${(d / 365).toFixed(1).replace(/\.0$/, '')}y`;
}
