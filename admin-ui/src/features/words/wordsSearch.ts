import type { WordRole } from '../../api/types';

/**
 * Route-owned search state for the word list. Every filter is a URL parameter,
 * so any view of the worklist is a shareable link. All fields are optional so
 * `<Link to="/words" />` needs no search object; the page applies the defaults.
 */
export interface WordsSearch {
  page?: number;
  page_size?: number;
  role?: WordRole;
  ready?: 'true' | 'false';
  blocker?: string;
  group?: number;
  q?: string;
}

export const DEFAULT_PAGE = 1;
export const DEFAULT_PAGE_SIZE = 25;
