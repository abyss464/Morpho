import { useSyncExternalStore } from 'react';

export type Route = { name: 'home' } | { name: 'stream' } | { name: 'unit'; unit: number; focus: number | null };

export function parse(hash: string): Route {
  const [path = '', query = ''] = hash.replace(/^#/, '').split('?');
  const parts = path.split('/').filter(Boolean);
  const params = new URLSearchParams(query);
  if (parts[0] === 'stream') return { name: 'stream' };
  if (parts[0] === 'unit' && /^\d+$/.test(parts[1] ?? '')) {
    const w = params.get('w');
    return { name: 'unit', unit: Number(parts[1]), focus: w && /^\d+$/.test(w) ? Number(w) : null };
  }
  return { name: 'home' };
}

export const href = {
  home: () => '#/',
  stream: () => '#/stream',
  unit: (n: number, focus?: number) => `#/unit/${n}${focus ? `?w=${focus}` : ''}`,
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
