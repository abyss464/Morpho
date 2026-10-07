import { unitWordIds } from '../api';
import type { Index } from '../api';
import { Search } from '../components/Search';
import { plural } from '../hooks';
import { href } from '../router';
import { setNewPerDay, useProgress } from '../store';
import { BACKLOG, dueReviews, newAllowance, today, week } from '../stream';

const PER_DAY = [10, 20, 30, 40, 50];

/** The only way in: one line on what today holds and one Continue into the stream. */
export function Today({ index }: { index: Index }) {
  const progress = today(useProgress());
  const due = dueReviews(progress, index).length;
  const fresh = due < BACKLOG ? newAllowance(progress) : 0;
  const met = new Set([...Object.keys(progress.cards), ...Object.keys(progress.words)]).size;
  const total = index.words.length;
  const days = week(progress);
  const peak = Math.max(1, ...days.map((d) => d.steps));
  const units = Array.from({ length: index.unitCount }, (_, k) => {
    const ids = unitWordIds(index, k + 1);
    return { n: k + 1, size: ids.length, done: ids.filter((id) => progress.cards[id]).length };
  });
  const finished = units.filter((u) => u.done === u.size).length;
  const firstOpen = units.findIndex((u) => u.done < u.size);
  const shown = units.slice(Math.max(0, firstOpen - 2), Math.max(0, firstOpen - 2) + 8);
  const midStream = progress.day.steps > 0 || progress.current !== null;
  const nothingLeft = !due && !fresh && Object.keys(progress.words).length === 0;

  return (
    <>
      <section className="today" aria-label="Today">
        <div>
          <h1>Today</h1>
          <p>
            {due || fresh
              ? `${plural(due, 'review')} and ${plural(fresh, 'new word')}`
              : 'All done for today.'}
            {due >= BACKLOG && ' · new words wait until reviews are cleared'}
          </p>
        </div>
        <a className="btn primary big" href={href.stream()}>
          {nothingLeft ? "Today's summary" : midStream ? 'Continue' : 'Start'}
        </a>
      </section>

      <div className="panels">
        <section className="panel" aria-label="Journey">
          <span className="eyebrow">Journey</span>
          <p className="big">
            {met.toLocaleString('en-US')}
            <small>of {total.toLocaleString('en-US')} words</small>
          </p>
          <div className="bar-track" aria-hidden="true">
            <i style={{ width: `${(met / Math.max(1, total)) * 100}%` }} />
          </div>
          {met < total && (
            <p>
              Unit {firstOpen < 0 ? index.unitCount : firstOpen + 1} of {index.unitCount} · about{' '}
              {plural(Math.ceil((total - met) / progress.newPerDay), 'day')} at {progress.newPerDay} a day
            </p>
          )}
          <label className="perday">
            New words a day
            <select value={progress.newPerDay} onChange={(e) => setNewPerDay(Number(e.target.value))}>
              {PER_DAY.map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </label>
        </section>
        <section className="panel" aria-label="This week">
          <span className="eyebrow">This week</span>
          <div className="week" aria-hidden="true">
            {days.map((d, k) => (
              <i key={k} className={d.today ? 'now' : ''} style={{ height: `${Math.max(4, (d.steps / peak) * 56)}px` }} />
            ))}
          </div>
          <div className="week-days">
            {days.map((d, k) => (
              <span key={k} className={d.today ? 'now' : ''}>
                {d.day}
              </span>
            ))}
          </div>
        </section>
      </div>

      <Search index={index} />

      <section className="deck" aria-label="Units">
        <h2>
          <span>Units · progress</span>
          <span>
            Finished <b>{finished}</b> / {index.unitCount}
          </span>
        </h2>
        <div className="chips">
          {shown.map((u) => (
            <a key={u.n} className="chip uchip" href={href.unit(u.n)} aria-label={`Unit ${u.n}, ${u.done} of ${u.size} words in review`}>
              <span className="top">
                <span className="cw">Unit {u.n}</span>
                <span className={u.done === u.size ? 'tally done' : 'tally'}>
                  {u.done}/{u.size}
                </span>
              </span>
              <span className="bar-track" aria-hidden="true">
                <i style={{ width: `${(u.done / u.size) * 100}%` }} />
              </span>
            </a>
          ))}
        </div>
      </section>
    </>
  );
}
