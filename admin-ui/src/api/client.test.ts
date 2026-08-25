import { HttpResponse, http } from 'msw';
import { describe, expect, it } from 'vitest';
import { server } from '../mocks/node';
import { ApiError, buildQuery, extractGateFailures, mediaUrl, request } from './client';

describe('buildQuery', () => {
  it('drops null, undefined and empty values', () => {
    expect(buildQuery({ page: 1, role: 'target', q: '', blocker: undefined, group: null })).toBe(
      '?page=1&role=target',
    );
  });

  it('returns an empty string when nothing survives', () => {
    expect(buildQuery({ a: undefined })).toBe('');
    expect(buildQuery(undefined)).toBe('');
  });

  it('encodes booleans the way the ?ready= filter expects', () => {
    expect(buildQuery({ ready: false })).toBe('?ready=false');
    expect(buildQuery({ ready: true })).toBe('?ready=true');
  });
});

describe('mediaUrl', () => {
  it('points at the content-addressed media endpoint', () => {
    expect(mediaUrl('deadbeef')).toBe('/api/media/deadbeef');
  });
});

describe('request error handling', () => {
  it('turns the error envelope into a typed ApiError', async () => {
    server.use(
      http.get('/api/words/999', () =>
        HttpResponse.json(
          { error: { code: 'word_not_found', message: 'No word with id 999.' } },
          { status: 404 },
        ),
      ),
    );

    await expect(request('/words/999')).rejects.toMatchObject({
      name: 'ApiError',
      status: 404,
      code: 'word_not_found',
      message: 'No word with id 999.',
    });

    const error = await request('/words/999').catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).isNotFound).toBe(true);
  });

  it('still fails usefully when the body is not an envelope', async () => {
    server.use(http.get('/api/plan', () => new HttpResponse('boom', { status: 500 })));
    const error = (await request('/plan').catch((caught: unknown) => caught)) as ApiError;
    expect(error.status).toBe(500);
    expect(error.message).toBe('boom');
  });
});

describe('extractGateFailures', () => {
  const failures = [{ gate: 'no_open_oos', message: 'still open', word_id: null, lemma: null }];

  it('reads a top-level failures array', () => {
    const error = new ApiError(409, 'export_gates_failed', 'nope', {
      error: { code: 'export_gates_failed', message: 'nope' },
      failures,
    });
    expect(extractGateFailures(error)).toEqual(failures);
  });

  it('also reads failures nested under error.details', () => {
    const error = new ApiError(409, 'export_gates_failed', 'nope', {
      error: { code: 'export_gates_failed', message: 'nope', details: { failures } },
    });
    expect(extractGateFailures(error)).toEqual(failures);
  });

  it('returns an empty list for anything else', () => {
    expect(extractGateFailures(new Error('plain'))).toEqual([]);
    expect(extractGateFailures(new ApiError(500, 'x', 'y', null))).toEqual([]);
  });
});
