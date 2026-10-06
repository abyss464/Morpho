import type { MouseEvent, ReactNode } from 'react';
import { mediaUrl } from '../api';
import { play, usePlaying } from '../audio';
import type { Note } from '../store';
import type { Example, Sense, WordFull } from '../types';

const POS: Record<string, string> = {
  noun: 'n.',
  verb: 'v.',
  adj: 'adj.',
  adv: 'adv.',
  prep: 'prep.',
  conj: 'conj.',
  interj: 'interj.',
  phrase: 'phr.',
};
export const posLabel = (pos: string): string => POS[pos] ?? `${pos}.`;

export const primarySense = (w: WordFull): Sense | undefined => w.senses.find((s) => s.primary) ?? w.senses[0];

/* ---------- highlighting ---------- */

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

export function wordPattern(word: string): RegExp | null {
  const w = word.trim().toLowerCase();
  if (!w) return null;
  if (/\s/.test(w)) return new RegExp(`\\b${w.split(/\s+/).map(esc).join('\\s+')}\\b`, 'gi');
  const alts = [`${esc(w)}(?:s|es|ed|d|ing|'s)?`];
  if (w.endsWith('e')) alts.push(`${esc(w.slice(0, -1))}(?:ing|ed)`);
  if (w.endsWith('y')) alts.push(`${esc(w.slice(0, -1))}(?:ies|ied)`);
  const last = w.at(-1) ?? '';
  if (/[bdgklmnprt]/.test(last)) alts.push(`${esc(w)}${last}(?:ed|ing)`);
  return new RegExp(`\\b(?:${alts.join('|')})\\b`, 'gi');
}

/** Bolds forms of the headword inside a definition. */
export function markWord(text: string, word: string): ReactNode {
  const re = wordPattern(word);
  if (!re) return text;
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of text.matchAll(re)) {
    const i = m.index ?? 0;
    if (i > last) out.push(text.slice(last, i));
    out.push(<b key={i}>{m[0]}</b>);
    last = i + m[0].length;
  }
  if (!out.length) return text;
  out.push(text.slice(last));
  return out;
}

export function markExample(ex: Example, word: string): ReactNode {
  if (!ex.hl) return markWord(ex.text, word);
  const [s, e] = ex.hl;
  return (
    <>
      {ex.text.slice(0, s)}
      <b>{ex.text.slice(s, e)}</b>
      {ex.text.slice(e)}
    </>
  );
}

/* ---------- pieces ---------- */

const SPEAKER = (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M4 9v6h4l5 4V5L8 9H4z" />
    <path d="M16.5 8.5a5 5 0 0 1 0 7" />
    <path d="M19 6a8.5 8.5 0 0 1 0 12" />
  </svg>
);

export function Say({ src, label, small, then }: { src: string; label: string; small?: boolean; then?: string[] }) {
  const on = usePlaying(src);
  const click = (e: MouseEvent) => {
    e.stopPropagation();
    play(then?.length ? [src, ...then] : src);
  };
  return (
    <button type="button" className={small ? 'say sm' : 'say'} aria-label={label} title={label} data-playing={on} onClick={click}>
      {SPEAKER}
    </button>
  );
}

export function Picture({ w, eager }: { w: WordFull; eager?: boolean }) {
  const img = w.example?.img;
  return img ? (
    <img
      className="photo"
      src={mediaUrl(img)}
      alt={`Picture for the example sentence of "${w.word}"`}
      loading={eager ? 'eager' : 'lazy'}
      decoding="async"
      width={500}
      height={400}
    />
  ) : (
    <div className="photo none" role="img" aria-label="No picture">
      {w.word}
    </div>
  );
}

/** The definition and example audio that follow the word when a card is read aloud in full. */
function afterWord(w: WordFull): string[] {
  return [primarySense(w)?.audio, w.example?.audio].filter((a): a is string => !!a);
}

/** Reads a card aloud: the word, its definition, then its example. */
export function readAloud(w: WordFull): void {
  play([w.audio, ...afterWord(w)]);
}

/** The headword's speaker reads the word, then (when they are on screen) its definition and example. */
export function Head({
  w,
  as: Tag = 'h1',
  link,
  readAll = true,
  children,
}: {
  w: WordFull;
  as?: 'h1' | 'h2';
  link?: string;
  readAll?: boolean;
  children?: ReactNode;
}) {
  const then = readAll ? afterWord(w) : [];
  return (
    <div className="head">
      <Tag className="word">{link ? <a href={link}>{w.word}</a> : w.word}</Tag>
      <Say
        src={w.audio}
        then={then}
        label={then.length ? `Play "${w.word}", its definition and example` : `Play "${w.word}"`}
      />
      {children}
    </div>
  );
}

export function Meta({ w, showPos = true }: { w: WordFull; showPos?: boolean }) {
  const p = primarySense(w);
  if (!w.phonetic && !(showPos && p)) return null;
  return (
    <div className="meta">
      {w.phonetic && <span>{w.phonetic}</span>}
      {showPos && p && <span className="pos">{posLabel(p.pos)}</span>}
    </div>
  );
}

export function Definition({ w }: { w: WordFull }) {
  const p = primarySense(w);
  if (!p) return null;
  const others = w.senses.filter((s) => s !== p);
  return (
    <div>
      <div className="labelrow">
        <span className="eyebrow">Definition</span>
        <Say small src={p.audio} label="Play the definition" />
      </div>
      <p className="def">{markWord(p.def, w.word)}</p>
      {others.length > 0 && (
        <ul className="senses" aria-label="Other meanings">
          {others.map((s, i) => (
            <li key={i}>
              <span className="pos">{posLabel(s.pos)}</span>
              <span className="txt">{s.def}</span>
              <Say small src={s.audio} label="Play this meaning" />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function ExampleBlock({ w }: { w: WordFull }) {
  const ex = w.example;
  if (!ex) return null;
  return (
    <div className="ex">
      <div className="labelrow">
        <span className="eyebrow">Example</span>
        <Say small src={ex.audio} label="Play the example" />
      </div>
      <p className="en">{markExample(ex, w.word)}</p>
    </div>
  );
}

const dateFmt = new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', year: 'numeric' });

export function Mine({ note, emptyText }: { note?: Note | null; emptyText?: string }) {
  if (!note) {
    if (!emptyText) return null;
    return (
      <blockquote className="mine empty">
        <span className="eyebrow">Your explanation</span>
        <p>{emptyText}</p>
      </blockquote>
    );
  }
  const at = Date.parse(note.at);
  return (
    <blockquote className="mine">
      <span className="eyebrow">Your explanation</span>
      <p>{note.text}</p>
      {Number.isFinite(at) && <span className="when">Written {dateFmt.format(at)}</span>}
    </blockquote>
  );
}

/** The full 4000-Essential-Words card: picture | word, phonetic, part of speech, definition, example. */
export function WordCard({
  w,
  variant,
  note,
  link,
  num,
  id,
  flash,
  onOpen,
  stage,
}: {
  w: WordFull;
  variant: 'study' | 'entry';
  /** The stream's step label, shown above the word. */
  stage?: ReactNode;
  note?: Note | null;
  link?: string;
  num?: string;
  id?: string;
  flash?: boolean;
  onOpen?: () => void;
}) {
  const entry = variant === 'entry';
  const click = (e: MouseEvent) => {
    if (!onOpen) return;
    const t = e.target as HTMLElement;
    if (t.closest('button, a') || window.getSelection()?.toString()) return;
    onOpen();
  };
  return (
    <article id={id} className={`card${entry ? ' entry' : ''}${flash ? ' flash' : ''}`} onClick={click}>
      <figure className="pic">
        <Picture w={w} eager={!entry} />
      </figure>
      <div className="body">
        {stage}
        <Head w={w} as={entry ? 'h2' : 'h1'} link={link}>
          {num && <span className="num">{num}</span>}
        </Head>
        <Meta w={w} />
        <Definition w={w} />
        <ExampleBlock w={w} />
        {entry ? (
          <Mine note={note} />
        ) : (
          <Mine note={note} emptyText="None yet. The explanation you write during review will be kept here." />
        )}
      </div>
    </article>
  );
}
