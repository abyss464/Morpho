import { createFileRoute } from '@tanstack/react-router';
import { WordDetailPage } from '../../features/words/WordDetailPage';
import { WORD_DETAIL_TABS, type WordDetailTab } from '../../features/words/tabs';

/** A route rather than a drawer: every tab of every word is deep-linkable. */
export const Route = createFileRoute('/words/$wordId')({
  validateSearch: (search: Record<string, unknown>): { tab?: WordDetailTab } =>
    WORD_DETAIL_TABS.includes(search.tab as WordDetailTab)
      ? { tab: search.tab as WordDetailTab }
      : {},
  component: RouteComponent,
});

function RouteComponent() {
  const { wordId } = Route.useParams();
  const { tab } = Route.useSearch();
  const navigate = Route.useNavigate();

  return (
    <WordDetailPage
      wordId={Number(wordId)}
      tab={tab ?? 'senses'}
      onTabChange={(next) => void navigate({ search: { tab: next }, replace: true })}
    />
  );
}
