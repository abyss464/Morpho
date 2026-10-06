import { useEffect, useState } from 'react';
import { loadUnit, unitWordIds, useAsync } from '../api';
import type { Index } from '../api';
import { WordCard } from '../components/parts';
import { href } from '../router';
import { useProgress } from '../store';

export function UnitNotFound({ unit, index }: { unit: number; index: Index }) {
  return (
    <section className="notice">
      <h1>There is no Unit {unit}.</h1>
      <p>This release has {index.unitCount} units.</p>
      <a className="btn primary" href={href.home()}>
        Back to today
      </a>
    </section>
  );
}

export function UnitPage({ index, unit, focus }: { index: Index; unit: number; focus: number | null }) {
  const valid = unit >= 1 && unit <= index.unitCount;
  const data = useAsync(() => (valid ? loadUnit(unit) : Promise.resolve([])), [unit, valid]);
  const progress = useProgress();
  const [flash, setFlash] = useState<number | null>(null);

  useEffect(() => {
    if (data.status !== 'ok' || focus === null) return;
    const el = document.getElementById(`w-${focus}`);
    if (!el) return;
    el.scrollIntoView({ block: 'start' });
    setFlash(focus);
    const t = window.setTimeout(() => setFlash(null), 2300);
    return () => window.clearTimeout(t);
  }, [data.status, focus, unit]);

  if (!valid) return <UnitNotFound unit={unit} index={index} />;

  const ids = unitWordIds(index, unit);
  const first = (unit - 1) * index.unitSize + 1;
  const met = ids.filter((id) => progress.cards[id] || progress.words[id]).length;
  const prev = unit > 1 ? unit - 1 : null;
  const next = unit < index.unitCount ? unit + 1 : null;

  return (
    <>
      <div className="crumbs">
        <a href={href.home()}>&larr; Today</a>
        <span className="actions">
          {prev && <a href={href.unit(prev)}>Unit {prev}</a>}
          {next && <a href={href.unit(next)}>Unit {next} &rarr;</a>}
        </span>
      </div>

      <header className="uhead">
        <div>
          <h1>Unit {unit}</h1>
          <p>
            Words {first}&ndash;{first + ids.length - 1} &middot; {met} of {ids.length} met
          </p>
        </div>
      </header>

      {data.status === 'loading' && <p className="loading">Loading Unit {unit}&hellip;</p>}
      {data.status === 'error' && (
        <section className="notice">
          <h2>Could not load Unit {unit}.</h2>
          <p>{data.error}</p>
        </section>
      )}
      {data.status === 'ok' && (
        <div className="entries">
          {data.data.map((w, i) => (
            <WordCard
              key={w.id}
              id={`w-${w.id}`}
              w={w}
              variant="entry"
              num={`${i + 1} / ${data.data.length}`}
              note={progress.notes[w.id]}
              flash={flash === w.id}
            />
          ))}
        </div>
      )}

      {data.status === 'ok' && (
        <div className="nav">
          {prev ? (
            <a className="btn" href={href.unit(prev)}>
              &larr; Unit {prev}
            </a>
          ) : (
            <span />
          )}
          <a className="btn primary" href={href.home()}>
            Back to today
          </a>
          {next ? (
            <a className="btn" href={href.unit(next)}>
              Unit {next} &rarr;
            </a>
          ) : (
            <span />
          )}
        </div>
      )}
    </>
  );
}
