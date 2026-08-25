import { createFileRoute } from '@tanstack/react-router';
import { WordsPage } from '../../features/words/WordsPage';
import type { WordsSearch } from '../../features/words/wordsSearch';
import type { WordRole } from '../../api/types';

const ROLES: WordRole[] = ['target', 'base', 'auxiliary'];

/** Filters live in the URL, so any view of the worklist is a shareable link. */
function validateSearch(search: Record<string, unknown>): WordsSearch {
  const role =
    typeof search.role === 'string' && ROLES.includes(search.role as WordRole)
      ? (search.role as WordRole)
      : undefined;
  const ready = search.ready === 'true' || search.ready === 'false' ? search.ready : undefined;
  const page = Number(search.page);
  const pageSize = Number(search.page_size);
  const group = Number(search.group);

  return {
    ...(Number.isFinite(page) && page > 0 ? { page } : {}),
    ...(Number.isFinite(pageSize) && pageSize > 0 ? { page_size: pageSize } : {}),
    ...(role ? { role } : {}),
    ...(ready ? { ready } : {}),
    ...(typeof search.blocker === 'string' && search.blocker ? { blocker: search.blocker } : {}),
    ...(Number.isFinite(group) && group > 0 ? { group } : {}),
    ...(typeof search.q === 'string' && search.q ? { q: search.q } : {}),
  };
}

export const Route = createFileRoute('/words/')({
  validateSearch,
  component: RouteComponent,
});

function RouteComponent() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  return (
    <WordsPage
      search={search}
      onSearchChange={(next) => void navigate({ search: next, replace: true })}
    />
  );
}
