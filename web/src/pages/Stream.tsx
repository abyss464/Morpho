import { useEffect, useRef, useState } from 'react';
import { Rating } from 'ts-fsrs';
import type { Grade } from 'ts-fsrs';
import { loadWords, unitOf, useAsync } from '../api';
import type { Index } from '../api';
import { Fill } from '../components/Fill';
import { Definition, ExampleBlock, Head, Meta, Picture, readAloud, Say, WordCard } from '../components/parts';
import { Rebuild } from '../components/Rebuild';
import { seeded, shuffle } from '../explain';
import { isTyping, plural, useKeydown } from '../hooks';
import { href } from '../router';
import { commit, formatInterval, saveNote, useProgress } from '../store';
import type { Current, Progress } from '../store';
import { complete, dueTomorrow, intervals, nextStep, overrideRating, streak, today } from '../stream';
import type { Outcome } from '../stream';
import type { WordFull } from '../types';

const RATINGS: { g: Grade; label: string }[] = [
  { g: Rating.Again, label: 'Again' },
  { g: Rating.Hard, label: 'Hard' },
  { g: Rating.Good, label: 'Good' },
  { g: Rating.Easy, label: 'Easy' },
];
const RATING_NAME: Record<number, string> = {
  1: 'Again',
  2: 'Hard',
  3: 'Good',
  4: 'Easy',
};

/** The step's kind as the learner reads it, with its motif: new (square), learning or review (diamonds). */
function Stage({ kind, unit, again }: { kind: Current['kind']; unit?: number; again?: boolean }) {
  const [mark, label] =
    kind === 'know' && again
      ? ['mk learn', 'Look again']
      : kind === 'know'
        ? ['mk new', `New word${unit ? ` · Unit ${unit}` : ''}`]
        : kind === 'explain1' || kind === 'explain2'
          ? ['mk learn', 'Explain it']
          : kind === 'use'
            ? ['mk learn', 'Use it']
            : ['mk review', 'Review'];
  return (
    <span className="stage">
      <i className={mark} aria-hidden="true" />
      {label}
    </span>
  );
}

/** Words whose definitions supply decoy pieces: those being learned now for easy puzzles, any met word for hard ones. */
function poolIds(p: Progress, index: Index, cur: Current): number[] {
  const met = Object.keys(p.cards).map(Number);
  if (cur.kind === 'explain1') {
    const recent = met.sort((a, b) => (index.pos.get(b) ?? 0) - (index.pos.get(a) ?? 0)).slice(0, 8);
    return [...new Set([...Object.keys(p.words).map(Number), ...recent])].filter((id) => id !== cur.word);
  }
  return shuffle(
    met.filter((id) => id !== cur.word),
    seeded(cur.word),
  ).slice(0, 12);
}

const needsPool = (c: Current) =>
  c.kind === 'explain1' || c.kind === 'explain2' || (c.kind === 'review' && c.task === 'rebuild');
const needsOthers = (c: Current) => c.kind === 'use' || (c.kind === 'review' && c.task === 'fill');

interface Loaded {
  w: WordFull;
  pool: WordFull[];
  others: WordFull[];
}

export function Stream({ index }: { index: Index }) {
  const progress = useProgress();
  const cur = progress.current;
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [result, setResult] = useState<WordFull | null>(null);
  const [cover, setCover] = useState(true);
  const started = useRef(Date.now());

  // Keep a step on screen: resume the saved one, else choose the next.
  useEffect(() => {
    if (progress.current || result) return;
    const next = nextStep(progress, index);
    if (next) commit({ ...today(progress), current: next });
  }, [progress, result, index]);

  const data = useAsync<Loaded | null>(async () => {
    if (!cur) return null;
    const w = (await loadWords([cur.word])).get(cur.word);
    if (!w) throw new Error(`Word ${cur.word} is not in this release.`);
    const pool = needsPool(cur) ? [...(await loadWords(poolIds(progress, index, cur))).values()] : [];
    const others = needsOthers(cur) ? [...(await loadWords(w.distractors)).values()] : [];
    return { w, pool, others };
    // Reload only when the step changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cur?.word, cur?.kind, cur?.task]);
  const loaded = data.status === 'ok' ? data.data : null;

  useEffect(() => {
    setOutcome(null);
    setCover(true);
    started.current = Date.now();
    if (loaded && cur?.kind === 'know') readAloud(loaded.w);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loaded]);

  const advance = () => {
    if (!cur) return;
    if (cur.kind !== 'know' && !outcome) return;
    const next = complete(progress, cur, outcome ?? 'clean', Date.now() - started.current);
    next.current = nextStep(next, index);
    commit(next);
    window.scrollTo(0, 0);
  };

  const reviewed = (o: Outcome) => {
    if (!cur || !loaded) return;
    commit(complete(progress, cur, o, Date.now() - started.current));
    setResult(loaded.w);
    readAloud(loaded.w);
  };

  const leaveResult = () => {
    setResult(null);
    window.scrollTo(0, 0);
  };

  useKeydown((e) => {
    if (isTyping(e) || e.altKey || e.ctrlKey || e.metaKey) return;
    if (e.key !== 'Enter' && e.key !== 'ArrowRight') return;
    if (e.target instanceof Element && e.target.closest('button, a')) return;
    if (result) leaveResult();
    else if (cur && (cur.kind === 'know' || outcome)) advance();
  });

  /* ---------- review result ---------- */
  if (result) return <ReviewResult w={result} progress={progress} onContinue={leaveResult} />;

  /* ---------- done ---------- */
  if (!cur) return <Done index={index} progress={progress} />;

  if (data.status === 'error') {
    return (
      <section className="notice">
        <h2>Could not load this step.</h2>
        <p>{data.error}</p>
      </section>
    );
  }
  if (!loaded) return <p className="loading">Loading&hellip;</p>;
  const { w } = loaded;

  /* ---------- know ---------- */
  if (cur.kind === 'know') {
    return (
      <>
        <WordCard
          key={w.id}
          w={w}
          variant="study"
          note={progress.notes[w.id]}
          stage={<Stage kind="know" unit={unitOf(index, w.id)} again={!!progress.words[w.id]} />}
        />
        <StepNav enabled onContinue={advance} hint="Reading aloud: word, definition, then example." />
      </>
    );
  }

  /* ---------- explain, use, review task ---------- */
  const review = cur.kind === 'review';
  const rebuild = cur.kind === 'explain1' || cur.kind === 'explain2' || (review && cur.task === 'rebuild');
  const showPicture = cur.kind === 'explain1' || cur.kind === 'use' || (review && !cover);
  const solved = (o: Outcome) => (review ? reviewed(o) : setOutcome(o));

  return (
    <>
      <article className="card step" key={`${w.id}-${cur.kind}`}>
        <figure className="pic">
          {showPicture ? (
            <Picture w={w} eager />
          ) : (
            <div className="cover">
              <span className="qm" aria-hidden="true">
                ?
              </span>
              {review && (
                <button type="button" className="btn" onClick={() => setCover(false)}>
                  Show picture
                </button>
              )}
            </div>
          )}
        </figure>
        <div className="body">
          <Stage kind={cur.kind} />
          {rebuild ? (
            <>
              <div className="head">
                <h1 className="ask-title">
                  What does <i>{w.word}</i> mean?
                </h1>
                <Say src={w.audio} label={`Play "${w.word}"`} />
              </div>
              <Rebuild
                w={w}
                pool={loaded.pool}
                difficulty={cur.kind === 'explain1' ? 'easy' : 'hard'}
                onSolved={solved}
              />
            </>
          ) : (
            <>
              <h1 className="ask-title">Which word fits?</h1>
              <Fill w={w} others={loaded.others} onSolved={solved} />
            </>
          )}
        </div>
      </article>
      {!review && (
        <StepNav
          enabled={!!outcome}
          onContinue={advance}
          hint={outcome ? undefined : 'Continue opens once it is done.'}
        />
      )}
    </>
  );
}

function StepNav({ enabled, onContinue, hint }: { enabled: boolean; onContinue: () => void; hint?: string }) {
  return (
    <div className="nav">
      <span className="count">{hint}</span>
      <button type="button" className="btn primary big" disabled={!enabled} onClick={onContinue}>
        Continue &rarr;
      </button>
    </div>
  );
}

function ReviewResult({ w, progress, onContinue }: { w: WordFull; progress: Progress; onContinue: () => void }) {
  const last = progress.lastReview;
  const [writing, setWriting] = useState(false);
  const [text, setText] = useState('');
  if (!last || last.word !== w.id) return null;
  const iv = intervals(last.prev);
  const pick = (g: Grade) => commit(overrideRating(progress, g));
  return (
    <>
      <article className="card step" key={`r-${w.id}`}>
        <figure className="pic">
          <Picture w={w} eager />
        </figure>
        <div className="body">
          <Stage kind="review" />
          <Head w={w} />
          <Meta w={w} />
          <Definition w={w} />
          <ExampleBlock w={w} />
          <div className="rating">
            <p className="verdict">
              <b>{RATING_NAME[last.rating]}</b> next review in {formatInterval(iv[last.rating])}
            </p>
            <div className="seg" role="radiogroup" aria-label="Change the rating">
              {RATINGS.map(({ g, label }) => (
                <button
                  key={g}
                  type="button"
                  role="radio"
                  data-v={g}
                  aria-checked={last.rating === g}
                  onClick={() => pick(g)}
                >
                  <span>{label}</span>
                  <span className="iv">{formatInterval(iv[g])}</span>
                </button>
              ))}
            </div>
            <span className="hintnote">Rated from how you did. Click another to change it.</span>
          </div>
          {writing ? (
            <div className="own">
              <label htmlFor="own" className="eyebrow">
                In your own words
              </label>
              <textarea
                id="own"
                rows={2}
                value={text}
                onChange={(e) => setText(e.target.value)}
                placeholder="Optional"
              />
              <div className="actions">
                <button
                  type="button"
                  className="btn"
                  disabled={!text.trim()}
                  onClick={() => {
                    saveNote(w.id, text);
                    setWriting(false);
                  }}
                >
                  Save
                </button>
              </div>
            </div>
          ) : (
            <button type="button" className="link" onClick={() => setWriting(true)}>
              {progress.notes[w.id] ? 'Rewrite it in your own words' : 'Add it in your own words (optional)'}
            </button>
          )}
        </div>
      </article>
      <StepNav enabled onContinue={onContinue} />
    </>
  );
}

function Done({ index, progress }: { index: Index; progress: Progress }) {
  const p = today(progress);
  const tomorrow = dueTomorrow(p, index);
  const days = streak(p);
  const more = () => commit({ ...p, day: { ...p.day, extra: p.day.extra + 5 } });
  return (
    <section className="notice done">
      <span className="eyebrow">Today's stream is done</span>
      <h1>
        {plural(p.day.steps, 'step')}, {plural(p.day.reviewed + p.day.met, 'word')}.
      </h1>
      <div className="tiles">
        <div className="tile">
          <span className="eyebrow">
            <i className="mk review" aria-hidden="true" />
            Reviewed
          </span>
          <b>{p.day.reviewed}</b>
          <span>{p.day.reviewedClean} with no mistakes</span>
        </div>
        <div className="tile">
          <span className="eyebrow">
            <i className="mk new" aria-hidden="true" />
            Met
          </span>
          <b>{p.day.met}</b>
          <span>{p.day.metClean} explained and used first time</span>
        </div>
        <div className="tile">
          <span className="eyebrow">
            <i className="mk learn" aria-hidden="true" />
            Tomorrow
          </span>
          <b>{tomorrow}</b>
          <span>reviews due, plus new words</span>
        </div>
      </div>
      <p>{days > 0 ? `${plural(days, 'day')} in a row.` : 'Come back tomorrow to start a streak.'}</p>
      <div className="actions">
        <a className="btn primary big" href={href.home()}>
          Done for today
        </a>
        <button type="button" className="btn big" onClick={more}>
          Meet 5 more words
        </button>
      </div>
    </section>
  );
}
