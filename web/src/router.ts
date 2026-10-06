import { useSyncExternalStore } from 'react';

export type Route =
  | { name: 'home' }
  | { name: 'unit'; unit: number; focus: number | null }
  | { name: 'study'; unit: number; index: number }
  | { name: 'done'; unit: number }
  | { name: 'review' };

export function parse(hash: string): Route {
  const [path = '', query = ''] = hash.replace(/^#/, '').split('?');
  const parts = path.split('/').filter(Boolean);
  const params = new URLSearchParams(query);
  if (parts[0] === 'review') return { name: 'review' };
  if (parts[0] === 'unit' && /^\d+$/.test(parts[1] ?? '')) {
    const unit = Number(parts[1]);
    if (parts[2] === 'study') return { name: 'study', unit, index: Math.max(1, Number(parts[3]) || 1) };
    if (parts[2] === 'done') return { name: 'done', unit };
    const w = params.get('w');
    return { name: 'unit', unit, focus: w && /^\d+$/.test(w) ? Number(w) : null };
  }
  return { name: 'home' };
}

export const href = {
  home: () => '#/',
  unit: (n: number, focus?: number) => `#/unit/${n}${focus ? `?w=${focus}` : ''}`,
  study: (n: number, i: number) => `#/unit/${n}/study/${i}`,
  done: (n: number) => `#/unit/${n}/done`,
  review: () => '#/review',
};

export function go(h: string, replace = false): void {
  if (replace) window.location.replace(h);
  else window.location.hash = h;
}

function subscribe(l: () => void): () => void {
  window.addEventListener('hashchange', l);
  return () => window.removeEventListener('hashchange', l);
}

export function useHash(): string {
  return useSyncExternalStore(subscribe, () => window.location.hash);
}
