/**
 * Transport for `GET /api/stream` (admin-api.md wave-2 ruling #7).
 *
 * Frame shape morphod emits:
 *
 * ```
 * event: change
 * data: {"entity_type":"word","entity_ids":["12","13"]}
 *
 * : ping
 * ```
 *
 * Changes are coalesced server-side over 250 ms and a `: ping` comment arrives
 * every 30 s to keep intermediaries from reaping the connection. Comments never
 * surface as events, so an idle stream is silent by design.
 *
 * The browser's own EventSource reconnect is a fixed 3 s with no ceiling and no
 * jitter, so this module drives reconnection itself: on error the socket is
 * closed and a fresh one opens after an exponential, jittered delay. That keeps
 * a restarting morphod from being hammered by every open console tab.
 */

import type { ChangeEvent } from './types';

export const CHANGE_EVENT_NAME = 'change';
export const RECONNECT_BASE_MS = 1_000;
export const RECONNECT_MAX_MS = 30_000;

/**
 * Liveness watchdog. Morphod pings every 30 s, but `: ping` is an SSE comment
 * and comments are never surfaced to JavaScript — the browser has no API for
 * "the last byte arrived N seconds ago". So instead of listening for the ping,
 * the stream re-opens itself after three ping intervals of total silence. On a
 * healthy but idle engine that costs one request every 90 s; on a stream that is
 * being held open by something that no longer has a server behind it — a dev
 * proxy, a load balancer, a laptop that slept — it is the only way the console
 * ever finds out it stopped being live.
 */
export const IDLE_RECONNECT_MS = 90_000;

export type StreamStatus = 'connecting' | 'open' | 'reconnecting' | 'closed';

/**
 * Exponential backoff with full jitter on the upper half of the window.
 * `attempt` is 0 for the first retry.
 */
export function reconnectDelay(attempt: number, random: () => number = Math.random): number {
  const ceiling = Math.min(RECONNECT_MAX_MS, RECONNECT_BASE_MS * 2 ** Math.max(0, attempt));
  const half = ceiling / 2;
  return Math.round(half + random() * half);
}

/** Decodes one `data:` payload; returns null for anything that is not a ChangeEvent. */
export function parseChangeFrame(data: string): ChangeEvent | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(data);
  } catch {
    return null;
  }
  if (typeof parsed !== 'object' || parsed === null) return null;
  const { entity_type: entityType, entity_ids: entityIds } = parsed as {
    entity_type?: unknown;
    entity_ids?: unknown;
  };
  if (typeof entityType !== 'string' || entityType.length === 0) return null;
  const ids = Array.isArray(entityIds)
    ? entityIds.filter(
        (id): id is number | string => typeof id === 'number' || typeof id === 'string',
      )
    : [];
  return { entity_type: entityType, entity_ids: ids };
}

export interface ChangeStreamOptions {
  url: string;
  onChange: (event: ChangeEvent) => void;
  onStatus?: (status: StreamStatus) => void;
  /** Injection seam for tests; defaults to the platform EventSource. */
  createSource?: (url: string) => EventSource;
  /** Injection seams for deterministic backoff in tests. */
  random?: () => number;
  /** Silence tolerated before the stream re-opens itself. */
  idleTimeoutMs?: number;
}

/**
 * Opens the stream and keeps it open. Returns an unsubscribe function that
 * closes the socket and cancels any pending reconnect.
 */
export function subscribeToChanges(options: ChangeStreamOptions): () => void {
  const {
    url,
    onChange,
    onStatus,
    createSource,
    random,
    idleTimeoutMs = IDLE_RECONNECT_MS,
  } = options;
  const open = createSource ?? ((target: string) => new EventSource(target));

  let source: EventSource | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let idleTimer: ReturnType<typeof setTimeout> | null = null;
  let attempt = 0;
  let stopped = false;

  const status = (next: StreamStatus) => onStatus?.(next);

  const clearIdle = () => {
    if (idleTimer !== null) {
      clearTimeout(idleTimer);
      idleTimer = null;
    }
  };

  const connect = () => {
    if (stopped) return;
    status(attempt === 0 ? 'connecting' : 'reconnecting');

    let current: EventSource;
    try {
      current = open(url);
    } catch {
      schedule();
      return;
    }
    source = current;

    /** Drops the socket without counting it as a failure, then re-opens. */
    const recycle = () => {
      if (stopped || source !== current) return;
      current.close();
      source = null;
      clearIdle();
      connect();
    };

    const armIdle = () => {
      clearIdle();
      if (idleTimeoutMs > 0) idleTimer = setTimeout(recycle, idleTimeoutMs);
    };

    current.addEventListener('open', () => {
      attempt = 0;
      status('open');
      armIdle();
    });

    current.addEventListener(CHANGE_EVENT_NAME, (event) => {
      armIdle();
      const frame = parseChangeFrame((event as MessageEvent<string>).data);
      if (frame) onChange(frame);
    });

    current.addEventListener('error', () => {
      // EventSource would retry on its own schedule; take the wheel instead.
      current.close();
      if (source === current) source = null;
      clearIdle();
      schedule();
    });
  };

  const schedule = () => {
    if (stopped || timer !== null) return;
    status('reconnecting');
    const delay = reconnectDelay(attempt, random);
    attempt += 1;
    timer = setTimeout(() => {
      timer = null;
      connect();
    }, delay);
  };

  connect();

  return () => {
    stopped = true;
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    clearIdle();
    source?.close();
    source = null;
    status('closed');
  };
}
