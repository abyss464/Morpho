import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import {
  Outlet,
  RouterProvider,
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from '@tanstack/react-router';
import { render } from '@testing-library/react';
import { ThemeProvider } from '../app/theme';

function testQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0, staleTime: 0 },
      mutations: { retry: false },
    },
  });
}

/**
 * Renders a page component inside a throwaway router that registers stubs for
 * every destination the console links to, so `Link` and `useNavigate` behave
 * without pulling in the generated route tree.
 */
export function renderPage(ui: ReactNode, initialPath = '/') {
  const rootRoute = createRootRoute({ component: () => <Outlet /> });

  const stub = (path: string) =>
    createRoute({
      getParentRoute: () => rootRoute,
      path,
      component: () => <div data-testid={`stub-${path}`} />,
      validateSearch: (search: Record<string, unknown>) => search,
    });

  const indexRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: '/',
    component: () => <>{ui}</>,
    validateSearch: (search: Record<string, unknown>) => search,
  });

  const routeTree = rootRoute.addChildren([
    indexRoute,
    stub('/words'),
    stub('/words/$wordId'),
    stub('/oov'),
    stub('/dead-letters'),
    stub('/plan'),
    stub('/releases'),
  ]);

  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [initialPath] }),
  });

  const queryClient = testQueryClient();

  const result = render(
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
        <RouterProvider router={router as any} />
      </ThemeProvider>
    </QueryClientProvider>,
  );

  return { ...result, queryClient, router };
}
