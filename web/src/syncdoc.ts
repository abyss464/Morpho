// The progress document the web app and the Android app sync through, and its merge
// (docs/contracts/sync.md). The server merges; clients only build and apply documents.

export interface SyncCard {
  due: string;
  stability: number;
  difficulty: number;
  elapsedDays: number;
  scheduledDays: number;
  reps: number;
  lapses: number;
  /** 0 New, 1 Learning, 2 Review, 3 Relearning. */
  state: number;
  lastReview: string | null;
}

export interface SyncStage {
  stage: 'learning';
  next: 'know' | 'explain' | 'spell' | 'use';
  immediate: boolean;
  flawed: boolean;
  /** Attempts already made at `next`. */
  attempt: number;
}

export interface SyncEntry {
  card: SyncCard | null;
  stage: SyncStage | null;
}

export interface SyncNote {
  text: string;
  at: string;
}

export interface SyncDoc {
  v: 1;
  words: Record<string, SyncEntry>;
  notes: Record<string, SyncNote>;
}

export const emptyDoc = (): SyncDoc => ({ v: 1, words: {}, notes: {} });

const STEP_ORDER = { know: 0, explain: 1, spell: 2, use: 3 } as const;
const time = (iso: string | null | undefined) => (iso ? Date.parse(iso) || 0 : 0);

/** Whether the incoming entry replaces the stored one (contract §3); a tie keeps the stored one. */
function wins(incoming: SyncEntry, stored: SyncEntry): boolean {
  if (incoming.card && stored.card) return time(incoming.card.lastReview) > time(stored.card.lastReview);
  if (incoming.card || stored.card) return !!incoming.card;
  // A word's learning only moves forward: to a later step, or to another attempt at the same one.
  const rank = (s: SyncStage | null) => (s ? STEP_ORDER[s.next] * 1000 + s.attempt : -1);
  return rank(incoming.stage) > rank(stored.stage);
}

export function mergeDocs(stored: SyncDoc, incoming: SyncDoc): SyncDoc {
  const words = { ...stored.words };
  for (const [id, entry] of Object.entries(incoming.words)) {
    const old = words[id];
    if (!old || wins(entry, old)) words[id] = entry;
  }
  const notes = { ...stored.notes };
  for (const [id, note] of Object.entries(incoming.notes)) {
    const old = notes[id];
    if (!old || time(note.at) > time(old.at)) notes[id] = note;
  }
  return { v: 1, words, notes };
}

/**
 * A stage as stored or received, older ones included (contract §2): `explain1` and `explain2`
 * read as `explain`, a relearning stage is dropped (its word stays in review), a missing
 * attempt is 0. Null when there is no stage to keep.
 */
export function readStage(value: unknown): SyncStage | null {
  if (!value || typeof value !== 'object') return null;
  const s = value as Record<string, unknown>;
  if (s.stage !== 'learning') return null;
  const next = s.next === 'explain1' || s.next === 'explain2' ? 'explain' : s.next;
  if (next !== 'know' && next !== 'explain' && next !== 'spell' && next !== 'use') return null;
  return {
    stage: 'learning',
    next,
    immediate: s.next === 'explain2' ? false : !!s.immediate,
    flawed: !!s.flawed,
    attempt: typeof s.attempt === 'number' ? s.attempt : 0,
  };
}

/** A document as received, or null when it is not a version-1 document. */
export function parseDoc(value: unknown): SyncDoc | null {
  if (!value || typeof value !== 'object') return null;
  const d = value as Partial<SyncDoc>;
  if (d.v !== 1 || typeof d.words !== 'object' || d.words === null) return null;
  const words: Record<string, SyncEntry> = {};
  for (const [id, e] of Object.entries(d.words as Record<string, Partial<SyncEntry>>)) {
    const entry = { card: e?.card ?? null, stage: readStage(e?.stage) };
    if (entry.card || entry.stage) words[id] = entry;
  }
  return { v: 1, words, notes: typeof d.notes === 'object' && d.notes !== null ? d.notes : {} };
}

/** Two entries say the same thing (so applying one over the other changes nothing). */
export const sameEntry = (a: SyncEntry | undefined, b: SyncEntry | undefined): boolean =>
  JSON.stringify(a ?? null) === JSON.stringify(b ?? null);
