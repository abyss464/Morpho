import { createFileRoute } from '@tanstack/react-router';
import { PlanPage } from '../features/plan/PlanPage';

export const Route = createFileRoute('/plan')({
  component: PlanPage,
});
