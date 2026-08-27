import { useCallback, useSyncExternalStore } from 'react';

/**
 * Client-only triage state for the gallery's flag → review → resolve flow
 * (backlog #40). Nothing here reaches the server: flagging a word is just a
 * worklist marker the owner uses while working through worst-CLIP-match
 * images, so it lives in localStorage rather than a new API concept.
 */
const STORAGE_KEY = 'morpho-image-flags';

interface FlagState {
  flagged: number[];
  needsRegen: number[];
}

function emptyState(): FlagState {
  return { flagged: [], needsRegen: [] };
}

function parse(raw: string | null): FlagState {
  if (!raw) return emptyState();
  try {
    const value = JSON.parse(raw) as Partial<FlagState> | null;
    const flagged = Array.isArray(value?.flagged)
      ? value.flagged.filter((id): id is number => typeof id === 'number')
      : [];
    const needsRegen = Array.isArray(value?.needsRegen)
      ? value.needsRegen.filter((id): id is number => typeof id === 'number')
      : [];
    return { flagged, needsRegen };
  } catch {
    return emptyState();
  }
}

// useSyncExternalStore requires getSnapshot to return a stable reference when
// nothing changed; cache the parsed object alongside the raw string it came
// from so repeated reads between writes don't allocate (or re-render) at all.
let cachedRaw: string | null = null;
let cachedState: FlagState = emptyState();

function readRaw(): string | null {
  return typeof window === 'undefined' ? null : window.localStorage.getItem(STORAGE_KEY);
}

function getSnapshot(): FlagState {
  const raw = readRaw();
  if (raw !== cachedRaw) {
    cachedRaw = raw;
    cachedState = parse(raw);
  }
  return cachedState;
}

function getServerSnapshot(): FlagState {
  return emptyState();
}

function writeState(next: FlagState): void {
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  // The native `storage` event only fires in *other* tabs/documents; the tab
  // that wrote the change has to tell itself so useSyncExternalStore re-reads.
  window.dispatchEvent(new StorageEvent('storage', { key: STORAGE_KEY }));
}

function subscribe(onStoreChange: () => void): () => void {
  window.addEventListener('storage', onStoreChange);
  return () => window.removeEventListener('storage', onStoreChange);
}

export interface UseImageFlags {
  flagged: number[];
  needsRegen: number[];
  flag: (wordId: number) => void;
  unflag: (wordId: number) => void;
  markNeedsRegen: (wordId: number) => void;
  clear: (wordId: number) => void;
  isFlagged: (wordId: number) => boolean;
  isNeedsRegen: (wordId: number) => boolean;
  counts: { flagged: number; needsRegen: number };
}

/** Reads/writes `morpho-image-flags`, synced across tabs via the storage event. */
export function useImageFlags(): UseImageFlags {
  const state = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);

  const flag = useCallback((wordId: number) => {
    const current = getSnapshot();
    if (current.flagged.includes(wordId)) return;
    writeState({
      flagged: [...current.flagged, wordId],
      needsRegen: current.needsRegen.filter((id) => id !== wordId),
    });
  }, []);

  const unflag = useCallback((wordId: number) => {
    const current = getSnapshot();
    if (!current.flagged.includes(wordId)) return;
    writeState({ ...current, flagged: current.flagged.filter((id) => id !== wordId) });
  }, []);

  const markNeedsRegen = useCallback((wordId: number) => {
    const current = getSnapshot();
    writeState({
      flagged: current.flagged.filter((id) => id !== wordId),
      needsRegen: current.needsRegen.includes(wordId)
        ? current.needsRegen
        : [...current.needsRegen, wordId],
    });
  }, []);

  const clear = useCallback((wordId: number) => {
    const current = getSnapshot();
    writeState({
      flagged: current.flagged.filter((id) => id !== wordId),
      needsRegen: current.needsRegen.filter((id) => id !== wordId),
    });
  }, []);

  return {
    flagged: state.flagged,
    needsRegen: state.needsRegen,
    flag,
    unflag,
    markNeedsRegen,
    clear,
    isFlagged: (wordId: number) => state.flagged.includes(wordId),
    isNeedsRegen: (wordId: number) => state.needsRegen.includes(wordId),
    counts: { flagged: state.flagged.length, needsRegen: state.needsRegen.length },
  };
}
