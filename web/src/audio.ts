// One shared player: clicking a speaker stops whatever was playing and starts that file (or run of files).
// Nothing plays unless the learner clicks.

import { useSyncExternalStore } from 'react';
import { mediaUrl } from './api';

let player: HTMLAudioElement | null = null;
let playing: string | null = null;
const listeners = new Set<() => void>();

function set(src: string | null): void {
  playing = src;
  listeners.forEach((l) => l());
}

/**
 * Plays one file, or several back to back (a word, then its definition, then its example).
 * The first file names the run, so the button that started it shows as playing until the
 * last file ends; a failed file is skipped rather than ending the run.
 */
export function play(src: string | string[]): void {
  const queue = (Array.isArray(src) ? src : [src]).filter(Boolean);
  if (!queue.length) return;
  if (player) {
    player.pause();
    player.onended = player.onerror = player.onpause = null;
  }
  let i = 0;
  const start = () => {
    const a = new Audio(mediaUrl(queue[i]!));
    player = a;
    const advance = () => {
      if (player !== a) return;
      i += 1;
      if (i < queue.length) start();
      else set(null);
    };
    a.onended = advance;
    a.onerror = advance;
    // A pause the learner caused (stop, another speaker) ends the run; the pause the
    // browser fires on reaching the end of a file does not.
    a.onpause = () => {
      if (player === a && !a.ended) set(null);
    };
    a.play().catch(() => {
      if (player === a) set(null);
    });
  };
  set(queue[0]!);
  start();
}

export function stop(): void {
  player?.pause();
}

export function usePlaying(src: string): boolean {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => playing === src,
  );
}
