import { useMemo, useState } from 'react';
import { play } from '../audio';
import { buildPuzzle } from '../explain';
import type { Difficulty } from '../explain';
import type { Outcome } from '../stream';
import type { WordFull } from '../types';
import { primarySense } from './parts';

const norm = (s: string) => s.trim().toLowerCase();

/**
 * Rebuild the meaning: the definition stands with its blanks open, the part that names the word,
 * its prepositions and punctuation already in place; the learner taps pieces into the blanks in
 * order, decoys mixed in. Filling the last blank checks it; wrong pieces turn red and go back with
 * a tap. At any time a hint fills the first wrong or open blank, or the whole answer is shown;
 * either counts as failed. Reports clean, shaky or failed once.
 */
export function Rebuild({
  w,
  pool,
  difficulty,
  onSolved,
}: {
  w: WordFull;
  pool: WordFull[];
  difficulty: Difficulty;
  onSolved: (outcome: Outcome) => void;
}) {
  const puzzle = useMemo(() => buildPuzzle(w, pool, difficulty), [w, pool, difficulty]);
  const [filled, setFilled] = useState<(number | null)[]>(() => puzzle?.answer.map(() => null) ?? []);
  const [checked, setChecked] = useState(false);
  const [misses, setMisses] = useState(0);
  const [hinted, setHinted] = useState(false);
  const [solved, setSolved] = useState(false);

  if (!puzzle) return null;
  const textOf = (id: number) => puzzle.pieces.find((p) => p.id === id)!.text;
  const answer = puzzle.answer.map((id) => norm(textOf(id)));
  const right = (k: number, slots = filled) => slots[k] != null && norm(textOf(slots[k]!)) === answer[k];
  const used = (id: number) => filled.includes(id);
  const nextOpen = filled.indexOf(null);

  const check = (next: (number | null)[], usedHint: boolean) => {
    if (next.includes(null)) return;
    setChecked(true);
    if (next.every((_, k) => right(k, next))) {
      setSolved(true);
      const def = primarySense(w)?.audio;
      if (def) play(def);
      onSolved(usedHint || misses >= 2 ? 'failed' : misses === 1 ? 'shaky' : 'clean');
    } else {
      setMisses((m) => m + 1);
    }
  };

  const update = (next: (number | null)[], usedHint = hinted) => {
    setFilled(next);
    setChecked(false);
    check(next, usedHint);
  };
  const place = (id: number) => {
    if (solved || used(id) || nextOpen < 0) return;
    update(filled.map((x, k) => (k === nextOpen ? id : x)));
  };
  const takeBack = (k: number) => {
    if (solved) return;
    update(filled.map((x, i) => (i === k ? null : x)));
  };
  const hint = () => {
    const k = filled.findIndex((_, i) => !right(i));
    const piece = puzzle.pieces.find((p) => norm(p.text) === answer[k] && !filled.some((x, i) => x === p.id && right(i)));
    if (k < 0 || !piece) return;
    setHinted(true);
    update(
      filled.map((x, i) => (i === k ? piece.id : x === piece.id ? null : x)),
      true,
    );
  };

  const wrongShown = checked && !filled.includes(null) && !solved;

  return (
    <div className="rebuild">
      <p className={solved ? 'tray solved' : 'tray'} aria-label="Your explanation" aria-live="polite">
        {puzzle.template.map((s, i) => {
          if ('text' in s) {
            return (
              <span key={i} className={/^[,;:.!?]/.test(s.text) ? 'given punct' : 'given'}>
                {s.text}
              </span>
            );
          }
          const id = filled[s.blank];
          if (id == null) {
            return (
              <span key={i} className={s.blank === nextOpen && !solved ? 'blank next' : 'blank'} aria-label="blank" />
            );
          }
          return (
            <button
              key={i}
              type="button"
              className={wrongShown && !right(s.blank) ? 'piece wrong' : 'piece'}
              onClick={() => takeBack(s.blank)}
              disabled={solved}
              aria-label={`${textOf(id)}, remove`}
            >
              {textOf(id)}
            </button>
          );
        })}
      </p>
      {!solved && (
        <div className="bank" aria-label="Pieces">
          {puzzle.pieces.map((p) => (
            <button
              key={p.id}
              type="button"
              className={used(p.id) ? 'piece used' : 'piece'}
              onClick={() => place(p.id)}
              disabled={used(p.id)}
              aria-hidden={used(p.id) || undefined}
            >
              {p.text}
            </button>
          ))}
        </div>
      )}
      <div className="actions">
        {solved ? (
          <span className="status good">That's the meaning.</span>
        ) : wrongShown ? (
          <span className="status bad">Not quite. Tap the red pieces to take them back.</span>
        ) : (
          <span className="status">Fill the blanks in order. Some pieces belong to other words.</span>
        )}
        {!solved && filled.some((x) => x != null) && (
          <button type="button" className="link" onClick={() => update(filled.map(() => null))}>
            Start over
          </button>
        )}
        {!solved && (
          <button type="button" className="link" onClick={hint}>
            Show the next piece
          </button>
        )}
        {!solved && (
          <button type="button" className="link" onClick={() => update(puzzle.answer, true)}>
            Show the answer
          </button>
        )}
      </div>
    </div>
  );
}
