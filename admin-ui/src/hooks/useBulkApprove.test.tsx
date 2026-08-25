import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { HttpResponse, http } from 'msw';
import { describe, expect, it } from 'vitest';
import { getWord, listWords } from '../api/endpoints';
import { server } from '../mocks/node';
import { tally } from '../features/words/bulkApprove';
import { collectMatchingWords, useBulkApprove } from './useBulkApprove';

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 }, mutations: { retry: false } },
  });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

async function unapprovedSenseWords(limit: number) {
  const page = await listWords({ blocker: 'sense_not_approved', page_size: 100 });
  return page.items.slice(0, limit).map((item) => ({ word_id: item.word_id, lemma: item.lemma }));
}

describe('collectMatchingWords', () => {
  it('walks every page of the filter, not just the visible one', async () => {
    const firstPage = await listWords({ page: 1, page_size: 5 });
    const all = await collectMatchingWords({}, new AbortController().signal);

    expect(all).toHaveLength(firstPage.total);
    expect(new Set(all.map((row) => row.word_id)).size).toBe(firstPage.total);
  });

  it('stops early when the caller aborts', async () => {
    const controller = new AbortController();
    controller.abort();
    await expect(collectMatchingWords({}, controller.signal)).resolves.toEqual([]);
  });
});

describe('useBulkApprove', () => {
  it('approves the primary sense of every selected word, one at a time', async () => {
    const words = await unapprovedSenseWords(3);
    expect(words.length).toBeGreaterThan(0);

    const { result } = renderHook(() => useBulkApprove(), { wrapper });
    await result.current.start('definition_primary', words);

    await waitFor(() => expect(result.current.progress?.running).toBe(false));
    const counts = tally(result.current.progress?.items ?? []);
    expect(counts.total).toBe(words.length);
    expect(counts.failed).toBe(0);
    expect(counts.approved).toBeGreaterThan(0);

    for (const word of words) {
      const detail = await getWord(word.word_id);
      const primary = detail.definitions.find((slot) => slot.selection?.is_primary);
      expect(primary?.selection?.approved).toBe(true);
      expect(primary?.selection?.approved_by).toBe('admin:local');
    }
  });

  it('records a skip instead of a write when there is nothing to approve', async () => {
    const page = await listWords({ page_size: 200 });
    const imageless = page.items
      .filter((item) => !item.has_image)
      .slice(0, 2)
      .map((item) => ({ word_id: item.word_id, lemma: item.lemma }));
    expect(imageless.length).toBeGreaterThan(0);

    const { result } = renderHook(() => useBulkApprove(), { wrapper });
    await result.current.start('image', imageless);
    await waitFor(() => expect(result.current.progress?.running).toBe(false));

    const counts = tally(result.current.progress?.items ?? []);
    expect(counts.skipped).toBe(imageless.length);
    expect(counts.failed).toBe(0);
    expect(result.current.progress?.items[0]?.note).toBe('No image is selected.');
  });

  it('surfaces a per-item failure and keeps going', async () => {
    const words = await unapprovedSenseWords(2);
    const [first] = words;
    server.use(
      http.post('/api/selections/definition/approve', async ({ request }) => {
        const body = (await request.json()) as { word_id: number };
        if (body.word_id !== first?.word_id) return;
        return HttpResponse.json(
          { error: { code: 'locked', message: 'Slot is locked by another operator.' } },
          { status: 409 },
        );
      }),
    );

    const { result } = renderHook(() => useBulkApprove(), { wrapper });
    await result.current.start('definition_primary', words);
    await waitFor(() => expect(result.current.progress?.running).toBe(false));

    const items = result.current.progress?.items ?? [];
    expect(items[0]?.status).toBe('failed');
    expect(items[0]?.note).toContain('locked by another operator');
    // The run does not abort on one bad row.
    expect(items[1]?.status).not.toBe('pending');
  });
});
