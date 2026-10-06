// Vite plugin: reads the newest Morpho release bundle (read-only) and serves it to the app.
//
//   GET /api/index          release meta + every word in learning order as [id, word]
//   GET /api/unit/:n        full data for unit n (unitSize consecutive words, 1-based)
//   GET /api/words?ids=1,2  full data for the given word ids (at most 200)
//   GET /media/img/{hash}.webp, /media/audio/{hash}.ogg   files straight from the bundle
//
// The release directory is re-resolved on every API request, so a newly cut release in
// data/releases/ is picked up without a restart.

import fs from 'node:fs';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Connect, Plugin } from 'vite';
import type { Example, ReleaseIndex, Sense, WordFull } from '../src/types';

export const UNIT_SIZE = 20;
const MAX_IDS = 200;
const MEDIA_RE = /^\/media\/(img|audio)\/([0-9a-f]{64})\.(webp|ogg)$/;
const CONTENT_TYPES: Record<string, string> = { webp: 'image/webp', ogg: 'audio/ogg' };

interface Loaded {
  dir: string;
  stamp: string;
  index: ReleaseIndex;
  order: WordFull[];
  byId: Map<number, WordFull>;
}

interface WordRow {
  word_id: number;
  word: string;
  phonetic: string | null;
  learning_order: number;
  word_audio_file: string;
}
interface SenseRow {
  word_id: number;
  pos: string;
  definition: string;
  is_primary: number;
  def_audio_file: string;
}
interface ExampleRow {
  word_id: number;
  sentence: string;
  hl_start: number;
  hl_end: number;
  ex_audio_file: string;
  image_file: string | null;
}

/** UTF-8 byte offset -> UTF-16 code-unit offset within `text`. */
function byteToChar(text: string, byteOffset: number): number {
  const buf = Buffer.from(text, 'utf8');
  const clamped = Math.max(0, Math.min(byteOffset, buf.length));
  return buf.subarray(0, clamped).toString('utf8').length;
}

function highlight(text: string, start: number, end: number): [number, number] | null {
  const s = byteToChar(text, start);
  const e = byteToChar(text, end);
  return e > s ? [s, e] : null;
}

function loadRelease(dir: string, stamp: string): Loaded {
  const db = new DatabaseSync(path.join(dir, 'release.db'), { readOnly: true });
  try {
    const meta = Object.fromEntries(
      (db.prepare('SELECT key, value FROM meta').all() as { key: string; value: string }[]).map((r) => [
        r.key,
        r.value,
      ]),
    );
    const words = db
      .prepare(
        'SELECT word_id, word, phonetic, learning_order, word_audio_file FROM words ORDER BY learning_order, word_id',
      )
      .all() as unknown as WordRow[];
    const senses = db
      .prepare(
        'SELECT word_id, pos, definition, is_primary, def_audio_file FROM senses ORDER BY word_id, is_primary DESC, sense_id',
      )
      .all() as unknown as SenseRow[];
    const examples = db
      .prepare(
        'SELECT word_id, sentence, hl_start, hl_end, ex_audio_file, image_file FROM examples WHERE display_order = 1 ORDER BY word_id, example_id',
      )
      .all() as unknown as ExampleRow[];

    const sensesBy = new Map<number, Sense[]>();
    for (const r of senses) {
      const list = sensesBy.get(r.word_id) ?? [];
      list.push({ pos: r.pos, def: r.definition, audio: r.def_audio_file, primary: r.is_primary === 1 });
      sensesBy.set(r.word_id, list);
    }
    const exampleBy = new Map<number, Example>();
    for (const r of examples) {
      if (exampleBy.has(r.word_id)) continue;
      exampleBy.set(r.word_id, {
        text: r.sentence,
        hl: highlight(r.sentence, r.hl_start, r.hl_end),
        audio: r.ex_audio_file,
        img: r.image_file,
      });
    }

    const order: WordFull[] = words.map((w, i) => ({
      id: w.word_id,
      word: w.word,
      phonetic: w.phonetic,
      order: i + 1,
      audio: w.word_audio_file,
      senses: sensesBy.get(w.word_id) ?? [],
      example: exampleBy.get(w.word_id) ?? null,
    }));

    return {
      dir,
      stamp,
      order,
      byId: new Map(order.map((w) => [w.id, w])),
      index: {
        release: path.basename(dir),
        contentVersion: meta.content_version ?? '',
        exportedAt: meta.exported_at ?? '',
        unitSize: UNIT_SIZE,
        words: order.map((w) => [w.id, w.word]),
      },
    };
  } finally {
    db.close();
  }
}

function hasDb(dir: string): boolean {
  return fs.existsSync(path.join(dir, 'release.db'));
}

/** MORPHO_RELEASE (a bundle name under data/releases, or a path), else the lexicographically last export-* bundle. */
function resolveReleaseDir(releasesDir: string, repoRoot: string): string {
  const override = process.env.MORPHO_RELEASE?.trim();
  if (override) {
    const candidates = path.isAbsolute(override)
      ? [override]
      : [
          path.join(releasesDir, override),
          path.resolve(process.env.INIT_CWD ?? process.cwd(), override),
          path.resolve(repoRoot, override),
        ];
    const hit = candidates.find(hasDb);
    if (!hit) throw new Error(`MORPHO_RELEASE=${override}: no release.db found in ${candidates.join(', ')}`);
    return hit;
  }
  const names = fs
    .readdirSync(releasesDir)
    .filter((n) => n.startsWith('export-') && hasDb(path.join(releasesDir, n)))
    .sort();
  const last = names.at(-1);
  if (!last) throw new Error(`no export-*/release.db under ${releasesDir}`);
  return path.join(releasesDir, last);
}

function sendJson(res: ServerResponse, status: number, body: unknown): void {
  const data = JSON.stringify(body);
  res.statusCode = status;
  res.setHeader('Content-Type', 'application/json; charset=utf-8');
  res.setHeader('Cache-Control', 'no-cache');
  res.end(data);
}

function serveMedia(req: IncomingMessage, res: ServerResponse, file: string, type: string, etag: string): void {
  let stat: fs.Stats;
  try {
    stat = fs.statSync(file);
  } catch {
    res.statusCode = 404;
    res.end('Not found');
    return;
  }
  res.setHeader('Content-Type', type);
  res.setHeader('Cache-Control', 'public, max-age=31536000, immutable');
  res.setHeader('ETag', etag);
  res.setHeader('Accept-Ranges', 'bytes');
  if (req.headers['if-none-match'] === etag) {
    res.statusCode = 304;
    res.end();
    return;
  }
  const size = stat.size;
  const range = /^bytes=(\d*)-(\d*)$/.exec(req.headers.range ?? '');
  let start = 0;
  let end = size - 1;
  if (range && (range[1] || range[2])) {
    if (range[1]) {
      start = Number(range[1]);
      end = range[2] ? Math.min(Number(range[2]), size - 1) : size - 1;
    } else {
      start = Math.max(0, size - Number(range[2]));
    }
    if (start > end || start >= size) {
      res.statusCode = 416;
      res.setHeader('Content-Range', `bytes */${size}`);
      res.end();
      return;
    }
    res.statusCode = 206;
    res.setHeader('Content-Range', `bytes ${start}-${end}/${size}`);
  } else {
    res.statusCode = 200;
  }
  res.setHeader('Content-Length', String(end - start + 1));
  if (req.method === 'HEAD') {
    res.end();
    return;
  }
  fs.createReadStream(file, { start, end }).pipe(res);
}

export function morphoRelease(): Plugin {
  let releasesDir = '';
  let repoRoot = '';
  let current: Loaded | null = null;

  function release(): Loaded {
    const dir = resolveReleaseDir(releasesDir, repoRoot);
    const stamp = `${dir}:${fs.statSync(path.join(dir, 'release.db')).mtimeMs}`;
    if (!current || current.stamp !== stamp) {
      current = loadRelease(dir, stamp);
      console.log(`[morpho] release ${current.index.release} (${current.index.contentVersion}), ${current.order.length} words`);
    }
    return current;
  }

  const handler: Connect.NextHandleFunction = (req, res, next) => {
    const url = new URL(req.url ?? '/', 'http://localhost');
    const p = url.pathname;
    if (!p.startsWith('/api/') && !p.startsWith('/media/')) return next();
    if (req.method !== 'GET' && req.method !== 'HEAD') return next();

    let rel: Loaded;
    try {
      rel = release();
    } catch (err) {
      sendJson(res, 500, { error: String(err instanceof Error ? err.message : err) });
      return;
    }

    if (p === '/api/index') return sendJson(res, 200, rel.index);

    const unit = /^\/api\/unit\/(\d+)$/.exec(p);
    if (unit) {
      const n = Number(unit[1]);
      const words = rel.order.slice((n - 1) * UNIT_SIZE, n * UNIT_SIZE);
      if (n < 1 || words.length === 0) return sendJson(res, 404, { error: `no unit ${n}` });
      return sendJson(res, 200, { unit: n, words });
    }

    if (p === '/api/words') {
      const ids = (url.searchParams.get('ids') ?? '')
        .split(',')
        .map((s) => Number(s))
        .filter((n) => Number.isInteger(n))
        .slice(0, MAX_IDS);
      const words = ids.map((id) => rel.byId.get(id)).filter((w): w is WordFull => !!w);
      return sendJson(res, 200, { words });
    }

    const media = MEDIA_RE.exec(p);
    if (media) {
      const [, kind, hash, ext] = media as unknown as [string, string, string, string];
      if ((kind === 'img') !== (ext === 'webp')) return sendJson(res, 404, { error: 'not found' });
      return serveMedia(req, res, path.join(rel.dir, kind, `${hash}.${ext}`), CONTENT_TYPES[ext]!, `"${hash}"`);
    }

    sendJson(res, 404, { error: 'not found' });
  };

  return {
    name: 'morpho-release',
    configResolved(config) {
      repoRoot = path.resolve(config.root, '..');
      releasesDir = path.join(repoRoot, 'data', 'releases');
    },
    configureServer(server) {
      server.middlewares.use(handler);
    },
    configurePreviewServer(server) {
      server.middlewares.use(handler);
    },
  };
}
