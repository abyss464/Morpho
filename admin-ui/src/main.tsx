import React from 'react';
import ReactDOM from 'react-dom/client';
import { QueryClientProvider } from '@tanstack/react-query';
import { RouterProvider, createRouter } from '@tanstack/react-router';

import { routeTree } from './routeTree.gen';
import { createQueryClient } from './app/queryClient';
import { ThemeProvider } from './app/theme';
import './styles.css';

const queryClient = createQueryClient();

const router = createRouter({
  routeTree,
  context: { queryClient },
  defaultPreload: 'intent',
  defaultPreloadStaleTime: 0,
});

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}

/** Dev default is fully mocked; VITE_API_MOCK=0 proxies /api to morphod. */
async function enableMocking(): Promise<void> {
  if (import.meta.env.VITE_API_MOCK !== '1') return;
  const { startMockWorker } = await import('./mocks/browser');
  await startMockWorker();
}

void enableMocking().then(() => {
  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <ThemeProvider>
          <RouterProvider router={router} />
        </ThemeProvider>
      </QueryClientProvider>
    </React.StrictMode>,
  );
});
