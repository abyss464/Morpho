import { createFileRoute } from '@tanstack/react-router';
import { GalleryPage } from '../features/gallery/GalleryPage';
import type { ImageSource } from '../api/types';

const SOURCES: ImageSource[] = [
  'unsplash',
  'pexels',
  'pixabay',
  'wikimedia',
  'openverse',
  'sdxl',
  'manual',
];

interface GallerySearch {
  source?: ImageSource;
  approved?: 'true' | 'false';
  q?: string;
}

function validateSearch(search: Record<string, unknown>): GallerySearch {
  const source =
    typeof search.source === 'string' && SOURCES.includes(search.source as ImageSource)
      ? (search.source as ImageSource)
      : undefined;
  const approved =
    search.approved === 'true' || search.approved === 'false' ? search.approved : undefined;

  return {
    ...(source ? { source } : {}),
    ...(approved ? { approved } : {}),
    ...(typeof search.q === 'string' && search.q ? { q: search.q } : {}),
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
