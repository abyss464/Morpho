import type { ReactNode } from 'react';
import { Link } from '@tanstack/react-router';
import { theme } from 'antd';
import type { WordDetailTab } from '../features/words/tabs';

/**
 * Deep link to a word detail route, styled like an AntD link.
 *
 * Rendering `<Typography.Link>` inside a router `<Link>` would nest one anchor
 * inside another, so the router link carries the styling itself.
 */
export function WordLink({
  wordId,
  children,
  strong = false,
  tab,
}: {
  wordId: number;
  children: ReactNode;
  strong?: boolean;
  tab?: WordDetailTab;
}) {
  const { token } = theme.useToken();
  return (
    <Link
      to="/words/$wordId"
      params={{ wordId: String(wordId) }}
      {...(tab ? { search: { tab } } : {})}
      style={{ color: token.colorLink, fontWeight: strong ? 600 : undefined }}
    >
      {children}
    </Link>
  );
}
