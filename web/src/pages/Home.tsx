import { useMemo, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { unitOf, unitWordIds } from '../api';
import type { Index } from '../api';
import { plural, useNow } from '../hooks';
import { go, href } from '../router';
import { dueAt, dueIds, formatInterval, upcomingIds, useProgress } from '../store';

interface Hit {
  id: number;
  word: string;
  unit: number;
}

function Search({ index }: { index: Index }) {
  const [q, setQ] = useState('');
  const [sel, setSel] = useState(0);
  const [open, setOpen] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const lower = useMemo(() => index.words.map(([, w]) => w.toLowerCase()), [index]);

  const hits = useMemo<Hit[]>(() => {
    const t = q.trim().toLowerCase();
    if (!t) return [];
    const scored: [number, number][] = [];
    lower.forEach((w, i) => {
      const s = w === t ? 0 : w.startsWith(t) ? 1 : w.includes(t) ? 2 : -1;
      if (s >= 0) scored.push([s, i]);
    });
    scored.sort((a, b) => a[0] - b[0] || lower[a[1]]!.length - lower[b[1]]!.length || a[1] - b[1]);
    return scored.slice(0, 8).map(([, i]) => {
      const [id, word] = index.words[i]!;
      return { id, word, unit: unitOf(index, id) };
    });
  }, [q, lower, index]);

  const pick = (h: Hit | undefined) => {
    if (!h) return;
    setQ('');
    setOpen(false);
    input.current?.blur();
    go(href.unit(h.unit, h.id));
  };

  const key = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setOpen(true);
      setSel((s) => Math.min(s + 1, hits.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSel((s) => Math.max(s - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      pick(hits[sel] ?? hits[0]);
    } else if (e.key === 'Escape') {
      setOpen(false);
    }
  };

  const showList = open && q.trim().length > 0;
  return (
    <div className="search">
      <svg className="glass" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
        <circle cx="11" cy="11" r="7" />
        <path d="M20 20l-4-4" />
      </svg>
      <label className="sr-only" htmlFor="word-search">
        Find a word
      </label>
      <input
        id="word-search"
        ref={input}
        type="search"
        autoComplete="off"
        spellCheck={false}
        placeholder={`Find a word among ${index.words.length.toLocaleString('en-US')}`}
        role="combobox"
        aria-expanded={showList}
        aria-controls="word-search-list"
        aria-activedescendant={showList && hits[sel] ? `hit-${hits[sel].id}` : undefined}
        value={q}
        onChange={(e) => {
          setQ(e.target.value);
          setSel(0);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => window.setTimeout(() => setOpen(false), 120)}
        onKeyDown={key}
      />
      {showList && (
        <ul className="suggest" id="word-search-list" role="listbox">
          {hits.length === 0 ? (
            <li className="empty" role="option" aria-selected={false} aria-disabled="true">
              No word matches "{q.trim()}".
            </li>
          ) : (
            hits.map((h, i) => (
              <li
                key={h.id}
                id={`hit-${h.id}`}
                role="option"
                aria-selected={i === sel}
                onMouseEnter={() => setSel(i)}
                onMouseDown={(e) => {
                  e.preventDefault();
                  pick(h);
                }}
              >
                <span className="sw">{h.word}</span>
                <span className="su">Unit {h.unit}</span>
              </li>
            ))
          )}
        </ul>
      )}
    </div>
  );
}

const CHECK = (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M5 12.5l4.5 4.5L19 7.5" />
  </svg>
);

export function Home({ index }: { index: Index }) {
  const progress = useProgress();
  const now = useNow(30000);
  const total = index.words.length;

  const units = useMemo(
    () =>
      Array.from({ length: index.unitCount }, (_, k) => {
        const n = k + 1;
        const ids = unitWordIds(index, n);
        const words = index.words.slice(k * index.unitSize, k * index.unitSize + 3).map(([, w]) => w);
        return { n, ids, words, studied: ids.filter((id) => progress.cards[id]).length };
      }),
    [index, progress],
  );

  const studied = units.reduce((s, u) => s + u.studied, 0);
  const finished = units.filter((u) => u.studied === u.ids.length).length;
  const next = units.find((u) => u.studied < u.ids.length);
  const due = dueIds(progress, now).filter((id) => index.pos.has(id)).length;
  const soon = upcomingIds(progress, now)[0];
  const soonCard = soon !== undefined ? progress.cards[soon] : undefined;

  return (
    <>
      <Search index={index} />

      <div className="panels">
        <section className="panel" aria-label="Learn">
          <span className="eyebrow">Learn</span>
          {next ? (
            <>
              <p className="big">
                Unit {next.n}
                <small>{next.studied ? `${next.studied} of ${next.ids.length} studied` : 'up next'}</small>
              </p>
              <p>
                {plural(studied, 'word')} of {total.toLocaleString('en-US')} studied
              </p>
              <div className="bar-track" aria-hidden="true">
                <i style={{ width: `${(studied / Math.max(1, total)) * 100}%` }} />
              </div>
              <div className="actions">
                <a className="btn primary" href={href.unit(next.n)}>
                  Open Unit {next.n}
                </a>
                <a className="btn" href={href.study(next.n, 1)}>
                  Study it
                </a>
              </div>
            </>
          ) : (
            <>
              <p className="big">All units studied</p>
              <p>Every word in this release is in your review.</p>
            </>
          )}
        </section>

        <section className="panel" aria-label="Review">
          <span className="eyebrow">Review</span>
          <p className="big">
            {due.toLocaleString('en-US')}
            <small>{due === 1 ? 'word due' : 'words due'}</small>
          </p>
          <p>
            {due
              ? 'Explain each word in your own words, then compare with the definition.'
              : soonCard
                ? `Nothing due now. The next word is due in ${formatInterval(dueAt(soonCard) - now)}.`
                : 'Finish a unit and its words join your review.'}
          </p>
          <div className="actions">
            <a className={due ? 'btn primary' : 'btn'} href={href.review()}>
              {due ? 'Start review' : 'Open review'}
            </a>
          </div>
        </section>
      </div>

      <section className="deck" aria-label="Units">
        <h2>
          <span>Units</span>
          <span>
            Finished <b>{finished}</b> / {index.unitCount}
          </span>
        </h2>
        <div className="chips">
          {units.map((u) => {
            const complete = u.studied === u.ids.length;
            return (
              <a
                key={u.n}
                className="chip uchip"
                href={href.unit(u.n)}
                aria-current={next?.n === u.n}
                aria-label={`Unit ${u.n}, ${u.studied} of ${u.ids.length} words studied`}
              >
                <span className="top">
                  <span className="cw">Unit {u.n}</span>
                  <span className={complete ? 'tally done' : 'tally'}>
                    {complete && CHECK}
                    {u.studied}/{u.ids.length}
                  </span>
                </span>
                <span className="sample">{u.words.join(', ')}</span>
                <span className="bar-track" aria-hidden="true">
                  <i style={{ width: `${(u.studied / u.ids.length) * 100}%` }} />
                </span>
              </a>
            );
          })}
        </div>
      </section>
    </>
  );
}
