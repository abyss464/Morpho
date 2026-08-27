import { createFileRoute } from '@tanstack/react-router';
import { GalleryPage } from '../features/gallery/GalleryPage';
import type { GallerySort, ImageSource } from '../api/types';

const SOURCES: ImageSource[] = [
  'unsplash',
  'pexels',
  'pixabay',
  'wikimedia',
  'openverse',
  'sdxl',
  'codex',
  'manual',
];

const SORTS: GallerySort[] = ['clip_asc', 'clip_desc'];
const VIEWS = ['flagged', 'needs_regen'] as const;
type GalleryViewMode = (typeof VIEWS)[number];

interface GallerySearch {
  source?: ImageSource;
  approved?: 'true' | 'false';
  q?: string;
  sort?: GallerySort;
  view?: GalleryViewMode;
}

function validateSearch(search: Record<string, unknown>): GallerySearch {
  const source =
    typeof search.source === 'string' && SOURCES.includes(search.source as ImageSource)
      ? (search.source as ImageSource)
      : undefined;
  const approved =
    search.approved === 'true' || search.approved === 'false' ? search.approved : undefined;
  const sort =
    typeof search.sort === 'string' && SORTS.includes(search.sort as GallerySort)
      ? (search.sort as GallerySort)
      : undefined;
  const view =
    typeof search.view === 'string' && VIEWS.includes(search.view as GalleryViewMode)
      ? (search.view as GalleryViewMode)
      : undefined;

  return {
    ...(source ? { source } : {}),
    ...(approved ? { approved } : {}),
    ...(typeof search.q === 'string' && search.q ? { q: search.q } : {}),
    ...(sort ? { sort } : {}),
    ...(view ? { view } : {}),
  };
}

export const Route = createFileRoute('/gallery')({
  validateSearch,
  component: RouteComponent,
});

function RouteComponent() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  return (
    <GalleryPage
      search={search}
      onSearchChange={(next) => void navigate({ search: next, replace: true })}
    />
  );
}
