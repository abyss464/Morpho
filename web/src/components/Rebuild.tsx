import { useMemo, useState } from 'react';
import { play } from '../audio';
import { buildPuzzle } from '../explain';
import type { Difficulty } from '../explain';
import type { Outcome } from '../stream';
import type { WordFull } from '../types';
import { primarySense } from './parts';

const norm = (s: string) => s.trim().toLowerCase();

/**
 * Rebuild the meaning: the learner taps the definition's pieces in order, decoys mixed in.
 * Placing the last piece checks it; misplaced pieces turn red and go back with a tap; after a
 * failed check a hint can place the next correct piece. Reports clean, shaky or failed once.
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
  const [placed, setPlaced] = useState<number[]>([]);
  const [checked, setChecked] = useState(false);
  const [misses, setMisses] = useState(0);
  const [hinted, setHinted] = useState(false);
  const [solved, setSolved] = useState(false);

  if (!puzzle) return null;
  const answer = puzzle.answer.map((id) => norm(puzzle.pieces.find((p) => p.id === id)!.text));
  const textOf = (id: number) => puzzle.pieces.find((p) => p.id === id)!.text;
  const right = (k: number) => norm(textOf(placed[k]!)) === answer[k];
  const full = placed.length === answer.length;

  const check = (next: number[], usedHint: boolean) => {
    if (next.length !== answer.length) return;
    setChecked(true);
    if (next.every((id, k) => norm(textOf(id)) === answer[k])) {
      setSolved(true);
      const def = primarySense(w)?.audio;
      if (def) play(def);
      onSolved(usedHint || misses >= 2 ? 'failed' : misses === 1 ? 'shaky' : 'clean');
    } else {
      setMisses((m) => m + 1);
    }
  };

  const place = (id: number) => {
    if (solved || placed.includes(id)) return;
    const next = [...placed, id];
    setPlaced(next);
    setChecked(false);
    check(next, hinted);
  };
  const unplace = (id: number) => {
    if (solved) return;
    setPlaced(placed.filter((x) => x !== id));
    setChecked(false);
  };
  const hint = () => {
    let keep = 0;
    while (keep < placed.length && right(keep)) keep += 1;
    const head = placed.slice(0, keep);
    const piece = puzzle.pieces.find((p) => !head.includes(p.id) && norm(p.text) === answer[keep]);
    if (!piece) return;
    const next = [...head, piece.id];
    setHinted(true);
    setPlaced(next);
    setChecked(false);
    check(next, true);
  };

  const wrongShown = checked && full && !solved;

  return (
    <div className="rebuild">
      <div className={solved ? 'tray solved' : 'tray'} aria-label="Your explanation" aria-live="polite">
        {placed.length === 0 && <span className="ph">Your explanation is built here.</span>}
        {placed.map((id, k) => (
          <button
            key={id}
            type="button"
            className={wrongShown && !right(k) ? 'piece wrong' : 'piece'}
            onClick={() => unplace(id)}
            disabled={solved}
            aria-label={`${textOf(id)}, remove`}
          >
            {textOf(id)}
          </button>
        ))}
      </div>
      {!solved && (
        <div className="bank" aria-label="Pieces">
          {puzzle.pieces.map((p) => (
            <button
              key={p.id}
              type="button"
              className={placed.includes(p.id) ? 'piece used' : 'piece'}
              onClick={() => place(p.id)}
              disabled={placed.includes(p.id)}
              aria-hidden={placed.includes(p.id) || undefined}
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
          <span className="status">Some pieces belong to other words.</span>
        )}
        {!solved && placed.length > 0 && (
          <button
            type="button"
            className="link"
            onClick={() => {
              setPlaced([]);
              setChecked(false);
            }}
          >
            Start over
          </button>
        )}
        {!solved && misses > 0 && (
          <button type="button" className="link" onClick={hint}>
            Show the next piece
          </button>
        )}
      </div>
    </div>
  );
}
