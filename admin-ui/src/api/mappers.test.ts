import { describe, expect, it } from 'vitest';
import {
  mapDashboard,
  mapDeadLetter,
  mapEvent,
  mapHoldbackReport,
  mapOovEntry,
  mapPaginated,
  mapPlanSummary,
  mapTtsStatus,
  mapWord,
  mapWordDetail,
  mapWordListItem,
  parseJsonColumn,
  toBlockers,
  toBool,
  toNullableNumber,
  toNumber,
} from './mappers';

/**
 * The mappers exist because SQLite stores booleans as 0/1 and structured
 * payloads as TEXT. These tests pin both encodings so a change in how morphod
 * serializes cannot silently reshape the UI.
 */

describe('scalar coercion', () => {
  it('treats SQLite integer booleans and JSON booleans alike', () => {
    expect(toBool(1)).toBe(true);
    expect(toBool(0)).toBe(false);
    expect(toBool(true)).toBe(true);
    expect(toBool(false)).toBe(false);
    expect(toBool('1')).toBe(true);
    expect(toBool('true')).toBe(true);
    expect(toBool('0')).toBe(false);
    expect(toBool(null)).toBe(false);
    expect(toBool(undefined)).toBe(false);
  });

  it('falls back rather than producing NaN', () => {
    expect(toNumber('42')).toBe(42);
    expect(toNumber('not a number', 7)).toBe(7);
    expect(toNumber(undefined, 3)).toBe(3);
    expect(toNullableNumber('')).toBeNull();
    expect(toNullableNumber(null)).toBeNull();
    expect(toNullableNumber('0')).toBe(0);
  });
});

describe('JSON TEXT columns', () => {
  it('parses a JSON string and passes an already-decoded value through', () => {
    expect(parseJsonColumn('{"a":1}', {})).toEqual({ a: 1 });
    expect(parseJsonColumn({ a: 1 }, {})).toEqual({ a: 1 });
    expect(parseJsonColumn('not json', { fallback: true })).toEqual({ fallback: true });
    expect(parseJsonColumn(null, [])).toEqual([]);
  });

  it('reads words.blockers whether it arrives as TEXT or as an array', () => {
    expect(toBlockers('["missing_image","tts_failed"]')).toEqual(['missing_image', 'tts_failed']);
    expect(toBlockers(['oos_pending'])).toEqual(['oos_pending']);
    expect(toBlockers('[]')).toEqual([]);
    expect(toBlockers(null)).toEqual([]);
    // Non-string members are dropped rather than rendered as "[object Object]".
    expect(toBlockers('["ok", 3, null]')).toEqual(['ok']);
  });
});

describe('mapWord / mapWordListItem', () => {
  it('normalizes a raw SQLite-shaped word row', () => {
    const word = mapWord({
      word_id: 12,
      lemma: 'benevolent',
      role: 'target',
      aux_status: null,
      phonetic: '/bəˈnevələnt/',
      frequency_rank: 2288,
      etymology: 'Latin bene + volens',
      etymology_source: 'wiktionary',
      ready: 0,
      blockers: '["oos_pending"]',
      created_by: 'import',
      created_at: '2026-08-18T09:00:00.000Z',
    });

    expect(word.ready).toBe(false);
    expect(word.blockers).toEqual(['oos_pending']);
    expect(word.frequency_rank).toBe(2288);
    expect(word.aux_status).toBeNull();
  });

  it('defaults missing rollup fields to zero instead of undefined', () => {
    const item = mapWordListItem({ word_id: 3, lemma: 'wary', role: 'target', ready: 1 });
    expect(item).toMatchObject({
      word_id: 3,
      lemma: 'wary',
      ready: true,
      blockers: [],
      has_image: false,
      sense_count: 0,
      example_count: 0,
      tts_missing: 0,
    });
  });
});

describe('mapEvent', () => {
  it('accepts detail as a JSON string or an object, and never throws', () => {
    expect(mapEvent({ event_id: 1, detail: '{"before":1,"after":2}' }).detail).toEqual({
      before: 1,
      after: 2,
    });
    expect(mapEvent({ event_id: 2, detail: { token: 'altruistic' } }).detail).toEqual({
      token: 'altruistic',
    });
    expect(mapEvent({ event_id: 3, detail: null }).detail).toBeNull();
    expect(mapEvent({ event_id: 4, detail: 'not json' }).detail).toBeNull();
  });
});

describe('mapPaginated', () => {
  it('derives total from the item count when the server omits it', () => {
    const page = mapPaginated({ items: [{ word_id: 1 }, { word_id: 2 }] }, mapWordListItem);
    expect(page.total).toBe(2);
  });

  it('yields an empty page for a malformed body rather than crashing', () => {
    expect(mapPaginated(null, mapWordListItem)).toEqual({ items: [], total: 0 });
  });
});

describe('mapWordDetail', () => {
  it('builds every slot group and coerces selection flags', () => {
    const detail = mapWordDetail({
      word: { word_id: 5, lemma: 'adapt', role: 'target', ready: 1, blockers: '[]' },
      definitions: [
        {
          pos: 'verb',
          selection: {
            word_id: 5,
            pos: 'verb',
            def_cand_id: 90,
            is_primary: 1,
            enabled: 1,
            selected_by: 'auto',
            pinned: 0,
            approved: 1,
            selection_rev: 2,
          },
          candidates: [
            {
              def_cand_id: 90,
              word_id: 5,
              pos: 'verb',
              text: 'to change something so that it fits new conditions',
              text_hash: 'abc',
              source: 'freedict',
              status: 'available',
              auto_score: 0.94,
              score_detail: '{"source_prior":0.38}',
            },
          ],
        },
      ],
      examples: [{ slot: 1, selection: null, candidates: [] }],
      image: { selection: null, candidates: [] },
      tts: [],
      distractors: [
        { rank: 1, word_id: 6, lemma: 'adopt', core_ready: 0, blockers: '["tts_failed"]' },
      ],
      recent_events: [],
    });

    expect(detail.word.ready).toBe(true);
    expect(detail.definitions[0]?.selection?.is_primary).toBe(true);
    expect(detail.definitions[0]?.selection?.pinned).toBe(false);
    expect(detail.definitions[0]?.candidates[0]?.score_detail).toEqual({ source_prior: 0.38 });
    expect(detail.distractors[0]?.core_ready).toBe(false);
    expect(detail.distractors[0]?.blockers).toEqual(['tts_failed']);
  });
});

describe('mapTtsStatus', () => {
  it('reports "missing" when the desired text has no asset row', () => {
    const view = mapTtsStatus({ kind: 'definition', text: 'x', ref: { pos: 'verb' } });
    expect(view.status).toBe('missing');
    expect(view.file_hash).toBeNull();
    expect(view.ref).toEqual({ pos: 'verb' });
  });

  it('keeps a slot reference numeric', () => {
    expect(mapTtsStatus({ kind: 'example', ref: { slot: '2' } }).ref).toEqual({ slot: 2 });
  });
});

describe('mapDashboard', () => {
  it('fills every asset rollup even when the server sends a partial body', () => {
    const dashboard = mapDashboard({
      words: { total: 60, target: 50, auxiliary: 10, ready: 21, blocked: 39 },
      assets: { definitions: { ready: 40, missing: 2, failed: 8 } },
      oos_open: 5,
      dead_letters: 9,
      plan: null,
      recent_events: [{ event_id: 1, action: 'approved' }],
    });

    expect(dashboard.assets.tts).toEqual({ ready: 0, missing: 0, failed: 0 });
    expect(dashboard.words.ready).toBe(21);
    expect(dashboard.plan).toBeNull();
    expect(dashboard.recent_events).toHaveLength(1);
  });
});

describe('mapPlanSummary', () => {
  it('accepts stats and params from either the parsed field or the *_json column', () => {
    const plan = mapPlanSummary({
      plan_id: 2,
      is_current: 1,
      stats_json: '{"word_count":60,"group_count":6}',
      params_json: '{"group_min":15}',
      groups: [{ group_seq: 1, group_type: 'scc', word_count: 3, ready_count: 2 }],
      diff: { previous_plan_id: 1, added: 4, removed: 0, reordered: 4 },
    });

    expect(plan.is_current).toBe(true);
    expect(plan.stats.word_count).toBe(60);
    expect(plan.params).toEqual({ group_min: 15 });
    expect(plan.groups[0]?.group_type).toBe('scc');
    expect(plan.diff.previous_plan_id).toBe(1);
  });
});

describe('mapHoldbackReport', () => {
  it('sorts excluded words by descending impact, then alphabetically', () => {
    const report = mapHoldbackReport({
      plan_id: 2,
      shippable_count: 20,
      exportable_count: 18,
      excluded_count: 3,
      gates_pass: 0,
      gate_failures: [{ gate: 'no_open_oos', message: 'still open', word_id: null }],
      excluded: [
        { word_id: 1, lemma: 'zeal', impact_count: 1 },
        { word_id: 2, lemma: 'benevolent', impact_count: 7 },
        { word_id: 3, lemma: 'alleviate', impact_count: 1 },
      ],
    });

    expect(report.excluded.map((row) => row.lemma)).toEqual(['benevolent', 'alleviate', 'zeal']);
    expect(report.gates_pass).toBe(false);
    expect(report.gate_failures[0]?.gate).toBe('no_open_oos');
  });
});

describe('mapOovEntry and mapDeadLetter', () => {
  it('derives occurrence_count and keeps the LLM draft', () => {
    const entry = mapOovEntry({
      oos_lemma: 'altruistic',
      status: 'open',
      occurrences: [
        {
          word_id: 12,
          lemma: 'benevolent',
          pos: 'adj',
          def_cand_id: 90,
          text: 'showing an altruistic wish to help',
          hits: 1,
          suggested_rewrite: 'showing a kindly wish to help',
        },
      ],
    });
    expect(entry.occurrence_count).toBe(1);
    expect(entry.occurrences[0]?.suggested_rewrite).toBe('showing a kindly wish to help');
  });

  it('falls back to subject_id when no joined label is present', () => {
    const row = mapDeadLetter({
      kind: 'synth_tts',
      subject_type: 'word',
      subject_id: '31',
      rate_key: 'edge_tts',
      status: 'dead',
      attempts: 5,
      subject: {},
    });
    expect(row.subject.label).toBe('31');
    expect(row.subject.word_id).toBeNull();
  });
});
