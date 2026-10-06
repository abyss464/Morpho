import { useEffect, useMemo, useState } from 'react';
import { play } from '../audio';
import { Head, Picture, primarySense } from '../components/parts';
import { buildPuzzle } from '../explain';
import type { WordFull } from '../types';

const norm = (s: string) => s.trim().toLowerCase();

/**
 * The step after each study card: the learner rebuilds the word's definition by clicking its
 * pieces in order. Placing the last piece checks the answer; misplaced pieces turn red and go
 * back to the bank with a click. Moving on is possible only once the meaning is rebuilt.
 */
export function ExplainCard({
  w,
  unitWords,
  solved,
  onSolved,
}: {
  w: WordFull;
  unitWords: WordFull[];
  solved: boolean;
  onSolved: () => void;
}) {
  const puzzle = useMemo(() => buildPuzzle(w, unitWords), [w, unitWords]);
  const answerText = useMemo(
    () => (puzzle ? puzzle.answer.map((id) => norm(puzzle.pieces.find((p) => p.id === id)!.text)) : []),
    [puzzle],
  );
  const [placed, setPlaced] = useState<number[]>(() => (solved && puzzle ? puzzle.answer : []));
  const [checked, setChecked] = useState(false);
  const [misses, setMisses] = useState(0);

  useEffect(() => {
    setPlaced(solved && puzzle ? puzzle.answer : []);
    setChecked(false);
    setMisses(0);
    // Reset only when the word changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [w.id]);

  if (!puzzle) return null;
  const textOf = (id: number) => puzzle.pieces.find((p) => p.id === id)!.text;
  const right = (k: number) => norm(textOf(placed[k]!)) === answerText[k];
  const full = placed.length === puzzle.answer.length;

  const check = (next: number[]) => {
    if (next.length !== puzzle.answer.length) return;
    setChecked(true);
    if (next.every((id, k) => norm(textOf(id)) === answerText[k])) {
      onSolved();
      const def = primarySense(w)?.audio;
      if (def) play(def);
    } else {
      setMisses((m) => m + 1);
    }
  };

  const place = (id: number) => {
    if (solved || placed.includes(id)) return;
    const next = [...placed, id];
    setPlaced(next);
    setChecked(false);
    check(next);
  };
  const unplace = (id: number) => {
    if (solved) return;
    setPlaced(placed.filter((x) => x !== id));
    setChecked(false);
  };
  /** Keeps the correct opening pieces and adds the next one. */
  const hint = () => {
    let keep = 0;
    while (keep < placed.length && right(keep)) keep += 1;
    const head = placed.slice(0, keep);
    const want = answerText[keep];
    const piece = puzzle.pieces.find((p) => !head.includes(p.id) && norm(p.text) === want);
    if (!piece) return;
    const next = [...head, piece.id];
    setPlaced(next);
    setChecked(false);
    check(next);
  };

  const wrongShown = checked && full && !solved;

  return (
    <article className="card explain">
      <figure className="pic">
        <Picture w={w} eager />
      </figure>
      <div className="body">
        <span className="eyebrow">Explain it</span>
        <Head w={w} readAll={false} />
        <p className="ask">
          What does <b>{w.word}</b> mean? Click the pieces in order.
        </p>
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
              disabled={solved || placed.includes(p.id)}
              aria-hidden={placed.includes(p.id) || undefined}
            >
              {p.text}
            </button>
          ))}
        </div>
        )}
        <div className="actions">
          {solved ? (
            <span className="status good">That's the meaning. On to the next word.</span>
          ) : wrongShown ? (
            <span className="status bad">Not quite. Click the red pieces to take them back, then try again.</span>
          ) : (
            <span className="status">Some pieces belong to other words.</span>
          )}
          {!solved && placed.length > 0 && (
            <button type="button" className="link" onClick={() => { setPlaced([]); setChecked(false); }}>
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
    </article>
  );
}
