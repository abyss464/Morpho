---
file: admin-ui/src/app/liveStreamContext.ts
---

SSE change stream health status context: whether local read connections are alive.

## export

- **LiveStreamContext** — React context that carries the change stream connection status (connection status enum, whether it is enabled). Do not read via `useContext` directly; use the hook below.
- **useLiveStream()** — hook for reading the current change stream status. Returns `{ status, enabled }`. `status` is one of `'open'` | `'connecting'` | `'reconnecting'` | `'closed'`; `enabled` is `false` in mock mode.
- **DISABLED_STREAM** — sentinel constant indicating that the stream is closed/unusable (`status: 'closed', enabled: false`). Used as the default value of the context, and can also be passed directly in tests.

## constraint

- Do not use `useContext(LiveStreamContext)` directly in components; always use the `useLiveStream()` hook.
- This file only defines the context and read interface; it does not manage the SSE connection lifecycle — the connection is managed in the `liveStream.tsx` Provider.
