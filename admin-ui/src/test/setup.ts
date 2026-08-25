import '@testing-library/jest-dom/vitest';
import { afterAll, afterEach, beforeAll, vi } from 'vitest';
import { cleanup } from '@testing-library/react';
import { server } from '../mocks/node';
import { resetDb } from '../mocks/db';

// AntD's responsive observer and ECharts both need these in jsdom.
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }),
});

class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as unknown as typeof ResizeObserver;

/**
 * Realm bridge. Under the jsdom environment `AbortController` comes from jsdom
 * while `fetch`/`Request` come from Node, and Node brand-checks the signal, so
 * every request TanStack Query makes would die with "Expected signal ... to be
 * an instance of AbortSignal". We strip the signal before it reaches Node and
 * re-implement cancellation on top of the promise.
 *
 * This wrapper must sit *outside* MSW's interceptor, hence the install right
 * after `server.listen()`. Test-only: the browser has one realm and passes the
 * signal straight through.
 */
function installAbortSignalBridge(): void {
  const inner = globalThis.fetch;
  globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
    const signal = init?.signal;
    if (!signal) return inner(input, init);

    const { signal: _dropped, ...rest } = init as RequestInit;
    const abortError = () =>
      Object.assign(new Error('The operation was aborted.'), { name: 'AbortError' });

    return new Promise<Response>((resolve, reject) => {
      if (signal.aborted) {
        reject(abortError());
        return;
      }
      const onAbort = () => reject(abortError());
      signal.addEventListener('abort', onAbort, { once: true });
      inner(input, rest)
        .then(resolve, reject)
        .finally(() => signal.removeEventListener('abort', onAbort));
    });
  }) as typeof fetch;
}

beforeAll(() => {
  server.listen({ onUnhandledRequest: 'error' });
  installAbortSignalBridge();
});

afterEach(() => {
  cleanup();
  server.resetHandlers();
  // Each test starts from the same fixture state so mutations cannot leak.
  resetDb();
});

afterAll(() => server.close());
