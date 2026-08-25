import { QueryClient } from '@tanstack/react-query';
import { describe, expect, it, vi } from 'vitest';
import { qk } from '../api/queryKeys';
import {
  CHANGE_TARGETS,
  applyInvalidations,
  collectInvalidations,
  targetsFor,
} from './useChangeStream';

describe('targetsFor', () => {
  it('maps every entity type morpho-domain publishes', () => {
    // Vocabulary of `EntityType` in core/crates/domain/src/change.rs.
    const vocabulary = [
      'word',
      'definition_candidate',
      'definition_selection',
      'example_candidate',
      'example_selection',
      'image_candidate',
      'image_selection',
      'def_extraction',
      'oos_queue',
      'tts_asset',
      'media_file',
      'job_state',
      'source_fetch',
      'plan',
      'distractor',
      'release',
    ];
    for (const entityType of vocabulary) {
      expect(CHANGE_TARGETS).toHaveProperty(entityType);
    }
  });

  it('keeps the job queue off the word worklist', () => {
    expect(targetsFor('job_state')).toEqual(['deadLetters', 'jobs', 'dashboard']);
  });

  it('falls back to the global rollups for an unknown type', () => {
    expect(targetsFor('something_core_added_later')).toEqual(['dashboard', 'events']);
  });
});

describe('collectInvalidations', () => {
  it('unions targets and pins the exact word details', () => {
    const batch = collectInvalidations([
      { entity_type: 'word', entity_ids: ['12', 13] },
      { entity_type: 'oos_queue', entity_ids: ['cordial'] },
      { entity_type: 'media_file', entity_ids: ['deadbeef'] },
    ]);

    expect([...batch.targets].sort()).toEqual([
      'dashboard',
      'events',
      'oov',
      'plan',
      'releases',
      'words',
    ]);
    expect([...batch.wordIds].sort((a, b) => a - b)).toEqual([12, 13]);
  });

  it('ignores non-numeric ids on non-word entities', () => {
    const batch = collectInvalidations([
      { entity_type: 'job_state', entity_ids: ['synth_tts:tts_input:abc'] },
    ]);
    expect(batch.wordIds.size).toBe(0);
  });
});

describe('applyInvalidations', () => {
  it('invalidates one query family per target plus each word detail', () => {
    const client = new QueryClient();
    const spy = vi.spyOn(client, 'invalidateQueries').mockResolvedValue(undefined);

    applyInvalidations(client, {
      targets: new Set(['words', 'dashboard']),
      wordIds: new Set([42]),
    });

    const keys = spy.mock.calls.map((call) => call[0]?.queryKey);
    expect(keys).toContainEqual(qk.words());
    expect(keys).toContainEqual(qk.dashboard());
    expect(keys).toContainEqual(qk.wordDetail(42));
    expect(spy).toHaveBeenCalledTimes(3);
  });
});
