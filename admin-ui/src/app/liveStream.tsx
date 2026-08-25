/**
 * One change-stream subscription for the whole app.
 *
 * The subscription has to be a singleton — a second EventSource would double
 * every invalidation and burn a second connection out of the browser's six —
 * while several places need to know whether it is healthy: the header badge
 * shows it, and the dashboard falls back to slow polling while it is down.
 */

import { LiveStreamContext } from './liveStreamContext';
import { useChangeStream } from '../hooks/useChangeStream';

export function LiveStreamProvider({ children }: { children: React.ReactNode }) {
  const state = useChangeStream();
  return <LiveStreamContext.Provider value={state}>{children}</LiveStreamContext.Provider>;
}
