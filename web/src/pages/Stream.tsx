import { useEffect, useRef, useState } from 'react';
import { Rating } from 'ts-fsrs';
import type { Grade } from 'ts-fsrs';
import { loadWords, unitOf, unitWordIds, useAsync } from '../api';
import type { Index } from '../api';
import { Fill } from '../components/Fill';
import { Definition, ExampleBlock, Head, Meta, Picture, readAloud, Say, WordCard } from '../components/parts';
import { stop } from '../audio';
import { Rebuild } from '../components/Rebuild';
import { Spell } from '../components/Spell';
import { seeded, shuffle } from '../explain';
import { isTyping, plural, useKeydown } from '../hooks';
import { href } from '../router';
import { commit, formatInterval, localDate, saveNote, useProgress } from '../store';
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

const ordinal = (n: number) => {
  const tens = n % 100;
  const suffix = tens >= 11 && tens <= 13 ? 'th' : (['th', 'st', 'nd', 'rd'][n % 10] ?? 'th');
  return `${n}${suffix}`;
};

/** How a review task went, as the result card says it. */
const HOW: Record<'rebuild' | 'fill', Record<Outcome, string>> = {
  rebuild: { clean: 'Rebuilt with no mistakes', shaky: 'Rebuilt after one wrong check', failed: 'Needed help or several tries' },
  fill: { clean: 'Filled in first time', shaky: 'Filled in after a wrong pick', failed: 'Filled in after a wrong pick' },
};
const SPELLED: Record<Outcome, string> = {
  clean: 'Spelled with no mistakes',
  shaky: 'Spelled after one wrong check',
  failed: 'Needed help or several tries to spell it',
};
const RANK: Record<Outcome, number> = { clean: 0, shaky: 1, failed: 2 };
const worse = (a: Outcome, b: Outcome) => (RANK[a] >= RANK[b] ? a : b);

/** A finished review as its result card shows it; a rebuild review has two parts. */
interface Reviewed {
  w: WordFull;
  outcome: Outcome;
  task: 'rebuild' | 'fill';
  parts?: { rebuild: Outcome; spell: Outcome };
}

/** The step's kind as the learner reads it, with its motif: new (square), learning or review (diamonds). */
function Stage({ kind, unit, again, nth }: { kind: Current['kind']; unit?: number; again?: boolean; nth?: number }) {
  const [mark, label] =
    kind === 'know' && again
      ? ['mk learn', 'Look again']
      : kind === 'know'
        ? ['mk new', `New word${unit ? ` · Unit ${unit}` : ''}`]
        : kind === 'explain1' || kind === 'explain2'
          ? ['mk learn', 'Explain it']
          : kind === 'use'
            ? ['mk learn', 'Use it']
            : ['mk review', nth ? `Review · ${ordinal(nth)} time` : 'Review'];
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
  const [result, setResult] = useState<Reviewed | null>(null);
  // A rebuild review continues with spelling the word: how the rebuild went and how long it took.
  const [spelling, setSpelling] = useState<{ rebuild: Outcome; ms: number } | null>(null);
  const [cover, setCover] = useState(true);
  const started = useRef(Date.now());
  const rebuildMs = useRef(0);

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
    setSpelling(null);
    setCover(true);
    started.current = Date.now();
    if (loaded && cur?.kind === 'know') readAloud(loaded.w);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loaded]);

  const advance = () => {
    if (!cur || cur.kind === 'review') return;
    if (cur.kind !== 'know' && !outcome) return;
    const next = complete(progress, cur, outcome ?? 'clean', Date.now() - started.current);
    next.current = nextStep(next, index);
    commit(next);
    window.scrollTo(0, 0);
  };

  const reviewed = (o: Outcome, ms: number, parts?: Reviewed['parts']) => {
    if (!cur || !loaded) return;
    commit(complete(progress, cur, o, ms));
    setResult({ w: loaded.w, outcome: o, task: cur.task ?? 'rebuild', parts });
    readAloud(loaded.w);
  };

  /** The meaning is rebuilt: on to spelling the word from it. */
  const startSpelling = () => {
    if (!outcome) return;
    // The definition read out after the rebuild names the word.
    stop();
    setSpelling({ rebuild: outcome, ms: rebuildMs.current });
    started.current = Date.now();
    window.scrollTo(0, 0);
  };
  const reviewRebuilt = cur?.kind === 'review' && cur.task !== 'fill' && !!outcome && !spelling;

  const leaveResult = () => {
    setResult(null);
    window.scrollTo(0, 0);
  };

  useKeydown((e) => {
    if (isTyping(e) || e.altKey || e.ctrlKey || e.metaKey) return;
    if (e.key !== 'Enter' && e.key !== 'ArrowRight') return;
    if (e.target instanceof Element && e.target.closest('button, a')) return;
    if (result) leaveResult();
    else if (reviewRebuilt) startSpelling();
    else if (cur && (cur.kind === 'know' || outcome)) advance();
  });

  /* ---------- review result ---------- */
  if (result) return <ReviewResult r={result} progress={progress} onContinue={leaveResult} />;

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
  const solved = (o: Outcome) => {
    if (review && !rebuild) reviewed(o, Date.now() - started.current);
    else {
      if (review) rebuildMs.current = Date.now() - started.current;
      setOutcome(o);
    }
  };
  const picture = (
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
  );

  /* ---------- review, second part: spell the word from its definition ---------- */
  if (review && spelling) {
    return (
      <article className="card step" key={`${w.id}-spell`}>
        {picture}
        <div className="body">
          <Stage kind="review" nth={progress.cards[cur.word]?.reps} />
          <h1 className="ask-title">Which word means this?</h1>
          <Spell
            w={w}
            onSolved={(o) =>
              reviewed(worse(spelling.rebuild, o), spelling.ms + Date.now() - started.current, {
                rebuild: spelling.rebuild,
                spell: o,
              })
            }
          />
        </div>
      </article>
    );
  }

  return (
    <>
      <article className="card step" key={`${w.id}-${cur.kind}`}>
        {picture}
        <div className="body">
          <Stage kind={cur.kind} nth={review ? progress.cards[cur.word]?.reps : undefined} />
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
      {review && rebuild && (
        <StepNav
          enabled={!!outcome}
          onContinue={startSpelling}
          label="Now spell it"
          hint={outcome ? undefined : 'Then you spell the word from its meaning.'}
        />
      )}
    </>
  );
}

function StepNav({
  enabled,
  onContinue,
  hint,
  label = 'Continue',
}: {
  enabled: boolean;
  onContinue: () => void;
  hint?: string;
  label?: string;
}) {
  return (
    <div className="nav">
      <span className="count">{hint}</span>
      <button type="button" className="btn primary big" disabled={!enabled} onClick={onContinue}>
        {label} &rarr;
      </button>
    </div>
  );
}

function ReviewResult({ r, progress, onContinue }: { r: Reviewed; progress: Progress; onContinue: () => void }) {
  const { w, outcome, task, parts } = r;
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
          <Stage kind="review" nth={last.prev.reps} />
          <Head w={w} />
          <Meta w={w} />
          <div className={`result ${outcome}`}>
            <span className="how">
              {parts ? `${HOW.rebuild[parts.rebuild]} · ${SPELLED[parts.spell]}` : HOW[task][outcome]}
            </span>
            <Definition w={w} />
          </div>
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
            <span className="hintnote">Rated from how it went. Click another to change it.</span>
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

/** Units whose last words graduated today, and whether the next one is still untouched. */
function unitLine(p: Progress, index: Index): string | null {
  const today = localDate();
  const graduatedToday = Object.entries(p.cards)
    .filter(([, c]) => c.reps === 1 && c.last_review && localDate(new Date(c.last_review)) === today)
    .map(([id]) => Number(id));
  const finished = [...new Set(graduatedToday.map((id) => unitOf(index, id)))]
    .filter((u) => u > 0 && unitWordIds(index, u).every((id) => p.cards[id]))
    .sort((a, b) => a - b);
  const last = finished[finished.length - 1];
  if (!last) return null;
  const done = finished.length > 1 ? `Units ${finished.join(', ')} are finished` : `Unit ${last} is finished`;
  if (last >= index.unitCount) return `${done}; that was the last unit.`;
  const begun = unitWordIds(index, last + 1).some((id) => p.cards[id] || p.words[id]);
  return begun ? `${done}.` : `${done}; Unit ${last + 1} starts tomorrow.`;
}

function Done({ index, progress }: { index: Index; progress: Progress }) {
  const p = today(progress);
  const tomorrow = dueTomorrow(p, index);
  const days = streak(p);
  const units = unitLine(p, index);
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
      <p>
        {days > 0 ? `${plural(days, 'day')} in a row.` : 'Come back tomorrow to start a streak.'}
        {units && ` ${units}`}
      </p>
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
