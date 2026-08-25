import { setupServer } from 'msw/node';
import { handlers } from './handlers';

/** Same handler set for Vitest; see `src/test/setup.ts`. */
export const server = setupServer(...handlers);
