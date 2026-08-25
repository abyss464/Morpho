import { createFileRoute } from '@tanstack/react-router';
import { DeadLettersPage } from '../features/deadletters/DeadLettersPage';

export const Route = createFileRoute('/dead-letters')({
  component: DeadLettersPage,
});
