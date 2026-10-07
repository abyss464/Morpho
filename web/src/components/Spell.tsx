import { useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { play } from '../audio';
import { outcomeOf, retrySeed, seeded, shuffle } from '../explain';
import type { Outcome } from '../stream';
import type { WordFull } from '../types';
import { primarySense, wordPattern } from './parts';

const isLetter = (c: string) => /\p{L}/u.test(c);
// Decoy tiles come from common letters the word does not use.
const DECOY_LETTERS = 'etaoinshrdlucmfwypbgvk';

interface SpellPuzzle {
  /** One entry per character: the character when given (a hint letter, a space, a hyphen), else null. */
  slots: (string | null)[];
  /** The letters the blanks take, in order. */
  answer: string[];
  /** Tiles on offer, shuffled: the missing letters plus two decoys. */
  tiles: { id: number; t: string }[];
}

/** Spaces and hyphens are given; so is the first letter, and the last one too for words over five letters. */
function buildSpell(word: string, id: number, attempt: number): SpellPuzzle {
  const chars = [...word.toLowerCase()];
  const letters = chars.flatMap((c, i) => (isLetter(c) ? [i] : []));
  const hints = new Set([letters[0]]);
  if (letters.length > 5) hints.add(letters[letters.length - 1]);
  const slots = chars.map((c, i) => (!isLetter(c) || hints.has(i) ? c : null));
  const answer = chars.filter((_, i) => slots[i] === null);
  const rand = seeded(retrySeed(id + 13, attempt));
  const decoys = shuffle([...DECOY_LETTERS].filter((l) => !chars.includes(l)), rand).slice(0, 2);
  const tiles = shuffle([...answer, ...decoys], rand).map((t, k) => ({ id: k, t }));
  return { slots, answer, tiles };
}

/** The definition with every form of the word blanked out, so it names nothing. */
function masked(text: string, word: string): ReactNode {
  const re = wordPattern(word);
  if (!re) return text;
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(re)) {
    if (m.index > last) out.push(text.slice(last, m.index));
    out.push(<span key={m.index} className="masked" aria-label="the word" />);
    last = m.index + m[0].length;
  }
  out.push(text.slice(last));
  return out;
}

/**
 * Spell it: from the definition back to the word. The word's letters are blanks with one or
 * two given as hints; the learner taps letter tiles into them in order. Filling the last blank
 * checks it; wrong letters turn red and go back with a tap. "Show the next letter" and "Show
 * the word" are always there. Each wrong letter checked and each hint is a mistake; showing the
 * word fails it. Reports clean, shaky or failed once (contract §2).
 */
export function Spell({
  w,
  attempt = 0,
  onSolved,
}: {
  w: WordFull;
  attempt?: number;
  onSolved: (outcome: Outcome) => void;
}) {
  const puzzle = useMemo(() => buildSpell(w.word, w.id, attempt), [w, attempt]);
  const [filled, setFilled] = useState<(number | null)[]>(() => puzzle.answer.map(() => null));
  const [checked, setChecked] = useState(false);
  const [solved, setSolved] = useState(false);
  // Mistakes so far; a wrong letter left in its blank and checked again counts once.
  const mistakes = useRef({ n: 0, seen: new Set<string>(), revealed: false });

  const def = primarySense(w)?.def ?? '';
  const tileOf = (id: number) => puzzle.tiles.find((t) => t.id === id)!.t;
  const right = (k: number, slots = filled) => slots[k] != null && tileOf(slots[k]!) === puzzle.answer[k];
  const used = (id: number) => filled.includes(id);
  const nextOpen = filled.indexOf(null);

  const update = (next: (number | null)[]) => {
    setFilled(next);
    setChecked(false);
    if (next.includes(null)) return;
    setChecked(true);
    if (next.every((_, k) => right(k, next))) {
      setSolved(true);
      play(w.audio);
      onSolved(outcomeOf(mistakes.current.n, mistakes.current.revealed));
      return;
    }
    const m = mistakes.current;
    next.forEach((id, k) => {
      const key = `${k}:${id}`;
      if (!right(k, next) && !m.seen.has(key)) {
        m.seen.add(key);
        m.n += 1;
      }
    });
  };
  const place = (id: number) => {
    if (solved || used(id) || nextOpen < 0) return;
    update(filled.map((x, k) => (k === nextOpen ? id : x)));
  };
  const takeBack = (k: number) => {
    if (!solved) update(filled.map((x, i) => (i === k ? null : x)));
  };
  /** Puts the right letter into the first blank that is open or wrong. */
  const hint = () => {
    const k = filled.findIndex((_, i) => !right(i));
    const tile = puzzle.tiles.find((t) => t.t === puzzle.answer[k] && !filled.some((x, i) => x === t.id && right(i)));
    if (k < 0 || !tile) return;
    mistakes.current.n += 1;
    update(filled.map((x, i) => (i === k ? tile.id : x === tile.id ? null : x)));
  };
  const reveal = () => {
    const taken = new Set<number>();
    const next = puzzle.answer.map((letter) => {
      const tile = puzzle.tiles.find((t) => t.t === letter && !taken.has(t.id))!;
      taken.add(tile.id);
      return tile.id;
    });
    mistakes.current.revealed = true;
    update(next);
  };

  const wrongShown = checked && !filled.includes(null) && !solved;
  let blank = 0;

  return (
    <div className="rebuild">
      <p className="def masked-def">{masked(def, w.word)}</p>
      <div className={solved ? 'spell solved' : 'spell'} aria-label="The word" aria-live="polite">
        {puzzle.slots.map((c, i) => {
          if (c !== null) {
            return (
              <span key={i} className={isLetter(c) ? 'letter given' : 'letter sep'}>
                {c}
              </span>
            );
          }
          const k = blank++;
          const id = filled[k];
          if (id == null) {
            return <span key={i} className={k === nextOpen && !solved ? 'letter open next' : 'letter open'} />;
          }
          return (
            <button
              key={i}
              type="button"
              className={wrongShown && !right(k) ? 'letter wrong' : 'letter'}
              onClick={() => takeBack(k)}
              disabled={solved}
              aria-label={`${tileOf(id)}, remove`}
            >
              {tileOf(id)}
            </button>
          );
        })}
      </div>
      {!solved && (
        <div className="tiles" aria-label="Letters">
          {puzzle.tiles.map((t) => (
            <button
              key={t.id}
              type="button"
              className={used(t.id) ? 'tile used' : 'tile'}
              onClick={() => place(t.id)}
              disabled={used(t.id)}
              aria-hidden={used(t.id) || undefined}
            >
              {t.t}
            </button>
          ))}
        </div>
      )}
      <div className="actions">
        {solved ? (
          <span className="status good">That's the word.</span>
        ) : wrongShown ? (
          <span className="status bad">Not quite. Tap the red letters to take them back.</span>
        ) : (
          <span className="status">Spell the word. Two of the letters are not in it.</span>
        )}
        {!solved && filled.some((x) => x != null) && (
          <button type="button" className="link" onClick={() => update(filled.map(() => null))}>
            Start over
          </button>
        )}
        {!solved && (
          <button type="button" className="link" onClick={hint}>
            Show the next letter
          </button>
        )}
        {!solved && (
          <button type="button" className="link" onClick={reveal}>
            Show the word
          </button>
        )}
      </div>
    </div>
  );
}
