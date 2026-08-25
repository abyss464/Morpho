import { describe, expect, it, vi } from 'vitest';
import {
  RECONNECT_MAX_MS,
  parseChangeFrame,
  reconnectDelay,
  subscribeToChanges,
} from './changeStream';

describe('parseChangeFrame', () => {
  it('decodes the frame morphod emits', () => {
    expect(parseChangeFrame('{"entity_type":"word","entity_ids":["12","13"]}')).toEqual({
      entity_type: 'word',
      entity_ids: ['12', '13'],
    });
  });

  it('accepts numeric ids and composite job keys alike', () => {
    expect(parseChangeFrame('{"entity_type":"word","entity_ids":[7]}')?.entity_ids).toEqual([7]);
    expect(
      parseChangeFrame('{"entity_type":"job_state","entity_ids":["synth_tts:tts_input:abc"]}')
        ?.entity_ids,
    ).toEqual(['synth_tts:tts_input:abc']);
  });

  it('tolerates a missing id list', () => {
    expect(parseChangeFrame('{"entity_type":"plan"}')).toEqual({
      entity_type: 'plan',
      entity_ids: [],
    });
  });

  it('rejects anything that is not a change frame', () => {
    expect(parseChangeFrame('not json')).toBeNull();
    expect(parseChangeFrame('null')).toBeNull();
    expect(parseChangeFrame('{"entity_ids":[1]}')).toBeNull();
    expect(parseChangeFrame('{"entity_type":""}')).toBeNull();
  });
});

describe('reconnectDelay', () => {
  it('grows exponentially and never exceeds the ceiling', () => {
    const noJitter = () => 0;
    expect(reconnectDelay(0, noJitter)).toBe(500);
    expect(reconnectDelay(1, noJitter)).toBe(1000);
    expect(reconnectDelay(2, noJitter)).toBe(2000);
    for (let attempt = 0; attempt < 20; attempt += 1) {
      expect(reconnectDelay(attempt, () => 1)).toBeLessThanOrEqual(RECONNECT_MAX_MS);
    }
  });

  it('jitters within the upper half of the window', () => {
    expect(reconnectDelay(3, () => 0)).toBe(4000);
    expect(reconnectDelay(3, () => 1)).toBe(8000);
  });
});

/** Minimal EventSource stand-in: jsdom has none. */
class FakeSource {
  readonly listeners = new Map<string, Set<(event: Event) => void>>();
  closed = false;

  addEventListener(type: string, handler: (event: Event) => void) {
    const bucket = this.listeners.get(type) ?? new Set();
    bucket.add(handler);
    this.listeners.set(type, bucket);
  }

  close() {
    this.closed = true;
  }

  emit(type: string, data?: string) {
    const event =
      data === undefined ? new Event(type) : (new MessageEvent(type, { data }) as Event);
    for (const handler of this.listeners.get(type) ?? []) handler(event);
  }
}

describe('subscribeToChanges', () => {
  it('reports status and forwards decoded change frames', () => {
    const source = new FakeSource();
    const onChange = vi.fn();
    const onStatus = vi.fn();

    const stop = subscribeToChanges({
      url: '/api/stream',
      onChange,
      onStatus,
      createSource: () => source as unknown as EventSource,
    });

    expect(onStatus).toHaveBeenCalledWith('connecting');
    source.emit('open');
    expect(onStatus).toHaveBeenCalledWith('open');

    source.emit('change', '{"entity_type":"oos_queue","entity_ids":["cordial"]}');
    expect(onChange).toHaveBeenCalledWith({
      entity_type: 'oos_queue',
      entity_ids: ['cordial'],
    });

    // A `: ping` comment never reaches a listener, and a malformed frame is dropped.
    source.emit('change', 'garbage');
    expect(onChange).toHaveBeenCalledTimes(1);

    stop();
    expect(source.closed).toBe(true);
    expect(onStatus).toHaveBeenLastCalledWith('closed');
  });

  it('re-opens itself after a silent window, and silence restarts on every frame', () => {
    vi.useFakeTimers();
    try {
      const sources: FakeSource[] = [];
      const stop = subscribeToChanges({
        url: '/api/stream',
        onChange: vi.fn(),
        idleTimeoutMs: 1_000,
        createSource: () => {
          const next = new FakeSource();
          sources.push(next);
          return next as unknown as EventSource;
        },
      });

      sources[0]?.emit('open');
      vi.advanceTimersByTime(900);
      sources[0]?.emit('change', '{"entity_type":"word","entity_ids":["1"]}');

      // The frame reset the window, so the original socket is still in play.
      vi.advanceTimersByTime(900);
      expect(sources).toHaveLength(1);
      expect(sources[0]?.closed).toBe(false);

      // Full window of silence: the stream drops itself and opens a new one.
      vi.advanceTimersByTime(200);
      expect(sources[0]?.closed).toBe(true);
      expect(sources).toHaveLength(2);

      stop();
      vi.advanceTimersByTime(10_000);
      expect(sources).toHaveLength(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it('closes the socket itself and reconnects on backoff', () => {
    vi.useFakeTimers();
    try {
      const sources: FakeSource[] = [];
      const onStatus = vi.fn();

      const stop = subscribeToChanges({
        url: '/api/stream',
        onChange: vi.fn(),
        onStatus,
        random: () => 0,
        createSource: () => {
          const next = new FakeSource();
          sources.push(next);
          return next as unknown as EventSource;
        },
      });

      sources[0]?.emit('error');
      expect(sources[0]?.closed).toBe(true);
      expect(onStatus).toHaveBeenLastCalledWith('reconnecting');
      expect(sources).toHaveLength(1);

      vi.advanceTimersByTime(500);
      expect(sources).toHaveLength(2);

      stop();
      expect(sources[1]?.closed).toBe(true);
      // Nothing further is scheduled once stopped.
      vi.advanceTimersByTime(60_000);
      expect(sources).toHaveLength(2);
    } finally {
      vi.useRealTimers();
    }
  });
});
