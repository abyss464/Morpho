// "Explain it" puzzle: the learner rebuilds a word's definition from shuffled pieces by
// clicking them in order, so every studied word is put into words once, without a keyboard.

import { primarySense } from './components/parts';
import type { WordFull } from './types';

export interface Piece {
  id: number;
  text: string;
}

export interface Puzzle {
  /** Every piece on offer, shuffled: the definition's own pieces plus a few decoys. */
  pieces: Piece[];
  /** Piece ids in the order that spells the definition. */
  answer: number[];
}

// A new piece starts before these words, so pieces follow the definition's own clauses.
const BREAK_BEFORE = new Set([
  'that', 'who', 'which', 'where', 'when', 'whose', 'because', 'but',
  'if', 'without', 'such', 'than', 'while', 'until', 'unless',
]);
// A piece that runs long is split before one of these.
const SOFT_BREAK = new Set(['of', 'in', 'on', 'at', 'for', 'from', 'by', 'with', 'about', 'into', 'to', 'over', 'under']);
const JOINERS = new Set(['or', 'and']);
const MIN_WORDS = 2;
const MAX_WORDS = 7;
// The opening clause splits right after its verb: "A system is" | "a group of ...".
const COPULA = new Set(['is', 'are', 'means', 'mean']);

/** Splits a definition into 3-6 clause-sized pieces that read naturally on their own. */
export function chunk(text: string): string[] {
  const words = text.trim().split(/\s+/);
  const raw: string[][] = [];
  let cur: string[] = [];
  for (const word of words) {
    const bare = word.toLowerCase().replace(/[^a-z']/g, '');
    if (cur.length >= MIN_WORDS && BREAK_BEFORE.has(bare)) {
      raw.push(cur);
      cur = [];
    }
    cur.push(word);
    if (/[,;:]$/.test(word) && cur.length >= MIN_WORDS) {
      raw.push(cur);
      cur = [];
    }
  }
  if (cur.length) raw.push(cur);
  const first = raw[0];
  if (first && first.length >= 4) {
    const at = first.findIndex((x, k) => k >= 1 && k <= 4 && COPULA.has(x.toLowerCase()));
    if (at >= 0 && first.length - at - 1 >= 1) raw.splice(0, 1, first.slice(0, at + 1), first.slice(at + 1));
  }

  // Split pieces that are still long, preferably before a preposition near the middle.
  const split: string[][] = [];
  for (const piece of raw) {
    let rest = piece;
    while (rest.length > MAX_WORDS) {
      const mid = Math.floor(rest.length / 2);
      // Prefer splitting before "or"/"and", then before a preposition, nearest the middle.
      const near = (set: Set<string>) => {
        for (let d = 0; d < rest.length; d += 1) {
          for (const k of [mid - d, mid + d]) {
            if (k >= MIN_WORDS && rest.length - k >= MIN_WORDS && set.has(rest[k]!.toLowerCase())) return k;
          }
        }
        return -1;
      };
      let at = near(JOINERS);
      if (at < 0) at = near(SOFT_BREAK);
      if (at < 0) at = mid;
      split.push(rest.slice(0, at));
      rest = rest.slice(at);
    }
    split.push(rest);
  }

  // Fold one-word scraps into a neighbour, then merge the shortest neighbours while there are too many.
  const merged: string[][] = [];
  for (const piece of split) {
    const prev = merged[merged.length - 1];
    if (prev && merged.length > 1 && (piece.length < MIN_WORDS || prev.length < MIN_WORDS)) prev.push(...piece);
    else if (prev && merged.length === 1 && piece.length < MIN_WORDS) prev.push(...piece);
    else merged.push([...piece]);
  }
  while (merged.length > 6) {
    let best = 0;
    for (let k = 1; k < merged.length - 1; k += 1) {
      if (merged[k]!.length + merged[k + 1]!.length < merged[best]!.length + merged[best + 1]!.length) best = k;
    }
    merged.splice(best, 2, [...merged[best]!, ...merged[best + 1]!]);
  }
  return merged.map((p) => p.join(' '));
}

/** A small deterministic generator, so a word's puzzle looks the same every time it is opened. */
function seeded(seed: number): () => number {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13;
    s ^= s >>> 17;
    s ^= s << 5;
    return (s >>> 0) / 4294967296;
  };
}

function shuffle<T>(items: T[], rand: () => number): T[] {
  const out = [...items];
  for (let i = out.length - 1; i > 0; i -= 1) {
    const j = Math.floor(rand() * (i + 1));
    [out[i], out[j]] = [out[j]!, out[i]!];
  }
  return out;
}

/** Builds the puzzle for one word; decoys come from the other words of the same unit. */
export function buildPuzzle(w: WordFull, unitWords: WordFull[]): Puzzle | null {
  const sense = primarySense(w);
  if (!sense) return null;
  const parts = chunk(sense.def);
  const rand = seeded(w.id);
  const own = new Set(parts.map((p) => p.toLowerCase()));
  const pool = unitWords
    .filter((x) => x.id !== w.id)
    .flatMap((x) => {
      const p = primarySense(x);
      if (!p) return [];
      const head = x.word.toLowerCase();
      // Opening pieces ("A tax is") and pieces naming their own word would give the decoy away.
      return chunk(p.def)
        .slice(1)
        .filter((c) => !c.toLowerCase().includes(head) && !own.has(c.toLowerCase()));
    });
  const decoys = shuffle(pool, rand).slice(0, parts.length >= 5 ? 3 : 2);
  const pieces = [...parts, ...decoys].map((text, id) => ({ id, text }));
  return { pieces: shuffle(pieces, rand), answer: parts.map((_, id) => id) };
}
