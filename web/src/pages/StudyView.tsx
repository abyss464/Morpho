import { useEffect } from 'react';
import { loadUnit, prefetchImage, unitWordIds, useAsync } from '../api';
import type { Index } from '../api';
import { readAloud, WordCard } from '../components/parts';
import { isTyping, plural, useKeydown, useNow } from '../hooks';
import { go, href } from '../router';
import { dueIds, markStudied, strength, STRENGTH_LABEL, useProgress } from '../store';
import { UnitNotFound } from './UnitPage';

export function StudyView({ index, unit, at }: { index: Index; unit: number; at: number }) {
  const valid = unit >= 1 && unit <= index.unitCount;
  const data = useAsync(() => (valid ? loadUnit(unit) : Promise.resolve([])), [unit, valid]);
  const progress = useProgress();
  const words = data.status === 'ok' ? data.data : [];
  const n = words.length;
  const i = Math.min(Math.max(1, at), Math.max(1, n));
  const w = words[i - 1];
  const last = i === n;

  /** Moving on to the next card reads it aloud once; going back or jumping does not. */
  const step = (k: number, read = false) => {
    if (k < 1 || k > n) return;
    go(href.study(unit, k), true);
    const next = words[k - 1];
    if (read && next) readAloud(next);
  };
  const finish = () => {
    markStudied(words.map((x) => x.id));
    go(href.done(unit));
  };

  useEffect(() => {
    prefetchImage(words[i]);
  }, [words, i]);

  useKeydown((e) => {
    if (isTyping(e) || e.altKey || e.ctrlKey || e.metaKey || !n) return;
    if (e.key === 'ArrowLeft') step(i - 1);
    else if (e.key === 'ArrowRight') step(i + 1, true);
  });

  if (!valid) return <UnitNotFound unit={unit} index={index} />;

  const studied = words.filter((x) => progress.cards[x.id]).length;

  return (
    <>
      <div className="crumbs">
        <a href={href.unit(unit)}>&larr; Unit {unit} word list</a>
        <span className="hintnote keyhint">
          <span className="kbd">&larr;</span> <span className="kbd">&rarr;</span> to move
        </span>
      </div>

      {data.status === 'loading' && <p className="loading">Loading Unit {unit}&hellip;</p>}
      {data.status === 'error' && (
        <section className="notice">
          <h2>Could not load Unit {unit}.</h2>
          <p>{data.error}</p>
        </section>
      )}

      {w && (
        <>
          <WordCard key={w.id} w={w} variant="study" note={progress.notes[w.id]} />
          <div className="nav">
            <button type="button" className="btn" disabled={i === 1} onClick={() => step(i - 1)}>
              &larr; Previous
            </button>
            <span className="count">
              {i} / {n}
            </span>
            {last ? (
              <button type="button" className="btn primary" onClick={finish}>
                Finish unit &rarr;
              </button>
            ) : (
              <button type="button" className="btn primary" onClick={() => step(i + 1, true)}>
                Next &rarr;
              </button>
            )}
          </div>

          <section className="deck" aria-label={`Unit ${unit} words`}>
            <h2>
              <span>Unit {unit}</span>
              <span>
                Studied <b>{studied}</b> / {n}
              </span>
            </h2>
            <div className="chips">
              {words.map((x, k) => {
                const s = strength(progress.cards[x.id]);
                return (
                  <a
                    key={x.id}
                    className="chip"
                    href={href.study(unit, k + 1)}
                    onClick={(e) => {
                      e.preventDefault();
                      step(k + 1);
                    }}
                    aria-current={k + 1 === i}
                    aria-label={`${x.word}, ${STRENGTH_LABEL[s]}`}
                  >
                    <span className="cw">{x.word}</span>
                    <span className="dots" aria-hidden="true">
                      {[1, 2, 3, 4].map((d) => (
                        <i key={d} className={d <= s ? 'on' : ''} />
                      ))}
                    </span>
                  </a>
                );
              })}
            </div>
          </section>
        </>
      )}
    </>
  );
}

export function UnitDone({ index, unit }: { index: Index; unit: number }) {
  const progress = useProgress();
  const now = useNow(30000);
  if (unit < 1 || unit > index.unitCount) return <UnitNotFound unit={unit} index={index} />;
  const ids = unitWordIds(index, unit);
  const studied = ids.filter((id) => progress.cards[id]).length;
  const due = dueIds(progress, now).filter((id) => index.pos.has(id)).length;
  const next = unit < index.unitCount ? unit + 1 : null;

  if (studied < ids.length) {
    return (
      <section className="notice">
        <span className="eyebrow">Unit {unit}</span>
        <h1>This unit is not finished yet.</h1>
        <p>Walk through all of its words in the study view; finishing it adds them to your review.</p>
        <div className="actions">
          <a className="btn primary" href={href.study(unit, 1)}>
            Study Unit {unit}
          </a>
          <a className="btn" href={href.unit(unit)}>
            Word list
          </a>
        </div>
      </section>
    );
  }

  return (
    <section className="notice">
      <span className="eyebrow">Unit {unit} complete</span>
      <h1>{plural(ids.length, 'word')} joined your review.</h1>
      <p>
        In review you see only the word. Explain it in your own words from memory, then compare with the definition,
        example and picture, and rate how well you knew it.
      </p>
      <div className="actions">
        <a className="btn primary" href={href.review()}>
          Review now &middot; {due} due
        </a>
        {next && (
          <a className="btn" href={href.unit(next)}>
            Next: Unit {next} &rarr;
          </a>
        )}
        <a className="link" href={href.home()}>
          All units
        </a>
      </div>
    </section>
  );
}
