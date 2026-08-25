import { createContext, useContext } from 'react';
import type { ChangeStreamState } from '../hooks/useChangeStream';

/** Mock mode never opens a stream, so `enabled: false` is the honest default. */
export const DISABLED_STREAM: ChangeStreamState = { status: 'closed', enabled: false };

export const LiveStreamContext = createContext<ChangeStreamState>(DISABLED_STREAM);

/** Health of the single app-wide `GET /api/stream` subscription. */
export function useLiveStream(): ChangeStreamState {
  return useContext(LiveStreamContext);
}
