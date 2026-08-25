import { Outlet, createRootRouteWithContext } from '@tanstack/react-router';
import type { QueryClient } from '@tanstack/react-query';
import { Button, Result } from 'antd';
import { AppLayout } from '../components/AppLayout';
import { errorMessage } from '../lib/errors';

export interface RouterContext {
  queryClient: QueryClient;
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: () => (
    <AppLayout>
      <Outlet />
    </AppLayout>
  ),
  notFoundComponent: () => (
    <AppLayout>
      <Result
        status="404"
        title="No such page"
        subTitle="The console has no route at this address."
        extra={
          <Button type="primary" href="/">
            Back to dashboard
          </Button>
        }
      />
    </AppLayout>
  ),
  errorComponent: ({ error, reset }) => (
    <AppLayout>
      <Result
        status="error"
        title="This screen failed to render"
        subTitle={errorMessage(error)}
        extra={
          <Button type="primary" onClick={reset}>
            Try again
          </Button>
        }
      />
    </AppLayout>
  ),
});
