import { createFileRoute } from '@tanstack/react-router';
import { OovPage, type OovSearch } from '../features/oov/OovPage';
import type { OovStatus } from '../api/types';

const STATUSES: OovStatus[] = ['open', 'resolved_rewrite', 'resolved_promote', 'auto_closed'];

export const Route = createFileRoute('/oov')({
  validateSearch: (search: Record<string, unknown>): OovSearch => {
    const page = Number(search.page);
    const pageSize = Number(search.page_size);
    return {
      ...(STATUSES.includes(search.status as OovStatus)
        ? { status: search.status as OovStatus }
        : {}),
      ...(Number.isFinite(page) && page > 0 ? { page } : {}),
      ...(Number.isFinite(pageSize) && pageSize > 0 ? { page_size: pageSize } : {}),
    };
  },
  component: RouteComponent,
});

function RouteComponent() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  return (
    <OovPage
      search={search}
      onSearchChange={(next) => void navigate({ search: next, replace: true })}
    />
  );
}
