// "Explain it" puzzle: the learner completes a word's definition by clicking pieces into its
// blanks in order, so every studied word is put into words once, without a keyboard.

import { primarySense, wordPattern } from './components/parts';
import type { WordFull } from './types';

export interface Piece {
  id: number;
  text: string;
}

/** One stretch of the definition: text given in place, or the blank with this index. */
export type Segment = { text: string } | { blank: number };

export interface Puzzle {
  /**
   * The definition in order. Given: the opening that names the word ("A community is"), the word
   * wherever it appears, prepositions and punctuation. Everything else is a blank.
   */
  template: Segment[];
  /** Every piece on offer, shuffled: one per blank plus a few decoys. */
  pieces: Piece[];
  /** For each blank, the id of the piece that fills it. */
  answer: number[];
}

/** Longest piece, in words, and most blanks: easy pieces follow whole clauses, hard pieces are cut finer. */
export type Difficulty = 'easy' | 'hard';
const MAX_WORDS: Record<Difficulty, number> = { easy: 7, hard: 4 };
const MAX_BLANKS: Record<Difficulty, number> = { easy: 6, hard: 8 };
const MIN_WORDS = 2;

// A new piece starts before these words, so pieces follow the definition's own clauses.
const BREAK_BEFORE = new Set([
  'that', 'who', 'which', 'where', 'when', 'whose', 'because', 'but',
  'if', 'without', 'such', 'than', 'while', 'until', 'unless',
]);
const JOINERS = new Set(['or', 'and']);
// The definition's grammar rather than its meaning: given in place, never picked.
const PREPOSITIONS = new Set([
  'of', 'in', 'on', 'at', 'for', 'from', 'by', 'with', 'about', 'into', 'onto', 'to', 'as',
  'over', 'under', 'through', 'between', 'among', 'across', 'against', 'during', 'within',
  'without', 'behind', 'below', 'above', 'around', 'along', 'towards', 'toward', 'upon',
  'beneath', 'beyond',
]);
// First words of two-word prepositions ("because of", "such as"), given with them.
const LEADS = new Set(['because', 'instead', 'according', 'due', 'apart', 'such', 'out']);
// An opening runs on through a copula right after the word ("A system is", "a case is also"),
// or, in an "If"/"When" clause, to that clause's comma ("If you create something,").
const COPULA = new Set(['is', 'are', 'means', 'mean', 'was', 'were']);
const OPENERS = new Set(['if', 'when']);
/** The word must be named within this many words of a sense's start for it to have an opening. */
const OPENING_REACH = 12;
/** An "If" clause closes within this many words after the word, or the opening ends at the word. */
const CLAUSE_REACH = 8;

const bare = (word: string) => word.toLowerCase().replace(/[^a-z']/g, '');
const TRAILING = /[,;:.!?]+$/;

/** What is given in place, word by word: 'name' for openings and the word itself, 'glue' for prepositions. */
type Given = 'name' | 'glue' | null;

function givenWords(text: string, words: string[], word: string, glue: boolean): Given[] {
  const marks: Given[] = words.map(() => null);
  const starts: number[] = [];
  let at = 0;
  for (const w of words) {
    at = text.indexOf(w, at);
    starts.push(at);
    at += w.length;
  }
  const wordAt = (offset: number) => {
    let k = 0;
    while (k + 1 < starts.length && starts[k + 1]! <= offset) k += 1;
    return k;
  };

  // The word itself, every time it appears.
  const named = words.map(() => false);
  const re = wordPattern(word);
  if (re) {
    for (const m of text.matchAll(re)) {
      const last = wordAt(m.index + m[0].length - 1);
      for (let k = wordAt(m.index); k <= last; k += 1) named[k] = true;
    }
  }

  // Each sense (senses are separated by ";") may open by naming the word.
  let from = 0;
  for (let k = 0; k < words.length; k += 1) {
    if (!/;$/.test(words[k]!) && k < words.length - 1) continue;
    const to = k + 1;
    const hit = named.findIndex((n, i) => n && i >= from && i < to);
    if (hit >= 0 && hit - from < OPENING_REACH) {
      let end = hit;
      while (end < to && named[end]) end += 1;
      const closed = /[,;:]$/.test(words[end - 1]!);
      let c = end;
      if (bare(words[c] ?? '') === 'also') c += 1;
      if (!closed && COPULA.has(bare(words[c] ?? '')) && c + 1 < to) {
        end = c + 1;
        if (bare(words[end] ?? '') === 'also') end += 1;
      } else if (!closed && OPENERS.has(bare(words[from]!))) {
        const comma = words.findIndex((x, i) => i >= end && i < Math.min(to - 1, end + CLAUSE_REACH) && /[,;:]$/.test(x));
        if (comma >= 0) end = comma + 1;
      }
      if (end < to) for (let i = from; i < end; i += 1) marks[i] = 'name';
    }
    from = to;
  }

  words.forEach((w, k) => {
    if (named[k]) marks[k] = 'name';
    else if (!glue) return;
    else if (!marks[k] && PREPOSITIONS.has(bare(w))) marks[k] = 'glue';
    else if (!marks[k] && LEADS.has(bare(w)) && PREPOSITIONS.has(bare(words[k + 1] ?? ''))) marks[k] = 'glue';
  });
  return marks;
}

/** Where to cut a run of words in two: before "or"/"and" nearest the middle, else the middle. */
function cutAt(words: string[]): number {
  const mid = Math.floor(words.length / 2);
  for (let d = 0; d < words.length; d += 1) {
    for (const k of [mid - d, mid + d]) {
      if (k >= MIN_WORDS && words.length - k >= MIN_WORDS && JOINERS.has(bare(words[k]!))) return k;
    }
  }
  return mid;
}

/** Splits a run of words between given text into clause-sized pieces. */
function pieces(run: string[], max: number): string[][] {
  const raw: string[][] = [];
  let cur: string[] = [];
  for (const word of run) {
    // "or because" stays together: a piece never ends on a joiner.
    if (cur.length >= MIN_WORDS && BREAK_BEFORE.has(bare(word)) && !JOINERS.has(bare(cur[cur.length - 1]!))) {
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

  const split: string[][] = [];
  for (const piece of raw) {
    let rest = piece;
    while (rest.length > max) {
      const at = cutAt(rest);
      split.push(rest.slice(0, at));
      rest = rest.slice(at);
    }
    split.push(rest);
  }

  // One-word scraps fold into a neighbour within the run.
  const merged: string[][] = [];
  for (const piece of split) {
    const prev = merged[merged.length - 1];
    if (prev && (piece.length < MIN_WORDS || prev.length < MIN_WORDS)) prev.push(...piece);
    else merged.push([...piece]);
  }
  return merged;
}

type Part = { given: Given; words: string[] } | { blank: string[] };

/**
 * Cuts a definition into given text and blanks, at most MAX_BLANKS of them. Prepositions are
 * blanks only when the meaning is nothing else ("Regarding means about.").
 */
function cut(def: string, word: string, difficulty: Difficulty, glue = true): Part[] {
  const text = def.trim();
  const words = text.split(/\s+/);
  const marks = givenWords(text, words, word, glue);
  const parts: Part[] = [];
  const give = (w: string, kind: Given) => {
    const last = parts[parts.length - 1];
    if (last && 'given' in last && (last.given === kind || /^[,;:.!?]+$/.test(w))) last.words.push(w);
    else parts.push({ given: kind, words: [w] });
  };
  let run: string[] = [];
  const flush = () => {
    for (const p of pieces(run, MAX_WORDS[difficulty])) {
      // Closing punctuation is given, so a piece's own ending never shows where it goes.
      const last = p[p.length - 1]!;
      const punct = last.match(TRAILING)?.[0];
      const body = punct && last.length > punct.length ? [...p.slice(0, -1), last.slice(0, -punct.length)] : p;
      parts.push({ blank: body });
      if (punct && body !== p) give(punct, 'glue');
    }
    run = [];
  };
  words.forEach((w, k) => {
    if (marks[k]) {
      flush();
      give(w, marks[k]);
    } else run.push(w);
  });
  flush();

  // Too many blanks: join the shortest pair that only prepositions or nothing separate.
  const blanks = () => parts.filter((p) => 'blank' in p).length;
  while (blanks() > MAX_BLANKS[difficulty]) {
    let best = -1;
    let bestSize = Infinity;
    for (let i = 0; i < parts.length; i += 1) {
      const a = parts[i]!;
      if (!('blank' in a)) continue;
      let j = i + 1;
      const glue: string[] = [];
      while (j < parts.length && 'given' in parts[j]!) {
        const g = parts[j] as { given: Given; words: string[] };
        if (g.given !== 'glue' || g.words.some((w) => /[;.!?]/.test(w))) break;
        glue.push(...g.words);
        j += 1;
      }
      const b = parts[j];
      if (!b || !('blank' in b) || (j > i + 1 && glue.length === 0)) continue;
      const size = a.blank.length + glue.length + b.blank.length;
      if (size < bestSize) {
        best = i;
        bestSize = size;
      }
    }
    if (best < 0) break;
    let j = best + 1;
    const joined = [...(parts[best] as { blank: string[] }).blank];
    while ('given' in parts[j]!) {
      for (const w of (parts[j] as { words: string[] }).words) {
        if (/^[,;:]+$/.test(w)) joined[joined.length - 1] += w;
        else joined.push(w);
      }
      j += 1;
    }
    joined.push(...(parts[j] as { blank: string[] }).blank);
    parts.splice(best, j - best + 1, { blank: joined });
  }

  const only = parts.filter((p) => 'blank' in p);
  if (only.length === 0 && glue) return cut(def, word, difficulty, false);
  // A meaning of four words or more is never a single blank: there is always an order to rebuild.
  if (only.length === 1) {
    const p = only[0] as { blank: string[] };
    if (p.blank.length >= 2 * MIN_WORDS) {
      const at = cutAt(p.blank);
      parts.splice(parts.indexOf(p), 1, { blank: p.blank.slice(0, at) }, { blank: p.blank.slice(at) });
    }
  }
  return parts;
}

/** A small deterministic generator, so a word's puzzle looks the same every time it is opened. */
export function seeded(seed: number): () => number {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13;
    s ^= s >>> 17;
    s ^= s << 5;
    return (s >>> 0) / 4294967296;
  };
}

export function shuffle<T>(items: T[], rand: () => number): T[] {
  const out = [...items];
  for (let i = out.length - 1; i > 0; i -= 1) {
    const j = Math.floor(rand() * (i + 1));
    [out[i], out[j]] = [out[j]!, out[i]!];
  }
  return out;
}

/** The pieces that fill a definition's blanks, in order. */
function blankTexts(parts: Part[]): string[] {
  return parts.flatMap((p) => ('blank' in p ? [p.blank.join(' ')] : []));
}

/**
 * Builds the puzzle for one word. Decoys are pieces of the pool words' definitions: easy
 * puzzles draw from the words being learned alongside, hard ones from every word met.
 */
/** The seed for a task's shuffle: the first attempt's own seed, moved on for each retry (contract §3). */
export const retrySeed = (seed: number, attempt = 0) => seed + attempt * 7919;

/** Outcome from a task's mistakes: none is clean, one still passes as shaky, more fail (contract §2). */
export const outcomeOf = (mistakes: number, revealed: boolean) =>
  revealed || mistakes >= 2 ? 'failed' : mistakes === 1 ? 'shaky' : 'clean';

export function buildPuzzle(w: WordFull, pool: WordFull[], difficulty: Difficulty = 'easy', attempt = 0): Puzzle | null {
  const sense = primarySense(w);
  if (!sense) return null;
  const parts = cut(sense.def, w.word, difficulty);
  const answerTexts = blankTexts(parts);
  const rand = seeded(retrySeed(w.id, attempt));
  const own = new Set(answerTexts.map((p) => p.toLowerCase()));
  const candidates = pool
    .filter((x) => x.id !== w.id)
    .flatMap((x) => {
      const p = primarySense(x);
      return p ? blankTexts(cut(p.def, x.word, difficulty)).filter((c) => !own.has(c.toLowerCase())) : [];
    });
  const count = (difficulty === 'hard' ? 3 : 2) + (answerTexts.length >= 5 ? 1 : 0);
  const decoys = shuffle([...new Set(candidates)], rand).slice(0, count);
  let blank = 0;
  const template: Segment[] = parts.map((p) => ('blank' in p ? { blank: blank++ } : { text: p.words.join(' ') }));
  const all = [...answerTexts, ...decoys].map((text, id) => ({ id, text }));
  return { template, pieces: shuffle(all, rand), answer: answerTexts.map((_, id) => id) };
}
