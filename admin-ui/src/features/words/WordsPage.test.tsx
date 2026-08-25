import { useState } from 'react';
import { describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { renderPage } from '../../test/renderWithProviders';
import { WordsPage } from './WordsPage';
import type { WordsSearch } from './wordsSearch';

function Harness() {
  const [search, setSearch] = useState<WordsSearch>({});
  return <WordsPage search={search} onSearchChange={setSearch} />;
}

describe('WordsPage', () => {
  it('renders the fixture rows with asset chips and blocker tags', async () => {
    renderPage(<Harness />);

    await waitFor(() => expect(screen.getByText('abandon')).toBeInTheDocument());

    // AntD mirrors aria-label onto both the Select wrapper and its inner input.
    expect(screen.getByLabelText('Filter by lemma')).toBeInTheDocument();
    expect(screen.getAllByLabelText('Filter by role').length).toBeGreaterThan(0);
    expect(screen.getAllByLabelText('Filter by blocker code').length).toBeGreaterThan(0);
    expect(screen.getAllByLabelText('Filter by plan group').length).toBeGreaterThan(0);

    // Readiness is derived, so both states must be present in the fixtures.
    expect(screen.getAllByText('ready').length).toBeGreaterThan(0);
    expect(screen.getAllByText('blocked').length).toBeGreaterThan(0);
  });

  it('makes rows keyboard reachable', async () => {
    renderPage(<Harness />);
    await waitFor(() => expect(screen.getByText('abandon')).toBeInTheDocument());

    const rows = screen.getAllByRole('row').filter((row) => row.getAttribute('tabindex') === '0');
    expect(rows.length).toBeGreaterThan(0);
    expect(rows[0]?.getAttribute('aria-label')).toMatch(/ready|blocked/);
  });
});
