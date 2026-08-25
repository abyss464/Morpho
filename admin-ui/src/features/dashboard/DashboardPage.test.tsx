import { describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { renderPage } from '../../test/renderWithProviders';
import { DashboardPage } from './DashboardPage';

describe('DashboardPage', () => {
  it('shows a skeleton first, then the stat cards and the event feed', async () => {
    renderPage(<DashboardPage />);

    // The router resolves the route asynchronously, then the query settles.
    await waitFor(() => expect(screen.getByText('Reconciliation overview')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText('Words in scope')).toBeInTheDocument());

    expect(screen.getByText('Ready to ship')).toBeInTheDocument();
    expect(screen.getByText('Blocked')).toBeInTheDocument();
    expect(screen.getByText('OOV open')).toBeInTheDocument();
    expect(screen.getByText('Dead letters')).toBeInTheDocument();
    expect(screen.getByText('Recent events')).toBeInTheDocument();
    expect(screen.getByText('Asset gaps by type')).toBeInTheDocument();
  });

  it('renders one readiness bar per asset type', async () => {
    renderPage(<DashboardPage />);
    await waitFor(() => expect(screen.getByText('Asset readiness rate')).toBeInTheDocument());

    for (const label of ['Definitions', 'Examples (slot 1)', 'Images', 'TTS assets']) {
      expect(screen.getByLabelText(`${label} readiness`)).toBeInTheDocument();
    }
  });
});
