// One shared player: clicking a speaker stops whatever was playing and starts that file.
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

export function play(src: string): void {
  if (player) {
    player.pause();
    player.onended = player.onerror = player.onpause = null;
  }
  const a = new Audio(mediaUrl(src));
  player = a;
  a.onended = a.onerror = a.onpause = () => {
    if (player === a) set(null);
  };
  set(src);
  a.play().catch(() => {
    if (player === a) set(null);
  });
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
