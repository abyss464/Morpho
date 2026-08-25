import { createFileRoute } from '@tanstack/react-router';
import { ReleasesPage } from '../features/releases/ReleasesPage';

export const Route = createFileRoute('/releases')({
  component: ReleasesPage,
});
