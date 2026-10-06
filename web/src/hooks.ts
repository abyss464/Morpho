import { useEffect, useRef, useState } from 'react';

/** Current time at render; also re-renders the component every `everyMs`. */
export function useNow(everyMs: number): number {
  const [, setTick] = useState(0);
  useEffect(() => {
    const t = window.setInterval(() => setTick((n) => n + 1), everyMs);
    return () => window.clearInterval(t);
  }, [everyMs]);
  return Date.now();
}

/** Document-level keydown listener that always calls the latest handler. */
export function useKeydown(handler: (e: KeyboardEvent) => void): void {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    const l = (e: KeyboardEvent) => ref.current(e);
    document.addEventListener('keydown', l);
    return () => document.removeEventListener('keydown', l);
  }, []);
}

export const isTyping = (e: KeyboardEvent): boolean => {
  const t = e.target as HTMLElement | null;
  return !!t && (t.tagName === 'TEXTAREA' || t.tagName === 'INPUT' || t.isContentEditable);
};

export const plural = (n: number, one: string, many = `${one}s`): string => `${n.toLocaleString('en-US')} ${n === 1 ? one : many}`;
