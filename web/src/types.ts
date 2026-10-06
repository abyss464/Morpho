// Shapes of the JSON the dev-server plugin (server/release.ts) serves from the release bundle.

export interface Sense {
  pos: string;
  def: string;
  /** Release-relative media path, e.g. "audio/{hash}.ogg". */
  audio: string;
  primary: boolean;
}

export interface Example {
  text: string;
  /** Highlight range in UTF-16 code units (converted from the release's UTF-8 byte offsets), or null. */
  hl: [number, number] | null;
  audio: string;
  /** Release-relative media path, e.g. "img/{hash}.webp", or null. */
  img: string | null;
}

export interface WordFull {
  id: number;
  word: string;
  phonetic: string | null;
  /** 1-based position in the release's learning order. */
  order: number;
  audio: string;
  /** Primary sense first, then the others in release order. */
  senses: Sense[];
  /** The display_order = 1 example; its picture was matched to this sentence. */
  example: Example | null;
}

export interface ReleaseIndex {
  release: string;
  contentVersion: string;
  exportedAt: string;
  unitSize: number;
  /** Every word in learning order as [word_id, word]. */
  words: [number, string][];
}

export interface UnitPayload {
  unit: number;
  words: WordFull[];
}

export interface WordsPayload {
  words: WordFull[];
}
