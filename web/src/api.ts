import { useEffect, useState } from 'react';
import type { ReleaseIndex, UnitPayload, WordFull, WordsPayload } from './types';

export const mediaUrl = (rel: string): string => `/media/${rel}`;

async function getJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`;
    try {
      const body = (await res.json()) as { error?: string };
      if (body.error) msg = body.error;
    } catch {
      /* not JSON */
    }
    throw new Error(msg);
  }
  return (await res.json()) as T;
}

/* ---------- index ---------- */

export interface Index extends ReleaseIndex {
  unitCount: number;
  /** word_id -> 0-based position in learning order */
  pos: Map<number, number>;
}

let indexPromise: Promise<Index> | null = null;

export function loadIndex(): Promise<Index> {
  indexPromise ??= getJson<ReleaseIndex>('/api/index')
    .then((r) => ({
      ...r,
      unitCount: Math.ceil(r.words.length / r.unitSize),
      pos: new Map(r.words.map(([id], i) => [id, i])),
    }))
    .catch((err: unknown) => {
      indexPromise = null;
      throw err;
    });
  return indexPromise;
}

export const unitOf = (index: Index, wordId: number): number => {
  const p = index.pos.get(wordId);
  return p === undefined ? 0 : Math.floor(p / index.unitSize) + 1;
};

export const unitWordIds = (index: Index, unit: number): number[] =>
  index.words.slice((unit - 1) * index.unitSize, unit * index.unitSize).map(([id]) => id);

/* ---------- word data ---------- */

const wordCache = new Map<number, WordFull>();
const unitPromises = new Map<number, Promise<WordFull[]>>();

export function loadUnit(unit: number): Promise<WordFull[]> {
  let p = unitPromises.get(unit);
  if (!p) {
    p = getJson<UnitPayload>(`/api/unit/${unit}`)
      .then((r) => {
        for (const w of r.words) wordCache.set(w.id, w);
        return r.words;
      })
      .catch((err: unknown) => {
        unitPromises.delete(unit);
        throw err;
      });
    unitPromises.set(unit, p);
  }
  return p;
}

export function cachedWord(id: number): WordFull | undefined {
  return wordCache.get(id);
}

export async function loadWords(ids: number[]): Promise<Map<number, WordFull>> {
  const missing = [...new Set(ids)].filter((id) => !wordCache.has(id));
  for (let i = 0; i < missing.length; i += 200) {
    const chunk = missing.slice(i, i + 200);
    const r = await getJson<WordsPayload>(`/api/words?ids=${chunk.join(',')}`);
    for (const w of r.words) wordCache.set(w.id, w);
  }
  return new Map(ids.flatMap((id) => (wordCache.has(id) ? [[id, wordCache.get(id)!] as const] : [])));
}

/** Warm the browser cache for a word's picture so the next card shows it at once. */
export function prefetchImage(w: WordFull | undefined): void {
  if (w?.example?.img) {
    const img = new Image();
    img.src = mediaUrl(w.example.img);
  }
}

/* ---------- hooks ---------- */

export type Async<T> = { status: 'loading' } | { status: 'error'; error: string } | { status: 'ok'; data: T };

export function useAsync<T>(fn: () => Promise<T>, deps: unknown[]): Async<T> {
  const [state, setState] = useState<Async<T>>({ status: 'loading' });
  useEffect(() => {
    let alive = true;
    setState({ status: 'loading' });
    fn().then(
      (data) => alive && setState({ status: 'ok', data }),
      (err: unknown) => alive && setState({ status: 'error', error: err instanceof Error ? err.message : String(err) }),
    );
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return state;
}
