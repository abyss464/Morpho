import { useMemo, useState } from 'react';
import { play } from '../audio';
import { seeded, shuffle } from '../explain';
import type { Outcome } from '../stream';
import type { WordFull } from '../types';
import { primarySense, wordPattern } from './parts';

/** The sentence split around the word to fill in: its example, else its definition. */
function gap(w: WordFull): { before: string; target: string; after: string; audio: string | null } | null {
  const ex = w.example;
  if (ex?.hl) return { before: ex.text.slice(0, ex.hl[0]), target: ex.text.slice(ex.hl[0], ex.hl[1]), after: ex.text.slice(ex.hl[1]), audio: ex.audio };
  for (const [text, audio] of [
    [ex?.text, ex?.audio ?? null],
    [primarySense(w)?.def, primarySense(w)?.audio ?? null],
  ] as const) {
    const m = text ? wordPattern(w.word)?.exec(text) : null;
    if (text && m) return { before: text.slice(0, m.index), target: m[0], after: text.slice(m.index + m[0].length), audio };
  }
  return null;
}

/**
 * Use it: the word is blanked out of its example and picked from four look-alike words.
 * A wrong pick turns red and shows what that word means; the learner picks again. Reports
 * clean (right first time) or failed once the right word is picked.
 */
export function Fill({ w, others, onSolved }: { w: WordFull; others: WordFull[]; onSolved: (outcome: Outcome) => void }) {
  const g = useMemo(() => gap(w), [w]);
  const options = useMemo(() => shuffle([w, ...others.slice(0, 3)], seeded(w.id + 7)), [w, others]);
  const [wrong, setWrong] = useState<number[]>([]);
  const [solved, setSolved] = useState(false);

  if (!g) return null;
  const pick = (o: WordFull) => {
    if (solved || wrong.includes(o.id)) return;
    if (o.id !== w.id) {
      setWrong([...wrong, o.id]);
      return;
    }
    setSolved(true);
    if (g.audio) play(g.audio);
    onSolved(wrong.length ? 'failed' : 'clean');
  };

  return (
    <div className="fill">
      <p className="gap">
        {g.before}
        <span className={solved ? 'blank filled' : 'blank'}>{solved ? g.target : ' '}</span>
        {g.after}
      </p>
      <div className="options" role="radiogroup" aria-label="Words">
        {options.map((o) => (
          <button
            key={o.id}
            type="button"
            role="radio"
            aria-checked={solved && o.id === w.id}
            className={solved && o.id === w.id ? 'option right' : wrong.includes(o.id) ? 'option wrong' : 'option'}
            disabled={solved || wrong.includes(o.id)}
            onClick={() => pick(o)}
          >
            {o.word}
          </button>
        ))}
      </div>
      {wrong.map((id) => {
        const o = options.find((x) => x.id === id);
        const def = o && primarySense(o)?.def;
        return def ? (
          <p key={id} className="whynot">
            <b>{o.word}</b>: {def}
          </p>
        ) : null;
      })}
      <div className="actions">
        {solved ? (
          <span className="status good">That's the word.</span>
        ) : wrong.length ? (
          <span className="status bad">Not that one. Pick again.</span>
        ) : (
          <span className="status">The wrong words look alike on purpose.</span>
        )}
      </div>
    </div>
  );
}
