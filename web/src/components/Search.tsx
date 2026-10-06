import { useMemo, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { unitOf } from '../api';
import type { Index } from '../api';
import { go, href } from '../router';

interface Hit {
  id: number;
  word: string;
  unit: number;
}

/** Word look-up: jumps to the word inside its unit's word list. */
export function Search({ index }: { index: Index }) {
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
        Look up a word
      </label>
      <input
        id="word-search"
        ref={input}
        type="search"
        autoComplete="off"
        spellCheck={false}
        placeholder="Look up a word"
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
