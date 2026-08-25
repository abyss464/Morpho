import { useState } from 'react';
import { describe, expect, it } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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

  it('offers the awaiting-approval presets', async () => {
    renderPage(<Harness />);
    await waitFor(() => expect(screen.getByText('abandon')).toBeInTheDocument());

    const presets = screen.getByLabelText('Approval worklist preset');
    for (const label of ['All words', 'Awaiting sense', 'Awaiting example', 'Awaiting image']) {
      expect(within(presets).getByText(label)).toBeInTheDocument();
    }
  });

  it('reveals the bulk-approve bar once a row is ticked', async () => {
    const user = userEvent.setup();
    renderPage(<Harness />);
    await waitFor(() => expect(screen.getByText('abandon')).toBeInTheDocument());

    expect(screen.queryByText(/selected$/)).not.toBeInTheDocument();

    const checkboxes = screen.getAllByRole('checkbox');
    // The first checkbox is the header "select this page" control.
    await user.click(checkboxes[1] as HTMLElement);

    await waitFor(() => expect(screen.getByText('1 selected')).toBeInTheDocument());
    expect(screen.getByText('Approve primary sense')).toBeInTheDocument();
    expect(screen.getByText('Approve example slot 1')).toBeInTheDocument();
    expect(screen.getByText('Approve image')).toBeInTheDocument();

    // Ticking a row must not navigate away from the list.
    expect(screen.queryByTestId('stub-/words/$wordId')).not.toBeInTheDocument();
  });
});
