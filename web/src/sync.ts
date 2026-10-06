// Keeps this browser's progress in step with the copy the web server syncs with the Android
// app (docs/contracts/sync.md): on load and a moment after every change it sends the local
// document and applies the merged one. Without the server the page simply works offline.

import { State } from 'ts-fsrs';
import { commit, getProgress, onChange } from './store';
import type { Progress, StoredCard, WordState } from './store';
import { parseDoc } from './syncdoc';
import type { SyncCard, SyncDoc, SyncEntry, SyncStage } from './syncdoc';

const DELAY_MS = 2000;
const iso = (t: string | null | undefined) => (t ? new Date(t).toISOString() : null);

function toCard(c: StoredCard): SyncCard {
  return {
    due: iso(c.due)!,
    stability: c.stability,
    difficulty: c.difficulty,
    elapsedDays: c.elapsed_days,
    scheduledDays: c.scheduled_days,
    reps: c.reps,
    lapses: c.lapses,
    // ts-fsrs accepts a state's name as well as its number.
    state: typeof c.state === 'number' ? c.state : State[c.state as keyof typeof State],
    lastReview: iso(c.last_review),
  };
}

function fromCard(c: SyncCard): StoredCard {
  return {
    due: c.due,
    stability: c.stability,
    difficulty: c.difficulty,
    elapsed_days: c.elapsedDays,
    scheduled_days: c.scheduledDays,
    learning_steps: 0,
    reps: c.reps,
    lapses: c.lapses,
    state: c.state as State,
    last_review: c.lastReview,
  };
}

const toStage = ({ stage, next, immediate, needClean, thenUse, flawed }: WordState): SyncStage => ({
  stage,
  next,
  immediate,
  needClean,
  thenUse,
  flawed,
});

export function toDoc(p: Progress): SyncDoc {
  const words: Record<string, SyncEntry> = {};
  for (const id of new Set([...Object.keys(p.cards), ...Object.keys(p.words)])) {
    const card = p.cards[id];
    const stage = p.words[id];
    words[id] = { card: card ? toCard(card) : null, stage: stage ? toStage(stage) : null };
  }
  return { v: 1, words, notes: { ...p.notes } };
}

/** An entry by value, whatever its key order or date format. */
function canon(e: SyncEntry | undefined): string {
  const c = e?.card;
  const s = e?.stage;
  return JSON.stringify([
    c && [Date.parse(c.due), c.stability, c.difficulty, c.elapsedDays, c.scheduledDays, c.reps, c.lapses, c.state,
      c.lastReview ? Date.parse(c.lastReview) : null],
    s && [s.stage, s.next, s.immediate, s.needClean, s.thenUse, s.flawed],
  ]);
}

/** Applies a merged document (contract §4); returns the new progress and how many words changed. */
export function applyDoc(p: Progress, merged: SyncDoc): { next: Progress; changed: number } {
  const local = toDoc(p);
  const cards = { ...p.cards };
  const words = { ...p.words };
  const notes = { ...p.notes };
  let { current, lastReview } = p;
  let changed = 0;
  let notesChanged = false;
  for (const [id, entry] of Object.entries(merged.words)) {
    if (canon(entry) === canon(local.words[id])) continue;
    changed += 1;
    if (entry.card) cards[id] = fromCard(entry.card);
    if (entry.stage) words[id] = { ...entry.stage, since: p.seq };
    else delete words[id];
    if (current?.word === Number(id)) current = null;
    if (lastReview?.word === Number(id)) lastReview = null;
  }
  for (const [id, note] of Object.entries(merged.notes)) {
    const old = notes[id];
    if (!old || Date.parse(note.at) > Date.parse(old.at)) {
      notes[id] = note;
      notesChanged = true;
    }
  }
  if (!changed && !notesChanged) return { next: p, changed: 0 };
  return { next: { ...p, cards, words, notes, current, lastReview }, changed };
}

let timer: ReturnType<typeof setTimeout> | undefined;
let running = false;
let again = false;

async function syncNow(): Promise<void> {
  if (running) {
    again = true;
    return;
  }
  running = true;
  try {
    const res = await fetch('/api/sync', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(toDoc(getProgress())),
    });
    const merged = res.ok ? parseDoc(await res.json()) : null;
    if (merged) {
      const { next } = applyDoc(getProgress(), merged);
      if (next !== getProgress()) commit(next);
    }
  } catch {
    /* the server is not reachable: try again after the next change */
  } finally {
    running = false;
    if (again) {
      again = false;
      schedule();
    }
  }
}

function schedule(): void {
  clearTimeout(timer);
  timer = setTimeout(() => void syncNow(), DELAY_MS);
}

export function startSync(): void {
  void syncNow();
  onChange(schedule);
}
