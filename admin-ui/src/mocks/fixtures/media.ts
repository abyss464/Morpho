/**
 * Byte sources for the mocked `/api/media/{file_hash}` endpoint.
 *
 * Audio: one real 350 ms silent Ogg/Opus clip, reused for every TTS hash. It is
 * a genuine container, so `<audio>` reports a duration and fires `ended` — the
 * play buttons in Word Detail behave exactly as they will against morphod.
 *
 * Images: deterministic SVG placeholders derived from the file hash. The real
 * store serves WebP; for a mock the only thing that matters is that each hash
 * yields stable, visually distinct bytes with a correct Content-Type.
 */

/** `ffmpeg -f lavfi -i anullsrc -t 0.35 -c:a libopus` — 450 bytes. */
const SILENT_OGG_BASE64 =
  'T2dnUwACAAAAAAAAAADQPhlSAAAAABaP9OQBE09wdXNIZWFkAQE4AYC7AAAAAABPZ2dTAAAAAAAAAAAAANA+GVIBAAAA' +
  '/4ShPAE8T3B1c1RhZ3MMAAAATGF2ZjYzLjEuMTAxAQAAABwAAABlbmNvZGVyPUxhdmM2My4xLjEwMSBsaWJvcHVzT2dn' +
  'UwAE2EIAAAAAAADQPhlSAgAAAAF18KUSDhAPDw8PDw8PDw8PDw8PDw8PaAvkwTbsxY2MSUhCbNBoB8lyJ+FE6lXx8MDR' +
  'MikwaAfJecjJV8CiEiP6WSBwaAfJecjJV8CiEiP6WSBwaAfJecjJV8CiEiP6WSBwaAfJecjJV8CiEiP6WSBwaAfJecjJ' +
  'V8CiEiP6WSBwaAfJecjJV8CiEiP6WSBwaAfJecjJV8CiEiP6WSBwaAfJecjJV8CiEiP6WSBwaAfJecjJV8CiEiP6WSBw' +
  'aAfJecjJV8CiEiP6WSBwaAfJecjJV8CiEiP6WSBwaAfJecjJV8CiEiP6WSBwaAfJecjJV8CiEiP6WSBwaAfJecjJV8Ci' +
  'EiP6WSBwaAfJecjJV8CiEiP6WSBwaAfJecjJV8CiEiP6WSBw';

let cachedOgg: Uint8Array | null = null;

export function silentOggBytes(): Uint8Array {
  if (cachedOgg) return cachedOgg;
  const binary = atob(SILENT_OGG_BASE64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  cachedOgg = bytes;
  return bytes;
}

export const SILENT_OGG_BYTES = 450;

function hueFromHash(hash: string): number {
  let acc = 0;
  for (let i = 0; i < hash.length; i += 1) acc = (acc * 31 + hash.charCodeAt(i)) >>> 0;
  return acc % 360;
}

/**
 * Stable placeholder artwork for one image candidate. `label` carries the query
 * that produced the candidate so the grid reads like real stock results.
 */
export function placeholderImageSvg(fileHash: string, label: string): string {
  const hue = hueFromHash(fileHash);
  const alt = (hue + 47) % 360;
  const safe = label.replace(/[<>&]/g, '').slice(0, 46);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 768 576" width="768" height="576">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="hsl(${hue} 62% 58%)"/>
      <stop offset="100%" stop-color="hsl(${alt} 55% 34%)"/>
    </linearGradient>
  </defs>
  <rect width="768" height="576" fill="url(#g)"/>
  <circle cx="${140 + (hue % 380)}" cy="${120 + (alt % 300)}" r="${90 + (hue % 70)}"
          fill="hsl(${alt} 70% 72%)" fill-opacity="0.35"/>
  <circle cx="${520 - (alt % 300)}" cy="${430 - (hue % 240)}" r="${60 + (alt % 90)}"
          fill="hsl(${hue} 80% 88%)" fill-opacity="0.25"/>
  <text x="38" y="522" font-family="system-ui, sans-serif" font-size="34" font-weight="600"
        fill="rgba(255,255,255,0.94)">${safe}</text>
  <text x="38" y="556" font-family="ui-monospace, monospace" font-size="18"
        fill="rgba(255,255,255,0.7)">${fileHash.slice(0, 16)}</text>
</svg>`;
}
