import { useCallback, useEffect, useRef, useState } from 'react';
import { Rating } from 'ts-fsrs';
import type { Grade } from 'ts-fsrs';
import { loadWords, prefetchImage, useAsync } from '../api';
import type { Index } from '../api';
import { Definition, ExampleBlock, Head, Meta, Mine, Picture } from '../components/parts';
import { isTyping, plural, useKeydown, useNow } from '../hooks';
import { href } from '../router';
import {
  dueAt,
  dueIds,
  formatInterval,
  getProgress,
  preview,
  rate,
  saveNote,
  upcomingIds,
  useProgress,
} from '../store';
import type { StoredCard } from '../store';

const GRADES: { g: Grade; label: string }[] = [
  { g: Rating.Again, label: 'Again' },
  { g: Rating.Hard, label: 'Hard' },
  { g: Rating.Good, label: 'Good' },
  { g: Rating.Easy, label: 'Easy' },
];

interface Turn {
  id: number;
  /** The card as it was when this turn started; every rating is scheduled from it. */
  prev: StoredCard;
  phase: 'ask' | 'shown';
  answer: string;
  hint: boolean;
  grade: Grade | null;
  extra: boolean;
}

export function Review({ index }: { index: Index }) {
  const progress = useProgress();
  const now = useNow(15000);
  const [more, setMore] = useState(false);
  const [turn, setTurn] = useState<Turn | null>(null);
  const [done, setDone] = useState(0);
  const seen = useRef(new Set<number>());
  const textarea = useRef<HTMLTextAreaElement>(null);

  /** Due words first; studied words that are not due yet only after the learner asks for more. */
  const pick = useCallback(
    (exclude?: number): { id: number; extra: boolean } | null => {
      const p = getProgress();
      const t = Date.now();
      const due = dueIds(p, t, index.pos).find((id) => id !== exclude && index.pos.has(id));
      if (due !== undefined) return { id: due, extra: false };
      if (!more) return null;
      const up = upcomingIds(p, t, index.pos).find((id) => id !== exclude && !seen.current.has(id) && index.pos.has(id));
      return up !== undefined ? { id: up, extra: true } : null;
    },
    [index, more],
  );

  const start = useCallback((next: { id: number; extra: boolean } | null) => {
    const card = next ? getProgress().cards[next.id] : undefined;
    setTurn(
      next && card
        ? { id: next.id, prev: card, phase: 'ask', answer: '', hint: false, grade: null, extra: next.extra }
        : null,
    );
  }, []);

  // Start (or resume after the queue refills) whenever there is no card on screen.
  useEffect(() => {
    if (!turn) start(pick());
  }, [turn, now, more, pick, start]);

  const word = useAsync(
    () => (turn ? loadWords([turn.id]).then((m) => m.get(turn.id) ?? null) : Promise.resolve(null)),
    [turn?.id],
  );
  const w = word.status === 'ok' ? word.data : null;

  useEffect(() => {
    if (turn?.phase === 'ask' && w) textarea.current?.focus();
  }, [turn?.id, turn?.phase, w]);

  // While the answer is shown, fetch the next card's data and picture.
  useEffect(() => {
    if (turn?.phase !== 'shown') return;
    const n = pick(turn.id);
    if (n) void loadWords([n.id]).then((m) => prefetchImage(m.get(n.id)));
  }, [turn?.phase, turn?.id, pick]);

  const applyGrade = (t: Turn, g: Grade) => {
    rate(t.id, t.prev, g);
    if (t.grade === null) setDone((d) => d + 1);
    setTurn({ ...t, phase: 'shown', grade: g });
  };

  const reveal = (forgot: boolean) => {
    if (!turn || turn.phase !== 'ask') return;
    const text = turn.answer.trim();
    if (!text && !forgot) return;
    if (text) saveNote(turn.id, text);
    if (forgot) applyGrade(turn, Rating.Again);
    else setTurn({ ...turn, phase: 'shown' });
  };

  const next = () => {
    if (!turn || turn.grade === null) return;
    seen.current.add(turn.id);
    start(pick(turn.id));
    window.scrollTo(0, 0);
  };

  useKeydown((e) => {
    if (!turn) return;
    if (turn.phase === 'ask') {
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        reveal(false);
      }
      return;
    }
    if (isTyping(e) || e.altKey || e.ctrlKey || e.metaKey) return;
    const g = GRADES.find((x) => String(x.g) === e.key);
    if (g) {
      e.preventDefault();
      applyGrade(turn, g.g);
    } else if (e.key === 'Enter' && turn.grade !== null && !(e.target instanceof Element && e.target.closest('button, a'))) {
      e.preventDefault();
      next();
    }
  });

  const dueNow = dueIds(progress, now).filter((id) => index.pos.has(id) && id !== turn?.id).length;

  /* ---------- nothing on screen ---------- */
  if (!turn) {
    const studied = Object.keys(progress.cards).filter((id) => index.pos.has(Number(id))).length;
    const upcoming = upcomingIds(progress, now).filter((id) => index.pos.has(id));
    const unseen = upcoming.filter((id) => !seen.current.has(id));
    const soon = upcoming[0] !== undefined ? progress.cards[upcoming[0]] : undefined;

    if (!studied) {
      return (
        <section className="notice">
          <span className="eyebrow">Review</span>
          <h1>Nothing to review yet.</h1>
          <p>Study a unit first. When you finish a unit, its words join your review.</p>
          <a className="btn primary" href={href.home()}>
            Go to units
          </a>
        </section>
      );
    }
    return (
      <section className="notice">
        <span className="eyebrow">Review{done ? ` · ${plural(done, 'word')} reviewed this session` : ''}</span>
        <h1>{more && !unseen.length ? 'You went through every studied word.' : 'All caught up.'}</h1>
        <p>
          No word is due right now
          {soon ? `; the next one is due in ${formatInterval(dueAt(soon) - now)}` : ''}. You have studied{' '}
          {plural(studied, 'word')} so far.
          {unseen.length > 0 && !more ? ' You can keep going with words that are not due yet.' : ''}
        </p>
        <div className="actions">
          {unseen.length > 0 && !more && (
            <button type="button" className="btn primary" onClick={() => setMore(true)}>
              Review more &middot; {unseen.length} not due yet
            </button>
          )}
          <a className={unseen.length > 0 && !more ? 'btn' : 'btn primary'} href={href.home()}>
            Back to units
          </a>
        </div>
      </section>
    );
  }

  if (word.status === 'error') {
    return (
      <section className="notice">
        <h2>Could not load this word.</h2>
        <p>{word.error}</p>
      </section>
    );
  }
  if (!w) return <p className="loading">Loading&hellip;</p>;

  const shown = turn.phase === 'shown';
  const intervals = shown ? preview(turn.prev) : null;

  return (
    <>
      <article className="card" key={w.id}>
        <figure className="pic">
          {shown || turn.hint ? (
            <Picture w={w} eager />
          ) : (
            <div className="cover">
              <span className="qm" aria-hidden="true">
                ?
              </span>
              <button
                type="button"
                className="btn"
                onClick={() => {
                  setTurn({ ...turn, hint: true });
                  textarea.current?.focus();
                }}
              >
                Show picture
              </button>
            </div>
          )}
        </figure>
        <div className="body">
          <Head w={w} readAll={shown} />
          <Meta w={w} showPos={shown} />
          {!shown ? (
            <>
              <label className="ask" htmlFor="answer">
                Explain it in your own words, from memory.
              </label>
              <textarea
                id="answer"
                ref={textarea}
                rows={3}
                placeholder="For example: If you ..., you ..."
                value={turn.answer}
                onChange={(e) => setTurn({ ...turn, answer: e.target.value })}
              />
              <div className="actions">
                <button type="button" className="btn primary" disabled={!turn.answer.trim()} onClick={() => reveal(false)}>
                  Compare
                </button>
                <button type="button" className="link" onClick={() => reveal(true)}>
                  I don't remember
                </button>
                <span className="hintnote keyhint">
                  <span className="kbd">Ctrl</span> + <span className="kbd">Enter</span> to compare
                </span>
              </div>
            </>
          ) : (
            <>
              {turn.answer.trim() ? (
                <blockquote className="mine">
                  <span className="eyebrow">Your explanation</span>
                  <p>{turn.answer.trim()}</p>
                </blockquote>
              ) : (
                <Mine
                  note={null}
                  emptyText={
                    progress.notes[w.id]
                      ? `Nothing written this time. Last time you wrote: "${progress.notes[w.id]!.text}"`
                      : 'Nothing written this time.'
                  }
                />
              )}
              <Definition w={w} />
              <ExampleBlock w={w} />
              <div className="grade">
                <span className="glabel">How well did you know it?</span>
                <div className="seg" role="radiogroup" aria-label="Rating">
                  {GRADES.map(({ g, label }) => (
                    <button
                      key={g}
                      type="button"
                      role="radio"
                      data-v={g}
                      aria-checked={turn.grade === g}
                      title={`Key ${g}`}
                      onClick={() => applyGrade(turn, g)}
                    >
                      <span>{label}</span>
                      {intervals && <span className="iv">{formatInterval(intervals[g].getTime() - Date.now())}</span>}
                    </button>
                  ))}
                </div>
                <span className="hintnote keyhint">
                  Keys <span className="kbd">1</span>&ndash;<span className="kbd">4</span>
                </span>
              </div>
            </>
          )}
        </div>
      </article>
      <div className="nav">
        <span className="count">
          {turn.extra ? 'Extra review · ' : ''}
          {plural(done, 'word')} reviewed &middot; {dueNow} due
        </span>
        {shown && (
          <button type="button" className="btn primary" disabled={turn.grade === null} onClick={next}>
            Next &rarr;
          </button>
        )}
      </div>
    </>
  );
}
